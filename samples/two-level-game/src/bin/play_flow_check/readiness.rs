//! Delivers an actual Resume confirmation after physical Start and a presented Playing HUD.

#[path = "../cook/content.rs"]
mod content;
#[path = "../player_desktop/output.rs"]
mod desktop_output;
#[path = "../player/evidence.rs"]
mod evidence;
#[path = "../player/output.rs"]
mod output;
#[path = "../player_desktop/presentation.rs"]
mod presentation;
#[path = "window.rs"]
mod window;

mod input {
    pub(super) use two_level_game::delivery::{PlayerSelection, RuntimeKind};
}

use std::{
    cell::RefCell,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    app::{AppExit, FixedTime, RuntimeControl, RuntimeControlStatus, RuntimeState},
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunOptions, DesktopRunOutcome,
        DesktopRunProgress,
    },
    project_tooling::host_fs,
    runtime_content::RuntimeInputContent,
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde_json::json;
use two_level_game::{
    GameSnapshot, RunOptions, Scenario,
    delivery::{ControllerBackend, GameFlow, PlayerSelection, RuntimeKind},
    flow::{FlowPhase, FlowSnapshot},
};

use presentation::{ContentIdentity, Presentation, SharedPresentation};
use window::{ENTER, Input, Window};

const POLL: Duration = Duration::from_millis(2);

#[derive(Default)]
struct Observation {
    target: Option<BackendWindowHandles>,
    flow: FlowSnapshot,
    game: GameSnapshot,
    fixed_tick: u64,
    launch_consumed: bool,
}

struct Fixture {
    run: DesktopRun,
    shared: SharedPresentation,
    latest: Rc<RefCell<Observation>>,
    input: Input,
    window: Option<Window>,
    _processed_input: RuntimeInputContent,
    retired: bool,
}

pub(super) fn run() -> ExitCode {
    if std::env::var("WGPU_BACKEND").as_deref() != Ok("dx12")
        || std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() != Ok("0")
    {
        eprintln!("readiness check requires WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0");
        return ExitCode::FAILURE;
    }
    let cache = match temporary() {
        Ok(cache) => cache,
        Err(error) => {
            eprintln!("readiness cache: {error}");
            return ExitCode::FAILURE;
        }
    };
    let prepared = catch_unwind(AssertUnwindSafe(|| Fixture::new(&cache)))
        .unwrap_or_else(|panic| Err(presentation::panic_detail(panic.as_ref())));
    let mut fixture = match prepared {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!(
                "readiness preparation: {error}; cache cleanup: {:?}",
                remove_temporary(&cache)
            );
            return ExitCode::FAILURE;
        }
    };
    let trajectory = catch_unwind(AssertUnwindSafe(|| fixture.qualify(&cache)))
        .unwrap_or_else(|panic| Err(presentation::panic_detail(panic.as_ref())));
    let cleanup = catch_unwind(AssertUnwindSafe(|| fixture.close()))
        .unwrap_or_else(|panic| Err(presentation::panic_detail(panic.as_ref())));
    let input_cleanup = catch_unwind(AssertUnwindSafe(|| fixture.input.finish()))
        .unwrap_or_else(|panic| Err(presentation::panic_detail(panic.as_ref())));
    if !fixture.retired {
        eprintln!(
            "readiness retains original native owners and cache: {trajectory:?}; {cleanup:?}; {input_cleanup:?}"
        );
        std::mem::forget(fixture);
        return ExitCode::FAILURE;
    }
    let mut failures: Vec<_> = [trajectory, cleanup, input_cleanup]
        .into_iter()
        .filter_map(Result::err)
        .collect();
    failures.extend(fixture.shared.borrow().failure.clone());
    let mut record = fixture
        .shared
        .borrow()
        .record(Scenario::Interactive, None, None);
    for field in [
        "original_runtime_retired",
        "original_pump_returned",
        "cleanup_complete",
    ] {
        record[field] = json!(fixture.retired);
    }
    let mut emitter = std::mem::take(&mut fixture.shared.borrow_mut().emitter);
    drop(fixture);
    if let Err(error) = remove_temporary(&cache) {
        failures.push(error);
    }
    let message = (!failures.is_empty()).then(|| failures.join("; "));
    if let Some(message) = &message {
        record["success"] = json!(false);
        record["error"] = json!(output::bounded(message));
        record["message"] = json!(output::bounded(message));
    }
    if let Err(error) = emitter.emit(desktop_output::Record::Result, record) {
        eprintln!("readiness result: {error}");
        return ExitCode::FAILURE;
    }
    if let Some(message) = message {
        eprintln!("readiness check: {message}");
        return ExitCode::FAILURE;
    }
    println!(
        "two_level_play_flow_readiness: ok physical_input=true ready_after_start=true launch_consumed=true original_native_retired=true"
    );
    ExitCode::SUCCESS
}

