#![deny(unsafe_code)]

#[cfg(windows)]
#[path = "picking/effect.rs"]
mod effect;
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "picking/native.rs"]
mod native;
#[cfg(windows)]
#[path = "picking/oracle.rs"]
mod oracle;
#[cfg(windows)]
#[path = "picking/scene.rs"]
mod scene;
#[cfg(windows)]
#[path = "picking/split_resize.rs"]
mod split_resize;

#[cfg(windows)]
mod application {
    use std::{
        any::Any,
        cell::{Cell, RefCell},
        collections::{BTreeSet, VecDeque},
        io,
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use super::{
        effect::Mode,
        native::NativeProbe,
        oracle,
        scene::{
            self, CameraMirror, Request, RetainedCapture, RetainedCaptures, Revision, SceneHandles,
            SceneState,
        },
    };
    use nara::{
        app::{AppExit, CoreStage, RuntimeInstance},
        core::Vec2,
        ecs::{
            Query, Res, ResMut, Resource, bevy_ecs::system::SystemState,
            schedule::IntoScheduleConfigs,
        },
        project_host::{
            DesktopPresentationControl, DesktopRun, DesktopRunOptions, DesktopRunOutcome,
            DesktopRunProgress,
        },
        render::{RenderFrame, RenderImage2d, RenderImagePacket, WorldPointer},
        window::Window,
    };
    use nara_render_wgpu::{
        WgpuPolicyCaptureSet, WgpuRenderHost, WgpuSceneColor, WgpuSubmissionPrefix,
    };
    use nara_stock_composition_consumer::readback;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const DEADLINE: Duration = Duration::from_secs(60);

    #[derive(Clone)]
    struct PreHostFact {
        frame: u64,
        resolution: nara::window::WindowResolution,
        points: Vec<(Vec2, Option<Vec2>)>,
    }

    #[derive(Default, Resource)]
    struct PreHostFacts(VecDeque<PreHostFact>);

    fn before_host(
        pointer: WorldPointer,
        windows: Query<&Window>,
        handles: Res<SceneHandles>,
        frame: Res<RenderFrame>,
        mut facts: ResMut<PreHostFacts>,
    ) {
        let Some(window) = windows.iter().next() else {
            return;
        };
        let resolution = window.resolution;
        let density = resolution.scale_factor as f32;
        if !density.is_finite() || density <= 0.0 {
            return;
        }
        let points = [[0.37, 0.43], [0.62, 0.57]].map(|fraction| {
            let logical = Vec2::new(
                resolution.physical_width as f32 * fraction[0] / density,
                resolution.physical_height as f32 * fraction[1] / density,
            );
            (logical, pointer.window_to_world(handles.camera, logical))
        });
        if facts.0.len() == 16 {
            facts.0.pop_front();
        }
        facts.0.push_back(PreHostFact {
            frame: frame.index,
            resolution,
            points: points.into(),
        });
    }

    fn assert_point(actual: Option<Vec2>, expected: Option<[f64; 2]>, label: &str) {
        match (actual, expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert!(actual.is_finite());
                for axis in 0..2 {
                    assert!(
                        (f64::from(actual[axis]) - expected[axis]).abs() <= 1.0e-5,
                        "{label}: {actual:?} != {expected:?}"
                    );
                }
            }
            (actual, expected) => panic!("{label}: {actual:?} != {expected:?}"),
        }
    }

    #[derive(Default)]
    struct Observed {
        last: Option<RetainedCapture>,
        frames: usize,
        pending_checks: usize,
        transitions: usize,
        failure: Option<Box<dyn Any + Send>>,
    }

    fn inspect_pointers(
        world: &nara::ecs::World,
        pointer: &mut SystemState<WorldPointer>,
        record: &RetainedCapture,
    ) {
        let camera = world.resource::<SceneHandles>().camera;
        let parameter = pointer.get(world).unwrap();
        for canvas in oracle::CANVAS_PROBES {
            let logical = oracle::presented_point(record, canvas).unwrap();
            let actual =
                parameter.window_to_world(camera, Vec2::new(logical[0] as f32, logical[1] as f32));
            assert_point(
                actual,
                oracle::world_at_logical(record, logical),
                "presented original inverse",
            );
        }
        for canvas in [[60.5, 24.0], [32.0, 44.5], [2.0, 24.0], [32.0, 46.0]] {
            let logical = oracle::presented_point(record, canvas).unwrap();
            assert!(
                parameter
                    .window_to_world(camera, Vec2::new(logical[0] as f32, logical[1] as f32))
                    .is_none(),
                "half-open View bounds reject {canvas:?}"
            );
        }
        let resolution = record.window.resolution;
        for logical in [
            [f64::NAN, 1.0],
            [-1.0, 1.0],
            [
                (f64::from(resolution.physical_width) + 1.0) / resolution.scale_factor,
                10.0,
            ],
            [
                10.0,
                (f64::from(resolution.physical_height) + 1.0) / resolution.scale_factor,
            ],
        ] {
            assert!(
                parameter
                    .window_to_world(camera, Vec2::new(logical[0] as f32, logical[1] as f32))
                    .is_none()
            );
        }
    }

