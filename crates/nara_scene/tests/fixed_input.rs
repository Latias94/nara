use std::time::Duration;

use nara_app::{
    App, CoreStage, ExecutionProfile, FixedTime, FixedUpdateSet, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance,
};
use nara_core::{Mat3, Vec2};
use nara_ecs::{
    Commands, Component, Entity, Query, Res, ResMut, Resource, World,
    bevy_ecs::entity_disabling::Disabled, schedule::IntoScheduleConfigs,
};
use nara_reflect::ComponentRegistryPlugin;
use nara_scene::fixed_spatial::{
    FixedInputSpatialSnapshot2d, FixedSpatialPlugin, FixedSpatialSnapshot2d,
    FixedTransformInterpolation, HistoricalSpatialSnapshot2d, SpatialSnapshot2d,
    SpatialSnapshotError, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
    add_fixed_transform_input_systems, add_fixed_transform_projection_systems,
    fixed_input_spatial_snapshot, fixed_spatial_snapshot, historical_spatial_snapshot,
    presented_spatial_snapshot,
};
use nara_scene::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
    HierarchyPlugin, HierarchyWriter, Parent,
};

fn spatial_app() -> App {
    let mut app = App::new();
    app.set_execution_profile(ExecutionProfile::Serial).unwrap();
    app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, FixedSpatialPlugin))
        .unwrap();
    app
}

fn translation(x: f32, y: f32) -> Transform2d {
    Transform2d::from_translation(Vec2::new(x, y))
}

fn assert_matrix(actual: Mat3, expected: Mat3) {
    for (actual, expected) in actual
        .to_cols_array()
        .into_iter()
        .zip(expected.to_cols_array())
    {
        assert!((actual - expected).abs() <= 1e-4 + 1e-5 * expected.abs());
    }
}

#[derive(Clone, Copy, Resource)]
struct Scene {
    root: Entity,
    destination: Entity,
    boundary: Entity,
    child: Entity,
    leaf: Entity,
}

#[derive(Default, Resource)]
struct WorkflowProbe {
    input: Option<SpatialSnapshotStamp>,
    historical: Option<SpatialSnapshotStamp>,
    completed: Option<SpatialSnapshotStamp>,
    new_entity: Option<Entity>,
    after_writeback: bool,
    finalized: bool,
}

fn prepared_destination() -> Transform2d {
    Transform2d {
        translation: Vec2::new(30.0, 40.0),
        rotation: 0.37,
        scale: Vec2::new(2.0, -3.0),
        z: 0.0,
    }
}

fn prepared_child() -> Transform2d {
    Transform2d {
        translation: Vec2::new(4.0, 5.0),
        rotation: -0.54,
        scale: Vec2::new(0.5, 1.25),
        z: 0.0,
    }
}

fn prepare_scene(scene: Res<Scene>, mut commands: Commands) {
    commands
        .entity(scene.root)
        .insert(translation(100.0, 200.0));
    commands
        .entity(scene.destination)
        .insert(prepared_destination());
    commands.entity(scene.child).insert(prepared_child());
    commands.entity(scene.boundary).remove::<Transform2d>();
    commands.entity(scene.leaf).insert(translation(6.0, 7.0));
    commands.apply_hierarchy_batch(vec![HierarchyOperation::Move {
        entity: scene.child,
        from: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: scene.root }),
        to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
            parent: scene.destination,
        }),
    }]);
}

