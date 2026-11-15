//! Physical source updates captured and executed between live managed Runtime frames.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use nara::{
    core::{Color, Vec2},
    ecs::schedule::IntoScheduleConfigs,
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    project_host::{HeadlessSession, HeadlessSessionClose},
    render::{Camera2d, RenderImage2d, RenderImagePacket, RenderTarget},
    render_wgpu::{
        WgpuImageOutput, WgpuPolicyCaptureSet, WgpuRenderHost, WgpuRenderInput, WgpuRenderProducer,
        WgpuStockPolicy, WgpuWorldColor, take_captured_policy_batch,
    },
    sprite::Sprite,
    transform::Transform2d,
    window::{Window, WindowResolution},
};

use super::*;

#[path = "gpu/expected.rs"]
mod expected;
#[path = "gpu/readback.rs"]
mod readback;

use expected::{HEIGHT, WIDTH, assert_image, expected_center};

#[derive(Clone)]
struct Sample {
    input: WgpuRenderInput,
    workflow: Workflow,
    expected: [u8; 4],
}

#[derive(Default, Resource)]
struct Latest {
    sample: Option<Sample>,
    logical_view: bool,
}

#[derive(Default)]
struct Capture;

impl Plugin for Capture {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-watching.gpu"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Latest>()?
            .add_systems(CoreStage::Update, configure_view.after(update_game))?
            .add_systems(
                CoreStage::Render,
                capture.in_set(WgpuPolicyCaptureSet::Publish),
            )?;
        Ok(())
    }
}

