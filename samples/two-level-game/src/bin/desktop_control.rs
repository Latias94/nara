//! Finite public desktop Runtime control through the original native session and stock renderer.

#[path = "../../tests/support/desktop_capture.rs"]
mod desktop_capture;

use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{
        AppExit, FixedTime, FrameExecutionStart, RuntimeControl, RuntimeControlStatus, RuntimeState,
    },
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandPayload,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue,
    },
    project_host::{
        DesktopControlError, DesktopPresentationControl, DesktopRun, DesktopRunIntent,
        DesktopRunOptions, DesktopRunOutcome, DesktopRunProgress, WinitEventLoop,
    },
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use two_level_game::{
    GamePhase, GameSnapshot,
    levels::{LevelId, LevelKey},
};
use two_level_game::{
    resume::{
        GameClock, LevelContentStamp, PlayerState, ResumeCompatibility, ResumeState,
        SessionCounters,
    },
    save::{Checkpoint, CheckpointLocation, CollectedPickup, MarkerId, MarkerRecord},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(2);

#[derive(Default)]
struct Presented {
    frames: u64,
    logical_tick: u64,
    player: [f32; 2],
    level: Option<LevelKey>,
    target: Option<BackendWindowHandles>,
    frame: Option<desktop_capture::PresentedGameFrame>,
    failure: Option<String>,
}

struct Game {
    run: DesktopRun,
    sequence: u64,
    presented: Rc<RefCell<Presented>>,
    expected: Option<ResumeState>,
}

fn main() -> ExitCode {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: desktop_control");
        return ExitCode::FAILURE;
    }
    let event_loop = match reject_pending_resume() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("desktop_control initial-pause refusal: {error}");
            return ExitCode::FAILURE;
        }
    };
    let presented = Rc::new(RefCell::new(Presented::default()));
    let observed = Rc::clone(&presented);
    let options = DesktopRunOptions::new()
        .with_event_loop(event_loop)
        .with_initial_pause()
        .with_initial_visibility(false)
        .with_presentation_observer(move |observation| {
            if let Some(world) = observation.world() {
                observed
                    .borrow_mut()
                    .target
                    .get_or_insert_with(|| world.resource::<BackendWindowHandles>().clone());
            }
            match desktop_capture::join_presented(observation) {
                Ok(Some(frame)) => {
                    let mut presented = observed.borrow_mut();
                    presented.frames += 1;
                    presented.logical_tick = frame.snapshot().logical_tick;
                    presented.player = frame.snapshot().player.to_array();
                    presented.level = frame.snapshot().level;
                    presented.frame = Some(frame);
                }
                Ok(None) => {}
                Err(error) => {
                    observed.borrow_mut().failure = Some(error.to_string());
                }
            }
            DesktopPresentationControl::Continue
        });
    let root = match two_level_game::open_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("desktop_control source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut game = Game {
        run: DesktopRun::new(
            root,
            DesktopRunIntent::new()
                .with_profile("desktop")
                .with_recipe(
                    two_level_game::product_recipe()
                        .add_plugin::<desktop_capture::DesktopCapturePlugin>()
                        .expect("the concrete capture observer must compose with the game"),
                )
                .with_execution(options),
        ),
        sequence: 0,
        presented,
        expected: None,
    };
    let trajectory = catch_unwind(AssertUnwindSafe(|| game.qualify())).unwrap_or_else(|_| {
        Err("desktop control trajectory panicked; original cleanup is retained".into())
    });
    let cleanup = game.close();
    if matches!(
        game.run.progress(),
        DesktopRunProgress::Retiring | DesktopRunProgress::CleanupIncomplete
    ) {
        std::mem::forget(game);
        eprintln!("desktop_control: original native cleanup remains incomplete at process exit");
        return ExitCode::FAILURE;
    }
    let verified = trajectory.and(cleanup).and_then(|()| {
        let frame = game.presented.borrow().frame.clone().ok_or("no joined final game frame")?;
        let expected = game.expected.as_ref().ok_or("no independent visual expectation")?;
        let replay = desktop_capture::replay_and_check(&frame, expected).map_err(|error| error.to_string())?;
        assert_eq!(replay.rgba8.len(), 960 * 540 * 4);
        assert!(replay.checks.player > 0 && replay.checks.markers > 0 && replay.checks.visible_pickups > 0);
        assert!(replay.checks.background > 0);
        println!("two_level_desktop_pixels: ok original_app_frame={} original_render_frame={} player={} markers={} pickups={} background={}",
            frame.presentation.app_frame, frame.presentation.render_frame, replay.checks.player,
            replay.checks.markers, replay.checks.visible_pickups, replay.checks.background);
        Ok(())
    });
    match verified {
        Ok(()) => {
            println!(
                "two_level_desktop_control: ok paused_capture=true exact_steps=true ordinary_gameplay=true original_native_retired=true"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("desktop_control: {error}");
            ExitCode::FAILURE
        }
    }
}

