use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    time::Duration,
};

use nara_app::{
    ExecutionProfile, FixedTime, FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult,
    RuntimeControlStatus, RuntimeState,
};
use nara_core::Mat3;
use nara_ecs::{Commands, Query, Res, ResMut, Resource, With, schedule::IntoScheduleConfigs};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_scene::{
    SceneDocumentCandidate, SceneEntityPlacement, SceneEntitySource, ScenePlacement,
    ScenePlacementParent,
};
use nara_transform::{
    FixedInputSpatialSnapshot2d, FixedSpatialSnapshot2d, PresentedSpatialSnapshot2d,
    SpatialSnapshotError, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
    add_fixed_transform_projection_systems, add_presented_transform_projection_systems,
    fixed_input_spatial_snapshot, historical_spatial_snapshot, presented_spatial_snapshot,
};

use super::{tests::*, *};

fn profiles() -> Vec<ExecutionProfile> {
    #[cfg(feature = "production-parallel")]
    {
        let pool = bevy_tasks::ComputeTaskPool::get_or_init(|| {
            bevy_tasks::TaskPoolBuilder::new().num_threads(2).build()
        });
        assert_eq!(pool.thread_num(), 2);
        vec![ExecutionProfile::Serial, ExecutionProfile::Parallel]
    }
    #[cfg(not(feature = "production-parallel"))]
    {
        vec![ExecutionProfile::Serial]
    }
}

#[derive(Debug)]
struct FixedObservation {
    input_stamp: SpatialSnapshotStamp,
    stamp: SpatialSnapshotStamp,
    input: BTreeMap<Entity, Transform2d>,
    locals: BTreeMap<Entity, Transform2d>,
    globals: BTreeMap<Entity, Mat3>,
    debug: Arc<motion_physics::authored::DebugSnapshot>,
}

#[derive(Debug)]
struct PresentedObservation {
    stamp: SpatialSnapshotStamp,
    globals: BTreeMap<Entity, Mat3>,
}

#[derive(Debug, Default, Resource)]
struct CutLog {
    fixed: Vec<FixedObservation>,
    presented: Vec<PresentedObservation>,
}

fn observe_fixed(
    input: FixedInputSpatialSnapshot2d,
    fixed: FixedSpatialSnapshot2d,
    locals: Query<(Entity, &Transform2d), With<CollisionDefinition>>,
    owner: Res<CollisionOwner>,
    mut log: ResMut<CutLog>,
) {
    let observation = FixedObservation {
        input_stamp: input.stamp().unwrap(),
        stamp: fixed.stamp().unwrap(),
        input: locals
            .iter()
            .map(|(entity, _)| (entity, input.local(entity).unwrap()))
            .collect(),
        locals: locals
            .iter()
            .map(|(entity, local)| (entity, *local))
            .collect(),
        globals: locals
            .iter()
            .map(|(entity, _)| (entity, fixed.global(entity).unwrap().matrix()))
            .collect(),
        debug: owner.0.lock().unwrap().debug_snapshot(),
    };
    assert_eq!(
        observation.input_stamp.phase,
        SpatialSnapshotPhase::FixedInput
    );
    assert_eq!(observation.stamp.phase, SpatialSnapshotPhase::Fixed);
    assert_eq!(observation.input_stamp.tick, observation.stamp.tick);
    assert_eq!(observation.input_stamp.frame, observation.stamp.frame);
    assert_ne!(
        observation.input_stamp.publication,
        observation.stamp.publication
    );
    log.fixed.push(observation);
}

fn observe_presented(
    presented: PresentedSpatialSnapshot2d,
    locals: Query<Entity, With<CollisionDefinition>>,
    mut log: ResMut<CutLog>,
) {
    log.presented.push(PresentedObservation {
        stamp: presented.stamp().unwrap(),
        globals: locals
            .iter()
            .map(|entity| (entity, presented.global(entity).unwrap().matrix()))
            .collect(),
    });
}

fn install_observers(app: &mut App) {
    app.init_resource::<CutLog>().unwrap();
    add_fixed_transform_projection_systems(app, observe_fixed).unwrap();
    add_presented_transform_projection_systems(app, observe_presented).unwrap();
}

fn observed_harness(
    scenes: Vec<SourceScene>,
    profile: ExecutionProfile,
    configure: impl FnOnce(&mut App),
) -> Harness {
    let owner = Arc::new(Mutex::new(CollisionWorld::new()));
    let sources = Arc::new(Mutex::new(scenes));
    let solve = Arc::new(AtomicBool::new(true));
    let mut app = App::new();
    app.set_execution_profile(profile).unwrap();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        TransformPlugin,
    ))
    .unwrap();
    app.add_plugin(CollisionPlugin {
        owner: CollisionOwner(Arc::clone(&owner)),
        sources: Sources(Arc::clone(&sources)),
        solve: SolveEnabled(Arc::clone(&solve)),
    })
    .unwrap();
    configure(&mut app);
    let mut harness = Harness {
        runtime: RuntimeInstance::start(app).unwrap(),
        owner,
        sources,
        solve,
    };
    harness.sync();
    harness.admit();
    harness
}

