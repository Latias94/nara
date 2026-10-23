use std::time::Duration;

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, FixedTime, FixedUpdateSet, PluginError, RealTime,
    RuntimeControl, RuntimeControlRequestResult, RuntimeGeneration, RuntimeInstance,
};
use nara_core::Vec2;
use nara_ecs::{
    Entity, Query, Res, ResMut, Resource, World,
    schedule::{IntoScheduleConfigs, Schedule},
};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};

use crate::{
    FixedSpatialSnapshot2d, FixedTransformInterpolation, HistoricalSpatialSnapshot2d,
    PresentedSpatialSnapshot2d, SpatialSnapshotError, SpatialSnapshotPhase, Transform2d,
    TransformPlugin, add_fixed_transform_projection_systems,
    add_presented_transform_projection_systems, fixed_spatial_snapshot,
    historical_spatial_snapshot, presented_spatial_snapshot,
    propagation::{TransformCompletionError, complete_at},
    snapshot::SimulationSnapshot,
};

type WorldCheck = Box<dyn FnOnce(&mut World)>;

#[derive(Default, Resource)]
struct TestPort;

impl __RuntimeDriverPort for TestPort {
    type Input = WorldCheck;
    type Output = ();

    fn apply_driver_input(&mut self, _input: Self::Input) {
        unreachable!("the port uses the owned World callback")
    }
}

impl TestPort {
    fn apply(
        world: &mut World,
        check: WorldCheck,
    ) -> Result<(), (nara_app::RuntimeWorldAccessError, WorldCheck)> {
        check(world);
        Ok(())
    }
}

fn spatial_app() -> Result<App, PluginError> {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        TransformPlugin,
    ))
    .unwrap();
    app.init_resource::<TestPort>()?;
    app.__register_driver_port::<TestPort>(TestPort::apply)?;
    Ok(app)
}

fn check_world(runtime: &mut RuntimeInstance, check: impl FnOnce(&mut World) + 'static) {
    assert!(
        runtime
            .with_driver_scope(|scope| scope.__apply_port::<TestPort>(Box::new(check)))
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .is_ok()
    );
}

#[test]
fn source_value_comparison_observes_same_change_tick_and_reuses_unchanged_payload() {
    let mut app = spatial_app().unwrap();
    let entity = app.world_mut().unwrap().spawn(Transform2d::default()).id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        let original_tick = world.change_tick();
        let original_publication = historical_spatial_snapshot(world)
            .unwrap()
            .stamp()
            .unwrap()
            .publication;
        world.get_mut::<Transform2d>(entity).unwrap().translation = Vec2::X;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert_eq!(
            fixed_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation(),
            Vec2::X
        );
        world.get_mut::<Transform2d>(entity).unwrap().translation = Vec2::Y;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert_eq!(world.change_tick(), original_tick);
        assert_eq!(
            fixed_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation(),
            Vec2::Y
        );
        let slot = world.resource::<SimulationSnapshot>();
        let payload_address = slot.payload.order.as_ptr();
        let source_capacity = slot.payload.sources.capacity();
        let stats = crate::propagation::propagation_stats(world);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert_eq!(crate::propagation::propagation_stats(world).2, stats.2);
        assert_eq!(
            world
                .resource::<SimulationSnapshot>()
                .payload
                .order
                .as_ptr(),
            payload_address
        );
        assert_eq!(
            world
                .resource::<SimulationSnapshot>()
                .payload
                .sources
                .capacity(),
            source_capacity
        );
        assert!(
            fixed_spatial_snapshot(world)
                .unwrap()
                .stamp()
                .unwrap()
                .publication
                > original_publication
        );
    });
    runtime.finish().unwrap();
}