fn reject_pending_resume() -> Result<WinitEventLoop, String> {
    let mut app = nara::app::App::new();
    app.add_plugins(nara::window::WindowPlugin::default())
        .map_err(|error| error.to_string())?;
    let mut runtime = nara::app::RuntimeInstance::start(app)
        .map_err(|error| format!("construct the original control Runtime: {error:?}"))?;
    runtime.request_control(RuntimeControl::Pause);
    runtime
        .drive(Duration::ZERO)
        .map_err(|error| error.to_string())?;
    assert_eq!(runtime.state(), RuntimeState::Paused);
    let resume = runtime.request_control(RuntimeControl::Resume);
    assert!(matches!(
        resume,
        nara::app::RuntimeControlRequestResult::Accepted(_)
    ));
    let mut run = DesktopRun::from_runtime(runtime, DesktopRunOptions::new().with_initial_pause());
    let report = run.execute();
    if run.progress() == DesktopRunProgress::CleanupIncomplete {
        std::mem::forget(run);
        return Err("refused initial Pause retains incomplete original cleanup".into());
    }
    assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "project.desktop.initial-pause-rejected")
    );
    run.take_event_loop()
        .ok_or_else(|| "initial-pause refusal lost the original physical pump".into())
}

impl Game {
    fn qualify(&mut self) -> Result<(), String> {
        assert_eq!(self.run.progress(), DesktopRunProgress::NotStarted);
        self.settle()?;
        assert_eq!(self.run.state(), Some(RuntimeState::Paused));
        let initial = self.snapshot()?;
        assert_eq!(initial.player.to_array(), [-120.0, 0.0]);
        assert_eq!(initial.logical_tick, 0);
        assert_eq!(initial.held, [false; 4]);
        self.wait_presented(&initial)?;
        let meadow = self
            .presented
            .borrow()
            .frame
            .as_ref()
            .unwrap()
            .source_stamp();
        let generation = self.run.runtime_generation().unwrap();
        self.step(Some(movement(true)?))?;
        let moved = self.snapshot()?;
        assert_eq!(moved.held, [false, true, false, false]);
        assert!(moved.player.x > initial.player.x);
        for _ in 0..120 {
            if self.snapshot()?.score == 10 {
                break;
            }
            self.step(None)?;
        }
        assert_eq!(self.snapshot()?.score, 10);
        self.step(Some(movement(false)?))?;
        let meadow_marker = self.snapshot()?.player.to_array();
        self.step(Some(simple("two-level-game.marker")?))?;
        assert_eq!(self.snapshot()?.markers.len(), 1);
        self.step(Some(simple("two-level-game.travel")?))?;
        assert_eq!(
            self.snapshot()?.level,
            Some(LevelKey::main(LevelId::Cavern))
        );
        self.step(Some(simple("two-level-game.marker")?))?;
        self.step(Some(movement(true)?))?;
        for _ in 0..9 {
            self.step(None)?;
        }
        self.step(Some(movement(false)?))?;
        let before = self.snapshot()?;
        self.step(None)?;
        let after = self.snapshot()?;
        assert_eq!(after.logical_tick, before.logical_tick + 1);
        assert_eq!(after.player, before.player);
        assert_eq!(self.run.runtime_generation(), Some(generation));
        self.wait_presented(&after)?;
        let cavern = self
            .presented
            .borrow()
            .frame
            .as_ref()
            .unwrap()
            .source_stamp();
        let frames = self.presented.borrow().frames;
        self.run.reveal().map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        while self.presented.borrow().frames <= frames {
            self.poll(deadline)?;
        }
        self.settle()?;
        self.wait_presented(&after)?;
        let resume = self
            .run
            .request_control(RuntimeControl::Resume)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        while self.run.control_status(resume) != Some(RuntimeControlStatus::Applied) {
            self.poll(deadline)?;
        }
        assert_eq!(self.run.state(), Some(RuntimeState::Running));
        assert!(self.run.capture(|_| ()).is_err());
        let pause = self
            .run
            .request_control(RuntimeControl::Pause)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        while self.run.control_status(pause) != Some(RuntimeControlStatus::Applied) {
            self.poll(deadline)?;
        }
        self.settle()?;
        let final_snapshot = self.snapshot()?;
        assert_eq!(final_snapshot.player, after.player);
        assert_eq!(final_snapshot.held, [false; 4]);
        self.wait_presented(&final_snapshot)?;
        let expected = visual_expectation(&final_snapshot, meadow_marker, meadow, cavern);
        self.presented
            .borrow()
            .frame
            .as_ref()
            .unwrap()
            .assert_expected(&expected)
            .map_err(|error| error.to_string())?;
        self.expected = Some(expected);
        assert!(self.run.capture(|_| ()).is_ok());
        Ok(())
    }