#[test]
fn rotated_dynamic_root_writes_back_before_original_fixed_and_presented_cuts_for_exact_ticks() {
    for profile in profiles() {
        let initial = transform([1.5, 4.0], 0.4, [1.0, 1.0]);
        let shape = definition(Vec2::new(0.4, 0.7), 0.5, 0.35, true);
        let document = SceneDocument::from_ordered_roots([
            record("floor", definition(Vec2::new(0.0, -0.5), 10.0, 0.5, false)),
            shape_record("box", shape, initial),
            shape_record(
                "unrelated",
                definition(Vec2::new(0.0, 7.0), 0.25, 0.25, true),
                Transform2d::from_translation(Vec2::new(30.0, 0.0)),
            ),
        ]);
        let mut harness = observed_harness(
            vec![SourceScene::new(document, "rotated-root")],
            profile,
            install_observers,
        );
        let entity = harness.entity(0, "box");
        let floor = harness.entity(0, "floor");
        let unrelated = harness.entity(0, "unrelated");
        let pair = generated(&harness.owner.lock().unwrap(), entity);
        let before = harness.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        assert!(!retained.solved);
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            entity,
            shape,
            &[initial],
        );
        let initial_tick = harness.runtime.world().resource::<FixedTime>().tick();
        assert_eq!(harness.runtime.world().resource::<CutLog>().fixed.len(), 1);
        harness.runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(harness.runtime.world().resource::<CutLog>().fixed.len(), 1);
        assert_eq!(*harness.owner.lock().unwrap().debug_snapshot(), retained);
        assert!(fixed_input_spatial_snapshot(harness.runtime.world()).is_err());
        harness
            .runtime
            .drive(FixedTime::DEFAULT_TIMESTEP * 2 + FixedTime::DEFAULT_TIMESTEP / 2)
            .unwrap();
        assert_eq!(
            harness.runtime.world().resource::<FixedTime>().tick(),
            initial_tick + 2
        );
        {
            let log = harness.runtime.world().resource::<CutLog>();
            assert_eq!(log.fixed.len(), 3);
            for (previous, current) in log.fixed.iter().zip(log.fixed.iter().skip(1)) {
                assert_eq!(current.input[&entity], previous.locals[&entity]);
                assert_eq!(current.input_stamp.tick, previous.input_stamp.tick + 1);
                assert_ne!(
                    current.input_stamp.publication,
                    previous.input_stamp.publication
                );
                assert_eq!(current.debug.input_stamp, Some(current.input_stamp));
                assert!(current.debug.solved);
                assert!(
                    current.locals[&entity].translation.y < current.input[&entity].translation.y
                );
                assert_eq!(current.globals[&entity], current.locals[&entity].matrix());
                assert_ne!(current.globals[&entity], current.input[&entity].matrix());
            }
            let fixed = log.fixed.last().unwrap();
            let presented = log.presented.last().unwrap();
            assert_eq!(presented.globals[&entity], fixed.globals[&entity]);
            assert_eq!(presented.stamp.tick, fixed.stamp.tick);
        }
        let paused_state = native_state(&harness.owner.lock().unwrap());
        let pause = match harness.runtime.request_control(RuntimeControl::Pause) {
            RuntimeControlRequestResult::Accepted(ticket) => ticket,
            other => panic!("Pause must be admitted: {other:?}"),
        };
        harness.runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(harness.runtime.state(), RuntimeState::Paused);
        assert_eq!(
            harness.runtime.control_status(pause),
            Some(RuntimeControlStatus::Applied)
        );
        harness
            .runtime
            .drive(FixedTime::DEFAULT_TIMESTEP * 3)
            .unwrap();
        assert_eq!(
            harness.runtime.world().resource::<FixedTime>().tick(),
            initial_tick + 2
        );
        assert_eq!(harness.runtime.world().resource::<CutLog>().fixed.len(), 3);
        assert_native_unchanged(&harness.owner.lock().unwrap(), &paused_state, None);
        let step = match harness
            .runtime
            .request_control(RuntimeControl::StepFixedTick)
        {
            RuntimeControlRequestResult::Accepted(ticket) => ticket,
            other => panic!("an exact paused Step must be admitted: {other:?}"),
        };
        harness.runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(harness.runtime.state(), RuntimeState::Paused);
        assert_eq!(
            harness.runtime.control_status(step),
            Some(RuntimeControlStatus::Applied)
        );
        assert_eq!(
            harness.runtime.world().resource::<FixedTime>().tick(),
            initial_tick + 3
        );
        assert_eq!(harness.runtime.world().resource::<CutLog>().fixed.len(), 4);
        let stepped_state = native_state(&harness.owner.lock().unwrap());
        {
            let owner = harness.owner.lock().unwrap();
            assert_same_pair(&generated(&owner, entity), &pair);
            let body = owner.body(pair.body).unwrap();
            let previous = paused_state
                .iter()
                .find(|state| state.entity == entity)
                .unwrap();
            assert!(body.translation().y < previous.translation.y);
            assert!(coordinate_close(
                body.linvel().y - previous.velocity.y,
                -9.81 * FixedTime::DEFAULT_TIMESTEP.as_secs_f64(),
            ));
            let log = harness.runtime.world().resource::<CutLog>();
            let fixed = log.fixed.last().unwrap();
            assert!(fixed.debug.solved);
            assert_eq!(fixed.debug.input_stamp, Some(fixed.input_stamp));
            assert_eq!(fixed.locals[&entity].translation, body.translation());
            assert!(fixed.locals[&entity].translation.y < fixed.input[&entity].translation.y);
            assert_eq!(fixed.globals[&entity], fixed.locals[&entity].matrix());
            assert_ne!(fixed.globals[&entity], fixed.input[&entity].matrix());
        }
        harness.runtime.drive(Duration::ZERO).unwrap();
        assert_native_unchanged(&harness.owner.lock().unwrap(), &stepped_state, None);
        assert_eq!(harness.runtime.world().resource::<CutLog>().fixed.len(), 4);
        assert!(matches!(
            harness.runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        harness.runtime.drive(Duration::ZERO).unwrap();
        for _ in 0..120 {
            harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        }
        let world = harness.runtime.world();
        let log = world.resource::<CutLog>();
        assert_eq!(log.fixed.len(), 124);
        let final_fixed = log.fixed.last().unwrap();
        let final_presented = log.presented.last().unwrap();
        let solved = *world.get::<Transform2d>(entity).unwrap();
        assert_eq!(solved, final_fixed.locals[&entity]);
        assert_eq!(solved.scale, Vec2::ONE);
        assert!(coordinate_close(
            solved.translation.x,
            f64::from(initial.translation.x)
        ));
        assert!(coordinate_close(
            solved.rotation,
            f64::from(initial.rotation)
        ));
        assert_eq!(final_fixed.globals[&entity], solved.matrix());
        assert_eq!(
            final_presented.globals[&entity],
            final_fixed.globals[&entity]
        );
        assert_eq!(final_presented.stamp.phase, SpatialSnapshotPhase::Presented);
        assert_eq!(final_presented.stamp.tick, final_fixed.stamp.tick);
        assert_eq!(final_presented.stamp.frame, final_fixed.stamp.frame);
        assert_eq!(
            historical_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .matrix(),
            final_fixed.globals[&entity]
        );
        assert_eq!(
            presented_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .matrix(),
            final_fixed.globals[&entity]
        );
        let owner = harness.owner.lock().unwrap();
        assert_same_pair(&generated(&owner, entity), &pair);
        let body = owner.body(pair.body).unwrap();
        assert_point(
            body.translation(),
            [
                f64::from(solved.translation.x),
                f64::from(solved.translation.y),
            ],
        );
        assert!(coordinate_close(
            body.rotation().angle(),
            f64::from(solved.rotation)
        ));
        assert!(owner.contact_between(entity, floor));
        assert!(!owner.contact_between(entity, unrelated));
        assert!(!owner.contact_between(floor, unrelated));
        assert_geometry(&owner, world, entity, shape, &[solved]);
        let center = oracle_point(
            [f64::from(shape.center.x), f64::from(shape.center.y)],
            &[solved],
        );
        assert_point(owner.collider(pair.collider).unwrap().translation(), center);
        let vertical_radius = f64::from(initial.rotation).sin().abs() * f64::from(shape.half_width)
            + f64::from(initial.rotation).cos().abs() * f64::from(shape.half_height);
        assert!((center[1] - vertical_radius).abs() < 0.025);
        assert!((f64::from(solved.translation.y) - center[1]).abs() > 0.5);
        assert_eq!(
            owner.debug_snapshot().input_stamp,
            Some(final_fixed.input_stamp)
        );
        assert!(owner.debug_snapshot().solved);
        drop(owner);
        let owner = Arc::clone(&harness.owner);
        harness.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_withheld_capture(&owner.lock().unwrap());
        assert_eq!(*before, retained);
    }
}

fn transform(translation: [f32; 2], rotation: f32, scale: [f32; 2]) -> Transform2d {
    Transform2d {
        translation: Vec2::from_array(translation),
        rotation,
        scale: Vec2::from_array(scale),
    }
}

fn spatial_record(name: &str, local: Transform2d) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name))
        .with_component(ComponentTypeId::new(TRANSFORM), transform_record(local))
}

fn shape_record(name: &str, shape: CollisionDefinition, local: Transform2d) -> SceneEntityRecord {
    record(name, shape).with_component(ComponentTypeId::new(TRANSFORM), transform_record(local))
}

fn under(child: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(child)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn oracle_point(mut point: [f64; 2], chain: &[Transform2d]) -> [f64; 2] {
    for local in chain.iter().rev() {
        let (sine, cosine) = f64::from(local.rotation).sin_cos();
        let x = point[0] * f64::from(local.scale.x);
        let y = point[1] * f64::from(local.scale.y);
        point = [
            cosine * x - sine * y + f64::from(local.translation.x),
            sine * x + cosine * y + f64::from(local.translation.y),
        ];
    }
    point
}

fn oracle_corners(shape: CollisionDefinition, chain: &[Transform2d]) -> [[f64; 2]; 4] {
    let center = [f64::from(shape.center.x), f64::from(shape.center.y)];
    let half_width = f64::from(shape.half_width);
    let half_height = f64::from(shape.half_height);
    [
        [-half_width, -half_height],
        [half_width, -half_height],
        [half_width, half_height],
        [-half_width, half_height],
    ]
    .map(|offset| oracle_point([center[0] + offset[0], center[1] + offset[1]], chain))
}

fn coordinate_close(actual: f32, expected: f64) -> bool {
    actual.is_finite()
        && expected.is_finite()
        && (f64::from(actual) - expected).abs() <= 1e-4 + 1e-5 * expected.abs()
}

fn assert_point(actual: Vec2, expected: [f64; 2]) {
    assert!(
        coordinate_close(actual.x, expected[0]) && coordinate_close(actual.y, expected[1]),
        "actual point {actual:?} differs from independent f64 point {expected:?}"
    );
}

fn assert_corners(actual: &[Vec2], expected: [[f64; 2]; 4]) {
    assert_eq!(actual.len(), 4);
    let mut matched = [false; 4];
    for expected in expected {
        let index = actual
            .iter()
            .enumerate()
            .position(|(index, actual)| {
                !matched[index]
                    && coordinate_close(actual.x, expected[0])
                    && coordinate_close(actual.y, expected[1])
            })
            .unwrap_or_else(|| panic!("native corners {actual:?} omit f64 corner {expected:?}"));
        matched[index] = true;
    }
    assert_eq!(matched, [true; 4]);
}

fn sorted_points(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|left, right| left.x.total_cmp(&right.x).then(left.y.total_cmp(&right.y)));
    points
}