#[test]
fn consuming_a_cut_then_mutating_membership_keeps_the_cut_until_next_publication() {
    let mut app = spatial_app().unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d::from_translation(Vec2::X))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        let stamp = historical_spatial_snapshot(world).unwrap().stamp().unwrap();
        let stale_incarnation = Entity::from_index_and_generation(
            entity.index(),
            entity.generation().after_versions(1),
        );
        assert_eq!(
            historical_spatial_snapshot(world)
                .unwrap()
                .global(stale_incarnation),
            Err(SpatialSnapshotError::MissingEntity {
                entity: stale_incarnation
            })
        );
        assert!(world.despawn(entity));
        let replacement = world.spawn(Transform2d::from_translation(Vec2::Y)).id();
        assert_ne!(entity, replacement);
        let selected = historical_spatial_snapshot(world).unwrap();
        assert_eq!(selected.stamp().unwrap(), stamp);
        assert_eq!(selected.global(entity).unwrap().translation(), Vec2::X);
        assert_eq!(
            selected.global(replacement),
            Err(SpatialSnapshotError::MissingEntity {
                entity: replacement
            })
        );
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let next = fixed_spatial_snapshot(world).unwrap();
        assert_eq!(
            next.global(entity),
            Err(SpatialSnapshotError::MissingEntity { entity })
        );
        assert_eq!(next.global(replacement).unwrap().translation(), Vec2::Y);
    });
    runtime.finish().unwrap();
}

#[test]
fn readers_reject_missing_foreign_world_generation_phase_and_time() {
    fn assert_read_only<T: nara_ecs::upstream::system::ReadOnlySystemParam>() {}
    assert_read_only::<HistoricalSpatialSnapshot2d>();
    assert_read_only::<FixedSpatialSnapshot2d>();
    assert_read_only::<PresentedSpatialSnapshot2d>();
    let mut missing = World::new();
    assert_eq!(
        historical_spatial_snapshot(&missing).err(),
        Some(SpatialSnapshotError::Unavailable)
    );
    let mut schedule = Schedule::default();
    schedule.add_systems(
        |historical: HistoricalSpatialSnapshot2d,
         fixed: FixedSpatialSnapshot2d,
         presented: PresentedSpatialSnapshot2d| {
            assert_eq!(historical.stamp(), Err(SpatialSnapshotError::Unavailable));
            assert_eq!(fixed.stamp(), Err(SpatialSnapshotError::Unavailable));
            assert_eq!(presented.stamp(), Err(SpatialSnapshotError::Unavailable));
        },
    );
    schedule.run(&mut missing);

    let mut runtime = RuntimeInstance::start(spatial_app().unwrap()).unwrap();
    check_world(&mut runtime, |world| {
        assert!(matches!(
            fixed_spatial_snapshot(world),
            Err(SpatialSnapshotError::WrongPhase { .. })
        ));
        let slot = world.remove_resource::<SimulationSnapshot>().unwrap();
        let mut foreign = World::new();
        foreign.insert_resource(slot);
        assert_eq!(
            historical_spatial_snapshot(&foreign).err(),
            Some(SpatialSnapshotError::WrongWorld)
        );
        world.insert_resource(foreign.remove_resource::<SimulationSnapshot>().unwrap());

        let generation = world.remove_resource::<RuntimeGeneration>().unwrap();
        assert_eq!(
            historical_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::WrongRuntimeGeneration)
        );
        world.insert_resource(generation);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let stamp = fixed_spatial_snapshot(world).unwrap().stamp().unwrap();
        world.resource_mut::<RealTime>().frame += 1;
        assert_eq!(
            fixed_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::WrongTime)
        );
        assert_eq!(
            historical_spatial_snapshot(world).unwrap().stamp().unwrap(),
            stamp
        );
        world.resource_mut::<RealTime>().frame -= 1;
        let clock = world.remove_resource::<FixedTime>().unwrap();
        assert_eq!(
            fixed_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::MissingClock)
        );
        world.insert_resource(clock);
    });
    runtime.finish().unwrap();
}