fn configure_view(world: &mut World) {
    if !world.resource::<Latest>().logical_view {
        assert_eq!(world.query::<&Window>().iter(world).count(), 0);
        let window = Window::new(
            "Live physical source palette",
            WindowResolution::new(WIDTH, HEIGHT),
        );
        let target = RenderTarget::Window(window.id);
        world.spawn(window);
        let camera = world
            .spawn((
                Camera2d {
                    target,
                    viewport_height: HEIGHT as f32,
                    clear_color: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Place {
                entity: camera,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }])
            .unwrap();
        world.resource_mut::<Latest>().logical_view = true;
    }
    let entity = world
        .resource::<Workflow>()
        .current
        .as_ref()
        .unwrap()
        .entity;
    if world.get::<Transform2d>(entity).is_none() {
        world.entity_mut(entity).insert(Transform2d::default());
    }
    let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
    sprite.size = Vec2::splat(8.0);
    sprite.material.tint = Color::WHITE;
}

fn capture(world: &mut World) {
    let batch = take_captured_policy_batch(world).expect("one original sealed CPU batch");
    assert!(take_captured_policy_batch(world).is_none());
    let input = batch.into_render_input().unwrap().expect("a logical View");
    assert_eq!(input.packet().views().len(), 1);
    let extent = input.packet().target().extent();
    assert_eq!((extent.width, extent.height), (WIDTH, HEIGHT));
    let workflow = world.resource::<Workflow>().clone();
    let current = workflow.current.as_ref().unwrap();
    let pixels = captured::material(&input, current);
    let gain = [1.0, 0.75, 0.5, 0.25, 0.125]
        .into_iter()
        .find(|gain| {
            let source = if *gain == 1.0 {
                SHADER.to_owned()
            } else {
                shader(&gain.to_string())
            };
            current.material.inputs[0].1 == source
        })
        .expect("the original captured source must select one fixture gain");
    let expected = expected_center(gain, pixels.indices, pixels.palette);
    let mut latest = world.resource_mut::<Latest>();
    if let Some(previous) = &latest.sample {
        assert!(previous.input.packet().frame_index() < input.packet().frame_index());
    }
    latest.sample = Some(Sample {
        input,
        workflow,
        expected,
    });
}

struct Harness {
    session: HeadlessSession,
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    host_frame: u64,
}

impl Harness {
    fn publish(&mut self, producer: usize, input: &WgpuRenderInput) -> WgpuImageOutput {
        self.host_frame += 1;
        self.host.begin_frame(self.host_frame).unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            input.packet().target().extent(),
        )
        .unwrap();
        let adapted = input.with_image_target(target).unwrap();
        assert_eq!(adapted.installation(), input.installation());
        assert_eq!(adapted.packet().views(), input.packet().views());
        assert_eq!(
            adapted.packet().source_generation(),
            input.packet().source_generation()
        );
        assert_eq!(adapted.packet().frame_index(), input.packet().frame_index());
        assert!(std::ptr::eq(
            adapted.captured::<WgpuStockPolicy>().unwrap(),
            input.captured::<WgpuStockPolicy>().unwrap(),
        ));
        let producer = &self.producers[producer];
        let reservation = self
            .host
            .reserve_image_target(producer, adapted.packet().source_generation(), target)
            .unwrap();
        self.host
            .activate_image_target(producer, reservation)
            .unwrap();
        let candidate = self.host.prepare(producer, &adapted).unwrap();
        let published = self.host.submit_candidate(producer, candidate).unwrap();
        let texture = published.final_texture::<WgpuWorldColor>().unwrap();
        assert_eq!(texture.frame_index(), input.packet().frame_index());
        assert_eq!(
            texture.source_generation(),
            input.packet().source_generation()
        );
        assert_eq!(texture.producer_id(), producer.id());
        assert_eq!(
            published.image().receipt().render_transaction_frame_index(),
            input.packet().frame_index()
        );
        published.into_image()
    }

    fn close_session(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.session.close() {
                HeadlessSessionClose::Complete => return Ok(()),
                HeadlessSessionClose::Failed => {
                    return Err(format!(
                        "source Runtime close failed: {:?}",
                        self.session.diagnostics()
                    ));
                }
                HeadlessSessionClose::Pending => {
                    if Instant::now() >= deadline {
                        return Err("source Runtime close timed out".to_owned());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    fn finish(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        match catch_unwind(AssertUnwindSafe(|| self.close_session())) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => failures.push(error),
            Err(panic) => failures.push(format!(
                "Runtime close panicked: {}",
                panic_message(panic.as_ref())
            )),
        }
        for producer in self.producers.drain(..) {
            let retired = catch_unwind(AssertUnwindSafe(|| {
                if let Some(prefix) = self.host.retire_producer(&producer) {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !prefix.poll().unwrap() {
                        assert!(
                            Instant::now() < deadline,
                            "original producer prefix timed out"
                        );
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }));
            if let Err(panic) = retired {
                failures.push(format!(
                    "producer retirement panicked: {}",
                    panic_message(panic.as_ref())
                ));
            }
        }
        let retired = catch_unwind(AssertUnwindSafe(|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self.host.poll_retirement().unwrap() {
                assert!(
                    Instant::now() < deadline,
                    "original GPU Host retirement timed out"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }));
        if let Err(panic) = retired {
            failures.push(format!(
                "GPU Host retirement panicked: {}",
                panic_message(panic.as_ref())
            ));
        }
        for (label, count) in [
            ("retained native bytes", self.host.retained_resource_bytes()),
            (
                "retained candidate bytes",
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
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic")
}

fn run(harness: &mut Harness, proof: &CloseProof) {
    harness.session.start().unwrap();
    let runtime = harness.session.runtime_generation().unwrap();
    let mut deferred: Option<Sample> = None;
    let mut shader_replayed = false;
    let mut image_replayed = false;
    let mut started = false;
    let mut retained = Vec::new();
    let mut observed_commits = [false; 9];
    let mut observed_rejections = [false; 4];
    let mut last_frame = None;
    let deadline = Instant::now() + Duration::from_secs(150);
    let final_workflow = loop {
        assert!(
            Instant::now() < deadline,
            "live Source to GPU trajectory timed out"
        );
        assert!(harness.session.frame(Duration::ZERO).unwrap().is_none());
        let sample = harness
            .session
            .observe(|world| {
                assert!(
                    world
                        .get_resource::<nara::window::backend::BackendWindowHandles>()
                        .is_none()
                );
                assert!(
                    world
                        .get_resource::<nara::window::backend::WindowPresentationRequirement>()
                        .is_none()
                );
                world.resource::<Latest>().sample.clone().unwrap()
            })
            .unwrap();
        assert_eq!(
            sample
                .input
                .packet()
                .source_generation()
                .runtime_generation()
                .map(|generation| generation.get()),
            Some(runtime)
        );
        let frame = sample.input.packet().frame_index();
        assert!(last_frame.is_none_or(|last| frame > last));
        last_frame = Some(frame);
        if !started {
            assert_eq!(sample.workflow.commits, 0);
            deferred = Some(sample);
            started = true;
            continue;
        }
        if shader_replayed
            && !image_replayed
            && deferred.is_none()
            && sample.workflow.step == Step::PalettePixels
        {
            assert_eq!(sample.workflow.commits, 5);
            assert_ne!(
                sample.workflow.current.as_ref().unwrap().images[&palette_product()].pixels,
                PIXEL
            );
            deferred = Some(sample);
            continue;
        }
        let output = harness.publish(0, &sample.input);
        let label = format!(
            "live {:?}, commits={}, rejections={}, frame={frame}",
            sample.workflow.step, sample.workflow.commits, sample.workflow.rejections
        );
        let pixels = readback::read(&mut harness.host, &output);
        assert_image(&pixels, sample.expected, &label);
        let new_commit = !observed_commits[sample.workflow.commits];
        let new_rejection = !observed_rejections[sample.workflow.rejections];
        observed_commits[sample.workflow.commits] = true;
        observed_rejections[sample.workflow.rejections] = true;
        if new_commit || new_rejection {
            assert!(retained.len() < 14);
            eprintln!(
                "{label}: actual center={:?}",
                pixels[((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize]
            );
            retained.push((output, sample.expected, label));
        }
        let replay = if !shader_replayed {
            sample.workflow.commits == 2
        } else {
            !image_replayed && sample.workflow.commits == 6
        };
        if replay {
            let original = deferred.take().expect("one retained original CPU capture");
            assert!(original.input.packet().frame_index() < frame);
            let before = &original.workflow.current.as_ref().unwrap().material;
            let after = &sample.workflow.current.as_ref().unwrap().material;
            let label = if !shader_replayed {
                assert_ne!(before.shader, after.shader);
                shader_replayed = true;
                "retained startup input after Shader publication"
            } else {
                assert_eq!(before.shader, after.shader);
                assert_ne!(
                    original.workflow.current.as_ref().unwrap().images[&palette_product()].pixels,
                    sample.workflow.current.as_ref().unwrap().images[&palette_product()].pixels
                );
                image_replayed = true;
                "retained palette input after Image publication"
            };
            let output = harness.publish(1, &original.input);
            let pixels = readback::read(&mut harness.host, &output);
            assert_image(&pixels, original.expected, label);
            eprintln!(
                "{label}: original frame={}, actual center={:?}",
                original.input.packet().frame_index(),
                pixels[((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize]
            );
            observed_commits[original.workflow.commits] = true;
            retained.push((output, original.expected, label.to_owned()));
        }
        if sample.workflow.step == Step::Done {
            break sample.workflow;
        }
    };
    assert!(observed_commits[1..].iter().all(|seen| *seen));
    assert!(observed_rejections[1..].iter().all(|seen| *seen));
    assert_eq!(final_workflow.commits, 8);
    assert_eq!(final_workflow.rejections, 3);
    assert!(final_workflow.request.is_none());
    assert!(final_workflow.candidate.is_none());
    assert!(shader_replayed && image_replayed);
    assert!(deferred.is_none());
    harness.close_session().unwrap();
    proof.assert_joined_before_world_drop();
    let observer = final_workflow.observer.as_ref().unwrap();
    let stopped = observer.stats();
    assert!(stopped.started() > 0);
    assert_eq!(stopped.started(), stopped.joined());
    assert_eq!(stopped.active(), 0);
    assert_eq!(stopped.retiring(), 0);
    for producer in &harness.producers {
        if let Some(prefix) = harness.host.retire_producer(producer) {
            assert!(prefix.poll().unwrap());
        }
    }
    for (output, expected, label) in &retained {
        assert_image(&readback::read(&mut harness.host, output), *expected, label);
    }
    assert_eq!(observer.stats(), stopped);
    eprintln!(
        "live submissions={}, retained outputs={}, Shader/Image replays=2, watchers={stopped:?}",
        harness.host_frame,
        retained.len(),
    );
    assert_eq!(
        final_workflow.current.as_ref().unwrap().gameplay,
        GameProgress {
            health: 7,
            score: 128,
            inventory: vec!["brass-key".to_owned(), "map".to_owned()],
        }
    );
}

#[test]
#[ignore = "requires an actual Vulkan or DX12 adapter; executes live native filesystem changes"]
fn native_source_updates_render_live_and_replay_original_inputs_after_publication() {
    let project = Project::new();
    let manifest_path = project.root().join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let manifest = manifest.replace(
        "requested = ['runtime-2d']",
        "requested = ['runtime-2d', 'asset-watch', 'presentation-2d', 'render-wgpu']",
    );
    fs::write(manifest_path, manifest).unwrap();
    let original_scene = fs::read(project.root().join(STARTUP)).unwrap();
    let proof = CloseProof::default();
    let watch = ProjectSceneWatchPlugin::new(project.root().to_path_buf()).unwrap();
    let product = recipe()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(watch, Clone::clone)
        .unwrap()
        .add_configured_plugin::<PaletteGame, _, _>(
            GameFiles(project.root().to_path_buf()),
            |files| PaletteGame(files.clone()),
        )
        .unwrap()
        .add_configured_plugin::<retirement::ObserveClose, _, _>(proof.clone(), |proof| {
            retirement::ObserveClose(proof.clone())
        })
        .unwrap()
        .add_plugin::<Capture>()
        .unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producers = (0..2).map(|_| host.create_producer().unwrap()).collect();
    let mut harness = Harness {
        session: HeadlessSession::new(project.capability(), product),
        host,
        producers,
        host_frame: 0,
    };
    let result = catch_unwind(AssertUnwindSafe(|| run(&mut harness, &proof)));
    let cleanup = harness.finish();
    if let Err(failure) = result {
        if !cleanup.is_empty() {
            eprintln!("original Runtime/GPU cleanup after the trajectory failure: {cleanup:?}");
        }
        resume_unwind(failure);
    }
    assert!(
        cleanup.is_empty(),
        "original Runtime/GPU cleanup: {cleanup:?}"
    );
    assert_eq!(
        fs::read(project.root().join(STARTUP)).unwrap(),
        original_scene
    );
}