fn observe_prepared(
    input: FixedInputSpatialSnapshot2d,
    historical: HistoricalSpatialSnapshot2d,
    scene: Res<Scene>,
    mut probe: ResMut<WorkflowProbe>,
) {
    let stamp = input.stamp().unwrap();
    assert_eq!(stamp.phase, SpatialSnapshotPhase::FixedInput);
    assert_eq!(stamp.tick, 1);
    assert_eq!(input.local(scene.child).unwrap(), prepared_child());
    assert_eq!(input.parent(scene.child).unwrap(), Some(scene.destination));
    assert_matrix(
        input.global(scene.child).unwrap().matrix(),
        prepared_destination().matrix() * prepared_child().matrix(),
    );
    assert_matrix(
        input
            .effective_parent(scene.child)
            .unwrap()
            .unwrap()
            .matrix(),
        prepared_destination().matrix(),
    );
    assert_eq!(input.local(scene.leaf).unwrap(), translation(6.0, 7.0));
    assert_eq!(input.parent(scene.leaf).unwrap(), Some(scene.boundary));
    assert_eq!(input.effective_parent(scene.leaf).unwrap(), None);
    assert_eq!(
        input.global(scene.leaf).unwrap().translation(),
        Vec2::new(6.0, 7.0)
    );
    assert_eq!(input.parent(scene.root).unwrap(), None);
    assert_eq!(input.effective_parent(scene.root).unwrap(), None);
    assert_eq!(
        input.order().unwrap(),
        [
            scene.root,
            scene.boundary,
            scene.leaf,
            scene.destination,
            scene.child
        ]
    );
    assert_eq!(input.structural_index(scene.boundary).unwrap(), 1);
    assert_eq!(input.structural_index(scene.child).unwrap(), 4);
    assert_eq!(
        input.global(scene.boundary),
        Err(SpatialSnapshotError::MissingEntity {
            entity: scene.boundary
        })
    );
    assert_eq!(
        historical.local(scene.child).unwrap(),
        translation(7.0, 8.0)
    );
    assert_eq!(historical.parent(scene.child).unwrap(), Some(scene.root));
    assert_eq!(
        historical.global(scene.child).unwrap().translation(),
        Vec2::new(8.0, 10.0)
    );
    assert_eq!(
        historical.global(scene.leaf).unwrap().translation(),
        Vec2::new(9.0, 12.0)
    );
    assert_eq!(historical.stamp().unwrap().tick, 0);
    probe.input = Some(stamp);
    probe.historical = Some(historical.stamp().unwrap());
}

fn writeback_scene(scene: Res<Scene>, mut probe: ResMut<WorkflowProbe>, mut commands: Commands) {
    commands.entity(scene.root).insert(translation(50.0, 60.0));
    commands
        .entity(scene.boundary)
        .insert(translation(2.0, 3.0));
    commands.entity(scene.child).insert(translation(11.0, 12.0));
    commands.entity(scene.leaf).insert(translation(9.0, 10.0));
    commands.apply_hierarchy_batch(vec![HierarchyOperation::Move {
        entity: scene.child,
        from: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
            parent: scene.destination,
        }),
        to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: scene.root }),
    }]);
    probe.new_entity = Some(commands.spawn(Transform2d::IDENTITY).id());
}

fn assert_captured_input(input: SpatialSnapshot2d<'_>, scene: Scene) {
    assert_eq!(input.local(scene.child).unwrap(), prepared_child());
    assert_eq!(input.parent(scene.child).unwrap(), Some(scene.destination));
    assert_matrix(
        input.global(scene.child).unwrap().matrix(),
        prepared_destination().matrix() * prepared_child().matrix(),
    );
    assert_eq!(input.local(scene.leaf).unwrap(), translation(6.0, 7.0));
    assert_eq!(input.parent(scene.leaf).unwrap(), Some(scene.boundary));
    assert_eq!(input.effective_parent(scene.leaf).unwrap(), None);
    assert_eq!(
        input.global(scene.leaf).unwrap().translation(),
        Vec2::new(6.0, 7.0)
    );
    assert_eq!(input.structural_index(scene.child).unwrap(), 4);
}