#[test]
fn overflow_rejects_the_whole_publication_and_does_not_serve_old_as_current() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let root = world.spawn(Transform2d::default()).id();
    let child = world.spawn(Transform2d::default()).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    check_world(&mut runtime, move |world| {
        let previous = historical_spatial_snapshot(world)
            .unwrap()
            .global(child)
            .unwrap();
        world.get_mut::<Transform2d>(root).unwrap().scale = Vec2::splat(f32::MAX);
        world.get_mut::<Transform2d>(child).unwrap().scale = Vec2::splat(2.0);
        assert_eq!(
            complete_at(world, SpatialSnapshotPhase::Fixed),
            Err(TransformCompletionError::NonFiniteGlobal { entity: child })
        );
        assert_eq!(
            historical_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::Unavailable)
        );
        assert_eq!(
            presented_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::Unavailable)
        );
        assert_eq!(
            world.resource::<SimulationSnapshot>().payload.globals[&child],
            previous
        );
        world.get_mut::<Transform2d>(root).unwrap().scale = Vec2::ONE;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert!(fixed_spatial_snapshot(world).is_ok());
    });
    runtime.finish().unwrap();
}

#[test]
fn nonspatial_direct_parent_terminates_inheritance_and_order_is_captured() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let ancestor = world
        .spawn(Transform2d::from_translation(Vec2::splat(10.0)))
        .id();
    let group = world.spawn_empty().id();
    let child = world.spawn(Transform2d::from_translation(Vec2::X)).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: ancestor,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: group,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: ancestor,
                }),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: group,
                }),
            },
        ])
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        let cut = historical_spatial_snapshot(world).unwrap();
        assert_eq!(cut.order().unwrap(), &[ancestor, group, child]);
        assert_eq!(cut.global(child).unwrap().translation(), Vec2::X);
        assert_eq!(cut.effective_parent(child).unwrap(), None);
        world.entity_mut(group).insert(Transform2d::default());
        assert_eq!(
            historical_spatial_snapshot(world)
                .unwrap()
                .effective_parent(child)
                .unwrap(),
            None
        );
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let next = fixed_spatial_snapshot(world).unwrap();
        assert_eq!(
            next.effective_parent(child).unwrap().unwrap().translation(),
            Vec2::splat(10.0)
        );
        assert_eq!(
            next.global(child).unwrap().translation(),
            Vec2::new(11.0, 10.0)
        );
    });
    runtime.finish().unwrap();
}

#[derive(Resource)]
struct PhaseProbe {
    entity: Entity,
    historical_ticks: Vec<u64>,
    fixed_ticks: Vec<u64>,
    captured_ticks: Vec<u64>,
    positions: Vec<Vec2>,
}

fn move_using_history(
    spatial: HistoricalSpatialSnapshot2d,
    mut locals: Query<&mut Transform2d>,
    mut probe: ResMut<PhaseProbe>,
) {
    let old = spatial.global(probe.entity).unwrap().translation();
    probe.historical_ticks.push(spatial.stamp().unwrap().tick);
    locals.get_mut(probe.entity).unwrap().translation = old + Vec2::X;
}

fn observe_fixed(
    spatial: FixedSpatialSnapshot2d,
    mut probe: ResMut<PhaseProbe>,
    fixed: Res<FixedTime>,
) {
    assert_eq!(
        spatial.global(probe.entity).unwrap().translation().x,
        fixed.tick() as f32
    );
    probe.fixed_ticks.push(spatial.stamp().unwrap().tick);
}

fn capture_fixed(world: &mut World) {
    let stamp = fixed_spatial_snapshot(world).unwrap().stamp().unwrap();
    world
        .resource_mut::<PhaseProbe>()
        .captured_ticks
        .push(stamp.tick);
}

fn observe_presented(spatial: PresentedSpatialSnapshot2d, mut probe: ResMut<PhaseProbe>) {
    let position = spatial.global(probe.entity).unwrap().translation();
    probe.positions.push(position);
}

