use nara_app::{
    App, CoreStage, FixedUpdateSet, PluginError, RenderTime, RuntimeGeneration,
    RuntimeTimeSettings, StartupStage,
};
use nara_ecs::{
    Entity, FromWorld, Mut, QueryState, Resource, World, bevy_ecs::entity::EntityHashMap,
    bevy_ecs::error::BevyError, bevy_ecs::schedule::ApplyDeferred, bevy_ecs::world::WorldId,
    schedule::IntoScheduleConfigs,
};

use crate::{
    __private::TransformSet,
    GlobalTransform2d, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
    historical_spatial_snapshot,
    snapshot::{PresentationSnapshot, SimulationSnapshot, SpatialSource, next_publication},
};

/// Opts this world's presentation into interpolation between previous and current fixed samples.
///
/// Insert this resource before startup. Paused frames, new entities, and edits outside the
/// captured fixed pair display the current captured transform immediately.
#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct FixedTransformInterpolation;

#[derive(Clone, Copy)]
struct LocalSamples {
    previous: Transform2d,
    current: Transform2d,
    parent: Option<Entity>,
    spatial_parent: Option<Entity>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HistoryOrigin {
    world: WorldId,
    generation: Option<RuntimeGeneration>,
}

impl HistoryOrigin {
    fn current(world: &World) -> Self {
        Self {
            world: world.id(),
            generation: world.get_resource::<RuntimeGeneration>().copied(),
        }
    }
}

#[derive(Resource)]
pub(super) struct TransformHistory {
    origin: HistoryOrigin,
    before_origin: Option<HistoryOrigin>,
    locals: QueryState<(
        Entity,
        &'static Transform2d,
        Option<&'static nara_hierarchy::Parent>,
    )>,
    samples: EntityHashMap<LocalSamples>,
    before: EntityHashMap<SpatialSource>,
    candidate: EntityHashMap<GlobalTransform2d>,
}

impl FromWorld for TransformHistory {
    fn from_world(world: &mut World) -> Self {
        Self {
            origin: HistoryOrigin::current(world),
            before_origin: None,
            locals: world.query(),
            samples: EntityHashMap::default(),
            before: EntityHashMap::default(),
            candidate: EntityHashMap::default(),
        }
    }
}

impl TransformHistory {
    fn bind(&mut self, world: &mut World) {
        let origin = HistoryOrigin::current(world);
        if self.origin != origin {
            self.locals = world.query();
            self.samples.clear();
            self.before.clear();
            self.before_origin = None;
            self.origin = origin;
        }
    }
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    // The transform plugin owns installation; an optional renderer may request it again.
    if app.world().contains_resource::<TransformHistory>() {
        return Ok(());
    }
    app.init_resource::<TransformHistory>()?;
    app.add_systems(
        StartupStage::Tooling,
        capture_current.after(TransformSet::Propagate),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        (ApplyDeferred, capture_previous)
            .chain()
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

pub(super) fn capture_previous(world: &mut World) {
    if !world.contains_resource::<FixedTransformInterpolation>() {
        return;
    }
    world.resource_scope(|world, mut history: Mut<TransformHistory>| {
        history.bind(world);
        history.before.clear();
        history.before_origin = None;
        if historical_spatial_snapshot(world).is_err() {
            history.samples.clear();
            return;
        }
        // Frame/Prepare edits are the fixed step's starting point, not motion from an older cut.
        let TransformHistory { locals, before, .. } = &mut *history;
        before.extend(locals.iter(world).map(|(entity, local, parent)| {
            (
                entity,
                SpatialSource {
                    local: *local,
                    parent: parent.map(nara_hierarchy::Parent::parent),
                },
            )
        }));
        history.before_origin = Some(history.origin);
    });
}

pub(super) fn capture_current(world: &mut World) {
    if !world.contains_resource::<FixedTransformInterpolation>() {
        return;
    }
    world.resource_scope(|world, mut history: Mut<TransformHistory>| {
        history.bind(world);
        if historical_spatial_snapshot(world).is_err() {
            history.samples.clear();
            history.before.clear();
            history.before_origin = None;
            return;
        }
        let slot = world.resource::<SimulationSnapshot>();
        history
            .samples
            .retain(|entity, _| slot.payload.sources.contains_key(entity));
        for (&entity, source) in &slot.payload.sources {
            let spatial_parent = source
                .parent
                .filter(|parent| slot.payload.sources.contains_key(parent));
            let previous = history
                .before
                .get(&entity)
                .filter(|before| {
                    history.before_origin == Some(history.origin)
                        && before.parent == source.parent
                        && before
                            .parent
                            .filter(|parent| history.before.contains_key(parent))
                            == spatial_parent
                })
                .map_or(source.local, |before| before.local);
            history.samples.insert(
                entity,
                LocalSamples {
                    previous,
                    current: source.local,
                    parent: source.parent,
                    spatial_parent,
                },
            );
        }
    });
}

pub(super) fn derive_render_projection(world: &mut World) -> Result<(), BevyError> {
    world.resource_mut::<PresentationSnapshot>().stamp = None;
    let source_stamp = historical_spatial_snapshot(world)
        .and_then(|snapshot| snapshot.stamp())
        .map_err(BevyError::error)?;
    let interpolate_enabled = world.contains_resource::<FixedTransformInterpolation>();
    let paused = world.resource::<RuntimeTimeSettings>().paused();
    let alpha = if paused {
        1.0
    } else {
        world.resource::<RenderTime>().interpolation_alpha
    };
    world.resource_scope(|world, mut history: Mut<TransformHistory>| {
        history.bind(world);
        let source = world.resource::<SimulationSnapshot>();
        let count = source.payload.sources.len();
        if !interpolate_enabled {
            history.samples.clear();
            history.before.clear();
        } else {
            history
                .samples
                .retain(|entity, _| source.payload.sources.contains_key(entity));
            for (&entity, captured) in &source.payload.sources {
                let spatial_parent = captured
                    .parent
                    .filter(|parent| source.payload.sources.contains_key(parent));
                let pair = history.samples.entry(entity).or_insert(LocalSamples {
                    previous: captured.local,
                    current: captured.local,
                    parent: captured.parent,
                    spatial_parent,
                });
                if paused
                    || pair.current != captured.local
                    || pair.parent != captured.parent
                    || pair.spatial_parent != spatial_parent
                {
                    *pair = LocalSamples {
                        previous: captured.local,
                        current: captured.local,
                        parent: captured.parent,
                        spatial_parent,
                    };
                }
            }
        }
        let interpolate_enabled = interpolate_enabled
            && !paused
            && source.payload.sources.iter().any(|(entity, source)| {
                history.samples.get(entity).is_some_and(|pair| {
                    pair.current == source.local
                        && pair.parent == source.parent
                        && pair.previous != pair.current
                })
            });
        if interpolate_enabled {
            history.candidate.clear();
            history.candidate.reserve(count);
            // Both local samples and parent edges are from one completed simulation cut.
            for &entity in &source.payload.spatial_order {
                let captured = source.payload.sources[&entity];
                let local = history
                    .samples
                    .get(&entity)
                    .filter(|pair| pair.current == captured.local && pair.parent == captured.parent)
                    .map_or(captured.local, |pair| {
                        interpolate(pair.previous, pair.current, alpha)
                    });
                let parent = captured
                    .parent
                    .and_then(|parent| history.candidate.get(&parent).copied());
                let matrix = parent
                    .map_or_else(|| local.matrix(), |parent| parent.matrix() * local.matrix());
                if !matrix.is_finite() {
                    return Err(BevyError::error(
                        crate::propagation::TransformCompletionError::NonFiniteGlobal { entity },
                    ));
                }
                history.candidate.insert(entity, GlobalTransform2d(matrix));
            }
        }
        let publication = next_publication().ok_or_else(|| {
            BevyError::error(crate::propagation::TransformCompletionError::PublicationExhausted)
        })?;
        let mut presentation = world.resource_mut::<PresentationSnapshot>();
        if interpolate_enabled {
            std::mem::swap(&mut presentation.globals, &mut history.candidate);
        }
        presentation.interpolated = interpolate_enabled;
        presentation.source_publication = source_stamp.publication;
        presentation.stamp = Some(SpatialSnapshotStamp {
            phase: SpatialSnapshotPhase::Presented,
            publication,
            ..source_stamp
        });
        Ok(())
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
