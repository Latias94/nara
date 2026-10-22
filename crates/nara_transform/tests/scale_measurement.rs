#![allow(clippy::too_many_lines)]

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, RenderTime, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeObligationLedger, RuntimeState,
};
use nara_core::Vec2;
use nara_ecs::{Entity, IntoScheduleConfigs, Query, ResMut, Resource, World};
use nara_hierarchy::{
    Children, HierarchyError, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
    HierarchyPlugin, HierarchyRoots, HierarchyWriter, Parent, validate_hierarchy,
};
use nara_reflect::ComponentRegistryPlugin;
use nara_transform::{
    FixedTransformInterpolation, Transform2d, TransformPlugin, historical_spatial_snapshot,
    presented_spatial_snapshot,
};

pub(crate) const WARMUP_SAMPLES: usize = 120;
const TIMED_SAMPLES: usize = 1_000;
const TIME_CAP: Duration = Duration::from_secs(15);
pub(crate) const FIXED_DELTA: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy)]
pub(crate) enum Edit {
    Translate { entity: Entity, delta: Vec2 },
}

#[derive(Debug, Default, Resource)]
struct EditPort {
    pending: Option<Edit>,
}

#[derive(Debug, Default, Resource)]
struct HierarchyBatchPort;

impl __RuntimeDriverPort for HierarchyBatchPort {
    type Input = Vec<HierarchyOperation>;
    type Output = Result<(), HierarchyError>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        unreachable!("hierarchy batches are applied by the registered world callback")
    }
}

impl HierarchyBatchPort {
    fn apply_in_world(
        world: &mut World,
        operations: Vec<HierarchyOperation>,
    ) -> Result<
        Result<(), HierarchyError>,
        (nara_app::RuntimeWorldAccessError, Vec<HierarchyOperation>),
    > {
        Ok(HierarchyWriter::new(world).apply_batch(&operations))
    }
}

impl __RuntimeDriverPort for EditPort {
    type Input = Edit;
    type Output = ();

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.pending = Some(input);
    }
}

fn apply_edit(mut port: ResMut<EditPort>, mut transforms: Query<&mut Transform2d>) {
    let Some(Edit::Translate { entity, delta }) = port.pending.take() else {
        return;
    };
    transforms
        .get_mut(entity)
        .expect("measurement actor must retain its transform")
        .translation += delta;
}

#[derive(Debug, Clone)]
pub(crate) struct Fixture {
    pub(crate) entities: Vec<Entity>,
    pub(crate) roots: Vec<usize>,
    parent_indices: Vec<Option<usize>>,
    pub(crate) children: Vec<Vec<usize>>,
}

fn output_path(test_name: &str) -> PathBuf {
    std::env::var_os("NARA_SCALE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|path| path.parent())
                .expect("transform crate should be two levels below the workspace")
                .join("target/foundation-refactor-plan-research/u12")
                .join(format!("{test_name}.jsonl"))
        })
}

pub(crate) fn output_file(test_name: &str) -> std::fs::File {
    let path = output_path(test_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("U12 output directory should be writable");
    }
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .expect("U12 output JSONL should be writable")
}

pub(crate) fn write_row(file: &mut std::fs::File, row: &str) {
    writeln!(file, "{row}").expect("U12 JSONL row should be writable");
}

pub(crate) fn spatial_app(interpolation: bool) -> App {
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, TransformPlugin))
        .expect("spatial plugins should install");
    app.init_resource::<EditPort>()
        .expect("edit port should install");
    app.add_systems(
        CoreStage::FixedUpdate,
        apply_edit
            .after(nara_app::FixedUpdateSet::Simulate)
            .before(nara_transform::__private::TransformSet::Propagate),
    )
    .expect("measurement edit system should install");
    app.init_resource::<HierarchyBatchPort>()
        .expect("hierarchy batch port should install");
    app.__register_driver_port::<HierarchyBatchPort>(HierarchyBatchPort::apply_in_world)
        .expect("hierarchy batch port should register");
    if interpolation {
        app.insert_resource(FixedTransformInterpolation)
            .expect("interpolation policy should install");
    }
    app
}