#[test]
fn historical_mutation_fixed_resolve_finalize_capture_and_multistep_presentation_share_cuts() {
    let mut app = spatial_app().unwrap();
    let entity = app.world_mut().unwrap().spawn(Transform2d::default()).id();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    app.insert_resource(PhaseProbe {
        entity,
        historical_ticks: Vec::new(),
        fixed_ticks: Vec::new(),
        captured_ticks: Vec::new(),
        positions: Vec::new(),
    })
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        move_using_history.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    add_fixed_transform_projection_systems(&mut app, observe_fixed).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_fixed.in_set(FixedUpdateSet::Finalize),
    )
    .unwrap();
    add_presented_transform_projection_systems(&mut app, observe_presented).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP * 2 + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    let probe = runtime.world().resource::<PhaseProbe>();
    assert_eq!(probe.historical_ticks, [0, 1]);
    assert_eq!(probe.fixed_ticks, [1, 2]);
    assert_eq!(probe.captured_ticks, [1, 2]);
    assert!((probe.positions[0].x - 1.5).abs() < 1e-5);
    runtime.drive(Duration::ZERO).unwrap();
    let probe = runtime.world().resource::<PhaseProbe>();
    assert_eq!(probe.fixed_ticks.len(), 2);
    assert!((probe.positions[1].x - 1.5).abs() < 1e-5);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<PhaseProbe>().positions.last(),
        Some(&Vec2::new(2.0, 0.0))
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<PhaseProbe>().captured_ticks,
        [1, 2, 3]
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<PhaseProbe>().positions.last(),
        Some(&Vec2::new(3.0, 0.0))
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<PhaseProbe>().positions.last(),
        Some(&Vec2::new(3.0, 0.0))
    );
    runtime.finish().unwrap();
}

#[derive(Resource)]
struct LateEdit {
    parent: Entity,
    child: Entity,
    edited: bool,
    positions: Vec<Vec2>,
}

fn edit_after_capture(mut edit: ResMut<LateEdit>, mut locals: Query<&mut Transform2d>) {
    if !edit.edited {
        locals.get_mut(edit.parent).unwrap().translation = Vec2::splat(100.0);
        locals.get_mut(edit.child).unwrap().translation = Vec2::splat(200.0);
        edit.edited = true;
    }
}

fn observe_after_late_edit(spatial: PresentedSpatialSnapshot2d, mut edit: ResMut<LateEdit>) {
    let position = spatial.global(edit.child).unwrap().translation();
    edit.positions.push(position);
}

#[test]
fn presentation_uses_one_captured_cut_despite_later_local_edits() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::from_translation(Vec2::X)).id();
    let child = world.spawn(Transform2d::from_translation(Vec2::Y)).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: parent,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            },
        ])
        .unwrap();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    app.insert_resource(LateEdit {
        parent,
        child,
        edited: false,
        positions: Vec::new(),
    })
    .unwrap();
    app.add_systems(
        CoreStage::Extract,
        edit_after_capture
            .after(crate::__private::TransformSet::Propagate)
            .before(crate::__private::TransformSet::Present),
    )
    .unwrap();
    add_presented_transform_projection_systems(&mut app, observe_after_late_edit).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<LateEdit>().positions,
        [Vec2::ONE]
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<LateEdit>().positions,
        [Vec2::ONE, Vec2::splat(300.0)]
    );
    runtime.finish().unwrap();
}

