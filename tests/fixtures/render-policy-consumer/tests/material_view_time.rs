#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
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
    core::{Color, Mat3, Vec2},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    image::ImagePreparePlugin,
    material::{
        AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dLibrary,
        MaterialParameters,
    },
    sprite::Sprite,
    sprite_render::SpriteRenderPlugin,
    transform::Transform2d,
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_app::{
    App, CoreStage, FixedTime, RealTime, RenderTime, RuntimeCandidateRetirementState,
    RuntimeRetirement, RuntimeStartFailure, RuntimeTimeSettings, VirtualTime,
};
use nara_ecs::{Entity, Resource, World, schedule::IntoScheduleConfigs};
use nara_render::{
    Camera2d, ExtractedViews, RenderComponentsPlugin, RenderFrameTarget, RenderImagePacket,
    RenderPlugin, RenderTarget, ViewportRect,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuImageOutput, WgpuPolicyCaptureSet, WgpuRenderHost, WgpuRenderInput,
    WgpuRenderProducer, WgpuStockCapturePlugin, WgpuStockPolicy, WgpuStockPolicyPlugin,
    WgpuStockSpritePlugin, WgpuSubmissionPrefix, WgpuWorldColor, take_captured_policy_batch,
};

#[path = "support/material_readback.rs"]
mod material_readback;
#[path = "support/motion.rs"]
mod motion;
pub mod support;

use motion::{MotionFrame, MotionRuntime};
use support::image::{self, NATIVE_TIMEOUT};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 24;
const VIEW_WIDTH: u32 = WIDTH / 2;
const TIMESTEP: Duration = Duration::from_millis(16);
const MATERIAL_ID: &str = "consumer.captured-view-time";

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraConfiguration {
    x: f64,
    y: f64,
    viewport_height: f64,
}

const INITIAL_CAMERAS: [CameraConfiguration; 2] = [
    CameraConfiguration {
        x: 0.5,
        y: 2.0,
        viewport_height: 1.5,
    },
    CameraConfiguration {
        x: 0.625,
        y: 2.125,
        viewport_height: 1.5,
    },
];
const CHANGED_CAMERAS: [CameraConfiguration; 2] = [
    CameraConfiguration {
        x: 0.625,
        y: 2.0625,
        viewport_height: 0.75,
    },
    CameraConfiguration {
        x: 0.75,
        y: 2.125,
        viewport_height: 0.75,
    },
];

impl CameraConfiguration {
    fn pixels_per_unit(self) -> f64 {
        f64::from(HEIGHT) / self.viewport_height
    }

    fn probe_bytes(self) -> [u8; 2] {
        let width = self.viewport_height * f64::from(VIEW_WIDTH) / f64::from(HEIGHT);
        let unprojected = [self.x + width * 0.25, self.y - self.viewport_height * 0.125];
        let projected = [
            (0.75 - self.x) * 2.0 / width,
            (2.375 - self.y) * 2.0 / self.viewport_height,
        ];
        [
            32.0 + (unprojected[0] * 64.0 + unprojected[1] * 32.0).round(),
            64.0 + (projected[0] * 32.0 + projected[1] * 24.0).round(),
        ]
        .map(|value| {
            assert!((0.0..=255.0).contains(&value));
            value as u8
        })
    }
}

#[derive(Resource)]
struct CameraState {
    entities: [Entity; 2],
    current: [CameraConfiguration; 2],
    pending: Arc<Mutex<Option<[CameraConfiguration; 2]>>>,
}