pub(crate) fn make_fixture(app: &mut App, count: usize) -> Fixture {
    let world = app.world_mut().expect("fixture world should be mutable");
    let entities = (0..count)
        .map(|_| world.spawn(Transform2d::default()).id())
        .collect::<Vec<_>>();
    let root_count = count.min(4);
    let roots = (0..root_count).collect::<Vec<_>>();
    let mut parent_indices = vec![None; count];
    let mut children = vec![Vec::new(); count];
    for index in root_count..count {
        let parent = (index - root_count) / 4;
        parent_indices[index] = Some(parent);
        children[parent].push(index);
    }
    let mut operations = Vec::with_capacity(count);
    for index in roots.iter().copied() {
        operations.push(HierarchyOperation::Place {
            entity: entities[index],
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        });
    }
    for index in root_count..count {
        operations.push(HierarchyOperation::Place {
            entity: entities[index],
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                parent: entities[parent_indices[index].expect("non-root parent")],
            }),
        });
    }
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .expect("fixture hierarchy should be valid");
    Fixture {
        entities,
        roots,
        parent_indices,
        children,
    }
}

fn make_chain_fixture(app: &mut App, count: usize) -> Vec<Entity> {
    let world = app
        .world_mut()
        .expect("chain fixture world should be mutable");
    let entities = (0..count)
        .map(|_| world.spawn(Transform2d::default()).id())
        .collect::<Vec<_>>();
    let mut operations = Vec::with_capacity(count);
    operations.push(HierarchyOperation::Place {
        entity: entities[0],
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
    for index in 1..count {
        operations.push(HierarchyOperation::Place {
            entity: entities[index],
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                parent: entities[index - 1],
            }),
        });
    }
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .expect("chain hierarchy should be valid");
    entities
}

pub(crate) fn start_runtime(app: App) -> nara_app::RuntimeInstance {
    let reservation = RuntimeAdmissionReservation::try_acquire().expect("runtime admission");
    let candidate = reservation
        .admit(
            app.seal().expect("measurement app should seal"),
            RuntimeObligationLedger::new(),
            Default::default(),
        )
        .expect("measurement app should admit");
    candidate
        .complete_startup()
        .expect("measurement app should start")
        .promote()
}

pub(crate) fn stop_runtime(runtime: nara_app::RuntimeInstance) {
    match runtime.finish() {
        Ok(_) => {}
        Err(mut retirement) => {
            while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
                retirement.drive_retirement();
            }
        }
    }
}

pub(crate) fn submit_edit(runtime: &mut nara_app::RuntimeInstance, edit: Edit) {
    runtime
        .with_driver_scope(|scope| scope.__apply_port::<EditPort>(edit))
        .expect("measurement edit port should be available")
        .expect("measurement edit should be accepted");
}

fn fixture_preorder(fixture: &Fixture) -> Vec<usize> {
    let mut pending = fixture.roots.iter().rev().copied().collect::<Vec<_>>();
    let mut preorder = Vec::with_capacity(fixture.entities.len());
    while let Some(index) = pending.pop() {
        preorder.push(index);
        pending.extend(fixture.children[index].iter().rev().copied());
    }
    assert_eq!(preorder.len(), fixture.entities.len());
    preorder
}

pub(crate) fn check_hierarchy(world: &World, fixture: &Fixture) {
    validate_hierarchy(world).expect("measurement hierarchy should remain valid");
    let expected_roots = fixture
        .roots
        .iter()
        .map(|index| fixture.entities[*index])
        .collect::<Vec<_>>();
    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        expected_roots
    );
    let expected_order = fixture_preorder(fixture)
        .iter()
        .map(|index| fixture.entities[*index])
        .collect::<Vec<_>>();
    assert_eq!(
        world
            .resource::<nara_hierarchy::HierarchyOrder>()
            .as_slice(),
        expected_order
    );
    for (index, entity) in fixture.entities.iter().copied().enumerate() {
        assert_eq!(
            world.get::<Parent>(entity).map(Parent::parent),
            fixture.parent_indices[index].map(|parent| fixture.entities[parent])
        );
        let expected_children = fixture.children[index]
            .iter()
            .map(|child| fixture.entities[*child])
            .collect::<Vec<_>>();
        assert_eq!(
            world
                .get::<Children>(entity)
                .map_or(&[][..], Children::as_slice),
            expected_children
        );
    }
}