#[test]
fn structural_edits_do_not_change_the_selected_order_before_the_next_cut() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let root = world.spawn(Transform2d::default()).id();
    let first = world.spawn(Transform2d::from_translation(Vec2::X)).id();
    let second = world.spawn(Transform2d::from_translation(Vec2::Y)).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: first,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
            HierarchyOperation::Place {
                entity: second,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        let prior = historical_spatial_snapshot(world).unwrap().stamp().unwrap();
        let from = HierarchyPlacement::current(world, second).unwrap();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Move {
                entity: second,
                from,
                to: HierarchyPlacement::first(HierarchyPlacementOwner::Parent { parent: root }),
            }])
            .unwrap();
        let cut = historical_spatial_snapshot(world).unwrap();
        assert_eq!(cut.stamp().unwrap(), prior);
        assert_eq!(cut.order().unwrap(), &[root, first, second]);
        assert_eq!(cut.structural_index(first).unwrap(), 1);
        assert_eq!(cut.structural_index(second).unwrap(), 2);
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        historical_spatial_snapshot(runtime.world())
            .unwrap()
            .order()
            .unwrap(),
        &[root, second, first]
    );
    assert_eq!(
        historical_spatial_snapshot(runtime.world())
            .unwrap()
            .structural_index(second)
            .unwrap(),
        1
    );
    runtime.finish().unwrap();
}

#[test]
fn structural_index_distinguishes_unplaced_spatial_and_placed_nonspatial_entities() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let unplaced = world.spawn(Transform2d::default()).id();
    let group = world.spawn_empty().id();
    HierarchyWriter::new(world)
        .place(
            group,
            HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        )
        .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    let cut = historical_spatial_snapshot(runtime.world()).unwrap();
    assert!(cut.global(unplaced).is_ok());
    assert_eq!(
        cut.structural_index(unplaced),
        Err(SpatialSnapshotError::MissingStructuralEntity { entity: unplaced })
    );
    assert_eq!(cut.structural_index(group), Ok(0));
    assert_eq!(
        cut.global(group),
        Err(SpatialSnapshotError::MissingEntity { entity: group })
    );
    runtime.finish().unwrap();
}

#[test]
fn first_stage_edit_is_the_fixed_starting_sample_not_interpolated_motion() {
    let mut app = spatial_app().unwrap();
    let entity = app.world_mut().unwrap().spawn(Transform2d::default()).id();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    app.add_systems(
        CoreStage::First,
        move |mut locals: Query<&mut Transform2d>| {
            locals.get_mut(entity).unwrap().translation.x = 100.0;
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    assert_eq!(
        presented_spatial_snapshot(runtime.world())
            .unwrap()
            .global(entity)
            .unwrap()
            .translation()
            .x,
        100.0
    );
    runtime.finish().unwrap();
}

#[test]
fn prepare_stage_parent_edit_is_the_fixed_starting_frame() {
    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let root = world.spawn(Transform2d::default()).id();
    let child = world.spawn(Transform2d::from_translation(Vec2::X)).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |mut commands: nara_ecs::Commands| {
            commands
                .entity(root)
                .insert(Transform2d::from_translation(Vec2::new(100.0, 0.0)));
        })
        .in_set(FixedUpdateSet::Prepare),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    assert_eq!(
        presented_spatial_snapshot(runtime.world())
            .unwrap()
            .global(child)
            .unwrap()
            .translation()
            .x,
        101.0
    );
    runtime.finish().unwrap();
}

#[test]
fn foreign_simulation_or_history_cannot_be_laundered_into_a_local_presentation() {
    let mut app = spatial_app().unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d::from_translation(Vec2::new(100.0, 0.0)))
        .id();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        use crate::presentation::{
            TransformHistory, capture_current, capture_previous, derive_render_projection,
        };
        let mut foreign = World::new();
        let foreign_entity = foreign
            .spawn_at(entity, Transform2d::default())
            .unwrap()
            .id();
        assert_eq!(entity, foreign_entity);
        let mut slot = world.remove_resource::<SimulationSnapshot>().unwrap();
        slot.stamp.as_mut().unwrap().world = foreign.id();
        slot.payload
            .sources
            .get_mut(&entity)
            .unwrap()
            .local
            .translation = Vec2::ZERO;
        world.insert_resource(slot);
        assert!(historical_spatial_snapshot(world).is_err());
        capture_previous(world);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        capture_current(world);
        world
            .resource_mut::<nara_app::RenderTime>()
            .interpolation_alpha = 0.5;
        derive_render_projection(world).unwrap();
        assert_eq!(
            presented_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation()
                .x,
            100.0
        );

        let history = world.remove_resource::<TransformHistory>().unwrap();
        foreign.insert_resource(history);
        foreign.insert_resource(FixedTransformInterpolation);
        let mut foreign_slot = world.remove_resource::<SimulationSnapshot>().unwrap();
        foreign_slot.stamp.as_mut().unwrap().world = foreign.id();
        foreign_slot.stamp.as_mut().unwrap().runtime_generation = None;
        foreign.insert_resource(foreign_slot);
        capture_previous(&mut foreign);
        world.insert_resource(foreign.remove_resource::<TransformHistory>().unwrap());
        world.insert_resource(foreign.remove_resource::<SimulationSnapshot>().unwrap());
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        capture_current(world);
        derive_render_projection(world).unwrap();
        assert_eq!(
            presented_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation()
                .x,
            100.0
        );
    });
    runtime.finish().unwrap();
}