fn assert_geometry(
    owner: &CollisionWorld,
    world: &World,
    entity: Entity,
    shape: CollisionDefinition,
    chain: &[Transform2d],
) {
    let expected = oracle_corners(shape, chain);
    let actual = native_vertices(owner, entity);
    assert_corners(&actual, expected);
    let record = generated(owner, entity);
    let snapshot = owner.debug_snapshot();
    let debug = snapshot
        .shapes
        .iter()
        .find(|shape| shape.entity == entity)
        .unwrap();
    assert_eq!(debug.entity, entity);
    assert_eq!(debug.origin, record.origin);
    let source = world.get::<SceneEntitySource>(entity).unwrap();
    assert_eq!(debug.origin.source, *source.containing_source());
    assert_eq!(debug.origin.occurrence, *source.occurrence());
    assert_eq!(debug.origin.provenance, *source.provenance());
    assert_eq!(debug.origin.component, ComponentTypeId::new(COMPONENT));
    assert_eq!(
        debug.origin.properties,
        ["center", "half_width", "half_height", "dynamic"]
            .map(|field| ComponentPropertyPath::new(ComponentFieldId::new(field)))
    );
    assert_eq!(
        sorted_points(debug.vertices.to_vec()),
        sorted_points(actual)
    );
    assert_corners(&debug.vertices, expected);
    assert_eq!(world.get::<CollisionDefinition>(entity), Some(&shape));
}

#[derive(Resource)]
struct DeferredEdit {
    applied: bool,
    parent: Transform2d,
    shape: CollisionDefinition,
}

fn prepare_deferred_edit(
    mut commands: Commands,
    time: Res<FixedTime>,
    sources: Query<(Entity, &SceneEntitySource)>,
    mut edit: ResMut<DeferredEdit>,
) {
    if time.tick() != 2 {
        return;
    }
    assert!(!edit.applied);
    let find = |name| {
        sources
            .iter()
            .find(|(_, source)| source.provenance().local_entity_id() == &id(name))
            .unwrap()
            .0
    };
    let shape = find("shape");
    let replacement = find("replacement");
    commands.entity(replacement).insert(edit.parent);
    commands.entity(shape).insert(edit.shape);
    commands.queue(move |world: &mut World| {
        let from = HierarchyPlacement::current(world, shape).unwrap();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Move {
                entity: shape,
                from,
                to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: replacement,
                }),
            }])
            .unwrap();
    });
    edit.applied = true;
}

#[test]
fn same_tick_prepare_deferred_shape_parent_edit_and_reparent_feed_original_input_completion() {
    for profile in profiles() {
        let original_parent = transform([-20.0, 4.0], -0.2, [1.2, 0.9]);
        let replacement = transform([30.0, 12.0], 0.1, [0.8, 1.1]);
        let edited_parent = transform([3.5, -1.0], 0.7, [1.8, 0.6]);
        let local = transform([0.25, 1.0], -0.45, [1.1, 0.8]);
        let original_shape = definition(Vec2::new(0.3, -0.2), 0.4, 0.6, false);
        let edited_shape = CollisionDefinition {
            half_width: 1.2,
            ..original_shape
        };
        let mut document = SceneDocument::from_ordered_roots([
            spatial_record("original", original_parent),
            spatial_record("replacement", replacement),
            shape_record("shape", original_shape, local),
        ]);
        document.placements = vec![
            SceneEntityPlacement::root(id("original")),
            SceneEntityPlacement::root(id("replacement")),
            under("shape", "original"),
        ];
        let mut harness = observed_harness(
            vec![SourceScene::new(document.clone(), "deferred-reparent")],
            profile,
            move |app| {
                install_observers(app);
                app.insert_resource(DeferredEdit {
                    applied: false,
                    parent: edited_parent,
                    shape: edited_shape,
                })
                .unwrap();
                app.add_systems(
                    CoreStage::FixedUpdate,
                    prepare_deferred_edit.in_set(FixedUpdateSet::Prepare),
                )
                .unwrap();
            },
        );
        let entity = harness.entity(0, "shape");
        let original = generated(&harness.owner.lock().unwrap(), entity);
        let before = harness.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            entity,
            original_shape,
            &[original_parent, local],
        );
        harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        let world = harness.runtime.world();
        assert!(world.resource::<DeferredEdit>().applied);
        assert_eq!(harness.entity(0, "shape"), entity);
        assert_eq!(
            harness.sources.lock().unwrap()[0].session.document(),
            &document
        );
        let input = fixed_input_spatial_snapshot(world).unwrap();
        assert_eq!(input.stamp().unwrap().tick, 2);
        assert_eq!(
            input.parent(entity).unwrap(),
            Some(harness.entity(0, "replacement"))
        );
        assert_eq!(input.local(entity).unwrap(), local);
        assert_eq!(
            input.local(harness.entity(0, "replacement")).unwrap(),
            edited_parent
        );
        assert_eq!(
            input.local(harness.entity(0, "original")).unwrap(),
            original_parent
        );
        let owner = harness.owner.lock().unwrap();
        let current = generated(&owner, entity);
        assert_ne!(current.body, original.body);
        assert_ne!(current.collider, original.collider);
        assert!(owner.body(original.body).is_none());
        assert!(owner.collider(original.collider).is_none());
        assert_eq!(current.origin, original.origin);
        assert_geometry(&owner, world, entity, edited_shape, &[edited_parent, local]);
        let log = world.resource::<CutLog>();
        assert_eq!(log.fixed.len(), 2);
        let fixed = log.fixed.last().unwrap();
        let presented = log.presented.last().unwrap();
        assert_eq!(fixed.input_stamp, input.stamp().unwrap());
        assert_eq!(
            fixed.globals[&entity],
            input.global(entity).unwrap().matrix()
        );
        assert_eq!(presented.globals[&entity], fixed.globals[&entity]);
        assert_eq!(owner.debug_snapshot().input_stamp, Some(fixed.input_stamp));
        assert!(owner.debug_snapshot().solved);
        drop(owner);
        harness.finish();
        assert_eq!(*before, retained);
    }
}

fn transform_field(name: &str, field: &str, value: f64) -> ScenePatchOperation {
    ScenePatchOperation::SetField {
        entity: id(name),
        component: ComponentTypeId::new(TRANSFORM),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value: ComponentValue::f64(value).unwrap(),
    }
}