pub(crate) fn check_globals(
    runtime: &nara_app::RuntimeInstance,
    fixture: &Fixture,
    local: &[Vec2],
) {
    assert_eq!(
        runtime.state(),
        RuntimeState::Running,
        "timed runtime must remain healthy"
    );
    let world = runtime.world();
    check_hierarchy(world, fixture);
    let mut global = vec![Vec2::ZERO; fixture.entities.len()];
    for index in fixture_preorder(fixture) {
        let entity = fixture.entities[index];
        global[index] = fixture.parent_indices[index]
            .map(|parent| global[parent] + local[index])
            .unwrap_or(local[index]);
        assert_eq!(
            historical_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .expect("global transform should be complete")
                .translation(),
            global[index],
            "completed globals must match the independent fixture oracle"
        );
    }
}

pub(crate) fn prepare_leaf_reparent(
    fixture: &mut Fixture,
    actors: &[usize],
    destination: usize,
) -> Vec<HierarchyOperation> {
    let mut operations = Vec::with_capacity(actors.len());
    for index in actors.iter().copied() {
        assert!(fixture.children[index].is_empty());
        let source = fixture.parent_indices[index].expect("reparent actor must have a parent");
        assert_ne!(
            source, destination,
            "measurement must change the leaf parent"
        );
        let position = fixture.children[source]
            .iter()
            .position(|child| *child == index)
            .expect("fixture must retain its reverse parent relation");
        let owner = HierarchyPlacementOwner::Parent {
            parent: fixture.entities[source],
        };
        let from = if position == 0 {
            HierarchyPlacement::first(owner)
        } else {
            HierarchyPlacement::after(
                owner,
                fixture.entities[fixture.children[source][position - 1]],
            )
        };
        operations.push(HierarchyOperation::Move {
            entity: fixture.entities[index],
            from,
            to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                parent: fixture.entities[destination],
            }),
        });
        fixture.children[source].remove(position);
        fixture.children[destination].push(index);
        fixture.parent_indices[index] = Some(destination);
    }
    operations
}

pub(crate) fn apply_leaf_reparent(
    runtime: &mut nara_app::RuntimeInstance,
    operations: Vec<HierarchyOperation>,
    independent: bool,
) {
    if independent {
        for operation in operations {
            runtime
                .with_driver_scope(|scope| {
                    scope.__apply_port::<HierarchyBatchPort>(vec![operation])
                })
                .expect("independent driver scope should remain available")
                .expect("independent hierarchy port should be available")
                .expect("independent hierarchy operation should succeed");
        }
    } else {
        runtime
            .with_driver_scope(|scope| scope.__apply_port::<HierarchyBatchPort>(operations))
            .expect("batch driver scope should remain available")
            .expect("batch hierarchy port should be available")
            .expect("batch hierarchy operation should succeed");
    }
    let status = runtime
        .drive(Duration::ZERO)
        .expect("reparent completion frame should succeed")
        .frame()
        .expect("reparent completion should produce a frame")
        .status;
    assert_eq!(status.fixed_steps, 0);
    assert!(!status.fixed_steps_capped);
}