fn observe_after_writeback(world: &mut World) {
    let scene = *world.resource::<Scene>();
    let probe = world.resource::<WorkflowProbe>();
    let input = fixed_input_spatial_snapshot(world).unwrap();
    assert_eq!(Some(input.stamp().unwrap()), probe.input);
    assert_captured_input(input, scene);
    let historical = historical_spatial_snapshot(world).unwrap();
    assert_eq!(Some(historical.stamp().unwrap()), probe.historical);
    assert_eq!(
        historical.local(scene.child).unwrap(),
        translation(7.0, 8.0)
    );
    assert_eq!(historical.parent(scene.child).unwrap(), Some(scene.root));
    assert_eq!(historical.structural_index(scene.child).unwrap(), 3);
    assert_eq!(
        historical.global(scene.child).unwrap().translation(),
        Vec2::new(8.0, 10.0)
    );
    assert_eq!(
        world.get::<Transform2d>(scene.child),
        Some(&translation(11.0, 12.0))
    );
    assert_eq!(
        world.get::<Parent>(scene.child).unwrap().parent(),
        scene.root
    );
    let new_entity = probe.new_entity.unwrap();
    assert_eq!(
        fixed_input_spatial_snapshot(world)
            .unwrap()
            .local(new_entity),
        Err(SpatialSnapshotError::MissingEntity { entity: new_entity })
    );
    world.resource_mut::<WorkflowProbe>().after_writeback = true;
}

fn observe_completed(
    fixed: FixedSpatialSnapshot2d,
    input: FixedInputSpatialSnapshot2d,
    scene: Res<Scene>,
    mut probe: ResMut<WorkflowProbe>,
) {
    assert_eq!(fixed.local(scene.child).unwrap(), translation(11.0, 12.0));
    assert_eq!(fixed.parent(scene.child).unwrap(), Some(scene.root));
    assert_eq!(
        fixed.global(scene.child).unwrap().translation(),
        Vec2::new(61.0, 72.0)
    );
    assert_eq!(
        fixed.global(scene.leaf).unwrap().translation(),
        Vec2::new(61.0, 73.0)
    );
    assert_eq!(fixed.structural_index(scene.child).unwrap(), 3);
    assert_eq!(
        fixed.local(probe.new_entity.unwrap()).unwrap(),
        Transform2d::IDENTITY
    );
    assert_eq!(input.local(scene.child).unwrap(), prepared_child());
    assert_eq!(Some(input.stamp().unwrap()), probe.input);
    let stamp = fixed.stamp().unwrap();
    assert_eq!(stamp.phase, SpatialSnapshotPhase::Fixed);
    assert_eq!(stamp.tick, input.stamp().unwrap().tick);
    assert_ne!(stamp.publication, input.stamp().unwrap().publication);
    probe.completed = Some(stamp);
}

fn finalize_scene(world: &mut World) {
    let probe = world.resource::<WorkflowProbe>();
    assert_eq!(
        Some(fixed_spatial_snapshot(world).unwrap().stamp().unwrap()),
        probe.completed
    );
    assert_eq!(
        Some(
            fixed_input_spatial_snapshot(world)
                .unwrap()
                .stamp()
                .unwrap()
        ),
        probe.input
    );
    world.resource_mut::<WorkflowProbe>().finalized = true;
}

#[test]
fn deferred_prepare_sources_and_writeback_publish_distinct_immutable_cuts() {
    let mut app = spatial_app();
    let world = app.world_mut().unwrap();
    let scene = Scene {
        root: world.spawn(translation(1.0, 2.0)).id(),
        destination: world.spawn(translation(10.0, 20.0)).id(),
        boundary: world.spawn(translation(3.0, 4.0)).id(),
        child: world.spawn(translation(7.0, 8.0)).id(),
        leaf: world.spawn(translation(5.0, 6.0)).id(),
    };
    let mut operations = Vec::new();
    for (entity, owner) in [
        (scene.root, HierarchyPlacementOwner::Root),
        (scene.destination, HierarchyPlacementOwner::Root),
        (
            scene.boundary,
            HierarchyPlacementOwner::Parent { parent: scene.root },
        ),
        (
            scene.child,
            HierarchyPlacementOwner::Parent { parent: scene.root },
        ),
        (
            scene.leaf,
            HierarchyPlacementOwner::Parent {
                parent: scene.boundary,
            },
        ),
    ] {
        operations.push(HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(owner),
        });
    }
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .unwrap();
    app.insert_resource(scene).unwrap();
    app.init_resource::<WorkflowProbe>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        prepare_scene.in_set(FixedUpdateSet::Prepare),
    )
    .unwrap();
    add_fixed_transform_input_systems(&mut app, observe_prepared).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (writeback_scene, observe_after_writeback)
            .chain()
            .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    add_fixed_transform_projection_systems(&mut app, observe_completed).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        finalize_scene.in_set(FixedUpdateSet::Finalize),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let world = runtime.world();
    let probe = world.resource::<WorkflowProbe>();
    assert!(probe.after_writeback && probe.finalized);
    assert_captured_input(fixed_input_spatial_snapshot(world).unwrap(), scene);
    let presented = presented_spatial_snapshot(world).unwrap();
    assert_eq!(
        presented.local(scene.child).unwrap(),
        translation(11.0, 12.0)
    );
    assert_eq!(presented.parent(scene.child).unwrap(), Some(scene.root));
    assert_eq!(
        presented.global(scene.child).unwrap().translation(),
        Vec2::new(61.0, 72.0)
    );
    runtime.finish().unwrap();
}

