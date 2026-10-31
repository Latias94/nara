#![forbid(unsafe_code)]

//! Real Source/Processed stock capture, followed by replay after both native Runtimes retire.
//! This finite executable owns the main thread and reuses one physical Winit event loop.

use std::{
    cell::{Cell, RefCell},
    error::Error,
    fs, io,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::AppExit,
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunIntent, DesktopRunOptions,
        DesktopRunOutcome, WinitEventLoop, open_explicit_product_project_root,
    },
    render::{RenderImage2d, RenderImagePacket},
    render_wgpu::{WgpuImageOutput, WgpuRenderHost, WgpuRenderHostError, WgpuRenderProgramError},
};

#[path = "../tests/support/content_capture.rs"]
mod content_capture;
#[path = "../tests/support/content_fixture.rs"]
#[allow(dead_code)] // The ordinary content test also uses this fixture's unchanged new() entry.
mod content_fixture;

use content_capture::{CapturePlugin, CaptureResult, CapturedFrame, ContentMode};
use content_fixture::ContentFixture;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn finish_run(
    run: &mut DesktopRun,
    captured: &RefCell<CaptureResult>,
    mode: ContentMode,
) -> Result<[CapturedFrame; 2]> {
    let mut report = run.execute();
    let deadline = Instant::now() + Duration::from_secs(10);
    while report.outcome() == DesktopRunOutcome::CleanupIncomplete {
        assert!(
            run.take_event_loop().is_none(),
            "incomplete native close retains its actual pump"
        );
        if Instant::now() >= deadline {
            return Err(io::Error::other(format!(
                "{mode:?} native retirement exceeded ten seconds: {report:?}"
            ))
            .into());
        }
        thread::sleep(Duration::from_millis(1));
        report = run.execute();
    }
    assert!(
        captured.borrow().failure.is_none(),
        "{:?}",
        captured.borrow().failure
    );
    assert_eq!(
        report.outcome(),
        DesktopRunOutcome::Completed(AppExit::Success),
        "the original native Runtime must succeed and fully retire: {report:?}"
    );
    assert!(!report.diagnostics().has_errors(), "{report:?}");
    assert_eq!(
        report.startup_content_closure().is_some(),
        mode == ContentMode::Source
    );
    let mut captured = captured.borrow_mut();
    assert!(captured.stores_emptied_before_present);
    let frames = std::array::from_fn(|index| {
        captured.frames[index]
            .take()
            .expect("both exact inputs require real native presentation")
    });
    for frame in &frames {
        frame.assert_retired();
    }
    Ok(frames)
}

fn recover_failed_startup(pump: WinitEventLoop) -> Result<WinitEventLoop> {
    let invalid = tempfile::tempdir()?;
    // This owned source fails TOML admission before any native Runtime or window is admitted.
    fs::write(invalid.path().join("nara.toml"), "schema_version = [\n")?;
    let observations = Rc::new(Cell::new(0_u32));
    let observed = Rc::clone(&observations);
    let options = DesktopRunOptions::new()
        .with_event_loop(pump)
        .with_presentation_observer(move |_| {
            observed.set(observed.get() + 1);
            DesktopPresentationControl::Exit(AppExit::Requested)
        });
    let mut run = DesktopRun::new(
        open_explicit_product_project_root(invalid.path())?,
        DesktopRunIntent::new().with_execution(options),
    );
    assert!(
        run.take_event_loop().is_none(),
        "fresh admission still owns the supplied pump"
    );
    let report = run.execute();
    assert_eq!(report.outcome(), DesktopRunOutcome::Failed, "{report:?}");
    assert!(report.diagnostics().has_errors());
    assert!(report.startup_content_closure().is_none());
    assert_eq!(
        observations.get(),
        0,
        "invalid Source must fail before native admission"
    );
    let pump = run
        .take_event_loop()
        .expect("failed pre-native admission returns the same owned pump");
    assert!(
        run.take_event_loop().is_none(),
        "pump recovery transfers ownership only once"
    );
    Ok(pump)
}

