//! Orthographic camera tracking and presentation effects.

#[cfg(feature = "camera-scene")]
mod authored;

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, VirtualTime,
};
use nara_core::{Affine2, Vec2};
use nara_ecs::{
    Component, DetectChanges, DetectChangesMut, Entity, Query, Ref, Res, Resource, SystemParam,
    bevy_ecs::change_detection::Tick,
};
use nara_scene::spatial::GlobalTransform2d;

use crate::{RENDER_PLUGIN_ID, RenderViewId};

const ID: PluginId = PluginId::new("nara.camera-rig-2d");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ID, PluginCategory::Render).requires_plugins(&[RENDER_PLUGIN_ID]);

/// Enables rig tracking and shake during view extraction. With `camera-scene` it also hydrates
/// authored cameras before the spatial pass in `Extract`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CameraRig2dPlugin;

impl Plugin for CameraRig2dPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<CameraRigsEnabled>()?;
        #[cfg(feature = "camera-scene")]
        authored::install(app)?;
        Ok(())
    }
}

/// An axis-aligned world rectangle that contains the camera's footprint when it fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraBounds2d {
    pub min: Vec2,
    pub max: Vec2,
}

impl CameraBounds2d {
    #[must_use]
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    fn is_valid(self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min.cmple(self.max).all()
    }

    fn center_limits(self, half_view: Vec2) -> (Vec2, Vec2) {
        let midpoint = self.min * 0.5 + self.max * 0.5;
        // An oversized view centers on the world independently on each axis.
        (
            (self.min + half_view).min(midpoint),
            (self.max - half_view).max(midpoint),
        )
    }
}

/// Tracks a target in the completed spatial state. The camera's transform anchors the rig.
/// Changes to the anchor position restart tracking from that position. The resulting view
/// pose is available through [`ExtractedViews`](crate::ExtractedViews).
#[derive(Debug, Default, Clone, Copy, PartialEq, Component)]
#[require(CameraTracking2d)]
pub struct CameraRig2d {
    /// A target with `GlobalTransform2d`. If it becomes unavailable, the rig holds its center.
    pub target: Option<Entity>,
    /// Full width and height of the deadzone in camera-local units. Invalid dimensions use zero.
    pub deadzone: Vec2,
    /// Exponential damping time constant in virtual seconds.
    /// Zero or invalid values snap to the goal.
    pub smooth_time: f32,
    /// Optional world bounds. Non-finite or inverted rectangles are ignored.
    pub bounds: Option<CameraBounds2d>,
    /// Positive, finite pixels per world unit for snapping the displayed center.
    pub pixel_snap: Option<f32>,
}

impl CameraRig2d {
    #[must_use]
    pub const fn with_target(mut self, target: Entity) -> Self {
        self.target = Some(target);
        self
    }

    fn tracking_goal(self, base: Affine2, center: Vec2, target: Vec2) -> Vec2 {
        if !self.deadzone.is_finite()
            || self.deadzone.min_element() < 0.0
            || self.deadzone == Vec2::ZERO
        {
            return target;
        }
        let half_zone = self.deadzone * 0.5;
        let delta = base.matrix2.inverse() * (target - center);
        let outside = delta - delta.clamp(-half_zone, half_zone);
        let goal = center + base.transform_vector2(outside);
        if goal.is_finite() { goal } else { center }
    }

    fn follow_weight(self, delta: f32) -> f32 {
        if self.smooth_time.is_finite() && self.smooth_time > 0.0 {
            -(-delta / self.smooth_time).exp_m1()
        } else {
            1.0
        }
    }
}

/// Seeded view shake. Trauma decays in virtual time and its square scales the displacement.
/// It can be used on any `Camera2d` with [`CameraRig2dPlugin`] installed.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct CameraShake2d {
    /// Current strength in `0.0..=1.0`.
    pub trauma: f32,
    /// Trauma lost per virtual second. Set to zero for a continuous effect.
    pub decay: f32,
    /// Peak translation on each world axis, in world units.
    pub max_translation: Vec2,
    /// Peak rotation in radians.
    pub max_rotation: f32,
    pub seed: u32,
}

impl Default for CameraShake2d {
    fn default() -> Self {
        Self {
            trauma: 0.0,
            decay: 1.5,
            max_translation: Vec2::splat(6.0),
            max_rotation: 0.03,
            seed: 0,
        }
    }
}

impl CameraShake2d {
    /// Adds a finite positive impulse, saturating at full trauma.
    pub fn add_trauma(&mut self, amount: f32) {
        if amount.is_finite() && amount > 0.0 {
            self.trauma = (valid_trauma(self.trauma) + amount).min(1.0);
        }
    }

    fn sample(self, trauma: f32, elapsed: f64) -> Affine2 {
        if trauma == 0.0 {
            return Affine2::IDENTITY;
        }
        let strength = trauma * trauma;
        let phase = f64::from(self.seed.wrapping_mul(0x9e37_79b9))
            * (std::f64::consts::TAU / 4_294_967_296.0);
        let wave = |speed: f64, offset: f64| (elapsed * speed + phase + offset).sin() as f32;
        let translation = strength
            * Vec2::new(
                nonnegative(self.max_translation.x) * wave(97.0, 0.0),
                nonnegative(self.max_translation.y) * wave(113.0, 1.0),
            );
        let rotation = strength * nonnegative(self.max_rotation) * wave(127.0, 2.0);
        Affine2::from_angle_translation(rotation, translation)
    }
}

fn nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn valid_trauma(value: f32) -> f32 {
    nonnegative(value).min(1.0)
}