    fn poll(&mut self, deadline: Instant) -> Result<(), String> {
        if Instant::now() >= deadline {
            return Err("desktop operation deadline elapsed".into());
        }
        if let Some(error) = &self.presented.borrow().failure {
            return Err(error.clone());
        }
        match self.run.poll(POLL) {
            DesktopRunProgress::Running => Ok(()),
            progress => Err(format!("desktop operation stopped: {progress:?}")),
        }
    }

    fn snapshot(&self) -> Result<GameSnapshot, String> {
        self.run
            .capture(|world| world.resource::<GameSnapshot>().clone())
            .map_err(|error| error.to_string())
    }

    fn settle(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self
                .run
                .capture(|world| world.resource::<GameSnapshot>().clone())
            {
                Ok(snapshot) if snapshot.initialized && snapshot.phase == GamePhase::Running => {
                    if let Some(error) = snapshot.error {
                        return Err(error);
                    }
                    return Ok(());
                }
                Ok(snapshot) => {
                    if let Some(error) = snapshot.error {
                        return Err(error);
                    }
                }
                Err(DesktopControlError::NotPaused) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn step(&mut self, command: Option<GameplayCommandDraft>) -> Result<(), String> {
        let tick = self
            .run
            .capture(|world| world.resource::<FixedTime>().tick())
            .map_err(|error| error.to_string())?;
        if let Some(command) = command {
            self.sequence += 1;
            self.run
                .submit(GameplayCommandSubmission::new(
                    GameplayCommandTick::new(tick + 1).unwrap(),
                    GameplayCommandIngressSource::external("two-level-game.desktop-control")
                        .unwrap(),
                    GameplayCommandSourceSequence::new(self.sequence).unwrap(),
                    command,
                ))
                .map_err(|error| error.to_string())?;
            assert!(matches!(
                self.run.capture(|_| ()),
                Err(DesktopControlError::NotPaused)
            ));
        }
        let ticket = self
            .run
            .request_control(RuntimeControl::StepFixedTick)
            .map_err(|error| error.to_string())?;
        assert!(matches!(
            self.run.capture(|_| ()),
            Err(DesktopControlError::NotPaused)
        ));
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self.run.control_status(ticket) {
                Some(RuntimeControlStatus::Applied) => break,
                Some(RuntimeControlStatus::Pending) => {}
                status => return Err(format!("exact desktop step failed: {status:?}")),
            }
        }
        self.settle()?;
        assert_eq!(
            self.run
                .capture(|world| world.resource::<FixedTime>().tick())
                .map_err(|error| error.to_string())?,
            tick + 1
        );
        Ok(())
    }

    fn wait_presented(&mut self, expected: &GameSnapshot) -> Result<(), String> {
        let minimum_frame = self
            .run
            .capture(|world| world.resource::<FrameExecutionStart>().frame())
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            {
                let presented = self.presented.borrow();
                if presented.frames > 0
                    && presented.logical_tick == expected.logical_tick
                    && presented.player == expected.player.to_array()
                    && presented.level == expected.level
                    && presented
                        .frame
                        .as_ref()
                        .is_some_and(|frame| frame.presentation.app_frame >= minimum_frame)
                {
                    return Ok(());
                }
            }
            self.poll(deadline)?;
        }
    }