#[test]
fn runtime_generation_change_discards_uncommitted_history_samples() {
    let mut app = spatial_app().unwrap();
    let entity = app.world_mut().unwrap().spawn(Transform2d::default()).id();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        use crate::presentation::{capture_current, capture_previous, derive_render_projection};
        capture_previous(world);
        let generation = world.remove_resource::<RuntimeGeneration>().unwrap();
        world.get_mut::<Transform2d>(entity).unwrap().translation.x = 100.0;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        capture_current(world);
        world
            .resource_mut::<nara_app::RenderTime>()
            .interpolation_alpha = 0.5;
        derive_render_projection(world).unwrap();
        assert_eq!(
            presented_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation()
                .x,
            100.0
        );
        world.insert_resource(generation);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
    });
    runtime.finish().unwrap();
}

#[test]
fn payload_reuse_requires_the_exact_hierarchy_publication() {
    let mut app = spatial_app().unwrap();
    app.world_mut().unwrap().spawn(Transform2d::default());
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, |world| {
        let old_stamp = historical_spatial_snapshot(world).unwrap().stamp().unwrap();
        let before = crate::propagation::propagation_stats(world).2;
        // The generation remains equal; missing exact association cannot authorize reuse.
        world
            .resource_mut::<SimulationSnapshot>()
            .hierarchy_publication = None;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert_eq!(
            historical_spatial_snapshot(world)
                .unwrap()
                .stamp()
                .unwrap()
                .hierarchy_generation,
            old_stamp.hierarchy_generation
        );
        assert_eq!(crate::propagation::propagation_stats(world).2, before + 1);
    });
    runtime.finish().unwrap();
}

#[test]
fn sequential_source_comparison_tracks_archetype_moves_without_change_tick_advances() {
    #[derive(nara_ecs::Component)]
    struct Extra;

    let mut app = spatial_app().unwrap();
    let world = app.world_mut().unwrap();
    let first = world.spawn(Transform2d::from_translation(Vec2::X)).id();
    let second = world.spawn(Transform2d::from_translation(Vec2::Y)).id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        let tick = world.change_tick();
        world.entity_mut(first).insert(Extra);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let cut = fixed_spatial_snapshot(world).unwrap();
        assert_eq!(cut.global(first).unwrap().translation(), Vec2::X);
        assert_eq!(cut.global(second).unwrap().translation(), Vec2::Y);
        world.entity_mut(first).remove::<Extra>();
        world.get_mut::<Transform2d>(second).unwrap().translation = Vec2::splat(7.0);
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        assert_eq!(world.change_tick(), tick);
        assert_eq!(
            fixed_spatial_snapshot(world)
                .unwrap()
                .global(second)
                .unwrap()
                .translation(),
            Vec2::splat(7.0)
        );
    });
    runtime.finish().unwrap();
}