#[derive(Default, Resource)]
struct CameraRigsEnabled;

#[derive(Default, PartialEq, Component)]
struct CameraTracking2d(Option<Tracking>);

#[derive(Clone, Copy, PartialEq)]
struct Tracking {
    view: RenderViewId,
    rig_added: Tick,
    anchor: Vec2,
    center: Vec2,
}

#[derive(SystemParam)]
pub(crate) struct CameraRigExtraction<'w, 's> {
    enabled: Option<Res<'w, CameraRigsEnabled>>,
    rigs: Query<'w, 's, (Ref<'static, CameraRig2d>, &'static mut CameraTracking2d)>,
    shakes: Query<'w, 's, &'static mut CameraShake2d>,
    globals: Query<'w, 's, &'static GlobalTransform2d>,
    #[cfg(feature = "camera-scene")]
    authored: Query<'w, 's, &'static authored::HydratedCamera2d>,
}

impl CameraRigExtraction<'_, '_> {
    #[cfg(feature = "camera-scene")]
    pub(crate) fn is_active(&self, entity: Entity) -> bool {
        self.authored
            .get(entity)
            .map_or(true, authored::HydratedCamera2d::is_active)
    }

    #[cfg(not(feature = "camera-scene"))]
    pub(crate) const fn is_active(&self, _: Entity) -> bool {
        true
    }

    pub(crate) fn advance_shake(&mut self, entity: Entity, time: &VirtualTime) -> Affine2 {
        if self.enabled.is_none() {
            return Affine2::IDENTITY;
        }
        let Ok(mut shake) = self.shakes.get_mut(entity) else {
            return Affine2::IDENTITY;
        };
        let trauma =
            (valid_trauma(shake.trauma) - nonnegative(shake.decay) * time.delta_seconds()).max(0.0);
        if shake.trauma != trauma {
            shake.trauma = trauma;
        }
        shake.sample(trauma, time.elapsed_seconds())
    }

    pub(crate) fn pose(
        &mut self,
        entity: Entity,
        view: RenderViewId,
        base: Affine2,
        half_view: Vec2,
        shake: Affine2,
        delta: f32,
    ) -> Affine2 {
        if self.enabled.is_none() {
            return base;
        }
        let mut display = Affine2 {
            matrix2: shake.matrix2 * base.matrix2,
            translation: base.translation,
        };
        let Ok((rig, mut tracking)) = self.rigs.get_mut(entity) else {
            let center = base.translation + shake.translation;
            if center.is_finite() {
                display.translation = center;
            }
            return display;
        };

        let mut current = Tracking {
            view,
            rig_added: rig.added(),
            anchor: base.translation,
            center: base.translation,
        };
        if let Some(previous) = tracking.0
            && previous.view == view
            && previous.rig_added == current.rig_added
            && previous.anchor == current.anchor
        {
            current.center = previous.center;
        }
        let bounds = rig.bounds.filter(|bounds| bounds.is_valid());
        let half_footprint = world_half_footprint(base, half_view);
        let limits = bounds.map(|bounds| bounds.center_limits(half_footprint));
        let center = constrain(current.center, limits, None);
        let target = rig
            .target
            .filter(|&target| target != entity)
            .and_then(|target| self.globals.get(target).ok())
            .map(GlobalTransform2d::translation)
            .filter(|target| target.is_finite());
        let goal = target.map_or(center, |target| rig.tracking_goal(base, center, target));
        let goal = constrain(goal, limits, None);
        current.center = center;
        if goal != center {
            let smoothed = center.lerp(goal, rig.follow_weight(delta));
            if smoothed.is_finite() {
                current.center = constrain(smoothed, limits, None);
            }
        }
        tracking.set_if_neq(CameraTracking2d(Some(current)));

        let display_limits = bounds.map(|bounds| {
            let shaken = world_half_footprint(display, half_view);
            let half_world = bounds.max * 0.5 - bounds.min * 0.5;
            if (shaken.cmpgt(half_world) & half_footprint.cmple(half_world)).any() {
                // Keep a fitting view inside the world when rotation would make it too large.
                display.matrix2 = base.matrix2;
                bounds.center_limits(half_footprint)
            } else {
                bounds.center_limits(shaken)
            }
        });
        let offset = current.center + shake.translation;
        let center = if offset.is_finite() {
            offset
        } else {
            current.center
        };
        display.translation = constrain(center, display_limits, rig.pixel_snap);
        display
    }
}

fn world_half_footprint(base: Affine2, half_view: Vec2) -> Vec2 {
    base.matrix2.x_axis.abs() * half_view.x + base.matrix2.y_axis.abs() * half_view.y
}

fn constrain(center: Vec2, limits: Option<(Vec2, Vec2)>, pixel_snap: Option<f32>) -> Vec2 {
    let (min, max) = limits.unwrap_or((Vec2::splat(f32::MIN), Vec2::splat(f32::MAX)));
    let Some(ppu) = pixel_snap.filter(|&ppu| ppu.is_finite() && ppu > 0.0) else {
        return center.clamp(min, max);
    };
    let ppu = f64::from(ppu);
    let snap = |value: f32, min: f32, max: f32| {
        let first_pixel = (f64::from(min) * ppu).ceil();
        let last_pixel = (f64::from(max) * ppu).floor();
        if first_pixel > last_pixel {
            return value.clamp(min, max);
        }
        let pixel = (f64::from(value) * ppu)
            .round()
            .clamp(first_pixel, last_pixel);
        (pixel / ppu) as f32
    };
    Vec2::new(snap(center.x, min.x, max.x), snap(center.y, min.y, max.y)).clamp(min, max)
}