#[test]
fn transformed_source_edit_history_and_disk_reopen_keep_local_data_and_unrelated_native_state() {
    for profile in profiles() {
        let parent = transform([2.0, 1.0], 0.25, [1.7, 0.65]);
        let local = transform([-0.4, 0.8], -0.5, [0.9, 1.2]);
        let shape = definition(Vec2::new(0.3, -0.2), 0.75, 0.4, false);
        let mut original = SceneDocument::from_ordered_roots([
            spatial_record("parent", parent),
            shape_record("shape", shape, local),
        ]);
        original.placements = vec![
            SceneEntityPlacement::root(id("parent")),
            under("shape", "parent"),
        ];
        let mut harness = Harness::with_profile(
            vec![
                SourceScene::new(original.clone(), "history"),
                SourceScene::new(document(), "unrelated-history"),
            ],
            profile,
        );
        for _ in 0..7 {
            harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        }
        let old_entity = harness.entity(0, "shape");
        let old_pair = generated(&harness.owner.lock().unwrap(), old_entity);
        let before = harness.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        let unrelated = native_state(&harness.owner.lock().unwrap())
            .into_iter()
            .filter(|entry| entry.entity != old_entity)
            .collect::<Vec<_>>();
        let registry = harness.registry();
        let original_revision = harness.sources.lock().unwrap()[0].session.revision();
        let edited_parent = Transform2d {
            rotation: 0.8,
            translation: Vec2::new(-3.0, 1.0),
            ..parent
        };
        let edited_shape = CollisionDefinition {
            half_width: 1.25,
            ..shape
        };
        harness.patch(
            0,
            &ScenePatchDocument::new([
                transform_field("parent", "rotation", f64::from(edited_parent.rotation)),
                transform_field("parent", "translation.x", -3.0),
                ScenePatchOperation::SetField {
                    entity: id("shape"),
                    component: ComponentTypeId::new(COMPONENT),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("half_width"),
                    value: ComponentValue::f64(1.25).unwrap(),
                },
            ]),
        );
        let forward_entity = harness.entity(0, "shape");
        let forward = generated(&harness.owner.lock().unwrap(), forward_entity);
        assert_ne!(forward.body, old_pair.body);
        assert_ne!(forward.collider, old_pair.collider);
        assert!(harness.owner.lock().unwrap().body(old_pair.body).is_none());
        assert!(
            harness
                .owner
                .lock()
                .unwrap()
                .collider(old_pair.collider)
                .is_none()
        );
        assert_ne!(forward.origin.revision, original_revision);
        assert_eq!(
            forward.origin.revision,
            harness.sources.lock().unwrap()[0].session.revision()
        );
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            forward_entity,
            edited_shape,
            &[edited_parent, local],
        );
        let assert_unrelated = |harness: &Harness| {
            let owner = harness.owner.lock().unwrap();
            let total = native_state(&owner);
            for entry in &unrelated {
                let mut expected = total.clone();
                let index = expected
                    .iter()
                    .position(|actual| actual.entity == entry.entity)
                    .unwrap();
                expected[index] = entry.clone();
                assert_native_unchanged(&owner, &expected, None);
            }
        };
        assert_unrelated(&harness);
        let saved = harness.sources.lock().unwrap()[0]
            .session
            .document()
            .clone();
        for record in &saved.entities {
            let expected = if record.id == id("shape") {
                [
                    ComponentTypeId::new(TRANSFORM),
                    ComponentTypeId::new(COMPONENT),
                ]
                .into_iter()
                .collect::<BTreeSet<_>>()
            } else {
                [ComponentTypeId::new(TRANSFORM)].into_iter().collect()
            };
            assert_eq!(
                record.components.keys().cloned().collect::<BTreeSet<_>>(),
                expected
            );
            assert!(
                record
                    .components
                    .values()
                    .all(|component| component.version == ComponentSchemaVersion::ONE)
            );
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(saved.to_json_string().unwrap().as_bytes())
            .unwrap();
        file.flush().unwrap();
        let reopened = SceneAuthoringSession::try_from_file_candidate(
            SceneDocumentCandidate::decode_json_bytes(&std::fs::read(file.path()).unwrap())
                .unwrap(),
            &registry,
        )
        .unwrap();
        assert_eq!(reopened.document(), &saved);
        assert!(
            harness.sources.lock().unwrap()[0]
                .session
                .undo(&registry)
                .applied
        );
        assert_eq!(
            harness.sources.lock().unwrap()[0].session.document(),
            &original
        );
        harness.sync();
        harness.admit();
        let undone_entity = harness.entity(0, "shape");
        let undone = generated(&harness.owner.lock().unwrap(), undone_entity);
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            undone_entity,
            shape,
            &[parent, local],
        );
        assert!(harness.owner.lock().unwrap().body(forward.body).is_none());
        assert!(
            harness
                .owner
                .lock()
                .unwrap()
                .collider(forward.collider)
                .is_none()
        );
        assert_unrelated(&harness);
        assert!(
            harness.sources.lock().unwrap()[0]
                .session
                .redo(&registry)
                .applied
        );
        assert_eq!(
            harness.sources.lock().unwrap()[0].session.document(),
            &saved
        );
        harness.sync();
        harness.admit();
        assert!(harness.owner.lock().unwrap().body(undone.body).is_none());
        assert!(
            harness
                .owner
                .lock()
                .unwrap()
                .collider(undone.collider)
                .is_none()
        );
        let redone_entity = harness.entity(0, "shape");
        let redone = generated(&harness.owner.lock().unwrap(), redone_entity);
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            redone_entity,
            edited_shape,
            &[edited_parent, local],
        );
        assert_unrelated(&harness);
        let mut restored = SourceScene::new(SceneDocument::default(), "reopened-transform");
        restored.session = reopened;
        harness.sources.lock().unwrap().push(restored);
        harness.sync();
        harness.admit();
        let reopened_entity = harness.entity(2, "shape");
        let reopened_pair = generated(&harness.owner.lock().unwrap(), reopened_entity);
        assert_ne!(reopened_pair.body, redone.body);
        assert_ne!(reopened_pair.collider, redone.collider);
        assert_eq!(reopened_pair.origin.provenance, redone.origin.provenance);
        assert_ne!(reopened_pair.origin.occurrence, redone.origin.occurrence);
        assert_eq!(
            harness
                .runtime
                .world()
                .get::<Transform2d>(harness.entity(2, "parent")),
            Some(&edited_parent)
        );
        assert_eq!(
            harness.runtime.world().get::<Transform2d>(reopened_entity),
            Some(&local)
        );
        assert_geometry(
            &harness.owner.lock().unwrap(),
            harness.runtime.world(),
            reopened_entity,
            edited_shape,
            &[edited_parent, local],
        );
        assert_unrelated(&harness);
        let owner = Arc::clone(&harness.owner);
        harness.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_eq!(*before, retained);
    }
}

#[derive(Debug, Clone, Copy)]
enum InvalidSpatial {
    ZeroFixedScale,
    NearSingularFixedScale,
    MissingTransform,
    ScaledDynamic,
    MirroredDynamic,
    ParentedDynamic,
    NonSpatialParentedDynamic,
}

impl InvalidSpatial {
    fn dynamic(self) -> bool {
        matches!(
            self,
            Self::ScaledDynamic
                | Self::MirroredDynamic
                | Self::ParentedDynamic
                | Self::NonSpatialParentedDynamic
        )
    }

    fn code(self) -> &'static str {
        match self {
            Self::MissingTransform => "fixture.motion.collision.missing-transform",
            Self::ZeroFixedScale | Self::NearSingularFixedScale => {
                "fixture.motion.collision.invalid-geometry"
            }
            _ => "fixture.motion.collision.unsupported-dynamic-transform",
        }
    }
}

