//! Original public Host submission, post-retirement replay and bounded physical cleanup.

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

use nara::{
    project_host::{HeadlessSession, HeadlessSessionClose},
    render::{RenderImage2d, RenderImagePacket},
    render_wgpu::{
        WgpuImageOutput, WgpuRenderHost, WgpuRenderInput, WgpuRenderProducer, WgpuStockPolicy,
        WgpuSubmissionPrefix, WgpuWorldColor,
    },
};

use crate::{expected, project};

#[path = "../source_watching/gpu/readback.rs"]
mod readback;
#[path = "scene.rs"]
mod scene;

use scene::{Mode, Proof};

struct Harness {
    session: HeadlessSession,
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    host_frame: u64,
}

#[derive(Debug)]
struct InjectedFailure;

fn wait_prefix(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix
        .poll()
        .expect("the original submission prefix must remain pollable")
    {
        assert!(Instant::now() < deadline, "original GPU prefix timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
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
        assert_eq!(adapted.packet().frame_index(), input.packet().frame_index());
        assert_eq!(
            adapted.packet().source_generation(),
            input.packet().source_generation()
        );
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
        assert_eq!(texture.producer_id(), producer.id());
        assert_eq!(texture.frame_index(), input.packet().frame_index());
        assert_eq!(
            texture.source_generation(),
            input.packet().source_generation()
        );
        let receipt = published.image().receipt();
        assert_eq!(receipt.producer_id(), producer.id());
        assert_eq!(
            receipt.backend_instance_id(),
            self.host.backend_instance_id()
        );
        assert_eq!(receipt.device_epoch(), self.host.device_epoch());
        assert_eq!(receipt.target(), target);
        assert_eq!(
            receipt.source_generation(),
            input.packet().source_generation()
        );
        assert_eq!(
            receipt.render_transaction_frame_index(),
            input.packet().frame_index()
        );
        wait_prefix(published.outputs().submitted_prefix());
        published.into_image()
    }

    fn close_session(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.session.close() {
                HeadlessSessionClose::Complete => {
                    assert!(self.session.is_closed());
                    assert!(
                        !self.session.diagnostics().has_errors(),
                        "{:?}",
                        self.session.diagnostics()
                    );
                    return;
                }
                HeadlessSessionClose::Failed => {
                    panic!(
                        "original Runtime close failed: {:?}",
                        self.session.diagnostics()
                    )
                }
                HeadlessSessionClose::Pending => {
                    assert!(
                        Instant::now() < deadline,
                        "original Runtime close timed out"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    fn finish(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        cleanup(&mut failures, "source Runtime", || self.close_session());
        for producer in self.producers.drain(..) {
            cleanup(&mut failures, "original producer prefix", || {
                if let Some(prefix) = self.host.retire_producer(&producer) {
                    wait_prefix(&prefix);
                }
            });
        }
        cleanup(&mut failures, "original GPU Host", || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self
                .host
                .poll_retirement()
                .expect("original GPU Host retirement")
            {
                assert!(
                    Instant::now() < deadline,
                    "original GPU Host retirement timed out"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        });
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

fn cleanup(failures: &mut Vec<String>, label: &str, operation: impl FnOnce()) {
    if let Err(panic) = catch_unwind(AssertUnwindSafe(operation)) {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic");
        failures.push(format!("{label}: {message}"));
    }
}

fn run(
    harness: &mut Harness,
    mode: Mode,
    proof: &Proof,
    fallback: bool,
    fail_after_first_output: bool,
) {
    for _ in 0..2 {
        harness
            .producers
            .push(harness.host.create_producer().unwrap());
    }
    harness.session.start().unwrap();
    let runtime = harness.session.runtime_generation().unwrap();
    let mut original: Option<WgpuRenderInput> = None;
    let mut outputs = Vec::new();
    let mut last_frame = None;
    for frame in 1..=mode.frames() {
        assert!(harness.session.frame(Duration::ZERO).unwrap().is_none());
        let (observed, input) = harness.session.observe(scene::latest).unwrap();
        assert_eq!(observed, frame);
        assert_eq!(
            input
                .packet()
                .source_generation()
                .runtime_generation()
                .map(|generation| generation.get()),
            Some(runtime)
        );
        assert!(last_frame.is_none_or(|last| input.packet().frame_index() > last));
        last_frame = Some(input.packet().frame_index());
        if frame == 1 {
            original = Some(input.clone());
        } else {
            assert_eq!(
                input.installation(),
                original.as_ref().unwrap().installation()
            );
        }
        let output = harness.publish(0, &input);
        if frame == 1 {
            let adapter = harness
                .host
                .adapter_summary()
                .expect("the original Host's actual adapter");
            assert_eq!(adapter.backend(), "dx12");
            if fallback {
                assert_eq!(
                    adapter.device_type(),
                    "cpu",
                    "explicit DX12 software control"
                );
            } else {
                assert!(
                    matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu"),
                    "hardware DX12 adapter required: {adapter:?}"
                );
            }
        }
        let label = format!("{mode:?} live frame {frame}");
        expected::assert_image(
            &readback::read(&mut harness.host, &output),
            mode.palette(frame),
            &label,
        );
        proof.assert_retained();
        outputs.push((output, mode.palette(frame), label));
        if fail_after_first_output && frame == 1 {
            std::panic::panic_any(InjectedFailure);
        }
    }

    let adapter = harness
        .host
        .adapter_summary()
        .expect("the original Host's actual adapter");
    let backend = harness.host.backend_instance_id();
    let epoch = harness.host.device_epoch();
    harness.close_session();
    proof.assert_world_dropped();
    assert!(harness.session.runtime_generation().is_none());
    assert!(harness.session.observe(|_| ()).is_err());
    proof.assert_retained();

    let original = original.unwrap();
    assert!(original.packet().frame_index() < last_frame.unwrap());
    let replay = harness.publish(1, &original);
    assert_eq!(harness.host.backend_instance_id(), backend);
    assert_eq!(harness.host.device_epoch(), epoch);
    expected::assert_image(
        &readback::read(&mut harness.host, &replay),
        Some(expected::A),
        "original A replay after B rendered and source World retired",
    );
    assert_ne!(
        replay.receipt().output_version(),
        outputs[0].0.receipt().output_version()
    );
    outputs.push((
        replay,
        Some(expected::A),
        "retired-world A replay".to_owned(),
    ));

    for producer in &harness.producers {
        let prefix = harness
            .host
            .retire_producer(producer)
            .expect("each original producer submitted real work");
        wait_prefix(&prefix);
    }
    for (output, palette, label) in &outputs {
        assert!(
            output.is_current(),
            "retained output must survive producer retirement"
        );
        expected::assert_image(&readback::read(&mut harness.host, output), *palette, label);
    }
    proof.assert_retained();
    eprintln!(
        "CAPTURED_TEXTURES_OK mode={mode:?} backend={} adapter={:?} device_type={} \
         live_frames={} post_world_replays=1 retained_outputs={}",
        adapter.backend(),
        adapter.name(),
        adapter.device_type(),
        mode.frames(),
        outputs.len(),
    );
}

fn exercise(mode: Mode, fail_after_first_output: bool) {
    assert_eq!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12"),
        "explicit WGPU_BACKEND=dx12 required"
    );
    let fallback = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        actual => panic!("explicit NARA_WGPU_FORCE_FALLBACK=0 or 1 required, got {actual:?}"),
    };
    let project = project::Project::new();
    let manifest_path = project.root().join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let native = manifest.replace(
        "requested = ['runtime-2d']",
        "requested = ['runtime-2d', 'presentation-2d', 'render-wgpu']",
    );
    assert_ne!(native, manifest);
    fs::write(manifest_path, native).unwrap();
    let original_scene = fs::read(project.root().join(project::STARTUP)).unwrap();
    let proof = Proof::new(mode);
    let product = project::recipe()
        .add_configured_plugin::<scene::Capture, _, _>(proof.clone(), |proof| {
            scene::Capture(proof.clone())
        })
        .unwrap();
    let mut harness = Harness {
        session: HeadlessSession::new(project.capability(), product),
        host: WgpuRenderHost::offscreen(),
        producers: Vec::new(),
        host_frame: 0,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        run(
            &mut harness,
            mode,
            &proof,
            fallback,
            fail_after_first_output,
        )
    }));
    let failures = harness.finish();
    match result {
        Err(panic) if fail_after_first_output && panic.is::<InjectedFailure>() => {}
        Err(panic) => {
            if !failures.is_empty() {
                eprintln!("original Runtime/GPU cleanup after assertion or panic: {failures:?}");
            }
            resume_unwind(panic);
        }
        Ok(()) => assert!(
            !fail_after_first_output,
            "the actual post-readback panic must fire"
        ),
    }
    assert!(
        failures.is_empty(),
        "original Runtime/GPU cleanup: {failures:?}"
    );
    proof.assert_world_dropped();
    proof.assert_pixels_dropped();
    if fail_after_first_output {
        eprintln!(
            "CAPTURED_TEXTURES_PANIC_CLEANED original_world_retired=true original_pixels_released=true native_bytes=0 candidate_bytes=0 cleanup_faults=0"
        );
    }
    assert_eq!(
        fs::read(project.root().join(project::STARTUP)).unwrap(),
        original_scene
    );
}

#[test]
#[ignore = "requires explicit DX12 hardware or software selection; adapter or readback failure fails the test"]
fn authored_material_keeps_extracted_textures_and_replays_after_publication_and_world_retirement() {
    exercise(Mode::Authored, false);
}

#[test]
#[ignore = "requires explicit DX12 hardware or software selection; adapter or readback failure fails the test"]
fn direct_binding_keeps_extracted_textures_and_replays_after_publication_and_world_retirement() {
    exercise(Mode::Direct, false);
}

#[test]
#[ignore = "requires explicit DX12 selection and an actual submitted/read-back frame before the injected panic"]
fn submitted_frame_panic_retires_original_world_pixels_and_gpu_resources() {
    exercise(Mode::Authored, true);
}
