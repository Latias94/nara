//! Opt-in fixed-input, fixed-step, historical, and interpolated spatial captures.
//!
//! [`FixedSpatialPlugin`] installs the capture pipeline for consumers that need these exact cuts.
//! The native spatial components and propagation live in [`crate::spatial`].

mod presentation;
mod propagation;
mod snapshot;
#[cfg(test)]
mod snapshot_tests;

pub use crate::spatial::Transform2d;
pub use presentation::FixedTransformInterpolation;
pub(crate) use snapshot::invalidate_spatial_snapshots;
pub use snapshot::{
    FixedInputSpatialSnapshot2d, FixedSpatialSnapshot2d, HistoricalSpatialSnapshot2d,
    PresentedSpatialSnapshot2d, SpatialSnapshot2d, SpatialSnapshotError, SpatialSnapshotPhase,
    SpatialSnapshotStamp, fixed_input_spatial_snapshot, fixed_spatial_snapshot,
    historical_spatial_snapshot, presented_spatial_snapshot,
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_core::{Mat3, Vec2};
use nara_ecs::{SystemSet, bevy_ecs::system::ScheduleSystem, schedule::IntoScheduleConfigs};

/// The world-space affine transform recorded by a fixed or presented snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapturedTransform2d(Mat3);

impl CapturedTransform2d {
    /// Returns the completed world-space affine matrix.
    #[must_use]
    #[inline]
    pub const fn matrix(&self) -> Mat3 {
        self.0
    }

    /// Returns the completed world-space origin.
    #[must_use]
    #[inline]
    pub fn translation(&self) -> Vec2 {
        self.0.transform_point2(Vec2::ZERO)
    }
}

/// Registers systems that consume this tick's completed spatial input before Simulate.
///
/// Include [`FixedSpatialPlugin`] and its required plugins in the App composition.
/// The first registration installs one input publisher after Prepare's deferred commands and
/// the original hierarchy validator, before Simulate. Subsequent registrations share that
/// publisher. Applications without input consumers retain their existing schedule work.
///
/// [`FixedInputSpatialSnapshot2d`] and [`fixed_input_spatial_snapshot`] expose immutable captured
/// inputs independently of Historical, completed Fixed and Presented publications. Simulation
/// writeback is observed by the later completed Fixed publication. Consumers run after input
/// completion and before [`nara_app::FixedUpdateSet::Simulate`], including its deferred boundary.
/// Consumers declare any ordering they need among peers.
pub fn add_fixed_transform_input_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    propagation::install_fixed_input(app)?;
    app.add_systems(
        CoreStage::FixedUpdate,
        systems
            .after(FixedSpatialSet::PrepareInput)
            .before(nara_app::FixedUpdateSet::Simulate),
    )
}

/// Registers fixed-tick systems that consume completed runtime 2D global transforms.
///
/// The transform module owns the concrete propagation set and its placement between simulation
/// mutation and finalization. Product systems use this semantic entry point instead of depending
/// on that private schedule topology.
/// Finalize/Capture systems read [`FixedSpatialSnapshot2d`] directly without using this helper.
pub fn add_fixed_transform_projection_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(
        CoreStage::FixedUpdate,
        systems
            .after(FixedSpatialSet::Propagate)
            .before(nara_app::FixedUpdateSet::Finalize),
    )
}

/// Registers Extract readers after this frame's presentation snapshot is complete.
pub fn add_presented_transform_projection_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(CoreStage::Extract, systems.after(FixedSpatialSet::Present))
}

/// Schedule sets owned by [`FixedSpatialPlugin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum FixedSpatialSet {
    PrepareInput,
    Propagate,
    Present,
}

/// Installs exact snapshot capture for systems that consume fixed or presented cuts.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedSpatialPlugin;

pub const FIXED_SPATIAL_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.scene.fixed-spatial");
pub const FIXED_SPATIAL_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(FIXED_SPATIAL_PLUGIN_ID, nara_app::PluginCategory::Core)
        .requires_plugins(&[crate::hierarchy::HIERARCHY_PLUGIN_ID]);

impl Plugin for FixedSpatialPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &FIXED_SPATIAL_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if app
            .world()
            .contains_resource::<snapshot::SimulationSnapshot>()
        {
            return Ok(());
        }
        propagation::install(app)?;
        presentation::install(app)
    }
}