#[test]
fn late_invalid_spatial_batches_preserve_every_native_pair_until_the_original_operation_is_repaired()
 {
    use nara_diagnostic::DiagnosticValueRef;

    for profile in profiles() {
        for invalid_case in [
            InvalidSpatial::ZeroFixedScale,
            InvalidSpatial::NearSingularFixedScale,
            InvalidSpatial::MissingTransform,
            InvalidSpatial::ScaledDynamic,
            InvalidSpatial::MirroredDynamic,
            InvalidSpatial::ParentedDynamic,
            InvalidSpatial::NonSpatialParentedDynamic,
        ] {
            let first = SceneDocument::from_ordered_roots([
                record("changed", definition(Vec2::new(0.0, 2.0), 0.7, 0.5, false)),
                record("retired", definition(Vec2::new(8.0, 0.0), 0.5, 0.5, false)),
            ]);
            let shape = definition(Vec2::new(0.0, 4.0), 0.5, 0.5, invalid_case.dynamic());
            let last = SceneDocument::from_ordered_roots([
                shape_record(
                    "invalid",
                    shape,
                    Transform2d::from_translation(Vec2::new(20.0, 0.0)),
                ),
                spatial_record("parent", Transform2d::IDENTITY),
                SceneEntityRecord::new(id("non-spatial")),
            ]);
            let mut last = SourceScene::new(last, "late-invalid");
            last.source = source("spatial-refusal", SourceDocumentKind::Scene);
            let registry = native_registry();
            let mut case = NativeCase::new(
                vec![SourceScene::new(first, "first-valid"), last],
                &registry,
                profile,
                |_| {},
            );
            for _ in 0..3 {
                case.admit(|world, owner, scenes| {
                    owner.reconcile(world, &collision_inputs(scenes)).unwrap();
                    owner.step_and_writeback(world).unwrap();
                });
            }
            let baseline = native_state(&case.owner.lock().unwrap());
            let old_debug = case.owner.lock().unwrap().debug_snapshot();
            let old_contents = (*old_debug).clone();
            let original_receipts = case
                .sources
                .lock()
                .unwrap()
                .iter()
                .map(|scene| {
                    (
                        scene.session.live_instance().unwrap().clone(),
                        scene.session.revision(),
                    )
                })
                .collect::<Vec<_>>();
            let (changed, retired, invalid, original_local, original_placement) =
                case.mutate(move |world, _, scenes| {
                    let inputs = collision_inputs(scenes);
                    let changed = resolve(world, inputs[0].instance, "changed");
                    let retired = resolve(world, inputs[0].instance, "retired");
                    let invalid = resolve(world, inputs[1].instance, "invalid");
                    let local = *world.get::<Transform2d>(invalid).unwrap();
                    let placement = HierarchyPlacement::current(world, invalid).unwrap();
                    world
                        .get_mut::<CollisionDefinition>(changed)
                        .unwrap()
                        .half_width = 1.2;
                    world.entity_mut(retired).remove::<CollisionDefinition>();
                    match invalid_case {
                        InvalidSpatial::ZeroFixedScale => {
                            world.get_mut::<Transform2d>(invalid).unwrap().scale.x = 0.0
                        }
                        InvalidSpatial::NearSingularFixedScale => {
                            world.get_mut::<Transform2d>(invalid).unwrap().scale.x =
                                f32::MIN_POSITIVE
                        }
                        InvalidSpatial::MissingTransform => {
                            world.entity_mut(invalid).remove::<Transform2d>();
                        }
                        InvalidSpatial::ScaledDynamic => {
                            world.get_mut::<Transform2d>(invalid).unwrap().scale.x = 1.25
                        }
                        InvalidSpatial::MirroredDynamic => {
                            world.get_mut::<Transform2d>(invalid).unwrap().scale.x = -1.0
                        }
                        InvalidSpatial::ParentedDynamic
                        | InvalidSpatial::NonSpatialParentedDynamic => {
                            let parent = resolve(
                                world,
                                inputs[1].instance,
                                if matches!(invalid_case, InvalidSpatial::NonSpatialParentedDynamic)
                                {
                                    "non-spatial"
                                } else {
                                    "parent"
                                },
                            );
                            HierarchyWriter::new(world)
                                .apply_batch(&[HierarchyOperation::Move {
                                    entity: invalid,
                                    from: placement,
                                    to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                                        parent,
                                    }),
                                }])
                                .unwrap();
                        }
                    }
                    (changed, retired, invalid, local, placement)
                });
            let retained_state = baseline.clone();
            let refused_stamp = case.admit(move |world, owner, scenes| {
                let failure = owner
                    .reconcile(world, &collision_inputs(scenes))
                    .unwrap_err();
                let diagnostic = failure
                    .iter()
                    .find(|diagnostic| diagnostic.code().as_str() == invalid_case.code())
                    .unwrap_or_else(|| {
                        panic!("{invalid_case:?} requires its exact diagnostic: {failure:?}")
                    });
                let field = |key: &str| {
                    diagnostic
                        .fields()
                        .iter()
                        .find(|field| field.key().as_str() == key)
                        .unwrap()
                        .value()
                };
                assert_eq!(field("input-index"), DiagnosticValueRef::Unsigned(1));
                assert_eq!(
                    field("source"),
                    DiagnosticValueRef::Identifier("spatial-refusal")
                );
                assert_eq!(
                    field("occurrence-slot"),
                    DiagnosticValueRef::Identifier("late-invalid")
                );
                assert_eq!(field("entity"), DiagnosticValueRef::Identifier("invalid"));
                assert_eq!(
                    field("component"),
                    DiagnosticValueRef::Identifier(COMPONENT)
                );
                assert_eq!(
                    field("revision"),
                    DiagnosticValueRef::Unsigned(scenes[1].session.revision().generation())
                );
                assert_native_unchanged(owner, &retained_state, None);
                assert_withheld_capture(owner);
                assert!(owner.step_and_writeback(world).is_err());
                assert_native_unchanged(owner, &retained_state, None);
                assert_withheld_capture(owner);
                fixed_input_spatial_snapshot(world)
                    .unwrap()
                    .stamp()
                    .unwrap()
            });
            assert_eq!(*old_debug, old_contents);
            case.mutate(move |world, owner, _| {
                world.entity_mut(invalid).insert(original_local);
                if matches!(
                    invalid_case,
                    InvalidSpatial::ParentedDynamic | InvalidSpatial::NonSpatialParentedDynamic
                ) {
                    let from = HierarchyPlacement::current(world, invalid).unwrap();
                    HierarchyWriter::new(world)
                        .apply_batch(&[HierarchyOperation::Move {
                            entity: invalid,
                            from,
                            to: original_placement,
                        }])
                        .unwrap();
                }
                assert_withheld_capture(owner);
            });
            let original_changed = baseline
                .iter()
                .find(|entry| entry.entity == changed)
                .unwrap()
                .record
                .clone();
            let original_retired = baseline
                .iter()
                .find(|entry| entry.entity == retired)
                .unwrap()
                .record
                .clone();
            let expected = baseline
                .into_iter()
                .filter(|entry| entry.entity != retired)
                .collect::<Vec<_>>();
            case.admit(move |world, owner, scenes| {
                for (scene, (receipt, revision)) in scenes.iter().zip(original_receipts) {
                    assert_eq!(scene.session.live_instance().unwrap(), &receipt);
                    assert_eq!(scene.session.revision(), revision);
                }
                owner.reconcile(world, &collision_inputs(scenes)).unwrap();
                let replacement = generated(owner, changed);
                assert_ne!(replacement.body, original_changed.body);
                assert_ne!(replacement.collider, original_changed.collider);
                assert!(owner.body(original_changed.body).is_none());
                assert!(owner.collider(original_changed.collider).is_none());
                assert!(owner.body(original_retired.body).is_none());
                assert!(owner.collider(original_retired.collider).is_none());
                assert_native_unchanged(owner, &expected, Some(changed));
                assert_geometry(
                    owner,
                    world,
                    changed,
                    definition(Vec2::new(0.0, 2.0), 1.2, 0.5, false),
                    &[Transform2d::IDENTITY],
                );
                let fresh = owner.debug_snapshot();
                assert_eq!(
                    fresh.input_stamp,
                    Some(
                        fixed_input_spatial_snapshot(world)
                            .unwrap()
                            .stamp()
                            .unwrap()
                    )
                );
                assert_ne!(fresh.input_stamp, Some(refused_stamp));
                assert!(!fresh.solved);
                assert_eq!(fresh.shapes.len(), 2);
            });
            let owner = Arc::clone(&case.owner);
            case.finish();
            assert_eq!(owner.lock().unwrap().counts(), (0, 0));
            assert_eq!(*old_debug, old_contents);
        }
    }
}

#[test]
fn missing_and_stale_fixed_inputs_refuse_native_changes_then_the_same_receipts_admit_a_genuine_cut()
{
    for profile in profiles() {
        let registry = native_registry();
        let mut case = NativeCase::new(
            vec![SourceScene::new(document(), "input-refusal")],
            &registry,
            profile,
            |_| {},
        );
        case.mutate(|world, owner, scenes| {
            assert!(matches!(
                fixed_input_spatial_snapshot(world),
                Err(SpatialSnapshotError::Unavailable)
            ));
            let failure = owner
                .reconcile(world, &collision_inputs(scenes))
                .unwrap_err();
            assert!(
                failure.iter().any(|diagnostic| diagnostic.code().as_str()
                    == "fixture.motion.collision.spatial-input")
            );
            assert_eq!(owner.counts(), (0, 0));
            assert_withheld_capture(owner);
            assert!(owner.step_and_writeback(world).is_err());
        });
        case.admit(|world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            assert_eq!(
                owner.debug_snapshot().input_stamp,
                Some(
                    fixed_input_spatial_snapshot(world)
                        .unwrap()
                        .stamp()
                        .unwrap()
                )
            );
        });
        let baseline = native_state(&case.owner.lock().unwrap());
        let old_debug = case.owner.lock().unwrap().debug_snapshot();
        let original = (*old_debug).clone();
        case.runtime.drive(Duration::ZERO).unwrap();
        let stale_state = baseline.clone();
        case.mutate(move |world, owner, scenes| {
            assert!(matches!(
                fixed_input_spatial_snapshot(world),
                Err(SpatialSnapshotError::WrongTime)
            ));
            let failure = owner
                .reconcile(world, &collision_inputs(scenes))
                .unwrap_err();
            assert!(
                failure.iter().any(|diagnostic| diagnostic.code().as_str()
                    == "fixture.motion.collision.spatial-input")
            );
            assert_native_unchanged(owner, &stale_state, None);
            assert_withheld_capture(owner);
            assert!(owner.step_and_writeback(world).is_err());
            assert_native_unchanged(owner, &stale_state, None);
        });
        let old_stamp = original.input_stamp;
        let old_shapes = original.shapes.clone();
        case.admit(move |world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            assert_native_unchanged(owner, &baseline, None);
            let repaired = owner.debug_snapshot();
            assert_eq!(repaired.shapes, old_shapes);
            assert_ne!(repaired.input_stamp, old_stamp);
            assert_eq!(
                repaired.input_stamp,
                Some(
                    fixed_input_spatial_snapshot(world)
                        .unwrap()
                        .stamp()
                        .unwrap()
                )
            );
            assert!(!repaired.solved);
        });
        let owner = Arc::clone(&case.owner);
        case.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_eq!(*old_debug, original);
    }
}

