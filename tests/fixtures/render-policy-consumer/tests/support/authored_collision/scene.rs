use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use motion_physics::authored::{
    COLLISION_SCHEMA_PROVIDER as COLLISION_SCHEMA, COLLISION_SCHEMA_PROVIDER_ID as PROVIDER,
    CollisionInput, CollisionWorld,
};
use nara::{
    hierarchy::{HIERARCHY_PLUGIN_ID, HierarchyPlugin},
    image::ImagePreparePlugin,
    reflect::{
        COMPONENT_REGISTRY_PLUGIN_ID, ComponentRegistryPlugin, ComponentRegistrySnapshot,
        component_registry, register_schema_provider_for_plugin,
    },
    scene::{SceneDocument, ScenePatchReport},
    sprite_render::SpriteRenderPlugin,
    transform::{
        FixedTransformInterpolation, SpatialSnapshotPhase, TRANSFORM_PLUGIN_ID,
        TRANSFORM_SCHEMA_PROVIDER, Transform2d, TransformPlugin, add_fixed_transform_input_systems,
        add_fixed_transform_projection_systems, fixed_input_spatial_snapshot,
        fixed_spatial_snapshot, presented_spatial_snapshot,
    },
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_app::{
    App, CoreStage, ExecutionProfile, FixedTime, FixedUpdateSet, Plugin, PluginCategory,
    PluginDeclaration, PluginError, PluginId, PluginShutdownObligationId, RealTime, RenderTime,
    RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlRequestResult, RuntimeFrameKind, RuntimeInstance, RuntimeRetirement,
    RuntimeStartFailure, RuntimeState, RuntimeTimeSettings, StartupStage, VirtualTime,
};
use nara_ecs::{Resource, World, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs};
use nara_render::{
    ExtractedViews, RENDER_SCHEMA_PROVIDER, RenderComponentsPlugin, RenderFrameTarget,
    RenderPlugin, RenderTarget,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuRenderInput, WgpuStockCapturePlugin,
    WgpuStockPolicy, WgpuStockPolicyPlugin, WgpuStockSpritePlugin, take_captured_policy_batch,
};

use super::{
    model::{FixedFact, HEIGHT, Sample, TIMESTEP, Variant, WIDTH},
    source::{Source, SourceHandle, document, synchronize},
};

const PLUGIN: PluginId = PluginId::new("fixture.render.authored-collision");
const SHUTDOWN: PluginShutdownObligationId =
    PluginShutdownObligationId::new("fixture.render.authored-collision.shutdown");
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Resource)]
struct NativeOwner(Arc<Mutex<CollisionWorld>>);

#[derive(Default, Resource)]
struct LatestFixed(Option<FixedFact>);

#[derive(Default)]
struct CaptureLog {
    count: usize,
    latest: Option<WgpuRenderInput>,
}

#[derive(Clone, Default, Resource)]
struct Captures(Arc<Mutex<CaptureLog>>);

#[derive(Resource)]
struct WorldDropped(Arc<AtomicBool>);

impl Drop for WorldDropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct CollisionPlugin {
    native: NativeOwner,
    source: SourceHandle,
}

impl Plugin for CollisionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PLUGIN, PluginCategory::Runtime)
                .requires_plugins(&[
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    HIERARCHY_PLUGIN_ID,
                    TRANSFORM_PLUGIN_ID,
                ])
                .provides_schema(&[PROVIDER])
                .shutdown_obligations(&[SHUTDOWN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        register_schema_provider_for_plugin(app, PLUGIN, "CollisionDefinition", &COLLISION_SCHEMA)?;
        app.insert_resource(self.native.clone())?
            .insert_resource(self.source.clone())?
            .init_resource::<LatestFixed>()?
            .add_systems(StartupStage::Scene, synchronize_source)?
            .add_systems(CoreStage::First, synchronize_source)?
            .add_systems(
                CoreStage::FixedUpdate,
                step.in_set(FixedUpdateSet::Simulate),
            )?;
        add_fixed_transform_input_systems(app, reconcile_inputs)?;
        add_fixed_transform_projection_systems(app, record_fixed)?;
        app.register_plugin_runtime_close_participant(
            SHUTDOWN,
            RuntimeCloseParticipantId::new("fixture.render.authored-collision.close"),
            CollisionClose(Arc::clone(&self.native.0)),
        )?;
        Ok(())
    }
}