    fn settle(prefix: &WgpuSubmissionPrefix) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(10);
        while !prefix.poll()? {
            if Instant::now() >= end {
                return Err(io::Error::other("original replay prefix timed out").into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(prefix.cleanup_fault_count(), 0);
        Ok(())
    }

    fn replay(records: &[RetainedCapture]) -> Result<()> {
        let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(64 * 1024 * 1024);
        let mut producers = Vec::new();
        let mut prefixes = Vec::new();
        let mut retained = Vec::new();
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
            for (index, record) in records.iter().enumerate() {
                host.begin_frame(index as u64 + 1)?;
                let producer = host.create_producer()?;
                let target = RenderImagePacket::new(
                    RenderImage2d::allocate_handle().unwrap(),
                    record.input.packet().target().extent(),
                )?;
                let input = record.input.with_image_target(target)?;
                assert_eq!(
                    input.world_input_mapping(),
                    record.input.world_input_mapping()
                );
                assert_eq!(input.packet().views(), record.input.packet().views());
                let reservation = host.reserve_image_target(
                    &producer,
                    input.packet().source_generation(),
                    target,
                )?;
                host.activate_image_target(&producer, reservation)?;
                let candidate = host.prepare(&producer, &input)?;
                let frame = host
                    .submit_candidate(&producer, candidate)
                    .map_err(|failure| failure.error)?;
                let output = frame.final_texture::<WgpuSceneColor>()?;
                prefixes.push(frame.outputs().submitted_prefix().clone());
                prefixes.push(host.retire_producer(&producer).unwrap());
                let actual = readback::read(&mut host, &output);
                let expected = oracle::expected(record, actual.format);
                oracle::assert_pixels(&expected, &actual);
                assert!(
                    actual.pixels.iter().any(|pixel| *pixel == [255, 0, 0, 255]
                        || *pixel == [0, 0, 255, 255]
                        || *pixel == [0, 255, 0, 255]),
                    "the mirrored scene must draw visible asymmetric geometry"
                );
                println!(
                    "PICKING_PIXELS frame={} mode={:?} mirror={:?} revision={:?} extent={}x{} scale={}",
                    record.input.packet().frame_index(),
                    record.mode,
                    record.state.mirror,
                    record.state.revision,
                    actual.width,
                    actual.height,
                    record.window.resolution.scale_factor
                );
                retained.push((output, actual));
                producers.push(producer);
            }
            for (output, original) in &retained {
                let actual = readback::read(&mut host, output);
                assert_eq!(
                    actual.pixels, original.pixels,
                    "old output retains exact pixels after later input realization"
                );
            }
            Ok(())
        }));
        for producer in &producers {
            if let Some(prefix) = host.retire_producer(producer) {
                prefixes.push(prefix);
            }
        }
        let prefix_results: Vec<_> = prefixes.iter().map(settle).collect();
        drop(retained);
        readback::finish(&mut host);
        assert_eq!(host.retained_resource_bytes(), 0);
        assert_eq!(host.retained_candidate_metadata_bytes(), 0);
        assert_eq!(host.cleanup_fault_count(), 0);
        match result {
            Ok(result) => result?,
            Err(payload) => resume_unwind(payload),
        }
        for result in prefix_results {
            result?;
        }
        Ok(())
    }

    pub fn run() -> Result<()> {
        let mixed = match std::env::args().nth(1).as_deref() {
            Some("--mixed-dpi") => true,
            Some("--single-display") => false,
            _ => return Err(io::Error::other("select --mixed-dpi or --single-display").into()),
        };
        let backend = std::env::var("WGPU_BACKEND")?;
        assert!(matches!(backend.as_str(), "dx12" | "vulkan"));
        let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK")?;
        assert!(matches!(fallback.as_str(), "0" | "1"));
        assert!(!mixed || fallback == "0");
        let title = format!("Nara presented picking {}", std::process::id());
        let inject_failure = std::env::args().nth(2).as_deref() == Some("--fail-observer");
        let inject = Rc::new(Cell::new(false));
        let observer_inject = Rc::clone(&inject);
        let commands = Arc::new(Mutex::new(VecDeque::new()));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut app = scene::build(title.clone(), Arc::clone(&commands), Arc::clone(&dropped))?;
        app.init_resource::<PreHostFacts>()?;
        app.add_systems(
            CoreStage::Render,
            before_host
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        let mut pointer = SystemState::<WorldPointer>::new(app.world_mut()?);
        let runtime = RuntimeInstance::start(app)?;
        let observed = Rc::new(RefCell::new(Observed::default()));
        let callback = Rc::clone(&observed);
        let observer_backend = backend.clone();
        let observer_fallback = fallback.clone();
        let options = DesktopRunOptions::new().with_presentation_observer(move |observation| {
            if callback.borrow().failure.is_some() {
                return DesktopPresentationControl::Exit(AppExit::Requested);
            }
            let checked = catch_unwind(AssertUnwindSafe(|| {
                let Some(receipt) = observation.presented_frame() else {
                    return DesktopPresentationControl::Continue;
                };
                let world = observation
                    .world()
                    .expect("healthy original presented World");
                let record = world
                    .resource::<RetainedCaptures>()
                    .0
                    .iter()
                    .find(|record| {
                        record.input.packet().frame_index()
                            == receipt.render_transaction_frame_index()
                    })
                    .expect("receipt joins the original sealed input")
                    .clone();
                assert_eq!(
                    record
                        .input
                        .packet()
                        .source_generation()
                        .runtime_generation(),
                    Some(observation.runtime_generation())
                );
                let adapter = observation.adapter_summary().unwrap();
                assert_eq!(adapter.backend(), observer_backend);
                if observer_fallback == "0" {
                    assert_ne!(adapter.device_type(), "cpu");
                }
                let mut observed = callback.borrow_mut();
                if observer_inject.get() {
                    panic!("injected original picking observer failure");
                }
                if observed.frames == 0 {
                    eprintln!("picking adapter={adapter:?}, forced_fallback={observer_fallback}");
                }
                if let Some(previous) = &observed.last {
                    let fact = world
                        .resource::<PreHostFacts>()
                        .0
                        .iter()
                        .find(|fact| fact.frame == record.input.packet().frame_index())
                        .expect("every presented input has a pre-Host observation");
                    if fact.resolution == previous.window.resolution {
                        for (logical, actual) in &fact.points {
                            assert_point(
                                *actual,
                                oracle::world_at_logical(
                                    previous,
                                    [f64::from(logical.x), f64::from(logical.y)],
                                ),
                                "pending capture retains preceding presented mapping",
                            );
                        }
                        if previous.state != record.state || previous.mode != record.mode {
                            assert!(
                                fact.points.iter().any(|(logical, _)| {
                                    let point = [f64::from(logical.x), f64::from(logical.y)];
                                    oracle::world_at_logical(previous, point)
                                        != oracle::world_at_logical(&record, point)
                                }) || (previous.mode == Mode::Identity
                                    && record.mode == Mode::Tint),
                                "a transition must distinguish old and new mapping"
                            );
                            observed.transitions += 1;
                        }
                        observed.pending_checks += 1;
                    }
                }
                inspect_pointers(world, &mut pointer, &record);
                observed.frames += 1;
                observed.last = Some(record);
                DesktopPresentationControl::Continue
            }));
            match checked {
                Ok(control) => control,
                Err(payload) => {
                    callback.borrow_mut().failure = Some(payload);
                    DesktopPresentationControl::Exit(AppExit::Requested)
                }
            }
        });
        let mut run = DesktopRun::from_runtime(runtime, options);
        let mut probe: Option<NativeProbe> = None;
        let mut records = Vec::new();
        let mut scales = BTreeSet::new();
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
            let deadline = Instant::now() + DEADLINE;
            while observed.borrow().last.is_none() {
                assert!(
                    Instant::now() < deadline,
                    "first genuine native presentation deadline"
                );
                assert!(matches!(
                    run.poll(Duration::from_millis(10)),
                    DesktopRunProgress::Running
                ));
            }
            probe = Some(NativeProbe::find_exact(&title).map_err(io::Error::other)?);
            if inject_failure {
                inject.set(true);
            }
            let native = probe.as_ref().unwrap();
            for (index, monitor) in native.monitors().iter().enumerate() {
                println!(
                    "PICKING_MONITOR index={index} physical={:?} work={:?} dpi={} scale={}",
                    monitor.physical, monitor.work, monitor.dpi, monitor.scale
                );
            }
            let source = native.current_index().map_err(io::Error::other)?;
            let other = native
                .monitors()
                .iter()
                .enumerate()
                .find(|(_, monitor)| (monitor.scale - native.monitors()[source].scale).abs() > 0.01)
                .map(|(index, _)| index);
            let destination = if mixed {
                other.ok_or_else(|| {
                    io::Error::other("two actually active different-DPI displays are required")
                })?
            } else {
                source
            };
            let states = [
                (source, CameraMirror::X, Revision::A, Mode::Identity),
                (source, CameraMirror::X, Revision::A, Mode::Tint),
                (source, CameraMirror::X, Revision::A, Mode::MirrorMapped),
                (source, CameraMirror::X, Revision::A, Mode::MirrorUnmapped),
                (source, CameraMirror::X, Revision::B, Mode::Identity),
                (destination, CameraMirror::Y, Revision::A, Mode::Identity),
                (
                    destination,
                    CameraMirror::Both,
                    Revision::A,
                    Mode::MirrorMapped,
                ),
                (destination, CameraMirror::None, Revision::A, Mode::Tint),
                (source, CameraMirror::X, Revision::A, Mode::Identity),
            ];
            for (monitor, mirror, revision, mode) in states {
                commands.lock().unwrap().push_back(Request {
                    mode,
                    state: SceneState { mirror, revision },
                });
                native.move_to(monitor).map_err(io::Error::other)?;
                let deadline = Instant::now() + Duration::from_secs(8);
                let started = Instant::now();
                let mut stable = None;
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "native picking stage timeout {mode:?} {mirror:?}"
                    );
                    assert!(matches!(
                        run.poll(Duration::from_millis(10)),
                        DesktopRunProgress::Running
                    ));
                    let geometry = native.geometry().map_err(io::Error::other)?;
                    let record = observed.borrow().last.clone().unwrap();
                    let resolution = record.window.resolution;
                    let ready = geometry.monitor == monitor
                        && geometry.size == [resolution.physical_width, resolution.physical_height]
                        && (geometry.scale - resolution.scale_factor).abs() < 0.001
                        && record.mode == mode
                        && record.state == (SceneState { mirror, revision });
                    if ready && started.elapsed() >= Duration::from_millis(150) {
                        let since = stable.get_or_insert_with(Instant::now);
                        if since.elapsed() >= Duration::from_millis(100) {
                            scales.insert(geometry.dpi);
                            println!(
                                "PICKING_PRESENT frame={} mode={mode:?} mirror={mirror:?} revision={revision:?} dpi={} extent={:?}",
                                record.input.packet().frame_index(),
                                geometry.dpi,
                                geometry.size
                            );
                            records.push(record);
                            break;
                        }
                    } else {
                        stable = None;
                    }
                }
            }
            assert!(!mixed || scales.len() >= 2);
            assert!(observed.borrow().pending_checks >= 4);
            assert!(observed.borrow().transitions >= 4);
            Ok(())
        }));
        retire_original(&mut run, probe.as_ref(), &dropped)?;
        drop(run);
        if let Some(payload) = observed.borrow_mut().failure.take() {
            eprintln!(
                "PICKING_FAILURE_CLEANED original_runtime_retired=true original_window_destroyed=true original_pump_returned=true"
            );
            resume_unwind(payload);
        }
        match result {
            Ok(result) => result?,
            Err(payload) => resume_unwind(payload),
        }
        replay(&records)?;
        println!(
            "PICKING_WINDOW_OK modes=4 world_pointer_checked=true original_runtime_retired=true original_window_destroyed=true prefix_cleanup=true pixel_replays={} monitor_scales={}",
            records.len(),
            scales.len()
        );
        Ok(())
    }

    pub(super) fn retire_original(
        run: &mut DesktopRun,
        probe: Option<&NativeProbe>,
        dropped: &AtomicBool,
    ) -> Result<()> {
        run.request_close();
        let close = Instant::now() + Duration::from_secs(15);
        while !matches!(
            run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            if Instant::now() >= close {
                return Err(io::Error::other("original DesktopRun retirement deadline").into());
            }
            if run.poll(Duration::from_millis(10)) == DesktopRunProgress::CleanupIncomplete {
                let _ = run.execute();
            }
        }
        let report = run.execute();
        let world_retired = dropped.load(Ordering::Acquire);
        let window = probe.map(NativeProbe::verify_destroyed);
        let pump_returned = run.take_event_loop().is_some();
        if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
            || report.diagnostics().has_errors()
            || !world_retired
            || window.as_ref().is_some_and(std::result::Result::is_err)
            || !pump_returned
        {
            return Err(io::Error::other(format!(
                "original DesktopRun retirement world={world_retired} window={window:?} pump={pump_returned}: {report:?}"
            )).into());
        }
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--split-resize") {
        split_resize::run()
    } else {
        application::run()
    }
}

#[cfg(not(windows))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err(std::io::Error::other("this native picking trajectory currently requires Windows").into())
}
