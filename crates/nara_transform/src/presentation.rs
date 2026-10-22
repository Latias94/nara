use nara_app::{
    App, CoreStage, FixedUpdateSet, PluginError, RenderTime, RuntimeTimeSettings, StartupStage,
};
use nara_core::Mat3;
use nara_ecs::{
    Entity, FromWorld, Mut, QueryState, Resource, World, entity::EntityHashMap, error::BevyError,
    schedule::IntoScheduleConfigs,
};
use nara_hierarchy::Parent;

use crate::{__private::TransformSet, Transform2d};

/// Opts this world's presentation into interpolation between its previous and current fixed tick.
///
/// Insert this resource before startup. Simulation transforms remain authoritative and unchanged;
/// paused frames, new entities, and authoring changes display their current transform immediately.
#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct FixedTransformInterpolation;

#[derive(Clone, Copy)]
struct LocalSamples {
    previous: Transform2d,
    current: Transform2d,
    parent: Option<Entity>,
}

#[derive(Resource)]
struct TransformHistory {
    locals: QueryState<(Entity, &'static Transform2d)>,
    samples: EntityHashMap<LocalSamples>,
}

impl FromWorld for TransformHistory {
    fn from_world(world: &mut World) -> Self {
        Self {
            locals: world.query(),
            samples: EntityHashMap::default(),
        }
    }
}

#[derive(Clone, Copy, Default)]
enum ProjectionState {
    #[default]
    Current,
    Incomplete,
    Interpolated,
}

/// Read-only render matrices derived by the transform owner. This never replaces simulation data.
#[doc(hidden)]
#[derive(Default, Resource)]
pub struct RenderTransformProjection {
    state: ProjectionState,
    matrices: EntityHashMap<Mat3>,
}

impl RenderTransformProjection {
    /// Resolves the explicit presentation policy, failing closed for an incomplete projection.
    #[must_use]
    pub fn matrix(&self, entity: Entity, current: Mat3) -> Option<Mat3> {
        match self.state {
            ProjectionState::Current => Some(current),
            ProjectionState::Incomplete => None,
            ProjectionState::Interpolated => self.matrices.get(&entity).copied(),
        }
    }
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<TransformHistory>()?;
    app.init_resource::<RenderTransformProjection>()?;
    app.add_systems(
        StartupStage::Tooling,
        capture_previous.after(TransformSet::Propagate),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_previous
            .after(FixedUpdateSet::Prepare)
            .before(FixedUpdateSet::Simulate),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_current
            .after(TransformSet::Propagate)
            .before(FixedUpdateSet::Finalize),
    )?;
    app.configure_sets(
        CoreStage::Extract,
        TransformSet::Present.after(TransformSet::Propagate),
    )?;
    app.add_systems(
        CoreStage::Extract,
        derive_render_projection.in_set(TransformSet::Present),
    )?;
    Ok(())
}

fn capture_previous(world: &mut World) {
    capture(world, true);
}
fn capture_current(world: &mut World) {
    capture(world, false);
}

fn capture(world: &mut World, before_simulation: bool) {
    if !world.contains_resource::<FixedTransformInterpolation>() {
        return;
    }
    world.resource_scope(|world, mut history: Mut<TransformHistory>| {
        let TransformHistory { locals, samples } = &mut *history;
        samples.retain(|entity, _| world.get::<Transform2d>(*entity).is_some());
        for (entity, local) in locals.iter(world) {
            let parent = world.get::<Parent>(entity).map(Parent::parent);
            let pair = samples.entry(entity).or_insert(LocalSamples {
                previous: *local,
                current: *local,
                parent,
            });
            if before_simulation || pair.parent != parent {
                pair.previous = *local;
            }
            pair.current = *local;
            pair.parent = parent;
        }
    });
}

fn derive_render_projection(world: &mut World) -> Result<(), BevyError> {
    if !world.contains_resource::<FixedTransformInterpolation>() {
        world.resource_mut::<TransformHistory>().samples.clear();
        let mut projection = world.resource_mut::<RenderTransformProjection>();
        projection.state = ProjectionState::Current;
        projection.matrices.clear();
        return Ok(());
    }
    let paused = world.resource::<RuntimeTimeSettings>().paused();
    let alpha = if paused {
        1.0
    } else {
        world.resource::<RenderTime>().interpolation_alpha
    };
    world.resource_mut::<RenderTransformProjection>().state = ProjectionState::Incomplete;
    world.resource_scope(|world, mut history: Mut<TransformHistory>| {
        let TransformHistory { locals, samples } = &mut *history;
        samples.retain(|entity, _| world.get::<Transform2d>(*entity).is_some());
        for (entity, local) in locals.iter(world) {
            let parent = world.get::<Parent>(entity).map(Parent::parent);
            let pair = samples.entry(entity).or_insert(LocalSamples {
                previous: *local,
                current: *local,
                parent,
            });
            if paused || pair.current != *local || pair.parent != parent {
                *pair = LocalSamples {
                    previous: *local,
                    current: *local,
                    parent,
                };
            }
        }
        world.resource_scope(|world, mut projection: Mut<RenderTransformProjection>| {
            crate::propagation::project_render_matrices(
                world,
                &mut projection.matrices,
                |entity, _| {
                    let pair = samples[&entity];
                    interpolate(pair.previous, pair.current, alpha).matrix()
                },
            )
            .map_err(BevyError::error)?;
            projection.state = ProjectionState::Interpolated;
            Ok(())
        })
    })
}

fn interpolate(previous: Transform2d, current: Transform2d, alpha: f32) -> Transform2d {
    if alpha >= 1.0 {
        return current;
    }
    if alpha <= 0.0 {
        return previous;
    }
    let rotation = if previous.rotation == current.rotation {
        previous.rotation
    } else {
        // Reduce through the same trigonometric domain as the endpoint matrices. Taking the
        // remainder of a huge angle by an approximate TAU loses its actual orientation.
        let (previous_sin, previous_cos) = previous.rotation.sin_cos();
        let (current_sin, current_cos) = current.rotation.sin_cos();
        let previous_angle = f64::from(previous_sin).atan2(f64::from(previous_cos));
        let current_angle = f64::from(current_sin).atan2(f64::from(current_cos));
        let delta = (current_angle - previous_angle + std::f64::consts::PI)
            .rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        (previous_angle + delta * f64::from(alpha)) as f32
    };
    Transform2d {
        translation: previous.translation.lerp(current.translation, alpha),
        rotation,
        scale: previous.scale.lerp(current.scale, alpha),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_rotation_extremes_keep_interpolated_matrices_finite() {
        for (from, to) in [
            (-f32::MAX, f32::MAX),
            (f32::MAX, -f32::MAX),
            (f32::MAX, f32::MAX),
            (0.0, f32::MAX),
        ] {
            let previous = Transform2d {
                rotation: from,
                ..Transform2d::IDENTITY
            };
            let current = Transform2d {
                rotation: to,
                ..Transform2d::IDENTITY
            };
            assert!(previous.matrix().is_finite());
            assert!(current.matrix().is_finite());
            assert_eq!(interpolate(previous, current, 0.0), previous);
            assert_eq!(interpolate(previous, current, 1.0), current);
            for alpha in [0.25, 0.5, 0.75] {
                assert!(
                    interpolate(previous, current, alpha).matrix().is_finite(),
                    "from {from} to {to}, alpha {alpha}"
                );
            }
            let previous_direction = previous.matrix().transform_vector2(nara_core::Vec2::X);
            let current_direction = current.matrix().transform_vector2(nara_core::Vec2::X);
            let midpoint_direction = interpolate(previous, current, 0.5)
                .matrix()
                .transform_vector2(nara_core::Vec2::X);
            let bisector = (previous_direction + current_direction).normalize();
            assert!((midpoint_direction - bisector).length() < 1e-5);
        }
    }

    #[test]
    fn rotation_interpolation_preserves_shortest_arc_and_equal_endpoints() {
        let previous = Transform2d {
            rotation: 170_f32.to_radians(),
            ..Transform2d::IDENTITY
        };
        let current = Transform2d {
            rotation: -170_f32.to_radians(),
            ..Transform2d::IDENTITY
        };
        let midpoint = interpolate(previous, current, 0.5);
        let direction = midpoint.matrix().transform_vector2(nara_core::Vec2::X);
        assert!((direction - nara_core::Vec2::NEG_X).length() < 1e-5);
        assert_eq!(interpolate(previous, previous, 0.5), previous);
    }
}
