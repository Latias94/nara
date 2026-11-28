use std::time::Duration;

use bevy_ecs::{
    entity::Entity,
    prelude::{Component, Local, Message, Messages, QueryState, With, World},
    schedule::IntoScheduleConfigs,
};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, FrameCleanup, Plugin, PluginCategory,
    PluginDeclaration, PluginError, PluginId, VirtualTime,
};
use nara_core::{Color, Vec2};
use nara_reflect::{ComponentFieldPath, ComponentTypeId, Schema};

use crate::spatial::Transform2d;

use super::{
    AnimationClock, AnimationSet, Easing, KeyframeValue, PropertyAnimationError, PropertyAppliers,
    PropertyWrite, SPRITE_COMPONENT_ID, player::apply_property,
};

const ID: PluginId = PluginId::new("nara.tween-2d");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime);

/// A one-shot tween on this entity. Set up any number of fields and receive [`TweenFinished`]
/// once. A zero-duration tween writes its final values when its delay ends.
#[derive(Component)]
pub struct Tween2d {
    tracks: Vec<TweenTrack>,
    duration: Duration,
    delay: Duration,
    elapsed: Duration,
    easing: Easing,
    finished: bool,
    valid: bool,
    error: Option<PropertyAnimationError>,
    pub clock: AnimationClock,
}

struct TweenTrack {
    component: ComponentTypeId,
    path: ComponentFieldPath,
    from: KeyframeValue,
    to: KeyframeValue,
}

impl Tween2d {
    #[must_use]
    pub const fn new(duration: Duration, easing: Easing) -> Self {
        Self {
            tracks: Vec::new(),
            duration,
            delay: Duration::ZERO,
            elapsed: Duration::ZERO,
            easing,
            finished: false,
            valid: true,
            error: None,
            clock: AnimationClock::Fixed,
        }
    }

    #[must_use]
    pub const fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    #[must_use]
    pub const fn with_clock(mut self, clock: AnimationClock) -> Self {
        self.clock = clock;
        self
    }

    #[must_use]
    pub fn with_translation(self, from: Vec2, to: Vec2) -> Self {
        self.track(
            Transform2d::COMPONENT_ID,
            "translation",
            KeyframeValue::Vec2(from),
            KeyframeValue::Vec2(to),
        )
    }

    #[must_use]
    pub fn with_scale(self, from: Vec2, to: Vec2) -> Self {
        self.track(
            Transform2d::COMPONENT_ID,
            "scale",
            KeyframeValue::Vec2(from),
            KeyframeValue::Vec2(to),
        )
    }

    #[must_use]
    pub fn with_rotation(self, from: f32, to: f32) -> Self {
        self.track(
            Transform2d::COMPONENT_ID,
            "rotation",
            KeyframeValue::F32(from),
            KeyframeValue::F32(to),
        )
    }

    /// Interpolates the tint of a runtime sprite when its renderer has registered a typed writer.
    #[must_use]
    pub fn with_tint(self, from: Color, to: Color) -> Self {
        self.track(
            SPRITE_COMPONENT_ID,
            "tint",
            KeyframeValue::Color(from),
            KeyframeValue::Color(to),
        )
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub const fn error(&self) -> Option<&PropertyAnimationError> {
        self.error.as_ref()
    }

    pub fn restart(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
        if self.valid {
            self.error = None;
        }
    }

    fn track(
        mut self,
        component: &'static str,
        field: &'static str,
        from: KeyframeValue,
        to: KeyframeValue,
    ) -> Self {
        if !finite(&from) || !finite(&to) {
            self.valid = false;
            self.error = Some(PropertyAnimationError::InvalidTweenEndpoints);
            return self;
        }
        self.tracks.push(TweenTrack {
            component: ComponentTypeId::new(component),
            path: ComponentFieldPath::from_fields([field]),
            from,
            to,
        });
        self
    }
}

fn finite(value: &KeyframeValue) -> bool {
    match *value {
        KeyframeValue::Vec2(value) => value.is_finite(),
        KeyframeValue::F32(value) => value.is_finite(),
        KeyframeValue::Color(color) => [color.r, color.g, color.b, color.a]
            .into_iter()
            .all(f32::is_finite),
        _ => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct TweenFinished {
    pub entity: Entity,
}

/// Runs code-first tweens on fixed or virtual time.
#[derive(Clone, Copy, Debug, Default)]
pub struct TweenPlugin;

impl Plugin for TweenPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<PropertyAppliers>()?
            .init_resource::<Messages<TweenFinished>>()?
            .configure_sets(
                CoreStage::FixedUpdate,
                AnimationSet::Fixed.in_set(FixedUpdateSet::Prepare),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                fixed_tweens
                    .in_set(AnimationSet::Fixed)
                    .after(super::player::advance_fixed),
            )?
            .add_systems(
                CoreStage::Update,
                virtual_tweens
                    .in_set(AnimationSet::Virtual)
                    .after(super::player::advance_virtual),
            )?
            .add_systems(FrameCleanup, |world: &mut World| {
                world.resource_mut::<Messages<TweenFinished>>().update()
            })?;
        Ok(())
    }
}

type ActiveTweens = QueryState<Entity, With<Tween2d>>;

fn fixed_tweens(world: &mut World, query: &mut ActiveTweens, scratch: Local<Vec<Entity>>) {
    let delta = world.resource::<FixedTime>().delta();
    advance_tweens(world, query, scratch, AnimationClock::Fixed, delta);
}

fn virtual_tweens(world: &mut World, query: &mut ActiveTweens, scratch: Local<Vec<Entity>>) {
    let delta = world.resource::<VirtualTime>().delta;
    advance_tweens(world, query, scratch, AnimationClock::Virtual, delta);
}

fn advance_tweens(
    world: &mut World,
    query: &mut ActiveTweens,
    mut scratch: Local<Vec<Entity>>,
    clock: AnimationClock,
    delta: Duration,
) {
    scratch.clear();
    scratch.extend(query.iter(world));
    for &entity in scratch.iter() {
        let Some(mut tween) = world.get_mut::<Tween2d>(entity) else {
            continue;
        };
        if tween.clock != clock || tween.finished || tween.error.is_some() {
            continue;
        }
        let elapsed = tween.elapsed.saturating_add(delta);
        let Some(active) = elapsed.checked_sub(tween.delay) else {
            tween.elapsed = elapsed;
            continue;
        };
        let finished = active >= tween.duration;
        let progress = if finished {
            1.0
        } else {
            active.div_duration_f32(tween.duration)
        };
        let progress = tween.easing.sample(progress);
        let tracks = std::mem::take(&mut tween.tracks);
        let applied = tracks.iter().try_fold(true, |ready, track| {
            let value = track.from.interpolate(&track.to, progress);
            apply_property(world, entity, &track.component, &track.path, value.as_ref())
                .map(|result| ready && result == PropertyWrite::Applied)
        });
        if let Some(mut tween) = world.get_mut::<Tween2d>(entity) {
            tween.tracks = tracks;
            if matches!(applied, Ok(false)) {
                continue;
            }
            tween.elapsed = elapsed;
            tween.finished = finished && applied.is_ok();
            tween.error = applied.err();
            if tween.finished {
                world
                    .resource_mut::<Messages<TweenFinished>>()
                    .write(TweenFinished { entity });
            }
        }
    }
}