struct ReplayProof {
    host: u64,
    pixels: usize,
    outputs: usize,
}

fn replay(frames: &[CapturedFrame; 4], order: [usize; 4]) -> Result<ReplayProof> {
    let mut host = WgpuRenderHost::offscreen();
    let outcome = (|| {
        let owners = (0..4)
            .map(|_| host.create_producer())
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut other = WgpuRenderHost::offscreen();
        let foreign = other.create_producer()?;
        let mut outputs: Vec<(WgpuImageOutput, usize)> = Vec::with_capacity(4);
        for (step, index) in order.into_iter().enumerate() {
            let original = &frames[index];
            original.assert_retired();
            host.begin_frame(step as u64 + 1)?;
            let target = RenderImagePacket::new(
                RenderImage2d::allocate_handle()?,
                original.input.packet().target().extent(),
            )?;
            let input = original.input.with_image_target(target)?;
            assert_eq!(
                input.packet().source_generation(),
                original.input.packet().source_generation()
            );
            assert_eq!(
                input.packet().frame_index(),
                original.input.packet().frame_index()
            );
            assert_eq!(input.installation(), original.input.installation());
            assert_eq!(input.packet().views(), original.input.packet().views());
            assert_eq!(
                input.packet().world_canvas(),
                original.input.packet().world_canvas()
            );
            let reservation = host.reserve_image_target(
                &owners[index],
                input.packet().source_generation(),
                target,
            )?;
            host.activate_image_target(&owners[index], reservation)?;
            if step == 0 {
                assert!(
                    matches!(
                        host.prepare(&foreign, &input),
                        Err(WgpuRenderHostError::ProducerClosed)
                    ),
                    "another Host's actual producer cannot prepare the retained input"
                );
            }
            let candidate = host.prepare(&owners[index], &input)?;
            let candidate = if step == 0 {
                let refusal = match host.submit_candidate(&owners[(index + 1) % 4], candidate) {
                    Err(refusal) => refusal,
                    Ok(_) => panic!("a candidate cannot execute under another actual producer"),
                };
                assert!(matches!(
                    refusal.error,
                    WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
                ));
                refusal.candidate
            } else {
                candidate
            };
            let published = host
                .submit_candidate(&owners[index], candidate)
                .map_err(|refusal| refusal.error)?;
            let output = published.image();
            let receipt = output.receipt();
            assert_eq!(receipt.producer_id(), owners[index].id());
            assert_eq!(
                receipt.source_generation(),
                original.input.packet().source_generation()
            );
            assert_eq!(
                receipt.source_generation().runtime_generation(),
                Some(original.runtime)
            );
            assert_eq!(
                receipt.render_transaction_frame_index(),
                original.input.packet().frame_index()
            );
            assert_eq!(receipt.target(), target);
            assert_eq!(receipt.backend_instance_id(), host.backend_instance_id());
            assert_eq!(receipt.device_epoch(), host.device_epoch());
            assert!(receipt.output_version() > 0 && receipt.target_generation() > 0);
            outputs.push((output.clone(), index));
        }
        // Read every exact old/new output only after all four have executed on this same Host.
        // An old source must never resolve through another producer's latest prepared resources.
        let mut pixels = 0;
        for (output, index) in &outputs {
            pixels +=
                content_capture::assert_output_pixels(&mut host, output, frames[*index].color())?;
        }
        for owner in &owners {
            let prefix = host.retire_producer(owner).ok_or_else(|| {
                io::Error::other("the actual submitted producer must retain its prefix")
            })?;
            assert!(
                prefix.poll()?,
                "readback has completed each producer's original queue prefix"
            );
        }
        drop(foreign);
        assert!(
            other.poll_retirement()?,
            "the foreign owner never acquired a native device"
        );
        Ok(ReplayProof {
            host: host.backend_instance_id(),
            pixels,
            outputs: outputs.len(),
        })
    })();
    // Even a failed replay keeps retirement with its original native Host.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !host.poll_retirement()? {
        if Instant::now() >= deadline {
            return Err(
                io::Error::other("independent stock Host retirement exceeded ten seconds").into(),
            );
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(host.poll_retirement()?);
    outcome
}

fn main() -> Result<()> {
    let backend = std::env::var("WGPU_BACKEND")?;
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one real native backend"
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let recipe = content_fixture::recipe().add_plugin::<CapturePlugin>()?;
    let fixture = ContentFixture::with_recipe(recipe.clone());
    let (options, source_result) = content_capture::options(ContentMode::Source);
    // Ordinary Source construction creates the only physical EventLoop in this process.
    let mut source = DesktopRun::new(
        open_explicit_product_project_root(fixture.source_root())?,
        DesktopRunIntent::new()
            .with_recipe(recipe)
            .with_execution(options),
    );
    let [source_red, source_green] = finish_run(&mut source, &source_result, ContentMode::Source)?;
    let source_native_host = source_result.borrow().backend_instance.unwrap();
    let pump = source
        .take_event_loop()
        .expect("full Source retirement returns its physical pump");
    assert!(source.take_event_loop().is_none());
    drop(source);
    let pump = recover_failed_startup(pump)?;
    let (options, processed_result) = content_capture::options(ContentMode::Processed);
    let mut processed =
        DesktopRun::from_runtime_content(fixture.open_processed(), options.with_event_loop(pump));
    let [processed_red, processed_green] =
        finish_run(&mut processed, &processed_result, ContentMode::Processed)?;
    let processed_native_host = processed_result.borrow().backend_instance.unwrap();
    let pump = processed
        .take_event_loop()
        .expect("full Processed retirement returns the reused pump");
    assert!(processed.take_event_loop().is_none());
    drop(processed);
    assert_ne!(source_red.runtime, processed_red.runtime);
    assert_ne!(
        source_red.instance.domain_id(),
        processed_red.instance.domain_id()
    );
    assert_ne!(source_native_host, processed_native_host);
    for (source, processed) in [
        (&source_red, &processed_red),
        (&source_green, &processed_green),
    ] {
        assert_eq!(
            source.source, processed.source,
            "production cook preserves the actual source key"
        );
        assert_eq!(
            source.provenance, processed.provenance,
            "production cook preserves actual provenance"
        );
        assert_ne!(source.input.installation(), processed.input.installation());
        assert_ne!(
            source.instance, processed.instance,
            "instance identity includes the original World/domain, not just its local sequence"
        );
        source.assert_retired();
        processed.assert_retired();
    }
    let frames = [source_red, source_green, processed_red, processed_green];
    fixture.destroy_backing();
    drop(fixture);
    // Both original Worlds, their native Hosts, and the actual fixture backing are gone.
    // Each independent replay owns its own producer admission; original generation/frame values
    // remain unchanged even when a newer green input executes before an older red input.
    let original = replay(&frames, [0, 3, 2, 1])?;
    let replacement = replay(&frames, [1, 2, 3, 0])?;
    assert_ne!(original.host, replacement.host);
    for host in [original.host, replacement.host] {
        assert_ne!(host, source_native_host);
        assert_ne!(host, processed_native_host);
    }
    drop(pump);
    println!(
        "CONTENT_WINDOW_OK captures=4 runtimes=2 native_presentations=4 source_runtime={} processed_runtime={} original_host={} replacement_host={} original_host_outputs={} replacement_host_outputs={} pixels_checked={} foreign_producer_refusals=2 candidate_owner_refusals=2 pump_reused=true failed_startup_pump_recovered=true original_runtimes_retired=true live_stores_emptied=true backing_destroyed=true source_processed_provenance=true retained_originals=true hosts_retired=true",
        frames[0].runtime.get(),
        frames[2].runtime.get(),
        original.host,
        replacement.host,
        original.outputs,
        replacement.outputs,
        original.pixels + replacement.pixels,
    );
    Ok(())
}