#[test]
fn admitted_fixed_geometry_uses_immutable_local_and_parent_sources_until_the_next_real_cut() {
    for profile in profiles() {
        let old_parent = transform([2.0, 1.0], 0.3, [1.5, 0.7]);
        let new_parent = transform([-4.0, 3.0], -0.4, [0.8, 1.6]);
        let local = transform([0.2, 0.6], -0.2, [1.0, 1.0]);
        let edited_local = transform([-0.7, 1.4], 0.65, [1.2, 0.9]);
        let shape = definition(Vec2::new(0.3, -0.1), 0.6, 0.35, false);
        let mut document = SceneDocument::from_ordered_roots([
            spatial_record("old-parent", old_parent),
            spatial_record("new-parent", new_parent),
            shape_record("shape", shape, local),
        ]);
        document.placements = vec![
            SceneEntityPlacement::root(id("old-parent")),
            SceneEntityPlacement::root(id("new-parent")),
            under("shape", "old-parent"),
        ];
        let mut case = NativeCase::new(
            vec![SourceScene::new(document, "captured-sources")],
            &native_registry(),
            profile,
            |_| {},
        );
        case.admit(|world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap()
        });
        let baseline = native_state(&case.owner.lock().unwrap());
        let before = case.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        let (entity, old_pair) = case.admit(move |world, owner, scenes| {
            let inputs = collision_inputs(scenes);
            let entity = resolve(world, inputs[0].instance, "shape");
            let old_parent_entity = resolve(world, inputs[0].instance, "old-parent");
            let new_parent_entity = resolve(world, inputs[0].instance, "new-parent");
            let original_global = fixed_input_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .matrix();
            let old_pair = generated(owner, entity);
            world.entity_mut(entity).insert(edited_local);
            let from = HierarchyPlacement::current(world, entity).unwrap();
            HierarchyWriter::new(world)
                .apply_batch(&[HierarchyOperation::Move {
                    entity,
                    from,
                    to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                        parent: new_parent_entity,
                    }),
                }])
                .unwrap();
            let captured = fixed_input_spatial_snapshot(world).unwrap();
            assert_eq!(captured.local(entity).unwrap(), local);
            assert_eq!(captured.parent(entity).unwrap(), Some(old_parent_entity));
            assert_eq!(captured.global(entity).unwrap().matrix(), original_global);
            assert_eq!(world.get::<Transform2d>(entity), Some(&edited_local));
            owner.reconcile(world, &inputs).unwrap();
            assert_native_unchanged(owner, &baseline, None);
            assert_geometry(owner, world, entity, shape, &[old_parent, local]);
            assert_eq!(
                owner.debug_snapshot().input_stamp,
                Some(captured.stamp().unwrap())
            );
            (entity, old_pair)
        });
        case.admit(move |world, owner, scenes| {
            let input = fixed_input_spatial_snapshot(world).unwrap();
            assert_eq!(input.local(entity).unwrap(), edited_local);
            assert_eq!(
                input.parent(entity).unwrap(),
                Some(resolve(
                    world,
                    scenes[0].session.live_instance().unwrap(),
                    "new-parent"
                ))
            );
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            let replacement = generated(owner, entity);
            assert_ne!(replacement.body, old_pair.body);
            assert_ne!(replacement.collider, old_pair.collider);
            assert!(owner.body(old_pair.body).is_none());
            assert!(owner.collider(old_pair.collider).is_none());
            assert_eq!(replacement.origin, old_pair.origin);
            assert_geometry(owner, world, entity, shape, &[new_parent, edited_local]);
        });
        case.finish();
        assert_eq!(*before, retained);
    }
}

#[test]
fn accepted_external_root_pose_rebuilds_its_pair_and_subsequent_writeback_preserves_native_dynamics()
 {
    for profile in profiles() {
        let mut case = NativeCase::new(
            vec![
                SourceScene::new(document(), "external-root"),
                SourceScene::new(document(), "untouched-root"),
            ],
            &native_registry(),
            profile,
            |_| {},
        );
        for _ in 0..4 {
            case.admit(|world, owner, scenes| {
                owner.reconcile(world, &collision_inputs(scenes)).unwrap();
                owner.step_and_writeback(world).unwrap();
            });
        }
        let baseline = native_state(&case.owner.lock().unwrap());
        let before = case.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        let edited = transform([-1.5, 5.0], -0.3, [1.0, 1.0]);
        let entity = case.mutate(move |world, _, scenes| {
            let entity = resolve(world, scenes[0].session.live_instance().unwrap(), "box");
            world.entity_mut(entity).insert(edited);
            entity
        });
        let previous = generated(&case.owner.lock().unwrap(), entity);
        let replacement = case.admit(move |world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            let replacement = generated(owner, entity);
            assert_ne!(replacement.body, previous.body);
            assert_ne!(replacement.collider, previous.collider);
            assert!(owner.body(previous.body).is_none());
            assert!(owner.collider(previous.collider).is_none());
            assert_eq!(replacement.origin, previous.origin);
            assert_native_unchanged(owner, &baseline, Some(entity));
            let body = owner.body(replacement.body).unwrap();
            assert_point(
                body.translation(),
                [
                    f64::from(edited.translation.x),
                    f64::from(edited.translation.y),
                ],
            );
            assert!(coordinate_close(
                body.rotation().angle(),
                f64::from(edited.rotation)
            ));
            assert_eq!(body.linvel(), Vec2::ZERO);
            assert_geometry(
                owner,
                world,
                entity,
                definition(Vec2::new(0.0, 3.0), 0.5, 0.5, true),
                &[edited],
            );
            assert!(!owner.debug_snapshot().solved);
            owner.step_and_writeback(world).unwrap();
            let solved = *world.get::<Transform2d>(entity).unwrap();
            assert!(solved.translation.y < edited.translation.y);
            assert!(coordinate_close(
                solved.rotation,
                f64::from(edited.rotation)
            ));
            assert!(owner.body(replacement.body).unwrap().linvel().y < 0.0);
            assert_geometry(
                owner,
                world,
                entity,
                definition(Vec2::new(0.0, 3.0), 0.5, 0.5, true),
                &[solved],
            );
            assert!(owner.debug_snapshot().solved);
            replacement
        });
        let solved_state = native_state(&case.owner.lock().unwrap());
        case.admit(move |world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            assert_same_pair(&generated(owner, entity), &replacement);
            assert_native_unchanged(owner, &solved_state, None);
            assert!(!owner.debug_snapshot().solved);
        });
        let owner = Arc::clone(&case.owner);
        case.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_eq!(*before, retained);
    }
}