#[derive(Resource)]
struct TickProbe {
    entity: Entity,
    inputs: Vec<SpatialSnapshotStamp>,
    peers: Vec<SpatialSnapshotStamp>,
    completed: Vec<SpatialSnapshotStamp>,
}

fn observe_tick_input(
    input: FixedInputSpatialSnapshot2d,
    historical: HistoricalSpatialSnapshot2d,
    time: Res<FixedTime>,
    mut probe: ResMut<TickProbe>,
) {
    let stamp = input.stamp().unwrap();
    assert_eq!(stamp.tick, time.tick());
    assert_eq!(stamp.phase, SpatialSnapshotPhase::FixedInput);
    assert_eq!(input.parent(probe.entity).unwrap(), None);
    assert_eq!(
        input.local(probe.entity).unwrap().translation.x,
        (stamp.tick - 1) as f32
    );
    assert_eq!(historical.stamp().unwrap().tick, stamp.tick - 1);
    probe.inputs.push(stamp);
}

fn writeback_tick(
    input: FixedInputSpatialSnapshot2d,
    probe: Res<TickProbe>,
    mut commands: Commands,
) {
    let stamp = input.stamp().unwrap();
    assert_eq!(Some(&stamp), probe.inputs.last());
    assert_eq!(Some(&stamp), probe.peers.last());
    commands
        .entity(probe.entity)
        .insert(translation(stamp.tick as f32, 0.0));
}

fn observe_tick_peer(input: FixedInputSpatialSnapshot2d, mut probe: ResMut<TickProbe>) {
    let stamp = input.stamp().unwrap();
    assert_eq!(Some(&stamp), probe.inputs.last());
    probe.peers.push(stamp);
}

fn observe_tick_completed(fixed: FixedSpatialSnapshot2d, mut probe: ResMut<TickProbe>) {
    let stamp = fixed.stamp().unwrap();
    assert_eq!(
        fixed.local(probe.entity).unwrap().translation.x,
        stamp.tick as f32
    );
    assert_eq!(
        fixed.global(probe.entity).unwrap().translation().x,
        stamp.tick as f32
    );
    probe.completed.push(stamp);
}

