use std::{
    collections::{BTreeMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use motion_contract::Actors;
use nara::{
    asset::AssetPath,
    core::{Color, Vec2},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    image::ImagePreparePlugin,
    material::{
        AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dLibrary,
        MaterialParameters, MaterialUpdateError,
    },
    sprite::Sprite,
    sprite_render::SpriteRenderPlugin,
    transform::{SpatialSnapshotStamp, Transform2d, presented_spatial_snapshot},
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_app::{
    App, CoreStage, FixedTime, RealTime, RenderTime, RuntimeCandidateRetirementState,
    RuntimeRetirement, RuntimeStartFailure, RuntimeTimeSettings, VirtualTime,
};
use nara_ecs::{Entity, Res, Resource, World, schedule::IntoScheduleConfigs};
use nara_render::{
    Camera2d, ExtractedViews, RenderComponentsPlugin, RenderFrameTarget, RenderPlugin,
    RenderTarget, ViewportRect,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuRenderInput, WgpuStockCapturePlugin,
    WgpuStockPolicy, WgpuStockPolicyPlugin, WgpuStockSpritePlugin, take_captured_policy_batch,
};

use super::motion::{MotionFrame, MotionRuntime};

pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 16;
pub const VIEW_WIDTH: u32 = 16;
pub const TIMESTEP: Duration = Duration::from_millis(16);

const MATERIAL_ID: &str = "consumer.joint-history";
const MATERIAL_SOURCE: &str = include_str!("joint_material.wgsl");
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    CameraCut,
    Parameters,
    Shader,
    InvalidShader,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub camera_x: f32,
    pub level: f32,
    pub shader_swapped: bool,
    pub rejections: usize,
    pub last_failure: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub input: WgpuRenderInput,
    pub motion: MotionFrame,
    pub state: State,
    pub material: Material2dBinding,
    pub captures: usize,
}

#[derive(MaterialParameters)]
struct Parameters {
    #[material(id = "consumer.joint-history.level")]
    level: f32,
}

#[derive(Resource)]
struct Source {
    cameras: [Entity; 2],
    control: Entity,
    pending: Arc<Mutex<VecDeque<Change>>>,
    state: State,
}

#[derive(Default, Resource)]
struct Captures {
    count: usize,
    latest: Option<WgpuRenderInput>,
}

#[derive(Resource)]
struct WorldDropped(Arc<AtomicBool>);

impl Drop for WorldDropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub struct Scene {
    motion: Option<MotionRuntime>,
    retirement: Option<RuntimeRetirement>,
    startup_failure: Option<RuntimeStartFailure>,
    pending: Arc<Mutex<VecDeque<Change>>>,
    dropped: Arc<AtomicBool>,
}

impl Scene {
    pub fn new() -> Self {
        let mut scene = Self {
            motion: None,
            retirement: None,
            startup_failure: None,
            pending: Arc::new(Mutex::new(VecDeque::new())),
            dropped: Arc::new(AtomicBool::new(false)),
        };
        let pending = Arc::clone(&scene.pending);
        let dropped = Arc::clone(&scene.dropped);
        match MotionRuntime::with_app(|app| configure(app, pending, dropped)) {
            Ok(runtime) => scene.motion = Some(runtime),
            Err(failure) => scene.startup_failure = Some(failure),
        }
        scene
    }

    pub fn drive(&mut self, delta: Duration) -> Sample {
        let runtime = self.runtime_mut();
        let motion = runtime.drive(delta);
        Sample::observe(runtime, motion)
    }

    pub fn pause(&mut self) {
        self.runtime_mut().pause();
    }

    pub fn set_time_scale(&mut self, scale: f32) {
        self.runtime_mut().set_time_scale(scale);
    }

    pub fn exact_step(&mut self) -> (SpatialSnapshotStamp, [Vec2; 2]) {
        let captures = self.capture_count();
        let retained = self.latest_input();
        let result = self.runtime_mut().exact_step();
        assert_eq!(self.capture_count(), captures);
        if let Some(retained) = retained {
            let latest = self.latest_input().unwrap();
            assert!(std::ptr::eq(retained.packet(), latest.packet()));
            assert!(std::ptr::eq(
                retained.captured::<WgpuStockPolicy>().unwrap(),
                latest.captured::<WgpuStockPolicy>().unwrap(),
            ));
        }
        result
    }

    pub fn correct_and_reparent(&mut self, position: Vec2) {
        self.runtime_mut().correct_and_reparent(position);
    }

    pub fn request(&mut self, change: Change) {
        self.pending.lock().unwrap().push_back(change);
    }

    pub fn capture_count(&self) -> usize {
        self.motion
            .as_ref()
            .expect("the original joint Scene Runtime is live")
            .observe(|world| world.resource::<Captures>().count)
    }

    pub fn latest_input(&self) -> Option<WgpuRenderInput> {
        self.motion
            .as_ref()
            .expect("the original joint Scene Runtime is live")
            .observe(|world| world.resource::<Captures>().latest.clone())
    }

    pub fn world_dropped(&self) -> bool {
        self.dropped.load(Ordering::Acquire)
    }

    pub fn close(&mut self) -> Result<(), String> {
        if let Some(runtime) = self.motion.take() {
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
                        "original joint Runtime physical close timed out; owner retained: {retirement}"
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
            if panicked {
                failures.push("original joint Runtime retirement panicked".to_owned());
            }
            if retirement.close_evidence().has_terminal_failure() {
                failures.push(format!(
                    "original joint Runtime teardown failed: {retirement}"
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
                        "joint Runtime startup physical close timed out; owner retained: {failure}"
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
            if panicked {
                failures.push("joint Runtime startup retirement panicked".to_owned());
            }
            if failure.close_evidence().has_terminal_failure() {
                failures.push(format!("joint Runtime startup teardown failed: {failure}"));
            }
        }
        self.startup_failure.take();
        if !self.world_dropped() {
            failures.push("the original joint Runtime World has not physically dropped".to_owned());
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn runtime_mut(&mut self) -> &mut MotionRuntime {
        assert!(
            self.startup_failure.is_none(),
            "joint Scene Runtime startup failed: {:?}",
            self.startup_failure,
        );
        self.motion
            .as_mut()
            .expect("the original joint Scene Runtime is live")
    }
}

fn publish_shader(library: &mut Material2dLibrary, swapped: bool) {
    let path = AssetPath::new("joint_material.wgsl").unwrap();
    let source = if swapped {
        Arc::<str>::from(
            MATERIAL_SOURCE.replace("vec4<f32>(r, g, b, 1.0)", "vec4<f32>(b, g, r, 1.0)"),
        )
    } else {
        Arc::<str>::from(MATERIAL_SOURCE)
    };
    let read = |_: &AssetPath| Ok(Arc::clone(&source));
    let prepared = library.prepare_update(MATERIAL_ID, &[path], read).unwrap();
    library.publish(&prepared, read).unwrap();
}

fn configure(app: &mut App, pending: Arc<Mutex<VecDeque<Change>>>, dropped: Arc<AtomicBool>) {
    app.insert_resource(WorldDropped(dropped)).unwrap();
    let window = Window::new(
        "Joint Material and History",
        WindowResolution::new(WIDTH, HEIGHT),
    );
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
    app.init_resource::<Captures>().unwrap();
    app.add_systems(
        CoreStage::First,
        apply_requests.run_if(|settings: Res<RuntimeTimeSettings>| settings.paused()),
    )
    .unwrap();
    app.add_systems(CoreStage::Update, apply_requests).unwrap();
    app.add_systems(
        CoreStage::Render,
        capture.in_set(WgpuPolicyCaptureSet::Publish),
    )
    .unwrap();
    let mut library = Material2dLibrary::default();
    library
        .declare(
            Material2dDeclaration::from_parameters::<Parameters>(
                MATERIAL_ID,
                1,
                AlphaMode2d::Opaque,
                Vec::new(),
            )
            .unwrap(),
        )
        .unwrap();
    publish_shader(&mut library, false);
    app.insert_resource(library).unwrap();
    let world = app.world_mut().unwrap();
    let actor = world.resource::<Actors>().kinematic;
    let mut sprite = Sprite::from_color(Vec2::splat(0.5), Color::WHITE);
    sprite.material.alpha_mode = AlphaMode2d::Opaque;
    world.entity_mut(actor).insert(sprite);
    let mut control_sprite = Sprite::from_color(Vec2::splat(0.5), Color::rgba(0.0, 0.0, 1.0, 1.0));
    control_sprite.material.alpha_mode = AlphaMode2d::Opaque;
    let control = world
        .spawn((
            control_sprite,
            Transform2d::from_translation(Vec2::new(100.0, 2.0)),
        ))
        .id();
    let cameras = [0.5, 100.0].map(|x| {
        world
            .spawn(Transform2d::from_translation(Vec2::new(x, 2.0)))
            .id()
    });
    for (index, camera) in cameras.into_iter().enumerate() {
        world.entity_mut(camera).insert(Camera2d {
            target,
            viewport: Some(
                ViewportRect::new(index as u32 * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT).unwrap(),
            ),
            clear_color: (index == 0).then_some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
            viewport_height: 2.0,
            order: index as i32,
        });
    }
    let roots = [control, cameras[0], cameras[1]].map(|entity| HierarchyOperation::Place {
        entity,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
    HierarchyWriter::new(world).apply_batch(&roots).unwrap();
    world.insert_resource(Source {
        cameras,
        control,
        pending,
        state: State {
            camera_x: 0.5,
            level: 0.0,
            shader_swapped: false,
            rejections: 0,
            last_failure: None,
        },
    });
    bind_current(world);
}

fn bind_current(world: &mut World) {
    let level = world.resource::<Source>().state.level;
    let binding = world
        .resource::<Material2dLibrary>()
        .current(MATERIAL_ID)
        .unwrap()
        .bind(&Parameters { level }, BTreeMap::new())
        .unwrap();
    let actor = world.resource::<Actors>().kinematic;
    world.entity_mut(actor).insert(binding);
}

fn apply_requests(world: &mut World) {
    let changes: Vec<_> = world
        .resource::<Source>()
        .pending
        .lock()
        .unwrap()
        .drain(..)
        .collect();
    for change in changes {
        match change {
            Change::CameraCut => {
                let camera = world.resource::<Source>().cameras[0];
                world.get_mut::<Transform2d>(camera).unwrap().translation.x = 0.75;
                world.resource_mut::<Source>().state.camera_x = 0.75;
            }
            Change::Parameters => {
                world.resource_mut::<Source>().state.level = 0.125;
                bind_current(world);
            }
            Change::Shader => {
                let swapped = !world.resource::<Source>().state.shader_swapped;
                publish_shader(&mut world.resource_mut::<Material2dLibrary>(), swapped);
                {
                    let mut source = world.resource_mut::<Source>();
                    source.state.shader_swapped = swapped;
                    source.state.last_failure = None;
                }
                bind_current(world);
            }
            Change::InvalidShader => {
                let actor = world.resource::<Actors>().kinematic;
                let binding = world.get::<Material2dBinding>(actor).unwrap().clone();
                let path = AssetPath::new("joint_material.wgsl").unwrap();
                let invalid = Arc::<str>::from("fn material_fragment( { invalid shader }");
                let failure = {
                    let mut library = world.resource_mut::<Material2dLibrary>();
                    let failure = library
                        .prepare_update(MATERIAL_ID, &[path], |_: &AssetPath| {
                            Ok(Arc::clone(&invalid))
                        })
                        .unwrap_err();
                    assert!(matches!(&failure, MaterialUpdateError::Shader(_)));
                    assert_eq!(library.last_failure(MATERIAL_ID), Some(&failure));
                    assert_eq!(library.current(MATERIAL_ID), Some(binding.generation()));
                    failure.to_string()
                };
                {
                    let mut source = world.resource_mut::<Source>();
                    source.state.rejections += 1;
                    source.state.last_failure = Some(failure);
                }
                assert_eq!(world.get::<Material2dBinding>(actor), Some(&binding));
            }
        }
    }
}

fn capture(world: &mut World) {
    let input = take_captured_policy_batch(world)
        .expect("one original sealed stock capture")
        .into_render_input()
        .unwrap()
        .expect("two actual Camera views");
    assert!(input.captured::<WgpuStockPolicy>().is_some());
    let mut captures = world.resource_mut::<Captures>();
    if let Some(previous) = &captures.latest {
        assert!(previous.packet().frame_index() < input.packet().frame_index());
    }
    captures.count += 1;
    captures.latest = Some(input);
}

impl Sample {
    fn observe(runtime: &MotionRuntime, motion: MotionFrame) -> Self {
        runtime.observe(|world| {
            let captures = world.resource::<Captures>();
            let input = captures.latest.clone().expect("the original stock input");
            let source = world.resource::<Source>();
            let actors = world.resource::<Actors>();
            let spatial = presented_spatial_snapshot(world).unwrap();
            assert_eq!(spatial.stamp().unwrap(), motion.stamp);
            assert_eq!(
                spatial.global(actors.kinematic).unwrap().translation(),
                motion.kinematic,
            );
            assert_eq!(
                spatial.global(actors.dynamic).unwrap().translation(),
                motion.dynamic,
            );
            assert_eq!(
                spatial.global(source.control).unwrap().translation(),
                Vec2::new(100.0, 2.0),
            );
            let packet = input.packet();
            assert!(matches!(packet.target(), RenderFrameTarget::Window(_)));
            assert_eq!(
                packet.views(),
                world.resource::<ExtractedViews>().as_slice()
            );
            assert_eq!(packet.views().len(), 2);
            assert_ne!(packet.views()[0].id, packet.views()[1].id);
            assert_eq!(
                packet.source_generation().runtime_generation(),
                motion.stamp.runtime_generation,
            );
            let time = packet.captured_time().expect("the original Extract clocks");
            assert_eq!(time.real(), *world.resource::<RealTime>());
            assert_eq!(time.virtual_time(), *world.resource::<VirtualTime>());
            assert_eq!(time.fixed(), *world.resource::<FixedTime>());
            assert_eq!(time.render(), *world.resource::<RenderTime>());
            assert_eq!(time.settings(), *world.resource::<RuntimeTimeSettings>());
            assert_eq!(time.spatial_stamp(), motion.stamp);
            assert_eq!(time.spatial_interpolated(), motion.spatial_interpolated);
            assert_eq!(time.real().elapsed, motion.real_elapsed);
            assert_eq!(time.virtual_time().elapsed, motion.virtual_elapsed);
            assert_eq!(
                time.render().interpolation_alpha(),
                motion.interpolation_alpha
            );
            assert_eq!(time.settings().paused(), motion.paused);
            let material = world
                .get::<Material2dBinding>(actors.kinematic)
                .unwrap()
                .clone();
            let library = world.resource::<Material2dLibrary>();
            assert_eq!(library.current(MATERIAL_ID), Some(material.generation()));
            assert_eq!(material.uniform_bytes(), &source.state.level.to_le_bytes());
            assert_eq!(
                library.last_failure(MATERIAL_ID).map(ToString::to_string),
                source.state.last_failure,
            );
            let batches = input
                .captured::<WgpuStockPolicy>()
                .unwrap()
                .sprite_batches()
                .unwrap();
            assert_eq!(batches.len(), 2);
            for (index, x) in [source.state.camera_x, 100.0].into_iter().enumerate() {
                let camera = source.cameras[index];
                assert_eq!(
                    world.get::<Transform2d>(camera).unwrap().translation,
                    Vec2::new(x, 2.0),
                );
                assert_eq!(world.get::<Camera2d>(camera).unwrap().viewport_height, 2.0);
                assert_eq!(packet.views()[index].camera_entity, camera);
                assert_eq!(
                    packet.views()[index].viewport,
                    ViewportRect::new(index as u32 * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT).unwrap(),
                );
                let mut view_batches = batches.for_view(index);
                let batch = view_batches.next().unwrap();
                assert!(view_batches.next().is_none());
                assert_eq!(batch.instances.len(), 1);
                if index == 0 {
                    assert_eq!(batch.material_extension.as_ref().unwrap().binding, material);
                    assert!(batch.instances[0].center.abs_diff_eq(
                        Vec2::new(motion.kinematic.x - x, motion.kinematic.y - 2.0),
                        1e-5,
                    ));
                } else {
                    assert!(batch.material_extension.is_none());
                    assert_eq!(batch.instances[0].center, Vec2::ZERO);
                    assert_eq!(batch.instances[0].color, Color::rgba(0.0, 0.0, 1.0, 1.0));
                }
            }
            Self {
                input,
                motion,
                state: source.state.clone(),
                material,
                captures: captures.count,
            }
        })
    }
}