#[test]
fn restoring_same_world_simulation_slots_cannot_collide_or_join_another_payload() {
    let mut app = spatial_app().unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d::from_translation(Vec2::X))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    check_world(&mut runtime, move |world| {
        use crate::presentation::derive_render_projection;
        use crate::snapshot::PresentationSnapshot;

        // B's first publication, then A's second: every stamp is minted by the same owner
        // at the same World/generation/phase/tick/frame, without modifying private fields.
        world.insert_resource(SimulationSnapshot::default());
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let slot_b = world.remove_resource::<SimulationSnapshot>().unwrap();
        world.insert_resource(SimulationSnapshot::default());
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let stamp_a = fixed_spatial_snapshot(world).unwrap().stamp().unwrap();
        let slot_a = world.remove_resource::<SimulationSnapshot>().unwrap();

        world.insert_resource(slot_b);
        world.get_mut::<Transform2d>(entity).unwrap().translation = Vec2::Y;
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let stamp_b = fixed_spatial_snapshot(world).unwrap().stamp().unwrap();
        assert_eq!(
            (
                stamp_a.world,
                stamp_a.runtime_generation,
                stamp_a.phase,
                stamp_a.tick,
                stamp_a.frame,
                stamp_a.hierarchy_generation
            ),
            (
                stamp_b.world,
                stamp_b.runtime_generation,
                stamp_b.phase,
                stamp_b.tick,
                stamp_b.frame,
                stamp_b.hierarchy_generation
            )
        );
        assert_ne!(stamp_a.publication, stamp_b.publication);
        derive_render_projection(world).unwrap();
        let presentation_b = world.remove_resource::<PresentationSnapshot>().unwrap();

        world.insert_resource(slot_a);
        world.insert_resource(presentation_b);
        assert_eq!(
            presented_spatial_snapshot(world).err(),
            Some(SpatialSnapshotError::Unavailable)
        );
        // Source rows still contain Y. Restoring A's old X payload must force a rebuild.
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        let restored = fixed_spatial_snapshot(world).unwrap();
        assert_eq!(restored.global(entity).unwrap().translation(), Vec2::Y);
        assert_ne!(restored.stamp().unwrap().publication, stamp_a.publication);
        assert_ne!(restored.stamp().unwrap().publication, stamp_b.publication);
    });
    runtime.finish().unwrap();
}

#[test]
fn presentation_identity_does_not_repeat_when_history_owners_are_rebuilt_or_restored() {
    let mut runtime = RuntimeInstance::start(spatial_app().unwrap()).unwrap();
    check_world(&mut runtime, |world| {
        use crate::presentation::{TransformHistory, derive_render_projection};
        complete_at(world, SpatialSnapshotPhase::Fixed).unwrap();
        derive_render_projection(world).unwrap();
        let first = presented_spatial_snapshot(world).unwrap().stamp().unwrap();
        let owner_a = world.remove_resource::<TransformHistory>().unwrap();
        world.init_resource::<TransformHistory>();
        derive_render_projection(world).unwrap();
        let rebuilt = presented_spatial_snapshot(world).unwrap().stamp().unwrap();
        assert_ne!(first.publication, rebuilt.publication);
        world.insert_resource(owner_a);
        derive_render_projection(world).unwrap();
        let restored = presented_spatial_snapshot(world).unwrap().stamp().unwrap();
        assert_ne!(restored.publication, first.publication);
        assert_ne!(restored.publication, rebuilt.publication);
        assert_eq!(
            (
                first.world,
                first.runtime_generation,
                first.phase,
                first.tick,
                first.frame
            ),
            (
                restored.world,
                restored.runtime_generation,
                restored.phase,
                restored.tick,
                restored.frame
            )
        );
        assert_ne!(
            restored.publication,
            fixed_spatial_snapshot(world)
                .unwrap()
                .stamp()
                .unwrap()
                .publication
        );
    });
    runtime.finish().unwrap();
}
