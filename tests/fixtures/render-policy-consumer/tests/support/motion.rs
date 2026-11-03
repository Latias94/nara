use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use motion_animation::{AnimationPlugin, MotionIntent};
use motion_contract::{Actors, MotionInput, Phase, Trace};
use motion_physics::{PhysicsPlugin, PhysicsSet, Solver, SolverMode};
use nara::{
    core::Vec2,
    hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
        HierarchyWriter,
    },
    reflect::ComponentRegistryPlugin,
    transform::{
        FixedSpatialSnapshot2d, FixedTransformInterpolation, HistoricalSpatialSnapshot2d,
        PresentedSpatialSnapshot2d, SpatialSnapshotStamp, Transform2d, TransformPlugin,
        add_fixed_transform_projection_systems, add_presented_transform_projection_systems,
        fixed_spatial_snapshot, historical_spatial_snapshot,
    },
};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, RealTime, RenderTime, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance, RuntimeTimeSettings, VirtualTime,
};
use nara_ecs::{Entity, Query, Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionFrame {
    pub stamp: SpatialSnapshotStamp,
    pub virtual_elapsed: Duration,
    pub real_elapsed: Duration,
    pub interpolation_alpha: f32,
    pub paused: bool,
    pub kinematic: Vec2,
    pub dynamic: Vec2,
    pub fixed_positions: [Vec2; 2],
}

impl MotionFrame {
    pub fn phase(&self) -> u32 {
        ((self.virtual_elapsed.as_nanos() * 120 / 1_000_000_000) % 64) as u32
    }
}

#[derive(Resource)]
struct Correction(Arc<Mutex<Option<Vec2>>>);

#[derive(Resource)]
struct OtherParent(Entity);

#[derive(Resource)]
struct FixedPositions([Vec2; 2]);

#[derive(Default, Resource)]
struct PresentedFrame(Option<MotionFrame>);

pub struct MotionRuntime {
    runtime: RuntimeInstance,
    correction: Arc<Mutex<Option<Vec2>>>,
}

impl MotionRuntime {
    pub fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, TransformPlugin))
            .unwrap();
        app.add_plugins((
            AnimationPlugin,
            PhysicsPlugin {
                mode: SolverMode::Borrowed,
            },
        ))
        .unwrap();
        let correction = Arc::new(Mutex::new(None));
        let world = app.world_mut().unwrap();
        let sample = world.resource::<Solver>().sample();
        let parent_translation = Vec2::new(10.0, 0.0);
        let parent = world
            .spawn(Transform2d::from_translation(parent_translation))
            .id();
        let other_parent = world
            .spawn(Transform2d::from_translation(Vec2::new(20.0, 0.0)))
            .id();
        let kinematic = world
            .spawn(Transform2d::from_translation(
                sample.kinematic - parent_translation,
            ))
            .id();
        let dynamic = world
            .spawn(Transform2d::from_translation(
                sample.dynamic - parent_translation,
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: parent,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                },
                HierarchyOperation::Place {
                    entity: other_parent,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                },
                HierarchyOperation::Place {
                    entity: kinematic,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
                },
                HierarchyOperation::Place {
                    entity: dynamic,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
                },
            ])
            .unwrap();
        world.insert_resource(Actors {
            parent,
            kinematic,
            dynamic,
        });
        world.insert_resource(OtherParent(other_parent));
        world.insert_resource(Correction(Arc::clone(&correction)));
        world.insert_resource(MotionInput {
            tick: 0,
            delta: Vec2::new(0.25, 0.0),
            parent_translation,
        });
        world.insert_resource(FixedPositions([sample.kinematic, sample.dynamic]));
        world.insert_resource(FixedTransformInterpolation);
        world.init_resource::<Trace>();
        world.init_resource::<PresentedFrame>();
        app.configure_sets(
            CoreStage::FixedUpdate,
            MotionIntent.before(PhysicsSet::Target),
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            apply_correction.in_set(FixedUpdateSet::Prepare),
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            sync_input
                .before(MotionIntent)
                .in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();
        add_fixed_transform_projection_systems(&mut app, observe_fixed).unwrap();
        add_presented_transform_projection_systems(&mut app, observe_presented).unwrap();
        Self {
            runtime: RuntimeInstance::start(app).unwrap(),
            correction,
        }
    }

    pub fn drive(&mut self, delta: Duration) -> MotionFrame {
        self.runtime.drive(delta).unwrap();
        self.runtime.world().resource::<PresentedFrame>().0.unwrap()
    }

    pub fn pause(&mut self) {
        assert!(matches!(
            self.runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        self.runtime.drive(Duration::ZERO).unwrap();
    }

    pub fn exact_step(&mut self) -> (SpatialSnapshotStamp, [Vec2; 2]) {
        let presented = self.runtime.world().resource::<PresentedFrame>().0;
        assert!(matches!(
            self.runtime.request_control(RuntimeControl::StepFixedTick),
            RuntimeControlRequestResult::Accepted(_)
        ));
        self.runtime.drive(Duration::ZERO).unwrap();
        let world = self.runtime.world();
        assert_eq!(world.resource::<PresentedFrame>().0, presented);
        let spatial = fixed_spatial_snapshot(world).unwrap();
        let actors = world.resource::<Actors>();
        (
            spatial.stamp().unwrap(),
            [
                spatial.global(actors.kinematic).unwrap().translation(),
                spatial.global(actors.dynamic).unwrap().translation(),
            ],
        )
    }

    pub fn correct_and_reparent(&mut self, position: Vec2) {
        let mut pending = self.correction.lock().unwrap();
        assert!(pending.is_none(), "a motion correction is already pending");
        *pending = Some(position);
    }

    pub fn finish(self) {
        self.runtime.finish().unwrap();
    }
}

fn apply_correction(world: &mut World) {
    let Some(position) = world.resource::<Correction>().0.lock().unwrap().take() else {
        return;
    };
    let actors = *world.resource::<Actors>();
    let parent = world.resource::<OtherParent>().0;
    let parent_translation = historical_spatial_snapshot(world)
        .unwrap()
        .global(parent)
        .unwrap()
        .translation();
    let entities = [actors.kinematic, actors.dynamic];
    let locals = entities.map(|entity| *world.get::<Transform2d>(entity).unwrap());
    for entity in entities {
        let operation = HierarchyOperation::Move {
            entity,
            from: HierarchyPlacement::current(world, entity).unwrap(),
            to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
        };
        HierarchyWriter::new(world)
            .apply_batch(&[operation])
            .unwrap();
    }
    for (entity, local) in entities.into_iter().zip(locals) {
        assert_eq!(
            *world.get::<Transform2d>(entity).unwrap(),
            local,
            "KeepLocal reparent"
        );
    }
    world.resource_mut::<OtherParent>().0 = actors.parent;
    world.resource_mut::<Actors>().parent = parent;
    world.resource_mut::<Solver>().correct_kinematic(position);
    let sample = world.resource::<Solver>().sample();
    for (entity, position) in [
        (actors.kinematic, sample.kinematic),
        (actors.dynamic, sample.dynamic),
    ] {
        world.get_mut::<Transform2d>(entity).unwrap().translation = position - parent_translation;
    }
}

fn sync_input(
    fixed: Res<FixedTime>,
    actors: Res<Actors>,
    spatial: HistoricalSpatialSnapshot2d,
    mut input: ResMut<MotionInput>,
) {
    input.tick = fixed.tick();
    input.parent_translation = spatial.global(actors.parent).unwrap().translation();
}

fn observe_fixed(
    spatial: FixedSpatialSnapshot2d,
    fixed: Res<FixedTime>,
    actors: Res<Actors>,
    solver: Res<Solver>,
    locals: Query<&Transform2d>,
    mut positions: ResMut<FixedPositions>,
    trace: Res<Trace>,
) {
    let sample = solver.sample();
    assert_eq!(spatial.stamp().unwrap().tick, fixed.tick());
    assert_eq!(sample.tick, fixed.tick());
    assert!(
        sample.contact,
        "the real dynamic body must contact the floor"
    );
    for (index, (entity, expected)) in [
        (actors.kinematic, sample.kinematic),
        (actors.dynamic, sample.dynamic),
    ]
    .into_iter()
    .enumerate()
    {
        let global = spatial.global(entity).unwrap().translation();
        let parent = spatial
            .effective_parent(entity)
            .unwrap()
            .unwrap()
            .translation();
        assert!((global - expected).length() < 1e-5);
        assert!((locals.get(entity).unwrap().translation + parent - expected).length() < 1e-5);
        positions.0[index] = global;
    }
    trace.record(fixed.tick(), Phase::Published);
}

fn observe_presented(
    spatial: PresentedSpatialSnapshot2d,
    actors: Res<Actors>,
    clocks: (
        Res<VirtualTime>,
        Res<RealTime>,
        Res<RenderTime>,
        Res<RuntimeTimeSettings>,
    ),
    fixed: Res<FixedPositions>,
    mut presented: ResMut<PresentedFrame>,
) {
    let (virtual_time, real_time, render_time, settings) = clocks;
    presented.0 = Some(MotionFrame {
        stamp: spatial.stamp().unwrap(),
        virtual_elapsed: virtual_time.elapsed,
        real_elapsed: real_time.elapsed,
        interpolation_alpha: render_time.interpolation_alpha,
        paused: settings.paused(),
        kinematic: spatial.global(actors.kinematic).unwrap().translation(),
        dynamic: spatial.global(actors.dynamic).unwrap().translation(),
        fixed_positions: fixed.0,
    });
}