struct CollisionClose(Arc<Mutex<CollisionWorld>>);

impl RuntimeCloseParticipant for CollisionClose {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .close();
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begin_close(context)
    }
}

pub struct Scene {
    runtime: Option<RuntimeInstance>,
    retirement: Option<RuntimeRetirement>,
    startup_failure: Option<RuntimeStartFailure>,
    native: Arc<Mutex<CollisionWorld>>,
    source: Arc<Mutex<Source>>,
    captures: Captures,
    dropped: Arc<AtomicBool>,
}

impl Scene {
    pub fn new() -> Self {
        Self::from_document(document(Variant::Initial), Variant::Initial)
    }

    pub fn from_document(document: SceneDocument, variant: Variant) -> Self {
        let mut scene = Self {
            runtime: None,
            retirement: None,
            startup_failure: None,
            native: Arc::new(Mutex::new(CollisionWorld::new())),
            source: Arc::new(Mutex::new(Source::new(document, variant))),
            captures: Captures::default(),
            dropped: Arc::new(AtomicBool::new(false)),
        };
        let mut app = App::new();
        #[cfg(feature = "production-parallel")]
        {
            let pool = bevy_tasks::ComputeTaskPool::get_or_init(|| {
                bevy_tasks::TaskPoolBuilder::new().num_threads(2).build()
            });
            assert_eq!(pool.thread_num(), 2);
            app.set_execution_profile(ExecutionProfile::Parallel)
                .unwrap();
        }
        #[cfg(not(feature = "production-parallel"))]
        app.set_execution_profile(ExecutionProfile::Serial).unwrap();
        app.insert_resource(WorldDropped(Arc::clone(&scene.dropped)))
            .unwrap();
        app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, TransformPlugin))
            .unwrap();
        let window = Window::new("Authored Collision", WindowResolution::new(WIDTH, HEIGHT));
        let target = RenderTarget::Window(window.id);
        app.add_plugins((
            WindowPlugin {
                primary_window: Some(window),
            },
            RenderComponentsPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
            SpriteRenderPlugin,
            WgpuCapturePlugin,
            WgpuStockCapturePlugin,
            WgpuStockPolicyPlugin,
        ))
        .unwrap();
        app.add_plugin(WgpuStockSpritePlugin).unwrap();
        app.insert_resource(FixedTime::new(TIMESTEP).unwrap())
            .unwrap();
        app.insert_resource(FixedTransformInterpolation).unwrap();
        app.insert_resource(scene.captures.clone()).unwrap();
        app.add_plugin(CollisionPlugin {
            native: NativeOwner(Arc::clone(&scene.native)),
            source: SourceHandle {
                state: Arc::clone(&scene.source),
                target,
            },
        })
        .unwrap();
        app.add_systems(
            CoreStage::Render,
            capture.in_set(WgpuPolicyCaptureSet::Publish),
        )
        .unwrap();
        match RuntimeInstance::start(app) {
            Ok(runtime) => scene.runtime = Some(runtime),
            Err(failure) => scene.startup_failure = Some(failure),
        }
        scene
    }

    pub fn drive(&mut self, delta: Duration) -> Sample {
        self.runtime_mut().drive(delta).unwrap();
        self.sample()
    }

    pub fn pause(&mut self) {
        assert!(matches!(
            self.runtime_mut().request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        self.runtime_mut().drive(Duration::ZERO).unwrap();
        assert_eq!(self.runtime().state(), RuntimeState::Paused);
        assert!(
            self.runtime()
                .world()
                .resource::<RuntimeTimeSettings>()
                .paused()
        );
    }

    pub fn resume(&mut self) {
        assert!(matches!(
            self.runtime_mut().request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        self.runtime_mut().drive(Duration::ZERO).unwrap();
        assert_eq!(self.runtime().state(), RuntimeState::Running);
    }

    pub fn exact_step(&mut self) -> u64 {
        let dirty = self.source.lock().unwrap().session.is_live_dirty();
        assert!(
            !dirty,
            "drive an ordinary paused maintenance frame before stepping an edited source"
        );
        assert_eq!(self.runtime().state(), RuntimeState::Paused);
        let tick = self.runtime().world().resource::<FixedTime>().tick();
        let captures = self.captures();
        let retained = self.latest_input();
        assert!(matches!(
            self.runtime_mut()
                .request_control(RuntimeControl::StepFixedTick),
            RuntimeControlRequestResult::Accepted(_)
        ));
        let outcome = self.runtime_mut().drive(Duration::ZERO).unwrap();
        assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
        assert_eq!(self.runtime().state(), RuntimeState::Paused);
        let completed = self.runtime().world().resource::<FixedTime>().tick();
        assert_eq!(completed, tick + 1);
        assert_eq!(self.captures(), captures);
        match (retained, self.latest_input()) {
            (Some(previous), Some(current)) => {
                assert!(std::ptr::eq(previous.packet(), current.packet()));
                assert!(std::ptr::eq(
                    previous.captured::<WgpuStockPolicy>().unwrap(),
                    current.captured::<WgpuStockPolicy>().unwrap()
                ));
            }
            (None, None) => {}
            _ => panic!("an exact fixed step changed the original Render capture"),
        }
        assert_eq!(
            self.runtime()
                .world()
                .resource::<LatestFixed>()
                .0
                .as_ref()
                .unwrap()
                .result_stamp
                .tick,
            completed
        );
        completed
    }

    pub fn edit(&mut self, variant: Variant) -> ScenePatchReport {
        let registry = self.registry();
        self.source.lock().unwrap().edit(variant, &registry)
    }

    pub fn undo(&mut self) -> ScenePatchReport {
        let registry = self.registry();
        self.source.lock().unwrap().undo(&registry)
    }

    pub fn redo(&mut self) -> ScenePatchReport {
        let registry = self.registry();
        self.source.lock().unwrap().redo(&registry)
    }

    pub fn reject_invalid_edit(&mut self) -> ScenePatchReport {
        let registry = self.registry();
        let tick = self.runtime().world().resource::<FixedTime>().tick();
        let captures = self.captures();
        let counts = self.native_counts();
        let debug = self.native.lock().unwrap().debug_snapshot();
        let report = self.source.lock().unwrap().reject_invalid_edit(&registry);
        assert_eq!(self.runtime().world().resource::<FixedTime>().tick(), tick);
        assert_eq!(self.captures(), captures);
        assert_eq!(self.native_counts(), counts);
        assert!(Arc::ptr_eq(
            &debug,
            &self.native.lock().unwrap().debug_snapshot()
        ));
        report
    }

    pub fn document(&self) -> SceneDocument {
        self.source.lock().unwrap().session.document().clone()
    }

    pub fn registry(&self) -> ComponentRegistrySnapshot {
        component_registry(self.runtime().world()).unwrap().clone()
    }

    pub fn captures(&self) -> usize {
        self.captures.0.lock().unwrap().count
    }

    pub fn capture_count(&self) -> usize {
        self.captures()
    }

    pub fn latest_input(&self) -> Option<WgpuRenderInput> {
        self.captures.0.lock().unwrap().latest.clone()
    }

    pub fn native_counts(&self) -> (usize, usize) {
        self.native
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .counts()
    }

    pub fn world_dropped(&self) -> bool {
        self.dropped.load(Ordering::Acquire)
    }

    pub fn close(&mut self) -> Result<(), String> {
        if let Some(runtime) = self.runtime.take() {
            self.retirement = Some(runtime.begin_retirement());
        }
        let deadline = Instant::now() + CLOSE_TIMEOUT;
        let mut failures = Vec::new();
        if let Some(retirement) = &mut self.retirement {
            let mut panicked = false;
            while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
                panicked |=
                    catch_unwind(AssertUnwindSafe(|| retirement.drive_retirement())).is_err();
                if Instant::now() >= deadline {
                    return Err(format!(
                        "original authored collision Runtime close timed out; owner retained: {retirement}"
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
            if panicked {
                failures.push("original authored collision Runtime retirement panicked".to_owned());
            }
            if retirement.close_evidence().has_terminal_failure() {
                failures.push(format!(
                    "original authored collision Runtime teardown failed: {retirement}"
                ));
            }
        }
        self.retirement.take();
        if let Some(failure) = &mut self.startup_failure {
            let mut panicked = false;
            while failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
                panicked |= catch_unwind(AssertUnwindSafe(|| failure.drive_retirement())).is_err();
                if Instant::now() >= deadline {
                    return Err(format!(
                        "authored collision startup close timed out; owner retained: {failure}"
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
            if panicked {
                failures.push("authored collision startup retirement panicked".to_owned());
            }
            if failure.close_evidence().has_terminal_failure() {
                failures.push(format!(
                    "authored collision startup teardown failed: {failure}"
                ));
            }
        }
        self.startup_failure.take();
        if !self.world_dropped() {
            failures.push(
                "the original authored collision World has not physically dropped".to_owned(),
            );
        }
        if self.native_counts() != (0, 0) {
            failures.push(
                "the original native collision owner still retains bodies or colliders".to_owned(),
            );
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn sample(&self) -> Sample {
        let world = self.runtime().world();
        let source = self.source.lock().unwrap();
        let entities = source.entities(world).unwrap();
        source.assert_installed(world, entities);
        let (authored, revision) = source.installed.unwrap();
        let presented = presented_spatial_snapshot(world).unwrap();
        let stamp = presented.stamp().unwrap();
        let body = *world.get::<Transform2d>(entities.body).unwrap();
        let body_presented = presented.global(entities.body).unwrap().matrix();
        let platform_presented = presented.global(entities.platform).unwrap().matrix();
        let input = self
            .latest_input()
            .expect("the original stock Sprite capture");
        let packet = input.packet();
        assert!(matches!(packet.target(), RenderFrameTarget::Window(_)));
        assert_eq!(
            packet.views(),
            world.resource::<ExtractedViews>().as_slice()
        );
        assert_eq!(packet.views().len(), 1);
        assert_eq!(packet.views()[0].camera_entity, entities.camera);
        assert_eq!(
            packet.source_generation().runtime_generation(),
            stamp.runtime_generation
        );
        let time = packet.captured_time().unwrap();
        assert_eq!(time.real(), *world.resource::<RealTime>());
        assert_eq!(time.virtual_time(), *world.resource::<VirtualTime>());
        assert_eq!(time.fixed(), *world.resource::<FixedTime>());
        assert_eq!(time.render(), *world.resource::<RenderTime>());
        assert_eq!(time.settings(), *world.resource::<RuntimeTimeSettings>());
        assert_eq!(time.spatial_stamp(), stamp);
        let frame = world.resource::<RealTime>().frame;
        assert_eq!(stamp.frame, frame);
        let native = self.native.lock().unwrap();
        Sample {
            input,
            authored,
            body,
            body_presented,
            platform_presented,
            render_time: *world.resource::<RenderTime>(),
            paused: world.resource::<RuntimeTimeSettings>().paused(),
            tick: world.resource::<FixedTime>().tick(),
            frame,
            captures: self.captures(),
            fixed: world.resource::<LatestFixed>().0.clone(),
            body_entity: entities.body,
            platform_entity: entities.platform,
            contact: native.contact_between(entities.platform, entities.body),
            native_counts: native.counts(),
            debug: native.debug_snapshot(),
            revision,
        }
    }

    fn runtime(&self) -> &RuntimeInstance {
        assert!(
            self.startup_failure.is_none(),
            "authored collision startup failed: {:?}",
            self.startup_failure
        );
        self.runtime
            .as_ref()
            .expect("the original authored collision Runtime is live")
    }

    fn runtime_mut(&mut self) -> &mut RuntimeInstance {
        assert!(
            self.startup_failure.is_none(),
            "authored collision startup failed: {:?}",
            self.startup_failure
        );
        self.runtime
            .as_mut()
            .expect("the original authored collision Runtime is live")
    }
}

fn reconcile_inputs(world: &mut World) -> Result<(), BevyError> {
    validate_receipts(world)?;
    let source = Arc::clone(&world.resource::<SourceHandle>().state);
    let source = source.lock().unwrap();
    assert!(!source.session.is_live_dirty());
    let native = Arc::clone(&world.resource::<NativeOwner>().0);
    native
        .lock()
        .unwrap()
        .reconcile(
            world,
            &[CollisionInput {
                instance: source.session.live_instance().unwrap(),
                revision: source.installed.unwrap().1,
            }],
        )
        .map_err(|report| {
            BevyError::error(std::io::Error::other(format!(
                "original collision admission refused: {report:?}"
            )))
        })
}

fn synchronize_source(world: &mut World) -> Result<(), BevyError> {
    validate_receipts(world)?;
    synchronize(world)
}

fn validate_receipts(world: &World) -> Result<(), BevyError> {
    let registry = component_registry(world).ok_or_else(|| {
        BevyError::error(std::io::Error::other(
            "the finished Schema registry is missing",
        ))
    })?;
    for provider in [
        COLLISION_SCHEMA,
        TRANSFORM_SCHEMA_PROVIDER,
        RENDER_SCHEMA_PROVIDER,
    ] {
        if registry.provider_receipt(provider.id()) != Some(provider.receipt()) {
            return Err(BevyError::error(std::io::Error::other(format!(
                "the finished Schema registry lacks the exact provider receipt: {:?}",
                provider.receipt(),
            ))));
        }
    }
    Ok(())
}

fn step(world: &mut World) -> Result<(), BevyError> {
    let native = Arc::clone(&world.resource::<NativeOwner>().0);
    native
        .lock()
        .unwrap()
        .step_and_writeback(world)
        .map_err(|report| {
            BevyError::error(std::io::Error::other(format!(
                "original collision writeback refused: {report:?}"
            )))
        })
}

fn record_fixed(world: &mut World) {
    let source = Arc::clone(&world.resource::<SourceHandle>().state);
    let source = source.lock().unwrap();
    let entities = source.entities(world).unwrap();
    let input = fixed_input_spatial_snapshot(world).unwrap();
    let result = fixed_spatial_snapshot(world).unwrap();
    let input_stamp = input.stamp().unwrap();
    let result_stamp = result.stamp().unwrap();
    assert_eq!(input_stamp.phase, SpatialSnapshotPhase::FixedInput);
    assert_eq!(result_stamp.phase, SpatialSnapshotPhase::Fixed);
    assert_eq!(input_stamp.tick, result_stamp.tick);
    assert_eq!(input_stamp.frame, result_stamp.frame);
    assert_ne!(input_stamp.publication, result_stamp.publication);
    let native = Arc::clone(&world.resource::<NativeOwner>().0);
    let native = native.lock().unwrap();
    let debug = native.debug_snapshot();
    assert!(debug.solved);
    assert_eq!(debug.input_stamp, Some(input_stamp));
    assert_eq!(debug.shapes.len(), 2);
    assert_eq!(native.counts(), (2, 2));
    assert!(
        debug
            .shapes
            .iter()
            .all(|shape| shape.origin.revision == source.installed.unwrap().1)
    );
    let fact = FixedFact {
        body_entity: entities.body,
        input_stamp,
        result_stamp,
        body_before: input.local(entities.body).unwrap(),
        body_after: *world.get::<Transform2d>(entities.body).unwrap(),
        body_global: result.global(entities.body).unwrap().matrix(),
        debug,
    };
    assert_eq!(fact.body_global, fact.body_after.matrix());
    world.resource_mut::<LatestFixed>().0 = Some(fact);
}

fn capture(world: &mut World) {
    let input = take_captured_policy_batch(world)
        .expect("one genuine sealed stock batch in Publish")
        .into_render_input()
        .unwrap()
        .expect("the original Scene Camera and domain Window target");
    assert!(input.captured::<WgpuStockPolicy>().is_some());
    let captures = world.resource::<Captures>();
    let mut captures = captures.0.lock().unwrap();
    if let Some(previous) = &captures.latest {
        assert!(previous.packet().frame_index() < input.packet().frame_index());
    }
    captures.count += 1;
    captures.latest = Some(input);
}