fn run_timed_row(
    file: &mut std::fs::File,
    test_name: &str,
    population: usize,
    scenario: &str,
    interpolation: bool,
) {
    let mut app = spatial_app(interpolation);
    let fixture = make_fixture(&mut app, population);
    let actor_index = fixture.entities.len() - 1;
    let actor = fixture.entities[actor_index];
    let edit_index = if scenario == "root_dirty" {
        fixture.roots[0]
    } else {
        actor_index
    };
    let edit_entity = fixture.entities[edit_index];
    let mut local = vec![Vec2::ZERO; population];
    let mut runtime = start_runtime(app);
    if interpolation {
        let status = runtime
            .drive(FIXED_DELTA / 2)
            .expect("interpolation half-tick setup should succeed")
            .frame()
            .expect("half-tick setup should produce a frame")
            .status;
        assert_eq!(status.fixed_steps, 0);
        assert!(
            (runtime.world().resource::<RenderTime>().interpolation_alpha - 0.5).abs()
                <= f32::EPSILON
        );
    }
    let delta = Vec2::new(1.0, 0.0);
    for _ in 0..WARMUP_SAMPLES {
        if scenario != "static" {
            submit_edit(
                &mut runtime,
                Edit::Translate {
                    entity: edit_entity,
                    delta,
                },
            );
            local[edit_index] += delta;
        }
        let frame_delta = match scenario {
            "static" => Duration::ZERO,
            "four_steps_one_pending_edit" => FIXED_DELTA * 4,
            _ => FIXED_DELTA,
        };
        let outcome = runtime
            .drive(frame_delta)
            .expect("warmup frame should succeed");
        let status = outcome.frame().expect("frame status").status;
        assert!(!status.fixed_steps_capped);
        if scenario == "four_steps_one_pending_edit" {
            assert_eq!(status.fixed_steps, 4);
        } else if scenario != "static" {
            assert_eq!(status.fixed_steps, 1);
        }
    }
    check_globals(&runtime, &fixture, &local);
    if interpolation {
        assert!(
            presented_spatial_snapshot(runtime.world())
                .unwrap()
                .global(actor)
                .is_ok()
        );
    }

    let mut measured_time = Duration::ZERO;
    let mut samples = 0usize;
    while samples < TIMED_SAMPLES && measured_time < TIME_CAP {
        let previous_actor = local[actor_index];
        if scenario != "static" {
            local[edit_index] += delta;
        }
        let sample_started = Instant::now();
        if scenario != "static" {
            submit_edit(
                &mut runtime,
                Edit::Translate {
                    entity: edit_entity,
                    delta,
                },
            );
        }
        let frame_delta = match scenario {
            "static" => Duration::ZERO,
            "four_steps_one_pending_edit" => FIXED_DELTA * 4,
            _ => FIXED_DELTA,
        };
        let outcome = runtime
            .drive(frame_delta)
            .expect("timed frame should succeed");
        let sample_duration = sample_started.elapsed();
        measured_time += sample_duration;
        let duration_ns = sample_duration.as_nanos();
        let status = outcome.frame().expect("frame status").status;
        assert!(!status.fixed_steps_capped);
        assert_eq!(status.fixed_debt, Duration::ZERO);
        assert_eq!(status.fixed_discarded, Duration::ZERO);
        if scenario == "four_steps_one_pending_edit" {
            assert_eq!(status.fixed_steps, 4);
        } else if scenario != "static" {
            assert_eq!(status.fixed_steps, 1);
        } else {
            assert_eq!(status.fixed_steps, 0);
        }
        if interpolation {
            let alpha = runtime.world().resource::<RenderTime>().interpolation_alpha;
            assert!((alpha - 0.5).abs() <= f32::EPSILON);
            let expected = previous_actor + delta * alpha;
            let projected = presented_spatial_snapshot(runtime.world())
                .unwrap()
                .global(actor)
                .expect("presentation should contain actor")
                .translation();
            assert!((projected - expected).abs().max_element() <= 0.0001);
        }
        write_row(
            file,
            &format!(
                "{{\"test\":\"{test_name}\",\"population\":{population},\"scenario\":\"{scenario}\",\"interpolation\":{interpolation},\"sample\":{samples},\"duration_ns\":{duration_ns},\"fixed_steps\":{},\"fixed_steps_capped\":{},\"local_writes\":{},\"interpolation_alpha\":{},\"allocation_calls\":null,\"allocated_bytes\":null,\"net_requested_bytes\":null}}",
                status.fixed_steps,
                status.fixed_steps_capped,
                usize::from(scenario != "static"),
                runtime.world().resource::<RenderTime>().interpolation_alpha,
            ),
        );
        samples += 1;
    }
    check_globals(&runtime, &fixture, &local);
    write_row(
        file,
        &format!(
            "{{\"type\":\"summary\",\"test\":\"{test_name}\",\"population\":{population},\"scenario\":\"{scenario}\",\"interpolation\":{interpolation},\"samples\":{samples},\"required_samples\":{TIMED_SAMPLES},\"incomplete_for_p99\":{},\"time_cap_ms\":{}}}",
            samples < TIMED_SAMPLES,
            TIME_CAP.as_millis()
        ),
    );
    stop_runtime(runtime);
}

#[test]
#[ignore = "U12 measurement harness; run explicitly in release mode"]
fn transform_scale_baseline() {
    let mut file = output_file("transform_scale_baseline");
    for population in [64, 4_096, 65_536] {
        for scenario in ["static", "sparse", "root_dirty"] {
            run_timed_row(
                &mut file,
                "transform_scale_baseline",
                population,
                scenario,
                false,
            );
        }
    }
    run_timed_row(&mut file, "transform_scale_baseline", 4_096, "sparse", true);
    run_timed_row(
        &mut file,
        "transform_scale_baseline",
        4_096,
        "four_steps_one_pending_edit",
        false,
    );
}