#[derive(MaterialParameters)]
struct Parameters {
    #[material(id = "consumer.gain")]
    gain: f32,
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

#[derive(Clone, Copy, Debug)]
struct Clocks {
    real: RealTime,
    virtual_time: VirtualTime,
    fixed: FixedTime,
    render: RenderTime,
    settings: RuntimeTimeSettings,
    interpolated: bool,
}

#[derive(Clone)]
struct Sample {
    input: WgpuRenderInput,
    motion: MotionFrame,
    clocks: Clocks,
    cameras: [CameraConfiguration; 2],
    capture_count: usize,
}

fn material() -> Material2dBinding {
    let mut library = Material2dLibrary::default();
    let declaration = Material2dDeclaration::from_parameters::<Parameters>(
        MATERIAL_ID,
        1,
        AlphaMode2d::Opaque,
        Vec::new(),
    )
    .unwrap();
    library.declare(declaration).unwrap();
    let path = AssetPath::new("captured-view-time.wgsl").unwrap();
    let source = |_: &AssetPath| {
        Ok(Arc::<str>::from(include_str!(
            "support/material_view_time.wgsl"
        )))
    };
    let prepared = library
        .prepare_update(MATERIAL_ID, &[path], source)
        .unwrap();
    let generation = library.publish(&prepared, source).unwrap();
    generation
        .bind(&Parameters { gain: 1.0 }, BTreeMap::new())
        .unwrap()
}

fn configure(
    app: &mut App,
    dropped: Arc<AtomicBool>,
    camera_pending: Arc<Mutex<Option<[CameraConfiguration; 2]>>>,
) {
    let window = Window::new(
        "Ordinary Material captured View/time",
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
    app.insert_resource(WorldDropped(dropped)).unwrap();
    app.init_resource::<Captures>().unwrap();
    app.add_systems(CoreStage::Update, update_cameras).unwrap();
    app.add_systems(
        CoreStage::Render,
        capture.in_set(WgpuPolicyCaptureSet::Publish),
    )
    .unwrap();
    let binding = material();
    let world = app.world_mut().unwrap();
    let actor = world.resource::<Actors>().kinematic;
    let mut sprite = Sprite::from_color(Vec2::new(0.125, 0.25), Color::WHITE);
    sprite.material.alpha_mode = AlphaMode2d::Opaque;
    world.entity_mut(actor).insert((sprite, binding));
    let mut cameras = Vec::new();
    for (index, camera_configuration) in INITIAL_CAMERAS.into_iter().enumerate() {
        let camera = world
            .spawn((
                Camera2d {
                    target,
                    viewport: Some(
                        ViewportRect::new(index as u32 * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT)
                            .unwrap(),
                    ),
                    clear_color: (index == 0).then_some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                    viewport_height: camera_configuration.viewport_height as f32,
                    order: index as i32,
                },
                Transform2d::from_translation(Vec2::new(
                    camera_configuration.x as f32,
                    camera_configuration.y as f32,
                )),
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Place {
                entity: camera,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }])
            .unwrap();
        cameras.push(camera);
    }
    world.insert_resource(CameraState {
        entities: cameras.try_into().unwrap(),
        current: INITIAL_CAMERAS,
        pending: camera_pending,
    });
}

fn update_cameras(world: &mut World) {
    let requested = world
        .resource::<CameraState>()
        .pending
        .lock()
        .unwrap()
        .take();
    let Some(configurations) = requested else {
        return;
    };
    let entities = world.resource::<CameraState>().entities;
    for (entity, configuration) in entities.into_iter().zip(configurations) {
        world.get_mut::<Transform2d>(entity).unwrap().translation =
            Vec2::new(configuration.x as f32, configuration.y as f32);
        world.get_mut::<Camera2d>(entity).unwrap().viewport_height =
            configuration.viewport_height as f32;
    }
    world.resource_mut::<CameraState>().current = configurations;
}

fn capture(world: &mut World) {
    let batch = take_captured_policy_batch(world).expect("one original sealed stock capture");
    assert!(take_captured_policy_batch(world).is_none());
    let input = batch
        .into_render_input()
        .unwrap()
        .expect("two actual Camera views");
    let mut captures = world.resource_mut::<Captures>();
    if let Some(previous) = &captures.latest {
        assert!(previous.packet().frame_index() < input.packet().frame_index());
    }
    captures.count += 1;
    captures.latest = Some(input);
}

impl Sample {
    fn observe(runtime: &MotionRuntime, motion: MotionFrame) -> Self {
        let sample = runtime.observe(|world| {
            let captures = world.resource::<Captures>();
            let input = captures
                .latest
                .clone()
                .expect("the actual extracted stock input");
            let cameras = world.resource::<CameraState>();
            for (entity, configuration) in cameras.entities.into_iter().zip(cameras.current) {
                assert_eq!(
                    world.get::<Transform2d>(entity).unwrap().translation,
                    Vec2::new(configuration.x as f32, configuration.y as f32)
                );
                assert_eq!(
                    world.get::<Camera2d>(entity).unwrap().viewport_height,
                    configuration.viewport_height as f32
                );
            }
            assert_eq!(
                input.packet().views(),
                world.resource::<ExtractedViews>().as_slice()
            );
            Self {
                input,
                motion,
                clocks: Clocks {
                    real: *world.resource::<RealTime>(),
                    virtual_time: *world.resource::<VirtualTime>(),
                    fixed: *world.resource::<FixedTime>(),
                    render: *world.resource::<RenderTime>(),
                    settings: *world.resource::<RuntimeTimeSettings>(),
                    interpolated: motion.spatial_interpolated,
                },
                cameras: cameras.current,
                capture_count: captures.count,
            }
        });
        sample.assert_original();
        sample
    }

    fn assert_original(&self) {
        let packet = self.input.packet();
        assert!(matches!(packet.target(), RenderFrameTarget::Window(_)));
        assert_eq!(
            packet.source_generation().runtime_generation(),
            self.motion.stamp.runtime_generation
        );
        let time = packet
            .captured_time()
            .expect("the original Extract time sample");
        assert_eq!(time.real(), self.clocks.real);
        assert_eq!(time.virtual_time(), self.clocks.virtual_time);
        assert_eq!(time.fixed(), self.clocks.fixed);
        assert_eq!(time.render(), self.clocks.render);
        assert_eq!(time.settings(), self.clocks.settings);
        assert_eq!(time.spatial_stamp(), self.motion.stamp);
        assert_eq!(time.spatial_interpolated(), self.clocks.interpolated);
        assert_eq!(time.real().elapsed, self.motion.real_elapsed);
        assert_eq!(time.virtual_time().elapsed, self.motion.virtual_elapsed);
        assert_eq!(
            time.render().interpolation_alpha,
            self.motion.interpolation_alpha
        );
        assert_eq!(time.settings().paused(), self.motion.paused);
        assert_eq!(packet.views().len(), 2);
        assert_ne!(packet.views()[0].id, packet.views()[1].id);
        assert_ne!(
            packet.views()[0].camera_entity,
            packet.views()[1].camera_entity
        );
        assert_ne!(packet.views()[0].mapping, packet.views()[1].mapping);
        let batches = self
            .input
            .captured::<WgpuStockPolicy>()
            .unwrap()
            .sprite_batches()
            .unwrap();
        assert_eq!(batches.len(), 2);
        for (index, view) in packet.views().iter().enumerate() {
            let configuration = self.cameras[index];
            let camera_x = configuration.x;
            let camera_y = configuration.y;
            assert_eq!(
                view.viewport,
                ViewportRect::new(index as u32 * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT).unwrap()
            );
            let scale_y = 2.0 / configuration.viewport_height as f32;
            let scale_x = scale_y * HEIGHT as f32 / VIEW_WIDTH as f32;
            let projection = Mat3::from_scale(Vec2::new(scale_x, scale_y))
                * Mat3::from_translation(Vec2::new(-camera_x as f32, -camera_y as f32));
            assert!(
                view.mapping
                    .world_to_clip_matrix()
                    .abs_diff_eq(projection, 1e-6)
            );
            assert!(
                view.mapping
                    .clip_to_world_matrix()
                    .abs_diff_eq(projection.inverse(), 1e-6)
            );
            let batch = batches.for_view(index).next().unwrap();
            assert_eq!(batch.instances.len(), 1);
            let material = batch
                .material_extension
                .as_ref()
                .expect("an ordinary Material binding");
            assert_eq!(
                material.binding.generation().declaration().id(),
                MATERIAL_ID
            );
            assert!(material.binding.textures().is_empty());
            assert_eq!(material.binding.uniform_bytes(), &1.0_f32.to_le_bytes());
            let expected_center = Vec2::new(
                (self.motion.kinematic.x - camera_x as f32) * scale_x,
                (self.motion.kinematic.y - camera_y as f32) * scale_y,
            );
            assert!(batch.instances[0].center.abs_diff_eq(expected_center, 1e-5));
        }
    }

    fn expected(&self) -> Vec<[u8; 4]> {
        let mut pixels = vec![[0; 4]; (WIDTH * HEIGHT) as usize];
        for (index, configuration) in self.cameras.into_iter().enumerate() {
            let camera_x = configuration.x;
            let camera_y = configuration.y;
            let pixels_per_unit = configuration.pixels_per_unit();
            let center_x = index as f64 * f64::from(VIEW_WIDTH)
                + f64::from(VIEW_WIDTH) / 2.0
                + (f64::from(self.motion.kinematic.x) - camera_x) * pixels_per_unit;
            let center_y = f64::from(HEIGHT) / 2.0
                - (f64::from(self.motion.kinematic.y) - camera_y) * pixels_per_unit;
            assert!((center_x - center_x.round()).abs() < 1e-4);
            assert!((center_y - center_y.round()).abs() < 1e-4);
            let width = (0.125 * pixels_per_unit).round() as u32;
            let height = (0.25 * pixels_per_unit).round() as u32;
            assert!(width >= 2 && width % 2 == 0);
            assert!(height >= 4 && height % 4 == 0);
            let left = center_x.round() as u32 - width / 2;
            let top = center_y.round() as u32 - height / 2;
            assert!(
                left >= index as u32 * VIEW_WIDTH
                    && left + width <= (index as u32 + 1) * VIEW_WIDTH
            );
            assert!(top + height <= HEIGHT);
            let probes = configuration.probe_bytes();
            let viewport_byte = ((VIEW_WIDTH + HEIGHT) / 2 + index as u32 * VIEW_WIDTH / 2) as u8;
            let clock = self.clocks;
            let rows = [
                [
                    time_byte(clock.real.elapsed),
                    time_byte(clock.virtual_time.elapsed),
                    0,
                    255,
                ],
                [
                    time_byte(clock.real.delta),
                    time_byte(clock.virtual_time.delta),
                    (16 + 7 * (clock.fixed.tick() % 32)) as u8 + viewport_byte,
                    255,
                ],
                [
                    time_byte(clock.fixed.elapsed()),
                    time_byte(clock.fixed.delta()),
                    time_byte(clock.fixed.timestep()),
                    255,
                ],
                [
                    (16 + 11 * (clock.real.frame % 16)) as u8,
                    (16 + 11 * (clock.virtual_time.frame % 16)) as u8,
                    16 + 64 * u8::from(clock.interpolated)
                        + 32 * u8::from(clock.settings.paused())
                        + (clock.settings.time_scale() * 16.0).round() as u8
                        + (clock.render.interpolation_alpha * 8.0).round() as u8
                        + 4 * clock.fixed.steps_this_frame() as u8,
                    255,
                ],
            ];
            for row in 0..height {
                for column in 0..width {
                    let band = (row * 4 / height) as usize;
                    let mut expected = rows[band];
                    if band == 0 {
                        expected[2] = probes[usize::from(column >= width / 2)];
                    }
                    pixels[((top + row) * WIDTH + left + column) as usize] = expected;
                }
            }
        }
        pixels
    }
}

fn time_byte(elapsed: Duration) -> u8 {
    let phase = (elapsed.as_nanos() * 2048 + 1_000_000_000) / 8_000_000_000;
    16 + 3 * (phase % 64) as u8
}

struct Record {
    sample: Sample,
    output: WgpuImageOutput,
    prefix: WgpuSubmissionPrefix,
    pixels: Vec<[u8; 4]>,
}

struct Harness {
    motion: Option<MotionRuntime>,
    retirement: Option<RuntimeRetirement>,
    startup_failure: Option<RuntimeStartFailure>,
    world_dropped: Arc<AtomicBool>,
    camera_pending: Arc<Mutex<Option<[CameraConfiguration; 2]>>>,
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    records: Vec<Record>,
    host_frame: u64,
    pipelines: Option<isize>,
    backend: String,
    fallback: bool,
}

impl Harness {
    fn sample(&mut self, delta: Duration) -> Sample {
        let runtime = self.motion.as_mut().unwrap();
        let motion = runtime.drive(delta);
        Sample::observe(runtime, motion)
    }

    fn publish(&mut self, producer_index: usize, sample: &Sample) -> usize {
        sample.assert_original();
        self.host_frame += 1;
        self.host
            .begin_frame(self.host_frame)
            .expect("the explicitly requested adapter must exist");
        let adapter = self
            .host
            .runtime_facts()
            .adapter
            .expect("actual selected adapter facts");
        if self.host_frame == 1 {
            eprintln!(
                "material-view-time: adapter={adapter:?}, forced_fallback={}",
                self.fallback
            );
        }
        assert_eq!(
            adapter.backend(),
            self.backend,
            "the requested native backend must be selected"
        );
        if !self.fallback {
            assert_ne!(
                adapter.device_type(),
                "cpu",
                "hardware selection excludes a software adapter"
            );
        }
        let target: RenderImagePacket = image::target(WIDTH, HEIGHT);
        let adapted = sample.input.with_image_target(target).unwrap();
        assert_eq!(adapted.installation(), sample.input.installation());
        assert_eq!(
            adapted.packet().source_generation(),
            sample.input.packet().source_generation()
        );
        assert_eq!(
            adapted.packet().frame_index(),
            sample.input.packet().frame_index()
        );
        assert_eq!(adapted.packet().views(), sample.input.packet().views());
        assert_eq!(
            adapted.packet().world_canvas(),
            sample.input.packet().world_canvas()
        );
        assert_eq!(
            adapted.packet().captured_time(),
            sample.input.packet().captured_time()
        );
        assert!(std::ptr::eq(
            adapted.captured::<WgpuStockPolicy>().unwrap(),
            sample.input.captured::<WgpuStockPolicy>().unwrap()
        ));
        let producer = &self.producers[producer_index];
        let reservation = self
            .host
            .reserve_image_target(producer, adapted.packet().source_generation(), target)
            .unwrap();
        self.host
            .activate_image_target(producer, reservation)
            .unwrap();
        let candidate = self.host.prepare(producer, &adapted).unwrap();
        assert_eq!(
            candidate.input().packet().captured_time(),
            sample.input.packet().captured_time()
        );
        let published = self.host.submit_candidate(producer, candidate).unwrap();
        let texture = published.final_texture::<WgpuWorldColor>().unwrap();
        assert_eq!(
            texture.source_generation(),
            sample.input.packet().source_generation()
        );
        assert_eq!(texture.frame_index(), sample.input.packet().frame_index());
        assert_eq!(texture.producer_id(), producer.id());
        let prefix = published.outputs().submitted_prefix().clone();
        let output = published.into_image();
        assert_eq!(
            output.receipt().backend_instance_id(),
            self.host.backend_instance_id()
        );
        assert_eq!(
            output.receipt().source_generation(),
            sample.input.packet().source_generation()
        );
        assert_eq!(
            output.receipt().render_transaction_frame_index(),
            sample.input.packet().frame_index()
        );
        assert!(prefix.submission_index().is_some());
        let index = self.records.len();
        self.records.push(Record {
            sample: sample.clone(),
            output,
            prefix,
            pixels: Vec::new(),
        });
        let record = &mut self.records[index];
        record.pixels = material_readback::read(&mut self.host, &record.output);
        assert_pixels(&record.pixels, sample);
        let pipelines = self
            .host
            .with_image(&record.output, |device, _, _| {
                device.get_internal_counters().hal.render_pipelines.read()
            })
            .unwrap();
        assert!(
            pipelines > 0,
            "native pipeline counters must be enabled and supported"
        );
        if let Some(original) = self.pipelines {
            assert_eq!(
                pipelines, original,
                "captured time and Views must not add pipeline variants"
            );
        } else {
            self.pipelines = Some(pipelines);
        }
        let stats = self.host.frame_transaction_stats();
        assert_eq!(stats.surface_acquire_attempts(), 0);
        assert_eq!(stats.presents(), 0);
        index
    }

    fn assert_retained(&mut self, index: usize) {
        let record = &self.records[index];
        record.sample.assert_original();
        let pixels = material_readback::read(&mut self.host, &record.output);
        assert_eq!(
            pixels, record.pixels,
            "an exact retained output must not change"
        );
        assert_pixels(&pixels, &record.sample);
        image::settle(&record.prefix);
    }

    fn close_runtime(&mut self) -> Vec<String> {
        if let Some(runtime) = self.motion.take() {
            self.retirement = Some(runtime.begin_retirement());
        }
        let mut failures = Vec::new();
        let deadline = Instant::now() + NATIVE_TIMEOUT;
        if let Some(retirement) = &mut self.retirement {
            while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
                match catch_unwind(AssertUnwindSafe(|| retirement.drive_retirement())) {
                    Ok(_) => {}
                    Err(_) => failures.push("original Runtime retirement panicked".to_owned()),
                }
                if Instant::now() >= deadline {
                    failures.push(format!(
                        "original Runtime physical close timed out: {retirement}"
                    ));
                    return failures;
                }
                thread::sleep(Duration::from_millis(1));
            }
            if retirement.close_evidence().has_terminal_failure() {
                failures.push(format!("original Runtime teardown failed: {retirement}"));
            }
        }
        self.retirement.take();
        if let Some(failure) = &mut self.startup_failure {
            while failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
                match catch_unwind(AssertUnwindSafe(|| failure.drive_retirement())) {
                    Ok(_) => {}
                    Err(_) => failures.push("startup retirement panicked".to_owned()),
                }
                if Instant::now() >= deadline {
                    failures.push(format!("startup physical close timed out: {failure}"));
                    return failures;
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
        self.startup_failure.take();
        if !self.world_dropped.load(Ordering::Acquire) {
            failures.push("the original Runtime World has not physically dropped".to_owned());
        }
        failures
    }

    fn cleanup(&mut self) -> Vec<String> {
        let mut failures = self.close_runtime();
        for producer in &self.producers {
            let result = catch_unwind(AssertUnwindSafe(|| {
                if let Some(prefix) = self.host.retire_producer(producer) {
                    image::settle(&prefix);
                }
            }));
            if result.is_err() {
                failures.push("original producer retirement failed".to_owned());
            }
        }
        self.records.clear();
        self.producers.clear();
        if catch_unwind(AssertUnwindSafe(|| image::finish(&mut self.host))).is_err() {
            failures.push("original Host retirement failed".to_owned());
        }
        for (label, count) in [
            ("retained native bytes", self.host.retained_resource_bytes()),
            (
                "retained candidate metadata",
                self.host.retained_candidate_metadata_bytes(),
            ),
            ("cleanup faults", self.host.cleanup_fault_count()),
        ] {
            if count != 0 {
                failures.push(format!("{label}: {count}"));
            }
        }
        failures
    }

    fn run(&mut self) {
        let dropped = Arc::clone(&self.world_dropped);
        let camera_pending = Arc::clone(&self.camera_pending);
        match MotionRuntime::with_app(|app| configure(app, dropped, camera_pending)) {
            Ok(runtime) => self.motion = Some(runtime),
            Err(failure) => {
                let message = failure.to_string();
                self.startup_failure = Some(failure);
                panic!("ordinary Material Runtime startup failed: {message}");
            }
        }
        for _ in 0..3 {
            self.producers.push(self.host.create_producer().unwrap());
        }
        let zero = self.sample(Duration::ZERO);
        assert_eq!(zero.capture_count, 1);
        assert_eq!(zero.clocks.fixed.tick(), 0);
        assert_eq!(zero.clocks.fixed.steps_this_frame(), 0);
        assert!(!zero.clocks.interpolated);
        self.publish(0, &zero);

        let a = self.sample(TIMESTEP * 2 + TIMESTEP / 2);
        assert_eq!(a.capture_count, 2);
        assert_eq!(a.clocks.fixed.tick(), 2);
        assert_eq!(a.clocks.fixed.steps_this_frame(), 2);
        assert!((a.motion.interpolation_alpha - 0.5).abs() < 1e-5);
        assert!((a.motion.kinematic.x - 0.375).abs() < 1e-5);
        assert!((a.motion.fixed_positions[0].x - 0.5).abs() < 1e-5);
        assert!(a.clocks.interpolated);

        assert!(
            self.camera_pending
                .lock()
                .unwrap()
                .replace(CHANGED_CAMERAS)
                .is_none()
        );
        self.motion.as_ref().unwrap().observe(|world| {
            assert_eq!(world.resource::<CameraState>().current, INITIAL_CAMERAS);
            assert_eq!(world.resource::<Captures>().count, a.capture_count);
        });
        let b = self.sample(TIMESTEP / 4);
        assert_eq!(a.cameras, INITIAL_CAMERAS);
        assert_eq!(b.cameras, CHANGED_CAMERAS);
        assert!(self.camera_pending.lock().unwrap().is_none());
        for index in 0..2 {
            let old = &a.input.packet().views()[index];
            let new = &b.input.packet().views()[index];
            assert_eq!(old.id, new.id);
            assert_eq!(old.camera_entity, new.camera_entity);
            assert_ne!(old.mapping, new.mapping);
            for (old, new) in a.cameras[index]
                .probe_bytes()
                .into_iter()
                .zip(b.cameras[index].probe_bytes())
            {
                assert!(
                    old.abs_diff(new) > 1,
                    "changed matrices must distinguish both shader probes"
                );
            }
        }
        assert_eq!(b.capture_count, 3);
        assert_eq!(b.clocks.fixed.tick(), 2);
        assert_eq!(b.clocks.fixed.steps_this_frame(), 0);
        assert_eq!(
            b.clocks.virtual_time.elapsed - a.clocks.virtual_time.elapsed,
            TIMESTEP / 4
        );
        assert!((b.motion.kinematic.x - 0.4375).abs() < 1e-5);
        assert!(b.clocks.interpolated);
        let b_record = self.publish(0, &b);
        let a_record = self.publish(1, &a);
        assert!(a.input.packet().frame_index() < b.input.packet().frame_index());
        assert_ne!(self.records[a_record].pixels, self.records[b_record].pixels);
        self.assert_retained(b_record);

        self.motion.as_mut().unwrap().set_time_scale(0.5);
        self.motion.as_ref().unwrap().observe(|world| {
            assert_eq!(world.resource::<Captures>().count, b.capture_count);
            assert_eq!(
                world
                    .resource::<Captures>()
                    .latest
                    .as_ref()
                    .unwrap()
                    .packet()
                    .captured_time(),
                b.input.packet().captured_time()
            );
        });
        let scaled = self.sample(TIMESTEP);
        assert_eq!(scaled.capture_count, 4);
        assert_eq!(scaled.clocks.settings.time_scale(), 0.5);
        assert_eq!(scaled.clocks.real.delta, TIMESTEP);
        assert_eq!(scaled.clocks.virtual_time.delta, TIMESTEP / 2);
        assert_eq!(scaled.clocks.fixed.tick(), 3);
        assert_eq!(scaled.clocks.fixed.steps_this_frame(), 1);
        assert!((scaled.motion.kinematic.x - 0.5625).abs() < 1e-5);
        assert!(scaled.clocks.interpolated);
        assert_ne!(
            time_byte(scaled.clocks.real.elapsed),
            time_byte(scaled.clocks.virtual_time.elapsed)
        );
        self.publish(0, &scaled);

        self.motion.as_mut().unwrap().pause();
        let paused = self.sample(Duration::from_millis(125));
        assert_eq!(paused.capture_count, 6);
        assert!(paused.motion.paused);
        assert_eq!(paused.clocks.fixed.tick(), scaled.clocks.fixed.tick());
        assert_eq!(
            paused.clocks.virtual_time.elapsed,
            scaled.clocks.virtual_time.elapsed
        );
        assert_eq!(paused.clocks.virtual_time.delta, Duration::ZERO);
        assert_eq!(
            paused.clocks.real.elapsed - scaled.clocks.real.elapsed,
            Duration::from_millis(125)
        );
        assert_eq!(
            paused.motion.interpolation_alpha,
            scaled.motion.interpolation_alpha
        );
        assert!(paused.motion.interpolation_alpha > 0.0);
        assert!(!paused.clocks.interpolated);
        assert_eq!(paused.motion.kinematic, paused.motion.fixed_positions[0]);
        assert!((paused.motion.kinematic.x - 0.75).abs() < 1e-5);
        assert_ne!(
            time_byte(paused.clocks.real.elapsed),
            time_byte(scaled.clocks.real.elapsed)
        );
        self.publish(0, &paused);

        let (stamp, poses) = self.motion.as_mut().unwrap().exact_step();
        assert_eq!(stamp.tick, 4);
        assert!((poses[0].x - 1.0).abs() < 1e-5);
        self.motion.as_ref().unwrap().observe(|world| {
            let captures = world.resource::<Captures>();
            let retained = captures.latest.as_ref().unwrap();
            assert_eq!(captures.count, paused.capture_count);
            assert_eq!(
                retained.packet().captured_time(),
                paused.input.packet().captured_time()
            );
            assert!(std::ptr::eq(
                retained.captured::<WgpuStockPolicy>().unwrap(),
                paused.input.captured::<WgpuStockPolicy>().unwrap()
            ));
            assert_eq!(
                world.resource::<VirtualTime>().elapsed,
                paused.clocks.virtual_time.elapsed + TIMESTEP
            );
            assert_eq!(
                world.resource::<RealTime>().elapsed,
                paused.clocks.real.elapsed
            );
        });
        let stepped = self.sample(Duration::ZERO);
        assert_eq!(stepped.capture_count, paused.capture_count + 1);
        assert_eq!(stepped.motion.stamp.frame, stamp.frame + 1);
        assert_eq!(stepped.clocks.fixed.tick(), 4);
        assert_eq!(stepped.motion.kinematic, poses[0]);
        assert!(!stepped.clocks.interpolated);
        assert!(stepped.clocks.real.frame > stepped.input.packet().frame_index());
        let current_record = self.publish(0, &stepped);
        let replay = self.publish(2, &a);
        assert_eq!(self.records[replay].pixels, self.records[a_record].pixels);
        assert_eq!(a.cameras, INITIAL_CAMERAS);
        assert_eq!(stepped.cameras, CHANGED_CAMERAS);
        assert_eq!(
            a.input
                .packet()
                .captured_time()
                .unwrap()
                .settings()
                .time_scale(),
            1.0
        );
        assert_eq!(
            a.input.packet().captured_time().unwrap().spatial_stamp(),
            a.motion.stamp
        );
        self.assert_retained(current_record);
        self.assert_retained(b_record);
        a.assert_original();

        let runtime_cleanup = self.close_runtime();
        assert!(
            runtime_cleanup.is_empty(),
            "original Runtime close: {runtime_cleanup:?}"
        );
        for producer in &self.producers {
            if let Some(prefix) = self.host.retire_producer(producer) {
                image::settle(&prefix);
            }
        }
        for index in 0..self.records.len() {
            self.assert_retained(index);
        }
        eprintln!(
            "material-view-time: captures={}, publications={}, pipelines={:?}, original World dropped={}",
            stepped.capture_count,
            self.records.len(),
            self.pipelines,
            self.world_dropped.load(Ordering::Acquire)
        );
    }
}

fn assert_pixels(actual: &[[u8; 4]], sample: &Sample) {
    let expected = sample.expected();
    let frame = sample.input.packet().frame_index();
    let covered = if sample.cameras == INITIAL_CAMERAS {
        16
    } else {
        assert_eq!(sample.cameras, CHANGED_CAMERAS);
        64
    };
    assert_eq!(actual.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        expected.iter().filter(|pixel| pixel[3] != 0).count(),
        covered
    );
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        for channel in 0..4 {
            let tolerance = u8::from(channel < 3 && expected[3] != 0);
            assert!(
                actual[channel].abs_diff(expected[channel]) <= tolerance,
                "frame {frame}, pixel ({}, {}), channel {channel}: actual={actual:?}, expected={expected:?}",
                index % WIDTH as usize,
                index / WIDTH as usize
            );
        }
    }
}

#[test]
#[ignore = "requires an explicitly selected native hardware or software adapter; missing adapters fail"]
fn ordinary_material_captures_view_time_motion_and_replays_original_frames() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    let fallback = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => panic!("explicitly select NARA_WGPU_FORCE_FALLBACK=0 or 1"),
    };
    let mut harness = Harness {
        motion: None,
        retirement: None,
        startup_failure: None,
        world_dropped: Arc::new(AtomicBool::new(false)),
        camera_pending: Arc::new(Mutex::new(None)),
        host: WgpuRenderHost::offscreen().with_resource_byte_limit(8 * 1024 * 1024),
        producers: Vec::new(),
        records: Vec::new(),
        host_frame: 0,
        pipelines: None,
        backend,
        fallback,
    };
    let result = catch_unwind(AssertUnwindSafe(|| harness.run()));
    let cleanup = harness.cleanup();
    if let Err(panic) = result {
        if !cleanup.is_empty() {
            eprintln!("original Runtime/Host cleanup after acceptance failure: {cleanup:?}");
        }
        resume_unwind(panic);
    }
    assert!(
        cleanup.is_empty(),
        "original Runtime/Host cleanup: {cleanup:?}"
    );
}