pub mod prelude {
    pub use crate::fixed_spatial::{
        CapturedTransform2d, FixedInputSpatialSnapshot2d, FixedSpatialPlugin,
        FixedSpatialSnapshot2d, FixedTransformInterpolation, HistoricalSpatialSnapshot2d,
        PresentedSpatialSnapshot2d, SpatialSnapshot2d, SpatialSnapshotError, SpatialSnapshotPhase,
        SpatialSnapshotStamp, Transform2d, add_fixed_transform_input_systems,
        add_fixed_transform_projection_systems, add_presented_transform_projection_systems,
        fixed_input_spatial_snapshot, fixed_spatial_snapshot, historical_spatial_snapshot,
        presented_spatial_snapshot,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed_spatial::propagation::TransformCompletionError;
    use std::time::Duration;

    use crate::hierarchy::{
        ChildOf, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara_app::{
        CoreStage, FixedTime, FixedUpdateSet, RuntimeCandidate, RuntimeCandidateRetirementState,
        RuntimeClosePolicy, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance,
        RuntimeObligationLedger, RuntimeState,
    };
    use nara_ecs::{
        Commands, Entity, Query, Res, ResMut, Resource, World, schedule::IntoScheduleConfigs,
    };
    use nara_reflect::ComponentRegistryPlugin;

    trait SnapshotTestRead {
        fn snapshot_global(&self, entity: Entity) -> Option<CapturedTransform2d>;
        fn snapshot_available(&self) -> bool;
    }

    impl SnapshotTestRead for nara_ecs::World {
        fn snapshot_global(&self, entity: Entity) -> Option<CapturedTransform2d> {
            historical_spatial_snapshot(self).ok()?.global(entity).ok()
        }
        fn snapshot_available(&self) -> bool {
            historical_spatial_snapshot(self).is_ok()
        }
    }

    fn spatial_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            crate::hierarchy::HierarchyPlugin,
            FixedSpatialPlugin,
        ))
        .expect("the spatial foundation should install");
        app
    }

    enum ProjectionEdit {
        Scale { entity: Entity, scale: Vec2 },
        RemoveTransform(Entity),
        RetireParent { parent: Entity, child: Entity },
    }

    impl ProjectionEdit {
        fn apply(self, world: &mut World) -> Result<(), TransformCompletionError> {
            match self {
                Self::Scale { entity, scale } => {
                    world.get_mut::<Transform2d>(entity).unwrap().scale = scale;
                }
                Self::RemoveTransform(entity) => {
                    world.entity_mut(entity).remove::<Transform2d>();
                }
                Self::RetireParent { parent, child } => {
                    let child_from = HierarchyPlacement::current(world, child).unwrap();
                    let parent_from = HierarchyPlacement::current(world, parent).unwrap();
                    HierarchyWriter::new(world)
                        .apply_batch(&[
                            HierarchyOperation::Move {
                                entity: child,
                                from: child_from,
                                to: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                            },
                            HierarchyOperation::Remove {
                                entity: parent,
                                from: parent_from,
                            },
                        ])
                        .unwrap();
                    assert!(world.despawn(parent));
                    return Ok(());
                }
            }
            propagation::complete_transform_projection(world)
        }
    }

    fn apply_projection_edit(
        runtime: &mut RuntimeInstance,
        edit: ProjectionEdit,
    ) -> Result<(), TransformCompletionError> {
        let outcome = runtime
            .with_driver_scope(|scope| {
                scope.apply_command(move |world: &mut World| edit.apply(world))
            })
            .unwrap();
        assert!(outcome.post_operation_fault.is_none());
        outcome.output
    }

    fn place_ordered_hierarchy(
        world: &mut nara_ecs::World,
        roots: &[Entity],
        edges: &[(Entity, Entity)],
    ) {
        let mut operations = Vec::with_capacity(roots.len() + edges.len());
        operations.extend(
            roots
                .iter()
                .copied()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                }),
        );
        operations.extend(edges.iter().copied().map(|(entity, parent)| {
            HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            }
        }));
        HierarchyWriter::new(world)
            .apply_batch(&operations)
            .expect("the ordered hierarchy should be valid");
    }

    #[test]
    fn opt_in_snapshot_captures_native_relationships_outside_ordered_scene_roots() {
        let mut app = spatial_app();
        app.add_plugin(crate::spatial::SpatialPlugin).unwrap();
        let world = app.world_mut().unwrap();
        let parent = world.spawn(Transform2d::from_xy(10.0, 0.0)).id();
        let child = world
            .spawn((Transform2d::from_xy(2.0, 0.0), ChildOf(parent)))
            .id();
        app.step_fixed(Duration::from_millis(16)).unwrap();
        let world = app.world();
        let snapshot = fixed_spatial_snapshot(world).unwrap();
        assert_eq!(snapshot.parent(child).unwrap(), Some(parent));
        assert_eq!(snapshot.global(child).unwrap().translation().x, 12.0);
        assert_eq!(
            snapshot.structural_index(child),
            Err(SpatialSnapshotError::MissingStructuralEntity { entity: child })
        );
        assert_eq!(
            world
                .get::<crate::spatial::GlobalTransform2d>(child)
                .unwrap()
                .translation()
                .x,
            12.0
        );
    }

    #[test]
    fn global_transform_is_an_ordinary_copyable_affine_value() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<CapturedTransform2d>();
    }

    #[test]
    fn startup_propagates_root_child_and_grandchild_globals() {
        let mut app = spatial_app();

        let (root, child, grandchild) = {
            let world = app
                .world_mut()
                .expect("the configuring App should expose its World");
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(10.0, 20.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            let grandchild = world
                .spawn(Transform2d::from_translation(Vec2::new(-2.0, 5.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root), (grandchild, child)]);
            (root, child, grandchild)
        };

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime
            .drive(Duration::ZERO)
            .expect("startup completion should succeed");

        let world = runtime.world();
        assert_eq!(
            world
                .snapshot_global(root)
                .expect("the root should receive derived global state")
                .translation(),
            Vec2::new(10.0, 20.0)
        );
        assert_eq!(
            world
                .snapshot_global(child)
                .expect("the child should receive derived global state")
                .translation(),
            Vec2::new(13.0, 24.0)
        );
        assert_eq!(
            world
                .snapshot_global(grandchild)
                .expect("the grandchild should receive derived global state")
                .translation(),
            Vec2::new(11.0, 29.0)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn failed_completion_withholds_the_snapshot_and_preserves_previous_payload() {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world
                .spawn(Transform2d::from_translation(Vec2::new(5.0, 7.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(2.0, 3.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root)]);
            (root, child)
        };
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let previous = runtime.world().snapshot_global(child).unwrap().matrix();
        assert!(runtime.world().snapshot_available());

        let error = apply_projection_edit(
            &mut runtime,
            ProjectionEdit::Scale {
                entity: root,
                scale: Vec2::new(f32::NAN, 1.0),
            },
        )
        .expect_err("non-finite local state must reject the complete candidate");
        assert_eq!(
            error,
            TransformCompletionError::NonFiniteLocal { entity: root }
        );
        assert!(!runtime.world().snapshot_available());
        assert_eq!(
            historical_spatial_snapshot(runtime.world()).err(),
            Some(SpatialSnapshotError::Unavailable),
        );
        assert_eq!(
            runtime
                .world()
                .resource::<snapshot::SimulationSnapshot>()
                .payload
                .globals[&child]
                .matrix(),
            previous
        );

        apply_projection_edit(
            &mut runtime,
            ProjectionEdit::Scale {
                entity: root,
                scale: Vec2::ONE,
            },
        )
        .expect("a corrected generation should republish the projection");

        assert!(runtime.world().snapshot_available());
        runtime.finish().unwrap();
    }

    #[test]
    fn failed_hierarchy_completion_invalidates_the_spatial_publication() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        let corrupt_child = app.world_mut().unwrap().spawn(Transform2d::default()).id();
        app.insert_resource(CorruptHierarchyBeforeCompletion {
            root,
            child: corrupt_child,
        })
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            corrupt_hierarchy_before_completion
                .before(crate::hierarchy::HierarchySet::ValidateAndComplete),
        )
        .unwrap();

        let candidate = RuntimeCandidate::admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        let previous = runtime.world().snapshot_global(child).unwrap().matrix();
        assert!(runtime.world().snapshot_available());

        runtime
            .drive(Duration::ZERO)
            .expect_err("an inconsistent dirty hierarchy must fault the frame");

        assert!(
            !runtime
                .world()
                .contains_resource::<crate::hierarchy::CompletedHierarchyProjection>(),
            "a dirty hierarchy must revoke its completion fact before validation"
        );
        assert!(historical_spatial_snapshot(runtime.world()).is_err());
        assert_eq!(
            runtime
                .world()
                .resource::<snapshot::SimulationSnapshot>()
                .payload
                .globals[&child]
                .matrix(),
            previous
        );
        assert!(
            !runtime.world().snapshot_available(),
            "the transform completion point must withhold a failed publication"
        );

        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
    }

    #[derive(Resource)]
    struct CorruptHierarchyBeforeCompletion {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
    }

    fn corrupt_hierarchy_before_completion(world: &mut nara_ecs::World) {
        let corruption = world.resource::<CorruptHierarchyBeforeCompletion>();
        let root = corruption.root;
        let child = corruption.child;
        world.entity_mut(child).insert_with_relationship_hook_mode(
            ChildOf(root),
            nara_ecs::bevy_ecs::relationship::RelationshipHookMode::Skip,
        );
    }

    #[test]
    fn removing_transform_removes_stale_global_projection() {
        let mut app = spatial_app();
        let entity = app
            .world_mut()
            .unwrap()
            .spawn(Transform2d::from_translation(Vec2::new(4.0, 8.0)))
            .id();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().snapshot_global(entity).is_some());

        apply_projection_edit(&mut runtime, ProjectionEdit::RemoveTransform(entity)).unwrap();

        assert!(runtime.world().snapshot_global(entity).is_none());
        assert!(runtime.world().snapshot_available());
        runtime.finish().unwrap();
    }

    #[test]
    fn parent_transform_removal_starts_a_new_spatial_domain() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        apply_projection_edit(&mut runtime, ProjectionEdit::RemoveTransform(root)).unwrap();
        let snapshot = historical_spatial_snapshot(runtime.world()).unwrap();
        assert_eq!(
            snapshot.global(child).unwrap().translation(),
            Vec2::new(3.0, 4.0)
        );
        assert_eq!(snapshot.effective_parent(child).unwrap(), None);
        assert_eq!(
            snapshot.global(root),
            Err(SpatialSnapshotError::MissingEntity { entity: root })
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn unchanged_completion_points_do_not_revisit_the_transform_forest() {
        let mut app = spatial_app();
        let world = app.world_mut().unwrap();
        let root = world.spawn(Transform2d::default()).id();
        let child = world.spawn(Transform2d::default()).id();
        place_ordered_hierarchy(world, &[root], &[(child, root)]);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let first = propagation::propagation_stats(runtime.world());
        assert_eq!((first.2, first.3, first.4), (1, 2, 1));

        runtime.drive(Duration::ZERO).unwrap();
        let second = propagation::propagation_stats(runtime.world());
        assert_eq!(
            (second.2, second.3, second.4),
            (1, 2, 1),
            "PostUpdate and Extract freshness fences must use the shared completion generation"
        );
        assert_eq!(
            (second.0 - first.0, second.1 - first.1),
            (2, 4),
            "each unchanged PostUpdate/Extract publication should perform one participant value scan only"
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn deep_and_wide_forests_complete_iteratively_in_one_linear_pass() {
        const DEPTH: usize = 2_048;
        const WIDTH: usize = 2_048;

        let mut app = spatial_app();
        let (deep_leaf, participant_count) = {
            let world = app.world_mut().unwrap();
            let deep_root = world.spawn(Transform2d::from_translation(Vec2::X)).id();
            let mut previous = deep_root;
            let mut edges = Vec::with_capacity(DEPTH.saturating_sub(1) + WIDTH);
            for _ in 1..DEPTH {
                let entity = world.spawn(Transform2d::from_translation(Vec2::X)).id();
                edges.push((entity, previous));
                previous = entity;
            }

            let wide_root = world.spawn(Transform2d::default()).id();
            for _ in 0..WIDTH {
                let child = world.spawn(Transform2d::default()).id();
                edges.push((child, wide_root));
            }
            place_ordered_hierarchy(world, &[deep_root, wide_root], &edges);
            (previous, DEPTH + WIDTH + 1)
        };

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .snapshot_global(deep_leaf)
                .unwrap()
                .translation(),
            Vec2::new(DEPTH as f32, 0.0)
        );
        assert_eq!(
            {
                let stats = propagation::propagation_stats(runtime.world());
                (stats.2, stats.3, stats.4)
            },
            (
                1,
                participant_count as u64,
                (DEPTH.saturating_sub(1) + WIDTH) as u64
            )
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn nonspatial_order_entries_do_not_become_transform_participants() {
        const NON_TRANSFORM_CHILDREN: usize = 4_096;

        let mut app = spatial_app();
        {
            let world = app.world_mut().unwrap();
            let root = world.spawn(Transform2d::default()).id();
            let mut edges = Vec::with_capacity(NON_TRANSFORM_CHILDREN);
            for _ in 0..NON_TRANSFORM_CHILDREN {
                let child = world.spawn_empty().id();
                edges.push((child, root));
            }
            place_ordered_hierarchy(world, &[root], &edges);
        }

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let stats = propagation::propagation_stats(runtime.world());
        assert_eq!(
            (stats.2, stats.3, stats.4),
            (1, 1, 0),
            "nonspatial structural entries must not add affine evaluations or spatial edges"
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn fixed_direct_transform_write_is_visible_to_same_tick_consumer() {
        let app = fixed_spatial_app(false);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            runtime.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn fixed_deferred_transform_write_is_visible_to_same_tick_consumer() {
        let app = fixed_spatial_app(true);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            runtime.world().resource::<FixedSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn explicit_parent_retirement_recomputes_the_surviving_child_as_a_root() {
        let mut app = spatial_app();
        let (parent, child) = {
            let world = app.world_mut().unwrap();
            let parent = world
                .spawn(Transform2d::from_translation(Vec2::new(10.0, 20.0)))
                .id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            place_ordered_hierarchy(world, &[parent], &[(child, parent)]);
            (parent, child)
        };
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(13.0, 24.0)
        );

        apply_projection_edit(&mut runtime, ProjectionEdit::RetireParent { parent, child })
            .unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert!(
            runtime
                .world()
                .get::<crate::hierarchy::ChildOf>(child)
                .is_none()
        );
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(3.0, 4.0)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn post_update_transform_write_is_visible_before_extract() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        app.insert_resource(PostUpdateSpatialProbe {
            root,
            child,
            observed: None,
        })
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            move_post_update_root.before(FixedSpatialSet::Propagate),
        )
        .unwrap();
        app.add_systems(
            CoreStage::PostUpdate,
            observe_post_update_child.after(FixedSpatialSet::Propagate),
        )
        .unwrap();

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(
            runtime
                .world()
                .resource::<PostUpdateSpatialProbe>()
                .observed,
            Some(Vec2::new(13.0, 24.0))
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn paused_authoring_write_refreshes_extract_and_resume_has_no_stale_frame() {
        let mut app = spatial_app();
        let (root, child) = spawn_parented_pair(&mut app);
        app.insert_resource(PendingPausedTransformEdit {
            root,
            pending_translation: None,
        })
        .unwrap();
        app.insert_resource(ExtractSpatialProbe {
            child,
            observed: None,
            samples: 0,
        })
        .unwrap();
        app.add_systems(CoreStage::First, apply_paused_transform_edit)
            .unwrap();
        app.add_systems(
            CoreStage::Extract,
            observe_extract_child.after(FixedSpatialSet::Present),
        )
        .unwrap();

        let candidate = RuntimeCandidate::admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        assert_eq!(
            runtime
                .world()
                .snapshot_global(child)
                .unwrap()
                .translation(),
            Vec2::new(3.0, 4.0),
            "startup must publish the first projection before any frame consumer"
        );

        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);

        let outcome = runtime
            .with_driver_scope(|scope| {
                scope.apply_command(|world: &mut World| {
                    world
                        .resource_mut::<PendingPausedTransformEdit>()
                        .pending_translation = Some(Vec2::new(10.0, 20.0));
                });
            })
            .unwrap();
        assert!(outcome.post_operation_fault.is_none());
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);
        assert_eq!(
            runtime.world().resource::<ExtractSpatialProbe>().observed,
            Some(Vec2::new(13.0, 24.0)),
            "the next paused Extract must see the authoring write"
        );

        let paused_samples = runtime.world().resource::<ExtractSpatialProbe>().samples;
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Running);
        let probe = runtime.world().resource::<ExtractSpatialProbe>();
        assert_eq!(probe.observed, Some(Vec2::new(13.0, 24.0)));
        assert_eq!(probe.samples, paused_samples + 1);

        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
    }

    fn spawn_parented_pair(app: &mut App) -> (nara_ecs::Entity, nara_ecs::Entity) {
        let world = app.world_mut().unwrap();
        let root = world.spawn(Transform2d::default()).id();
        let child = world
            .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
            .id();
        place_ordered_hierarchy(world, &[root], &[(child, root)]);
        (root, child)
    }

    #[derive(Resource)]
    struct PostUpdateSpatialProbe {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
    }

    fn move_post_update_root(
        probe: Res<PostUpdateSpatialProbe>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        transforms.get_mut(probe.root).unwrap().translation = Vec2::new(10.0, 20.0);
    }

    fn observe_post_update_child(
        mut probe: ResMut<PostUpdateSpatialProbe>,
        globals: HistoricalSpatialSnapshot2d,
    ) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
    }

    #[derive(Resource)]
    struct PendingPausedTransformEdit {
        root: nara_ecs::Entity,
        pending_translation: Option<Vec2>,
    }

    fn apply_paused_transform_edit(
        mut edit: ResMut<PendingPausedTransformEdit>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        let Some(translation) = edit.pending_translation.take() else {
            return;
        };
        transforms.get_mut(edit.root).unwrap().translation = translation;
    }

    #[derive(Resource)]
    struct ExtractSpatialProbe {
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
        samples: u32,
    }

    fn observe_extract_child(
        mut probe: ResMut<ExtractSpatialProbe>,
        globals: PresentedSpatialSnapshot2d,
    ) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
        probe.samples = probe.samples.saturating_add(1);
    }

    #[derive(Resource)]
    struct FixedSpatialProbe {
        root: nara_ecs::Entity,
        child: nara_ecs::Entity,
        observed: Option<Vec2>,
    }

    fn fixed_spatial_app(deferred_write: bool) -> App {
        let mut app = spatial_app();
        let (root, child) = {
            let world = app.world_mut().unwrap();
            let root = world.spawn(Transform2d::default()).id();
            let child = world
                .spawn(Transform2d::from_translation(Vec2::new(3.0, 4.0)))
                .id();
            place_ordered_hierarchy(world, &[root], &[(child, root)]);
            (root, child)
        };
        app.insert_resource(FixedSpatialProbe {
            root,
            child,
            observed: None,
        })
        .unwrap();
        if deferred_write {
            app.add_systems(
                CoreStage::FixedUpdate,
                replace_root_transform_deferred.in_set(FixedUpdateSet::Simulate),
            )
            .unwrap();
        } else {
            app.add_systems(
                CoreStage::FixedUpdate,
                move_root_transform_direct.in_set(FixedUpdateSet::Simulate),
            )
            .unwrap();
        }
        add_fixed_transform_projection_systems(&mut app, observe_child_global).unwrap();
        app
    }

    fn move_root_transform_direct(
        probe: Res<FixedSpatialProbe>,
        mut transforms: Query<&mut Transform2d>,
    ) {
        transforms.get_mut(probe.root).unwrap().translation = Vec2::new(10.0, 20.0);
    }

    fn replace_root_transform_deferred(probe: Res<FixedSpatialProbe>, mut commands: Commands) {
        commands
            .entity(probe.root)
            .insert(Transform2d::from_translation(Vec2::new(10.0, 20.0)));
    }

    fn observe_child_global(mut probe: ResMut<FixedSpatialProbe>, globals: FixedSpatialSnapshot2d) {
        probe.observed = Some(globals.global(probe.child).unwrap().translation());
    }
}