#[test]
#[ignore = "U12 measurement harness; run explicitly in release mode"]
fn hierarchy_scale_baseline() {
    let mut file = output_file("hierarchy_scale_baseline");
    for population in [64, 4_096, 65_536] {
        let mut app = spatial_app(false);
        let baseline = make_fixture(&mut app, population);
        let mut runtime = start_runtime(app);
        for batch_size in [1usize, 8, 32] {
            for independent in [false, true] {
                if independent && batch_size != 8 {
                    continue;
                }
                let scenario = if independent {
                    "independent_leaf_reparent"
                } else {
                    "batch_leaf_reparent"
                };
                let mut fixture = baseline.clone();
                let actors = (0..population)
                    .rev()
                    .filter(|index| fixture.children[*index].is_empty())
                    .take(batch_size)
                    .collect::<Vec<_>>();
                assert_eq!(actors.len(), batch_size);
                let mut iteration = 0usize;
                for _ in 0..WARMUP_SAMPLES {
                    let destination = fixture.roots[iteration % 2];
                    let operations = prepare_leaf_reparent(&mut fixture, &actors, destination);
                    apply_leaf_reparent(&mut runtime, operations, independent);
                    iteration += 1;
                }
                check_hierarchy(runtime.world(), &fixture);
                let mut samples = 0usize;
                let mut measured_time = Duration::ZERO;
                while samples < TIMED_SAMPLES && measured_time < TIME_CAP {
                    let destination = fixture.roots[iteration % 2];
                    let operations = prepare_leaf_reparent(&mut fixture, &actors, destination);
                    let started_sample = Instant::now();
                    apply_leaf_reparent(&mut runtime, operations, independent);
                    let sample_duration = started_sample.elapsed();
                    measured_time += sample_duration;
                    let duration_ns = sample_duration.as_nanos();
                    write_row(
                        &mut file,
                        &format!(
                            "{{\"test\":\"hierarchy_scale_baseline\",\"population\":{population},\"scenario\":\"{scenario}\",\"batch_size\":{batch_size},\"sample\":{samples},\"duration_ns\":{duration_ns},\"transactions\":{},\"destination_root_index\":{destination},\"allocation_calls\":null,\"allocated_bytes\":null}}",
                            if independent { batch_size } else { 1 },
                        ),
                    );
                    samples += 1;
                    iteration += 1;
                    if samples % 64 == 0 {
                        check_hierarchy(runtime.world(), &fixture);
                    }
                }
                check_hierarchy(runtime.world(), &fixture);
                write_row(
                    &mut file,
                    &format!(
                        "{{\"type\":\"summary\",\"test\":\"hierarchy_scale_baseline\",\"population\":{population},\"scenario\":\"{scenario}\",\"batch_size\":{batch_size},\"samples\":{samples},\"required_samples\":{TIMED_SAMPLES},\"incomplete_for_p99\":{}}}",
                        samples < TIMED_SAMPLES
                    ),
                );
                let mut reset_fixture = fixture.clone();
                let mut reset = Vec::with_capacity(batch_size);
                for index in actors.iter().copied().rev() {
                    let parent =
                        baseline.parent_indices[index].expect("baseline leaf must have a parent");
                    reset.extend(prepare_leaf_reparent(&mut reset_fixture, &[index], parent));
                }
                apply_leaf_reparent(&mut runtime, reset, false);
                check_hierarchy(runtime.world(), &reset_fixture);
                assert_eq!(reset_fixture.parent_indices, baseline.parent_indices);
                assert_eq!(reset_fixture.children, baseline.children);
            }
        }
        stop_runtime(runtime);
    }

    let mut app = spatial_app(false);
    let entities = make_chain_fixture(&mut app, 2_048);
    let mut runtime = start_runtime(app);
    let outcome = runtime
        .drive(Duration::ZERO)
        .expect("depth fixture frame should succeed");
    assert!(
        !outcome
            .frame()
            .expect("depth frame status")
            .status
            .fixed_steps_capped
    );
    assert_eq!(
        runtime
            .world()
            .resource::<nara_hierarchy::HierarchyOrder>()
            .len(),
        2_048
    );
    assert!(runtime.world().get::<Children>(entities[0]).is_some());
    assert!(runtime.world().get::<Parent>(entities[1]).is_some());
    write_row(
        &mut file,
        "{\"type\":\"fixture-check\",\"test\":\"hierarchy_scale_baseline\",\"scenario\":\"2048deep\",\"population\":2048,\"timed\":false}",
    );
    stop_runtime(runtime);
}