#[test]
fn zero_multiple_paused_and_exact_ticks_share_one_input_role() {
    let mut app = spatial_app();
    let entity = app.world_mut().unwrap().spawn(Transform2d::IDENTITY).id();
    app.insert_resource(TickProbe {
        entity,
        inputs: Vec::new(),
        peers: Vec::new(),
        completed: Vec::new(),
    })
    .unwrap();
    add_fixed_transform_input_systems(&mut app, observe_tick_input).unwrap();
    add_fixed_transform_input_systems(&mut app, observe_tick_peer.after(observe_tick_input))
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        writeback_tick.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    add_fixed_transform_projection_systems(&mut app, observe_tick_completed).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world()).err(),
        Some(SpatialSnapshotError::Unavailable)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<TickProbe>().inputs.is_empty());
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    let probe = runtime.world().resource::<TickProbe>();
    assert_eq!(
        probe
            .inputs
            .iter()
            .map(|stamp| stamp.tick)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(probe.peers, probe.inputs);
    assert_eq!(
        probe
            .completed
            .iter()
            .map(|stamp| stamp.tick)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world())
            .unwrap()
            .stamp()
            .unwrap(),
        probe.inputs[2]
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world()).err(),
        Some(SpatialSnapshotError::WrongTime)
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();
    assert_eq!(runtime.world().resource::<TickProbe>().inputs.len(), 3);
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world()).err(),
        Some(SpatialSnapshotError::WrongTime)
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let probe = runtime.world().resource::<TickProbe>();
    assert_eq!(probe.inputs.len(), 4);
    assert_eq!(probe.completed.len(), 4);
    assert_eq!(probe.peers, probe.inputs);
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world())
            .unwrap()
            .stamp()
            .unwrap()
            .tick,
        4
    );
    assert_eq!(
        fixed_spatial_snapshot(runtime.world())
            .unwrap()
            .global(entity)
            .unwrap()
            .translation(),
        Vec2::new(4.0, 0.0)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<TickProbe>().inputs.len(), 4);
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world()).err(),
        Some(SpatialSnapshotError::WrongTime)
    );
    runtime.finish().unwrap();
}

#[test]
fn applications_without_consumers_do_not_publish_fixed_inputs() {
    let mut app = spatial_app();
    app.world_mut().unwrap().spawn(Transform2d::IDENTITY);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();
    assert_eq!(
        fixed_input_spatial_snapshot(runtime.world()).err(),
        Some(SpatialSnapshotError::Unavailable)
    );
    assert!(historical_spatial_snapshot(runtime.world()).is_ok());
    assert!(presented_spatial_snapshot(runtime.world()).is_ok());
    runtime.finish().unwrap();
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct GameplayDisabled;

#[test]
fn disabled_parent_participates_in_fixed_input_and_interpolation() {
    let timestep = Duration::from_millis(20);
    for built_in in [true, false] {
        let mut app = App::new();
        app.set_execution_profile(ExecutionProfile::Serial).unwrap();
        if !built_in {
            app.world_mut()
                .unwrap()
                .register_disabling_component::<GameplayDisabled>();
        }
        app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, FixedSpatialPlugin))
            .unwrap();
        app.insert_resource(FixedTime::new(timestep).unwrap())
            .unwrap();
        app.insert_resource(FixedTransformInterpolation).unwrap();

        let world = app.world_mut().unwrap();
        let mut parent = world.spawn(translation(100.0, 0.0));
        if built_in {
            parent.insert(Disabled);
        } else {
            parent.insert(GameplayDisabled);
        }
        let parent = parent.id();
        let child = world.spawn(translation(3.0, 0.0)).id();
        let mut hierarchy = HierarchyWriter::new(world);
        hierarchy
            .place(
                parent,
                HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            )
            .unwrap();
        hierarchy
            .place(
                child,
                HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            )
            .unwrap();

        add_fixed_transform_input_systems(
            &mut app,
            move |input: FixedInputSpatialSnapshot2d, transforms: Query<&Transform2d>| {
                assert!(transforms.get(parent).is_err());
                assert_eq!(transforms.get(child).unwrap(), &translation(3.0, 0.0));
                assert_eq!(input.parent(child), Ok(Some(parent)));
                assert_eq!(input.local(parent), Ok(translation(100.0, 0.0)));
                assert_eq!(
                    input.global(child).unwrap().translation(),
                    Vec2::new(103.0, 0.0)
                );
            },
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            (move |world: &mut World| {
                world.get_mut::<Transform2d>(parent).unwrap().translation.x = 110.0;
            })
            .in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();
        add_fixed_transform_projection_systems(&mut app, move |fixed: FixedSpatialSnapshot2d| {
            assert_eq!(
                fixed.global(child).unwrap().translation(),
                Vec2::new(113.0, 0.0)
            );
        })
        .unwrap();

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(timestep + timestep / 2).unwrap();
        let presented = presented_spatial_snapshot(runtime.world()).unwrap();
        assert_eq!(
            presented.global(child).unwrap().translation(),
            Vec2::new(108.0, 0.0)
        );
        runtime.finish().unwrap();
    }
}