    fn close(&mut self) -> Result<(), String> {
        self.run.request_close();
        assert!(self.run.capture(|_| ()).is_err());
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err("desktop cleanup remains retained at its deadline".into());
            }
            match self.run.poll(POLL) {
                DesktopRunProgress::Completed(AppExit::Requested) => break,
                DesktopRunProgress::Retiring => assert!(self.run.take_event_loop().is_none()),
                DesktopRunProgress::CleanupIncomplete => {
                    let report = self.run.execute();
                    if report.outcome() == DesktopRunOutcome::Failed {
                        return Err(format!("desktop cleanup failed: {report:?}"));
                    }
                }
                progress => return Err(format!("desktop close failed: {progress:?}")),
            }
        }
        let presented = self.presented.borrow();
        let target = presented
            .target
            .as_ref()
            .ok_or("no original native target was observed")?
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert_eq!(target.fault, None);
        assert!(!target.surface_active && !target.provider_present && target.native_destroyed);
        assert!(self.run.take_event_loop().is_some());
        assert!(self.run.take_event_loop().is_none());
        Ok(())
    }
}

/// Finite visual expectations for this exact command trajectory. Configuration is not observed
/// by this oracle; this value is never offered as an authoritative capture or restore input.
fn visual_expectation(
    snapshot: &GameSnapshot,
    meadow_marker: [f32; 2],
    meadow: nara::project_host::ProjectSceneContentStamp,
    cavern: nara::project_host::ProjectSceneContentStamp,
) -> ResumeState {
    use nara::scene::SceneEntityId;
    let meadow_key = LevelKey::main(LevelId::Meadow);
    let cavern_key = LevelKey::main(LevelId::Cavern);
    assert_eq!(snapshot.level, Some(cavern_key));
    assert_eq!(snapshot.score, 10);
    assert_eq!(snapshot.travel_count, 1);
    assert_eq!(snapshot.held, [false; 4]);
    assert_eq!(
        snapshot.logical_elapsed_nanos,
        snapshot.logical_tick * 20_000_000
    );
    let mut progress = Checkpoint::new(CheckpointLocation {
        level: cavern_key,
        anchor: SceneEntityId::new("checkpoint").unwrap(),
    });
    progress.score = 10;
    progress.collected = vec![CollectedPickup {
        level: meadow_key,
        entity: SceneEntityId::new("gem-near").unwrap(),
    }];
    progress.markers = vec![
        MarkerRecord {
            id: MarkerId::new(1).unwrap(),
            level: meadow_key,
            position: meadow_marker,
        },
        MarkerRecord {
            id: MarkerId::new(2).unwrap(),
            level: cavern_key,
            position: [-120.0, 0.0],
        },
    ];
    progress.next_marker_id = MarkerId::new(3).unwrap();
    ResumeState {
        progress,
        player: PlayerState {
            translation: snapshot.player.to_array(),
            rotation: 0.0,
            scale: [1.0; 2],
        },
        held: [false; 4],
        clock: GameClock {
            ticks: snapshot.logical_tick,
            elapsed_nanos: snapshot.logical_tick * 20_000_000,
            timestep_nanos: 20_000_000,
        },
        counters: SessionCounters {
            travel_count: 1,
            restarts: 0,
            saves: 0,
            restores: 0,
        },
        compatibility: ResumeCompatibility {
            schema: cavern.schema_fingerprint().as_bytes(),
            configuration: [0; 32],
            levels: vec![
                LevelContentStamp {
                    level: LevelId::Meadow,
                    manifest: *meadow.manifest_digest().as_bytes(),
                    content: *meadow.content_digest().as_bytes(),
                },
                LevelContentStamp {
                    level: LevelId::Cavern,
                    manifest: *cavern.manifest_digest().as_bytes(),
                    content: *cavern.content_digest().as_bytes(),
                },
            ],
        },
    }
}

fn movement(pressed: bool) -> Result<GameplayCommandDraft, String> {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(1))
        .map_err(|error| error.to_string())?;
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .map_err(|error| error.to_string())?;
    Ok(simple("two-level-game.move")?.with_payload(payload))
}

fn simple(id: &str) -> Result<GameplayCommandDraft, String> {
    Ok(GameplayCommandDraft::new(
        GameplayCommandTypeId::new(id).map_err(|error| error.to_string())?,
    ))
}