#[test]
fn reparent_after_admission_refuses_the_whole_writeback_batch_then_root_repair_solves_once() {
    for profile in profiles() {
        let scene = SceneDocument::from_ordered_roots([
            record("floor", definition(Vec2::new(0.0, -0.5), 10.0, 0.5, false)),
            record("box", definition(Vec2::new(0.0, 3.0), 0.5, 0.5, true)),
            shape_record(
                "unrelated",
                definition(Vec2::new(0.0, 5.0), 0.5, 0.5, true),
                Transform2d::from_translation(Vec2::new(30.0, 0.0)),
            ),
            spatial_record("parent", transform([4.0, 2.0], 0.6, [1.0, 1.0])),
        ]);
        let mut case = NativeCase::new(
            vec![SourceScene::new(scene, "writeback-reparent")],
            &native_registry(),
            profile,
            |_| {},
        );
        for _ in 0..3 {
            case.admit(|world, owner, scenes| {
                owner.reconcile(world, &collision_inputs(scenes)).unwrap();
                owner.step_and_writeback(world).unwrap();
            });
        }
        let (admitted, admitted_contents, solved, solved_contents) =
            case.admit(|world, owner, scenes| {
                let inputs = collision_inputs(scenes);
                let entity = resolve(world, inputs[0].instance, "box");
                let unrelated = resolve(world, inputs[0].instance, "unrelated");
                let parent = resolve(world, inputs[0].instance, "parent");
                owner.reconcile(world, &inputs).unwrap();
                let input_stamp = fixed_input_spatial_snapshot(world)
                    .unwrap()
                    .stamp()
                    .unwrap();
                let admitted = owner.debug_snapshot();
                let admitted_contents = (*admitted).clone();
                assert_eq!(admitted.input_stamp, Some(input_stamp));
                assert!(!admitted.solved);
                let baseline = native_state(owner);
                let locals = inputs[0]
                    .instance
                    .entity_ids()
                    .iter()
                    .map(|id| {
                        let EntityLookup::Resolved(entity) = inputs[0].instance.resolve(world, id)
                        else {
                            panic!("the original receipt must resolve every writeback member");
                        };
                        (entity, *world.get::<Transform2d>(entity).unwrap())
                    })
                    .collect::<BTreeMap<_, _>>();
                let original = HierarchyPlacement::current(world, entity).unwrap();
                assert_eq!(original.owner, HierarchyPlacementOwner::Root);
                assert_eq!(
                    fixed_input_spatial_snapshot(world)
                        .unwrap()
                        .parent(entity)
                        .unwrap(),
                    None
                );
                HierarchyWriter::new(world)
                    .apply_batch(&[HierarchyOperation::Move {
                        entity,
                        from: original,
                        to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
                    }])
                    .unwrap();
                assert!(world.get::<nara_hierarchy::Parent>(entity).is_some());
                assert_eq!(world.get::<Transform2d>(entity), Some(&locals[&entity]));
                assert_eq!(
                    fixed_input_spatial_snapshot(world)
                        .unwrap()
                        .parent(entity)
                        .unwrap(),
                    None
                );
                let failure = owner.step_and_writeback(world).unwrap_err();
                let diagnostic = failure
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.code().as_str() == "fixture.motion.collision.writeback-target"
                    })
                    .unwrap();
                use nara_diagnostic::DiagnosticValueRef;
                let field = |key: &str| {
                    diagnostic
                        .fields()
                        .iter()
                        .find(|field| field.key().as_str() == key)
                        .unwrap()
                        .value()
                };
                assert_eq!(field("entity"), DiagnosticValueRef::Identifier("box"));
                assert_eq!(
                    field("component"),
                    DiagnosticValueRef::Identifier(COMPONENT)
                );
                assert_native_unchanged(owner, &baseline, None);
                for (entity, expected) in &locals {
                    assert_eq!(world.get::<Transform2d>(*entity), Some(expected));
                }
                assert_withheld_capture(owner);
                assert_eq!(*admitted, admitted_contents);
                let from = HierarchyPlacement::current(world, entity).unwrap();
                HierarchyWriter::new(world)
                    .apply_batch(&[HierarchyOperation::Move {
                        entity,
                        from,
                        to: original,
                    }])
                    .unwrap();
                assert!(world.get::<nara_hierarchy::Parent>(entity).is_none());
                assert_eq!(
                    fixed_input_spatial_snapshot(world)
                        .unwrap()
                        .stamp()
                        .unwrap(),
                    input_stamp
                );
                owner.reconcile(world, &inputs).unwrap();
                assert_native_unchanged(owner, &baseline, None);
                let repaired = owner.debug_snapshot();
                assert_eq!(repaired.shapes, admitted.shapes);
                assert_eq!(repaired.input_stamp, Some(input_stamp));
                assert!(!repaired.solved);
                owner.step_and_writeback(world).unwrap();
                let solved = owner.debug_snapshot();
                let solved_contents = (*solved).clone();
                assert_eq!(solved.input_stamp, Some(input_stamp));
                assert!(solved.solved);
                for dynamic in [entity, unrelated] {
                    let previous = baseline
                        .iter()
                        .find(|entry| entry.entity == dynamic)
                        .unwrap();
                    let body = owner.body(previous.record.body).unwrap();
                    let local = *world.get::<Transform2d>(dynamic).unwrap();
                    assert!(local.translation.y < locals[&dynamic].translation.y);
                    assert_point(
                        body.translation(),
                        [
                            f64::from(local.translation.x),
                            f64::from(local.translation.y),
                        ],
                    );
                    assert_eq!(local.scale, Vec2::ONE);
                    let definition = *world.get::<CollisionDefinition>(dynamic).unwrap();
                    assert_geometry(owner, world, dynamic, definition, &[local]);
                }
                let after = native_state(owner);
                let after_locals = locals
                    .keys()
                    .map(|entity| (*entity, *world.get::<Transform2d>(*entity).unwrap()))
                    .collect::<BTreeMap<_, _>>();
                let repeated = owner.step_and_writeback(world).unwrap_err();
                assert!(repeated.iter().any(|diagnostic| diagnostic.code().as_str()
                    == "fixture.motion.collision.already-solved"));
                assert_native_unchanged(owner, &after, None);
                for (entity, expected) in &after_locals {
                    assert_eq!(world.get::<Transform2d>(*entity), Some(expected));
                }
                assert_withheld_capture(owner);
                assert_eq!(*admitted, admitted_contents);
                assert_eq!(*solved, solved_contents);
                (admitted, admitted_contents, solved, solved_contents)
            });
        let owner = Arc::clone(&case.owner);
        case.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_eq!(*admitted, admitted_contents);
        assert_eq!(*solved, solved_contents);
    }
}