impl Fixture {
    fn new(cache: &Path) -> Result<Self, String> {
        let root = two_level_game::open_project_root().map_err(|error| error.to_string())?;
        let store = host_fs::writable_directory(cache).map_err(|error| error.to_string())?;
        let selection = PlayerSelection::new(RuntimeKind::Desktop, ControllerBackend::Disabled)?
            .with_flow(GameFlow::Round)?;
        let (prepared, _) =
            content::prepare_for_player(&root, &store, "x86_64-pc-windows-msvc", selection)
                .map_err(|error| error.to_string())?
                .into_parts();
        let base = prepared.base();
        let shared = Presentation::new(
            false,
            Scenario::Interactive,
            ContentIdentity {
                catalog: base.catalog_digest(),
                target: base.catalog().target().triple().to_owned(),
                profile: base.catalog().target().profile().to_owned(),
            },
        );
        let processed_input = base
            .acquire_startup_input()
            .map_err(|error| error.to_string())?
            .ok_or("processed Round content has no original Input")?;
        shared
            .borrow_mut()
            .evidence
            .expect_input(&processed_input)?;
        shared.borrow_mut().selection = Some(selection);
        let latest = Rc::new(RefCell::new(Observation::default()));
        let observed = Rc::clone(&latest);
        let state = Rc::clone(&shared);
        let execution = DesktopRunOptions::new()
            .with_initial_pause()
            .with_presentation_observer(move |facts| {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let world = facts.world();
                    let receipt = facts.presented_frame();
                    let control = presentation::observer(&state, facts);
                    if let Some(world) = world {
                        let mut observed = observed.borrow_mut();
                        if let Some(handles) = world.get_resource::<BackendWindowHandles>() {
                            observed.target.get_or_insert_with(|| handles.clone());
                        }
                        if let Some(receipt) = receipt
                            && state
                                .borrow()
                                .frame
                                .as_ref()
                                .is_some_and(|frame| frame.app_frame == receipt.app_frame_index())
                        {
                            observed.flow = world.resource::<FlowSnapshot>().clone();
                            observed.game = world.resource::<GameSnapshot>().clone();
                            observed.fixed_tick = world.resource::<FixedTime>().tick();
                            observed.launch_consumed =
                                matches!(two_level_game::player_launch_result(world), Some(Ok(())));
                        }
                    }
                    control
                }));
                match result {
                    Ok(control) => control,
                    Err(panic) => {
                        let mut state = state.borrow_mut();
                        state.failure.get_or_insert_with(|| {
                            format!(
                                "readiness observer panicked: {}",
                                presentation::panic_detail(panic.as_ref())
                            )
                        });
                        state.panic.get_or_insert(panic);
                        DesktopPresentationControl::Exit(AppExit::Requested)
                    }
                }
            });
        Ok(Self {
            run: DesktopRun::from_runtime_content(prepared.into_base(), execution),
            shared,
            latest,
            input: Input::new(),
            window: None,
            _processed_input: processed_input,
            retired: false,
        })
    }

    fn wait(
        &mut self,
        predicate: impl Fn(&Presentation, &Observation, &Input) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let progress = self.run.poll(POLL);
            let state = self.shared.borrow();
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            if progress != DesktopRunProgress::Running {
                return Err(format!("original readiness run stopped: {progress:?}"));
            }
            if predicate(&state, &self.latest.borrow(), &self.input) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "readiness checkpoint deadline: {}",
                    state.record(Scenario::Interactive, None, None)
                ));
            }
        }
    }

    fn qualify(&mut self, cache: &Path) -> Result<(), String> {
        self.wait(|state, latest, _| {
            latest.flow.phase == FlowPhase::Title
                && latest.flow.round == 0
                && state.record(Scenario::Interactive, None, None)["initial_title_presentation"]
                    .is_object()
        })?;
        assert_eq!(self.run.state(), Some(RuntimeState::Paused));
        let title = self
            .shared
            .borrow()
            .record(Scenario::Interactive, None, None);
        assert_eq!(title["state"]["tick"], 0);
        assert_eq!(self.latest.borrow().fixed_tick, 0);
        assert_eq!(self.latest.borrow().game.logical_tick, 0);
        assert_eq!(title["ready"], false);
        assert_eq!(title["adapter"]["backend"], "dx12");
        assert!(matches!(
            title["adapter"]["device_type"].as_str(),
            Some("discrete-gpu" | "integrated-gpu")
        ));
        println!(
            "two_level_play_flow_readiness_checkpoint: initial-title {}",
            title["initial_title_presentation"]
        );
        let command = two_level_game::player_launch(RunOptions {
            scenario: Scenario::Interactive,
            save_path: Some(cache.join("checkpoint.json")),
        })?
        .ok_or("Save-path launch must use the original first-tick ingress")?;
        self.run
            .submit(command)
            .map_err(|error| error.to_string())?;
        self.shared.borrow_mut().launch_admitted = true;
        self.shared.borrow_mut().launch_required = true;
        let ticket = self
            .run
            .request_control(RuntimeControl::Resume)
            .map_err(|error| error.to_string())?;
        assert_eq!(Some(ticket.generation()), self.run.runtime_generation());
        self.wait(|_, _, _| true)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.run.control_status(ticket) == Some(RuntimeControlStatus::Pending) {
            if Instant::now() >= deadline {
                return Err("actual Resume was not applied".into());
            }
            self.wait(|_, _, _| true)?;
        }
        assert_eq!(
            self.run.control_status(ticket),
            Some(RuntimeControlStatus::Applied)
        );
        assert_eq!(self.run.state(), Some(RuntimeState::Running));
        let window = Window::find_owned()?.ok_or("original readiness HWND is absent")?;
        self.window = Some(window);
        self.input.bind(window)?;
        self.wait(|_, _, input| input.focused())?;
        self.input.check()?;
        assert_eq!(self.latest.borrow().game.tick, 0);
        assert!(!self.latest.borrow().launch_consumed);
        assert!(!self.shared.borrow().launch_accepted);
        assert!(!self.shared.borrow().ready);
        self.input.press(ENTER)?;
        self.wait(|state, latest, _| {
            let current = state.record(Scenario::Interactive, None, None);
            latest.flow.phase == FlowPhase::Playing
                && latest.flow.round == 1
                && latest.game.tick > 0
                && current["flow"]["presented_ui"]["matches_snapshot"] == true
                && current["flow"]["presented_ui"]["receipt"]["submitted_glyph_quads"]
                    .as_u64()
                    .is_some_and(|glyphs| glyphs > 0)
        })?;
        let released_after = self.shared.borrow().frames;
        self.input.release(ENTER)?;
        self.wait(|state, _, input| {
            state.frames > released_after && input.down(ENTER) == Ok(false)
        })?;
        self.input.acknowledge_release(ENTER)?;
        let before = self
            .shared
            .borrow()
            .record(Scenario::Interactive, None, None);
        assert_eq!(before["ready"], false);
        assert_eq!(before["resume_applied"], false);
        assert_eq!(before["launch_accepted"], true);
        assert_eq!(
            before["initial_title_presentation"],
            title["initial_title_presentation"]
        );
        assert!(self.latest.borrow().launch_consumed);
        assert_eq!(
            self.latest.borrow().game.tick,
            self.latest.borrow().fixed_tick
        );
        assert_ne!(before["startup_receipt"], before["current_receipt"]);
        println!(
            "two_level_play_flow_readiness_checkpoint: playing-before-confirmation {}",
            before["flow"]
        );
        self.shared.borrow_mut().resume_applied = true;
        let frame = before["presentation"]["app_frame"]
            .as_u64()
            .ok_or("missing original Playing frame")?;
        self.wait(|state, _, _| {
            state.ready
                && state
                    .frame
                    .as_ref()
                    .is_some_and(|current| current.app_frame > frame)
        })?;
        let ready = self
            .shared
            .borrow()
            .record(Scenario::Interactive, None, None);
        assert_eq!(ready["flow"]["phase"], "Playing");
        assert_eq!(ready["flow"]["round"], 1);
        assert_eq!(ready["flow"]["presented_ui"]["matches_snapshot"], true);
        assert_eq!(
            ready["flow"]["presented_ui"]["receipt"]["app_frame"],
            ready["presentation"]["app_frame"]
        );
        assert_eq!(
            ready["initial_title_presentation"],
            title["initial_title_presentation"]
        );
        assert!(
            ready["initial_title_presentation"]["presented_ui"]["receipt"]["app_frame"]
                .as_u64()
                .unwrap()
                < frame
        );
        assert!(self.latest.borrow().launch_consumed);
        assert_eq!(
            self.latest.borrow().game.tick,
            self.latest.borrow().fixed_tick
        );
        println!(
            "two_level_play_flow_readiness_checkpoint: ready-after-start {}",
            ready["presentation"]
        );
        Ok(())
    }

    fn close(&mut self) -> Result<(), String> {
        let window = match self.window {
            Some(window) => Ok(Some(window)),
            None => Window::find_owned(),
        };
        let release = self.input.release_owned();
        self.run.request_close();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !matches!(
            self.run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            if Instant::now() >= deadline {
                return Err("original readiness native cleanup remains incomplete".into());
            }
            if self.run.progress() == DesktopRunProgress::CleanupIncomplete {
                self.run.execute();
            } else {
                self.run.poll(POLL);
            }
        }
        let report = self.run.execute();
        let target = self
            .latest
            .borrow_mut()
            .target
            .take()
            .ok_or("no original native target")?
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert!(
            target.fault.is_none()
                && !target.surface_active
                && !target.provider_present
                && target.native_destroyed
        );
        if let Some(window) = window? {
            assert!(!window.exists());
        }
        assert!(Window::find_owned()?.is_none());
        assert_eq!(self.run.runtime_generation(), None);
        assert_eq!(self.run.state(), None);
        assert!(self.run.take_event_loop().is_some());
        assert!(self.run.take_event_loop().is_none());
        self.retired = true;
        release?;
        assert_eq!(
            report.outcome(),
            DesktopRunOutcome::Completed(AppExit::Requested)
        );
        assert!(!report.diagnostics().has_errors());
        assert!(report.startup_content_closure().is_none());
        println!(
            "two_level_play_flow_readiness_checkpoint: exited-cleanly original_hwnd_destroyed=true original_runtime_retired=true original_pump_returned=true"
        );
        Ok(())
    }
}

fn temporary() -> Result<PathBuf, String> {
    let parent = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
    let repo = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .map_err(|error| error.to_string())?;
    if parent.starts_with(repo) {
        return Err("readiness cache must be outside the repository".into());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let path = parent.join(format!("nara-readiness-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn remove_temporary(path: &Path) -> Result<(), String> {
    let parent = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
    let actual = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if actual != path
        || actual.parent() != Some(parent.as_path())
        || !actual
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("nara-readiness-"))
        || fs::symlink_metadata(path)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("refusing readiness cleanup after temporary directory identity changed".into());
    }
    fs::remove_dir_all(actual).map_err(|error| error.to_string())
}