#[test]
fn actual_contact_targets_the_sheared_parented_platform_and_writes_the_dynamic_root_pose() {
    for profile in profiles() {
        let ancestor = transform([3.0, -0.4], 0.3, [2.0, 0.7]);
        let angle = (-f64::from(ancestor.scale.x) * f64::from(ancestor.rotation).sin())
            .atan2(f64::from(ancestor.scale.y) * f64::from(ancestor.rotation).cos())
            as f32;
        let platform_local = transform([0.2, -0.1], angle, [1.0, 1.0]);
        let platform_shape = definition(Vec2::new(0.3, -0.2), 2.0, 0.4, false);
        let platform_center = oracle_point(
            [
                f64::from(platform_shape.center.x),
                f64::from(platform_shape.center.y),
            ],
            &[ancestor, platform_local],
        );
        let dynamic_shape = definition(Vec2::new(0.2, 0.7), 0.35, 0.3, true);
        let dynamic_local = transform([platform_center[0] as f32 - 0.2, 3.0], 0.0, [1.0, 1.0]);
        let mut document = SceneDocument::from_ordered_roots([
            spatial_record("ancestor", ancestor),
            shape_record("platform", platform_shape, platform_local),
            shape_record("dynamic", dynamic_shape, dynamic_local),
            shape_record(
                "distractor",
                platform_shape,
                Transform2d::from_translation(Vec2::new(50.0, 0.0)),
            ),
        ]);
        document.placements = vec![
            SceneEntityPlacement::root(id("ancestor")),
            under("platform", "ancestor"),
            SceneEntityPlacement::root(id("dynamic")),
            SceneEntityPlacement::root(id("distractor")),
        ];
        let mut harness =
            Harness::with_profile(vec![SourceScene::new(document, "sheared-contact")], profile);
        let platform = harness.entity(0, "platform");
        let dynamic = harness.entity(0, "dynamic");
        let distractor = harness.entity(0, "distractor");
        let fixed_pair = generated(&harness.owner.lock().unwrap(), platform);
        let dynamic_pair = generated(&harness.owner.lock().unwrap(), dynamic);
        let before = harness.owner.lock().unwrap().debug_snapshot();
        let retained = (*before).clone();
        let corners = oracle_corners(platform_shape, &[ancestor, platform_local]);
        let horizontal = [corners[1][0] - corners[0][0], corners[1][1] - corners[0][1]];
        let side = [corners[3][0] - corners[0][0], corners[3][1] - corners[0][1]];
        assert!(horizontal[1].abs() < 1e-6);
        assert!((horizontal[0] * side[0] + horizontal[1] * side[1]).abs() > 0.5);
        for _ in 0..120 {
            harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        }
        let world = harness.runtime.world();
        let owner = harness.owner.lock().unwrap();
        assert_same_pair(&generated(&owner, platform), &fixed_pair);
        assert_same_pair(&generated(&owner, dynamic), &dynamic_pair);
        assert!(owner.contact_between(platform, dynamic));
        assert!(!owner.contact_between(distractor, dynamic));
        assert!(!owner.contact_between(platform, distractor));
        assert_geometry(
            &owner,
            world,
            platform,
            platform_shape,
            &[ancestor, platform_local],
        );
        assert_eq!(
            world.get::<Transform2d>(harness.entity(0, "ancestor")),
            Some(&ancestor)
        );
        assert_eq!(world.get::<Transform2d>(platform), Some(&platform_local));
        let solved = *world.get::<Transform2d>(dynamic).unwrap();
        assert_geometry(&owner, world, dynamic, dynamic_shape, &[solved]);
        let center = oracle_point(
            [
                f64::from(dynamic_shape.center.x),
                f64::from(dynamic_shape.center.y),
            ],
            &[solved],
        );
        let surface = corners
            .into_iter()
            .map(|point| point[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((center[1] - f64::from(dynamic_shape.half_height) - surface).abs() < 0.025);
        assert!(
            (f64::from(solved.translation.y) + f64::from(dynamic_shape.center.y) - center[1]).abs()
                < 1e-4
        );
        assert!((f64::from(solved.translation.y) - center[1]).abs() > 0.65);
        let body = owner.body(dynamic_pair.body).unwrap();
        assert_point(
            body.translation(),
            [
                f64::from(solved.translation.x),
                f64::from(solved.translation.y),
            ],
        );
        assert_eq!(solved.scale, Vec2::ONE);
        assert!(owner.debug_snapshot().solved);
        drop(owner);
        let owner = Arc::clone(&harness.owner);
        harness.finish();
        assert_eq!(owner.lock().unwrap().counts(), (0, 0));
        assert_eq!(*before, retained);
    }
}

#[test]
fn native_free_fall_and_paused_exact_step_use_the_admitted_nondefault_fixed_duration() {
    for profile in profiles() {
        for milliseconds in [8, 32] {
            let timestep = Duration::from_millis(milliseconds);
            let mut case = NativeCase::new(
                vec![SourceScene::new(document(), "fixed-duration")],
                &native_registry(),
                profile,
                move |app| {
                    app.insert_resource(FixedTime::new(timestep).unwrap())
                        .unwrap();
                },
            );
            let (entity, original, first_velocity) =
                case.admit_delta(timestep, move |world, owner, scenes| {
                    let inputs = collision_inputs(scenes);
                    owner.reconcile(world, &inputs).unwrap();
                    let entity = resolve(world, inputs[0].instance, "box");
                    let original = generated(owner, entity);
                    assert_eq!(owner.body(original.body).unwrap().linvel(), Vec2::ZERO);
                    assert_eq!(world.resource::<FixedTime>().delta(), timestep);
                    owner.step_and_writeback(world).unwrap();
                    let velocity = owner.body(original.body).unwrap().linvel();
                    assert_eq!(velocity.x, 0.0);
                    (entity, original, velocity.y)
                });
            let before = *case.runtime.world().get::<Transform2d>(entity).unwrap();
            assert!(before.translation.y < 0.0);
            assert!(matches!(
                case.runtime.request_control(RuntimeControl::Pause),
                RuntimeControlRequestResult::Accepted(_)
            ));
            case.runtime.drive(Duration::ZERO).unwrap();
            assert_eq!(case.runtime.world().resource::<FixedTime>().tick(), 1);
            assert!(matches!(
                case.runtime.request_control(RuntimeControl::StepFixedTick),
                RuntimeControlRequestResult::Accepted(_)
            ));
            let stepped_velocity = case.admit_delta(Duration::ZERO, move |world, owner, scenes| {
                owner.reconcile(world, &collision_inputs(scenes)).unwrap();
                assert_same_pair(&generated(owner, entity), &original);
                assert_eq!(world.resource::<FixedTime>().delta(), timestep);
                owner.step_and_writeback(world).unwrap();
                let actual = owner.body(original.body).unwrap().linvel().y;
                assert!(
                    world.get::<Transform2d>(entity).unwrap().translation.y < before.translation.y
                );
                actual
            });
            assert_eq!(case.runtime.world().resource::<FixedTime>().tick(), 2);
            case.finish();
            let expected = -9.81 * timestep.as_secs_f32();
            assert!(
                (first_velocity - expected).abs() < 0.000_01,
                "native velocity {first_velocity} must use this fixed duration: {expected}"
            );
            assert!(
                (stepped_velocity - 2.0 * expected).abs() < 0.000_02,
                "exact step native velocity {stepped_velocity} must retain this fixed duration: {}",
                2.0 * expected
            );
        }
    }
}

#[test]
fn changed_or_zero_fixed_duration_refuses_before_native_step_and_original_input_can_recover() {
    for profile in profiles() {
        let longer = Duration::from_millis(32);
        let mut other = NativeCase::new(vec![], &native_registry(), profile, move |app| {
            app.insert_resource(FixedTime::new(longer).unwrap())
                .unwrap();
        });
        let different_delta =
            other.admit_delta(longer, |world, _, _| *world.resource::<FixedTime>());
        other.runtime.drive(Duration::ZERO).unwrap();
        let zero_delta = *other.runtime.world().resource::<FixedTime>();
        assert_eq!(different_delta.tick(), 1);
        assert_eq!(zero_delta.tick(), 1);
        assert!(zero_delta.delta().is_zero());
        other.finish();

        let timestep = Duration::from_millis(8);
        let mut case = NativeCase::new(
            vec![SourceScene::new(document(), "fixed-duration-refusal")],
            &native_registry(),
            profile,
            move |app| {
                app.insert_resource(FixedTime::new(timestep).unwrap())
                    .unwrap();
            },
        );
        let velocity = case.admit_delta(timestep, move |world, owner, scenes| {
            let inputs = collision_inputs(scenes);
            owner.reconcile(world, &inputs).unwrap();
            let before = native_state(owner);
            let original_time = *world.resource::<FixedTime>();
            let body = resolve(world, inputs[0].instance, "box");
            for (replacement, expected) in [
                (
                    different_delta,
                    "fixture.motion.collision.fixed-duration-changed",
                ),
                (
                    zero_delta,
                    "fixture.motion.collision.invalid-fixed-duration",
                ),
            ] {
                world.insert_resource(replacement);
                assert!(fixed_input_spatial_snapshot(world).is_ok());
                let failure = owner.step_and_writeback(world).unwrap_err();
                assert!(
                    failure
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str() == expected)
                );
                assert_native_unchanged(owner, &before, None);
                assert_withheld_capture(owner);
                world.insert_resource(original_time);
                owner.reconcile(world, &inputs).unwrap();
                assert_native_unchanged(owner, &before, None);
            }
            owner.step_and_writeback(world).unwrap();
            let current = generated(owner, body);
            let velocity = owner.body(current.body).unwrap().linvel().y;
            assert!(owner.debug_snapshot().solved);
            velocity
        });
        case.finish();
        assert!((velocity + 9.81 * timestep.as_secs_f32()).abs() < 0.000_01);
    }
}

#[test]
fn affine_fixed_boxes_retain_native_corners_for_shear_four_mirrors_and_non_spatial_parents() {
    for profile in profiles() {
        for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let root = transform([8.0, -3.0], 0.38, [sx * 1.75, sy * 0.65]);
            let parent = transform([-1.25, 2.0], -0.62, [1.3, 0.8]);
            let local = transform([0.4, -0.7], 0.21, [1.1, 0.9]);
            let shape = definition(Vec2::new(0.35, -0.4), 1.1, 0.45, false);
            let distant = transform([130.0, 70.0], -0.8, [3.0, 2.0]);
            let detached = transform([3.0, -1.0], 0.2, [-1.2, 0.7]);
            let mut scene = SceneDocument::from_ordered_roots([
                spatial_record("root", root),
                spatial_record("parent", parent),
                shape_record("shape", shape, local),
                spatial_record("distant", distant),
                SceneEntityRecord::new(id("boundary")),
                shape_record("detached", shape, detached),
            ]);
            scene.placements = vec![
                SceneEntityPlacement::root(id("root")),
                under("parent", "root"),
                under("shape", "parent"),
                SceneEntityPlacement::root(id("distant")),
                under("boundary", "distant"),
                under("detached", "boundary"),
            ];
            let mut harness =
                Harness::with_profile(vec![SourceScene::new(scene, "affine")], profile);
            let entity = harness.entity(0, "shape");
            let detached_entity = harness.entity(0, "detached");
            let boundary = harness.entity(0, "boundary");
            let world = harness.runtime.world();
            let input = fixed_input_spatial_snapshot(world).unwrap();
            assert_eq!(input.local(entity).unwrap(), local);
            assert_eq!(
                input.parent(entity).unwrap(),
                Some(harness.entity(0, "parent"))
            );
            assert_eq!(input.parent(detached_entity).unwrap(), Some(boundary));
            assert!(input.effective_parent(detached_entity).unwrap().is_none());
            assert!(world.get::<Transform2d>(boundary).is_none());
            let expected = oracle_corners(shape, &[root, parent, local]);
            let first = [
                expected[1][0] - expected[0][0],
                expected[1][1] - expected[0][1],
            ];
            let second = [
                expected[3][0] - expected[0][0],
                expected[3][1] - expected[0][1],
            ];
            assert!((first[0] * second[0] + first[1] * second[1]).abs() > 0.05);
            for (entity, chain) in [
                (entity, vec![root, parent, local]),
                (detached_entity, vec![detached]),
            ] {
                let owner = harness.owner.lock().unwrap();
                assert!(
                    !owner
                        .body(generated(&owner, entity).body)
                        .unwrap()
                        .is_dynamic()
                );
                assert_geometry(&owner, world, entity, shape, &chain);
                assert_eq!(
                    owner.debug_snapshot().input_stamp,
                    Some(input.stamp().unwrap())
                );
                assert!(!owner.debug_snapshot().solved);
                let cut = input.global(entity).unwrap().matrix();
                let corners = oracle_corners(shape, &chain);
                let local_corners = oracle_corners(shape, &[]);
                for (point, expected) in local_corners.into_iter().zip(corners) {
                    assert_point(
                        cut.transform_point2(Vec2::new(point[0] as f32, point[1] as f32)),
                        expected,
                    );
                }
            }
            let before = harness.owner.lock().unwrap().debug_snapshot();
            let retained = (*before).clone();
            let state = native_state(&harness.owner.lock().unwrap());
            harness.admit();
            {
                let owner = harness.owner.lock().unwrap();
                assert_native_unchanged(&owner, &state, None);
                let current = owner.debug_snapshot();
                assert_eq!(current.shapes, retained.shapes);
                assert_ne!(current.input_stamp, retained.input_stamp);
                assert!(!current.solved);
            }
            let owner = Arc::clone(&harness.owner);
            harness.finish();
            assert_eq!(owner.lock().unwrap().counts(), (0, 0));
            assert_withheld_capture(&owner.lock().unwrap());
            assert_eq!(*before, retained);
        }
    }
}
