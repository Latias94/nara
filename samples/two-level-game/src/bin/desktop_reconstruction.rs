//! Sequential native reconstruction through the public game and DesktopRun owners.
//!
//! Both logical games run on this main thread and reuse one recovered physical event loop.
//! The incumbent is fully retired before creating the candidate. This qualifies reconstruction,
//! original presentation and input activation, not incumbent-preserving cross-process adoption.
//! Physical activation requires a focused Windows desktop with A/D/W/S released.

#![forbid(unsafe_code)]

#[path = "../../tests/support/desktop_capture.rs"]
mod desktop_capture;

use std::{
    cell::RefCell,
    collections::BTreeSet,
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
        GameplayCommandQueue, GameplayCommandQueueStats, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    input::{ActionOutcomes, ActionPhase, InputSeatId, KeyboardHold, SemanticActions},
    project_host::{
        DesktopControlError, DesktopPresentationControl, DesktopRun, DesktopRunIntent,
        DesktopRunOptions, DesktopRunOutcome, DesktopRunProgress, WinitEventLoop,
        WinitInputActivation,
    },
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use two_level_game::{
    GamePhase, GameResumeError, GameSnapshot, capture_desktop_resume,
    levels::{LevelId, LevelKey},
    movement_keyboard_bindings, restore_desktop_resume,
    resume::{ResumeState, RuntimeTimeState, encode_resume},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(2);

#[derive(Default)]
struct Presented {
    target: Option<BackendWindowHandles>,
    frame: Option<desktop_capture::PresentedGameFrame>,
    failure: Option<String>,
}

struct Game {
    run: DesktopRun,
    sequence: u64,
    presented: Rc<RefCell<Presented>>,
}

struct Capture {
    state: ResumeState,
    bytes: Vec<u8>,
    app_frame: u64,
    engine_tick: u64,
    frame: desktop_capture::PresentedGameFrame,
    target: BackendWindowHandles,
}

fn main() -> ExitCode {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: desktop_reconstruction");
        return ExitCode::FAILURE;
    }
    match qualify() {
        Ok(()) => {
            println!(
                "two_level_desktop_reconstruction: ok sequential_runtimes=true shared_physical_loop=true canonical_bytes=true fresh_owners=true restored_presentation=true native_cleanup=true"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("desktop_reconstruction: {error}");
            ExitCode::FAILURE
        }
    }
}

fn qualify() -> Result<(), String> {
    let mut original = Game::new(None)?;
    let captured = catch_unwind(AssertUnwindSafe(|| original.prepare_original()))
        .unwrap_or_else(|_| Err("original trajectory panicked; retaining original cleanup".into()));
    let physical_loop = original.close("original")?;
    let captured = captured?;

    let mut plain = Game::new_with_observer(Some(physical_loop), false)?;
    let refused = catch_unwind(AssertUnwindSafe(|| {
        plain.refuse_observer_mismatch(&captured)
    }))
    .unwrap_or_else(|_| Err("ordinary-recipe refusal panicked; retaining original cleanup".into()));
    let physical_loop = plain.close("ordinary-recipe-control")?;
    refused?;

    let mut candidate = Game::new(Some(physical_loop))?;
    let restored = catch_unwind(AssertUnwindSafe(|| candidate.reconstruct(&captured)))
        .unwrap_or_else(
            |_| Err("candidate trajectory panicked; retaining original cleanup".into()),
        );
    let physical_loop = candidate.close("candidate")?;
    let restored = restored?;
    drop(physical_loop);

    let source_pixels = desktop_capture::replay_and_check(&captured.frame, &captured.state)
        .map_err(|error| error.to_string())?;
    let restored_pixels = desktop_capture::replay_and_check(&restored.frame, &restored.state)
        .map_err(|error| error.to_string())?;
    for pixels in [&source_pixels, &restored_pixels] {
        assert_eq!(pixels.rgba8.len(), 960 * 540 * 4);
        assert!(pixels.checks.player > 0 && pixels.checks.markers > 0);
        assert!(pixels.checks.visible_pickups > 0 && pixels.checks.collected_pickups > 0);
        assert!(pixels.checks.background > 0);
    }
    assert!(
        source_pixels.rgba8 == restored_pixels.rgba8,
        "independent replays of the original and reconstructed presentations differ"
    );
    println!(
        "two_level_desktop_reconstruction_pixels: ok original_app_frame={} restored_app_frame={} full_capture_frame={} equal_replays=true player={} markers={} visible_pickups={} collected_pickups={} background={}",
        captured.frame.presentation.app_frame,
        restored.frame.presentation.app_frame,
        restored.app_frame,
        restored_pixels.checks.player,
        restored_pixels.checks.markers,
        restored_pixels.checks.visible_pickups,
        restored_pixels.checks.collected_pickups,
        restored_pixels.checks.background
    );
    Ok(())
}

impl Game {
    fn new(event_loop: Option<WinitEventLoop>) -> Result<Self, String> {
        Self::new_with_observer(event_loop, true)
    }

    fn new_with_observer(
        event_loop: Option<WinitEventLoop>,
        evidence: bool,
    ) -> Result<Self, String> {
        let root = two_level_game::open_project_root().map_err(|error| error.to_string())?;
        let recipe = two_level_game::product_recipe();
        let recipe = if evidence {
            recipe
                .add_plugin::<desktop_capture::DesktopCapturePlugin>()
                .map_err(|error| error.to_string())?
        } else {
            recipe
        };
        let presented = Rc::new(RefCell::new(Presented::default()));
        let observed = Rc::clone(&presented);
        let mut options = DesktopRunOptions::new()
            .with_initial_pause()
            .with_initial_visibility(false)
            .with_input_isolation(true)
            .with_presentation_observer(move |observation| {
                if let Some(world) = observation.world() {
                    observed
                        .borrow_mut()
                        .target
                        .get_or_insert_with(|| world.resource::<BackendWindowHandles>().clone());
                }
                if !evidence {
                    return DesktopPresentationControl::Continue;
                }
                match desktop_capture::join_presented(observation) {
                    Ok(Some(frame)) => observed.borrow_mut().frame = Some(frame),
                    Ok(None) => {}
                    Err(error) => observed.borrow_mut().failure = Some(error.to_string()),
                }
                DesktopPresentationControl::Continue
            });
        if let Some(event_loop) = event_loop {
            options = options.with_event_loop(event_loop);
        }
        Ok(Self {
            run: DesktopRun::new(
                root,
                DesktopRunIntent::new()
                    .with_profile("desktop")
                    .with_recipe(recipe)
                    .with_execution(options),
            ),
            sequence: 0,
            presented,
        })
    }

    fn refuse_observer_mismatch(&mut self, original: &Capture) -> Result<(), String> {
        self.settle()?;
        let fresh = capture_desktop_resume(&self.run).map_err(|error| error.to_string())?;
        let bytes = encode_resume(&fresh).map_err(|error| error.to_string())?;
        let before = self.command_stats()?;
        assert_ne!(
            fresh.compatibility.configuration,
            original.state.compatibility.configuration
        );
        assert!(matches!(
            restore_desktop_resume(&mut self.run, &original.state, Instant::now() + TIMEOUT),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("configuration")
        ));
        assert_eq!(self.canonical_bytes()?, bytes);
        assert_eq!(self.command_stats()?, before);
        assert_eq!(self.engine_tick()?, 0);
        println!(
            "two_level_desktop_reconstruction_recipe: ok actual_observer_absence_refused=true fresh_capture_unchanged=true"
        );
        Ok(())
    }

    fn prepare_original(&mut self) -> Result<Capture, String> {
        self.settle()?;
        assert_eq!(self.run.state(), Some(RuntimeState::Paused));
        assert_eq!(
            self.run.input_activation(),
            Some(WinitInputActivation::Isolated)
        );
        let mut time = RuntimeTimeState::from_continuation(
            self.run
                .capture_time_continuation()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        time.pending_nanos = 13_000_000;
        self.run
            .restore_time_continuation(time.to_continuation().map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        assert!(self.run.capture(|_| ()).is_err());
        self.settle()?;

        self.walk_to_score(10)?;
        self.step(Some(movement(false)?))?;
        let meadow_marker = self.snapshot()?.player.to_array();
        self.step(Some(simple("two-level-game.marker")?))?;
        self.step(Some(simple("two-level-game.travel")?))?;
        assert_eq!(
            self.snapshot()?.level,
            Some(LevelKey::main(LevelId::Cavern))
        );
        self.walk_to_score(20)?;
        self.step(Some(movement(false)?))?;
        let cavern_marker = self.snapshot()?.player.to_array();
        self.step(Some(simple("two-level-game.marker")?))?;
        self.step(Some(simple("two-level-game.instance")?))?;
        assert_eq!(
            self.snapshot()?.level,
            Some(LevelKey::new(LevelId::Cavern, 1).unwrap())
        );
        self.walk_to_score(30)?;
        self.step(Some(movement(false)?))?;
        let instance_marker = self.snapshot()?.player.to_array();
        self.step(Some(simple("two-level-game.marker")?))?;
        self.step(Some(movement(true)?))?;
        for _ in 0..12 {
            self.step(None)?;
        }
        assert!(matches!(capture_desktop_resume(&self.run),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("keyboard inventory")));
        let holds = movement_keyboard_bindings()
            .into_iter()
            .zip(self.snapshot()?.held)
            .map(|(binding, held)| KeyboardHold { binding, held })
            .collect();
        let before = self.command_stats()?;
        self.run
            .restore_keyboard_holds(holds)
            .map_err(|error| error.to_string())?;
        assert!(self.run.capture(|_| ()).is_err());
        self.settle()?;
        assert_eq!(self.command_stats()?.accepted, before.accepted);
        self.assert_input(true)?;
        let captured = self.full_capture()?;
        assert_meaningful(
            &captured.state,
            [meadow_marker, cavern_marker, instance_marker],
        );
        println!(
            "two_level_desktop_reconstruction_source: ok canonical_bytes={} configuration={:?} runtime={:?} scene_domain={:?} scene_instance={:?} render_source={:?} backend={} surface={} device={} app_frame={}",
            captured.bytes.len(),
            captured.state.compatibility.configuration,
            captured.frame.presentation.runtime,
            captured.frame.scene_instance().domain_id(),
            captured.frame.scene_instance().instance_id(),
            captured.frame.input().packet().source_generation(),
            captured.frame.presentation.backend_instance,
            captured.frame.presentation.surface_generation,
            captured.frame.presentation.device_epoch,
            captured.app_frame
        );
        Ok(captured)
    }

    fn reconstruct(&mut self, original: &Capture) -> Result<Capture, String> {
        self.settle()?;
        let fresh = capture_desktop_resume(&self.run).map_err(|error| error.to_string())?;
        let fresh_bytes = encode_resume(&fresh).map_err(|error| error.to_string())?;
        assert_eq!(fresh.clock.ticks, 0);
        assert_eq!(fresh.held, [false; 4]);
        assert_eq!(
            fresh.compatibility.configuration,
            original.state.compatibility.configuration
        );
        assert_ne!(
            self.run.runtime_generation(),
            Some(original.frame.presentation.runtime)
        );
        let before = self.command_stats()?;
        let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        assert!(
            matches!(restore_desktop_resume(&mut self.run, &original.state, expired),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("deadline"))
        );
        assert_eq!(self.canonical_bytes()?, fresh_bytes);
        let mut incompatible = original.state.clone();
        incompatible.compatibility.configuration[0] ^= 1;
        assert!(
            matches!(restore_desktop_resume(&mut self.run, &incompatible, Instant::now() + TIMEOUT),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("configuration"))
        );
        assert_eq!(self.canonical_bytes()?, fresh_bytes);
        assert_eq!(self.command_stats()?, before);
        assert_eq!(self.engine_tick()?, 0);
        assert_eq!(
            self.run.input_activation(),
            Some(WinitInputActivation::Isolated)
        );
        println!(
            "two_level_desktop_reconstruction_preflight: ok deadline_refused=true configuration_refused=true fresh_capture_unchanged=true"
        );

        restore_desktop_resume(&mut self.run, &original.state, Instant::now() + TIMEOUT)
            .map_err(|error| error.to_string())?;
        assert_eq!(self.canonical_bytes()?, original.bytes);
        self.assert_input(true)?;
        let restored = self.full_capture()?;
        assert_eq!(restored.bytes, original.bytes);
        assert_ne!(
            restored.frame.presentation.runtime,
            original.frame.presentation.runtime
        );
        assert_ne!(
            restored.frame.scene_instance().domain_id(),
            original.frame.scene_instance().domain_id()
        );
        assert_ne!(
            restored.frame.input().packet().source_generation(),
            original.frame.input().packet().source_generation()
        );
        assert_ne!(
            restored.frame.presentation.backend_instance,
            original.frame.presentation.backend_instance
        );
        assert!(!restored.target.shares_authority_with(&original.target));
        assert_eq!(restored.frame.source_stamp(), original.frame.source_stamp());
        assert_eq!(restored.engine_tick, 1);
        assert!(restored.engine_tick < original.engine_tick);
        assert!(restored.frame.presentation.app_frame >= restored.app_frame);
        assert_eq!(
            self.run.input_activation(),
            Some(WinitInputActivation::Isolated)
        );
        assert!(self.run.request_control(RuntimeControl::Resume).is_err());

        let stats = self.command_stats()?;
        let mut expected = original.state.clone();
        for _ in 0..3 {
            self.step(None)?;
            advance(&mut expected, true);
            assert_eq!(
                self.canonical_bytes()?,
                encode_resume(&expected).map_err(|error| error.to_string())?
            );
            self.assert_input(true)?;
        }
        let continued = self.command_stats()?;
        assert_eq!(
            continued.accepted, stats.accepted,
            "restored holds must not submit Started commands"
        );
        assert_eq!(continued.admitted, stats.admitted);
        assert_eq!(continued.acknowledged, stats.acknowledged);
        self.activate_after_release_gap(&mut expected)?;
        println!(
            "two_level_desktop_reconstruction_continuation: ok held_steps=3 new_started_commands=0 release_gap_stop=true no_stuck_hold=true"
        );
        Ok(restored)
    }

    fn activate_after_release_gap(&mut self, expected: &mut ResumeState) -> Result<(), String> {
        self.run.reveal().map_err(|error| error.to_string())?;
        self.settle()?;
        self.run
            .request_input_activation()
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(error) = self.run.input_activation_error() {
                return Err(error.to_string());
            }
            match self.run.input_activation() {
                Some(WinitInputActivation::AwaitingFocus) => self.poll(deadline)?,
                Some(WinitInputActivation::ReconciliationPending) => break,
                state => {
                    return Err(format!(
                        "release-gap qualification requires physical D up and actual focus, got {state:?}"
                    ));
                }
            }
        }
        self.settle()?;
        assert!(
            self.run
                .capture_keyboard_holds(&movement_keyboard_bindings())
                .is_err()
        );
        assert!(capture_desktop_resume(&self.run).is_err());
        assert!(self.run.request_control(RuntimeControl::Resume).is_err());
        let stopped_at = expected.player.translation;
        let before = self.command_stats()?;
        self.step(None)?;
        expected.held = [false; 4];
        advance(expected, false);
        assert_eq!(self.snapshot()?.player.to_array(), stopped_at);
        while self.run.input_activation() != Some(WinitInputActivation::Active) {
            if let Some(error) = self.run.input_activation_error() {
                return Err(error.to_string());
            }
            self.poll(deadline)?;
        }
        self.settle()?;
        self.assert_input(false)?;
        assert_eq!(
            self.canonical_bytes()?,
            encode_resume(expected).map_err(|error| error.to_string())?
        );
        let stopped = self.command_stats()?;
        assert_eq!(stopped.accepted, before.accepted + 1);
        assert_eq!(stopped.admitted, before.admitted + 1);
        assert_eq!(stopped.acknowledged, before.acknowledged + 1);
        self.step(None)?;
        advance(expected, false);
        assert_eq!(
            self.canonical_bytes()?,
            encode_resume(expected).map_err(|error| error.to_string())?
        );
        assert_eq!(self.command_stats()?.accepted, stopped.accepted);
        self.full_capture()?;
        Ok(())
    }

    fn full_capture(&mut self) -> Result<Capture, String> {
        let state = capture_desktop_resume(&self.run).map_err(|error| error.to_string())?;
        let bytes = encode_resume(&state).map_err(|error| error.to_string())?;
        let app_frame = self
            .run
            .capture(|world| world.resource::<FrameExecutionStart>().frame())
            .map_err(|error| error.to_string())?;
        let engine_tick = self.engine_tick()?;
        let deadline = Instant::now() + TIMEOUT;
        let frame = loop {
            let frame = self.presented.borrow().frame.clone();
            if let Some(frame) = frame
                && frame.presentation.app_frame >= app_frame
                && frame.snapshot().logical_tick == state.clock.ticks
            {
                frame
                    .assert_expected(&state)
                    .map_err(|error| error.to_string())?;
                break frame;
            }
            self.poll(deadline)?;
        };
        assert_eq!(self.canonical_bytes()?, bytes);
        assert_eq!(
            self.run.runtime_generation(),
            Some(frame.presentation.runtime)
        );
        let target = self
            .presented
            .borrow()
            .target
            .clone()
            .ok_or("no original native target")?;
        let native = target
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        assert_eq!(native.phase, WindowTargetPhase::Active);
        assert_eq!(
            native.surface_generation,
            Some(frame.presentation.surface_generation)
        );
        assert!(native.provider_present && native.surface_active && !native.native_destroyed);
        Ok(Capture {
            state,
            bytes,
            app_frame,
            engine_tick,
            frame,
            target,
        })
    }

    fn assert_input(&self, held: bool) -> Result<(), String> {
        let bindings = movement_keyboard_bindings();
        let holds = self
            .run
            .capture_keyboard_holds(&bindings)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            holds.iter().map(|hold| hold.held).collect::<Vec<_>>(),
            [false, held, false, false]
        );
        self.run
            .capture(|world| {
                let actions = world.resource::<SemanticActions>();
                assert_eq!(
                    actions.pressed(
                        InputSeatId::PRIMARY,
                        &bindings[1].action,
                        &bindings[1].context
                    ),
                    held
                );
                assert!(
                    actions
                        .as_slice()
                        .iter()
                        .all(|edge| edge.phase != ActionPhase::Started)
                );
                assert!(
                    world
                        .resource::<ActionOutcomes>()
                        .as_slice()
                        .iter()
                        .all(|edge| edge.phase != ActionPhase::Started)
                );
            })
            .map_err(|error| error.to_string())
    }

    fn walk_to_score(&mut self, score: u32) -> Result<(), String> {
        self.step(Some(movement(true)?))?;
        for _ in 0..100 {
            let snapshot = self.snapshot()?;
            if snapshot.score == score {
                assert_eq!(snapshot.held, [false, true, false, false]);
                return Ok(());
            }
            assert!(snapshot.score < score, "unexpected pickup: {snapshot:?}");
            self.step(None)?;
        }
        Err(format!("movement failed to reach score {score}"))
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        encode_resume(&capture_desktop_resume(&self.run).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
    }

    fn engine_tick(&self) -> Result<u64, String> {
        self.run
            .capture(|world| world.resource::<FixedTime>().tick())
            .map_err(|error| error.to_string())
    }

    fn command_stats(&self) -> Result<GameplayCommandQueueStats, String> {
        self.run
            .capture(|world| world.resource::<GameplayCommandQueue>().stats())
            .map_err(|error| error.to_string())
    }

    fn snapshot(&self) -> Result<GameSnapshot, String> {
        self.run
            .capture(|world| world.resource::<GameSnapshot>().clone())
            .map_err(|error| error.to_string())
    }

    fn poll(&mut self, deadline: Instant) -> Result<(), String> {
        if Instant::now() >= deadline {
            return Err("desktop reconstruction operation reached its deadline".into());
        }
        if let Some(error) = &self.presented.borrow().failure {
            return Err(error.clone());
        }
        match self.run.poll(POLL) {
            DesktopRunProgress::Running => Ok(()),
            progress => Err(format!("desktop reconstruction stopped: {progress:?}")),
        }
    }

    fn settle(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self.snapshot() {
                Ok(snapshot) => {
                    if let Some(error) = snapshot.error {
                        return Err(error);
                    }
                    if snapshot.initialized
                        && snapshot.phase == GamePhase::Running
                        && !snapshot.restoring
                    {
                        assert_eq!(self.run.state(), Some(RuntimeState::Paused));
                        return Ok(());
                    }
                }
                Err(error) => {
                    if !matches!(
                        self.run.capture(|_| ()),
                        Err(DesktopControlError::NotPaused)
                    ) {
                        return Err(error);
                    }
                }
            }
        }
    }

    fn step(&mut self, command: Option<GameplayCommandDraft>) -> Result<(), String> {
        let tick = self.engine_tick()?;
        if let Some(command) = command {
            self.sequence += 1;
            self.run
                .submit(GameplayCommandSubmission::new(
                    GameplayCommandTick::new(tick + 1).unwrap(),
                    GameplayCommandIngressSource::external("two-level-game.desktop-reconstruction")
                        .unwrap(),
                    GameplayCommandSourceSequence::new(self.sequence).unwrap(),
                    command,
                ))
                .map_err(|error| error.to_string())?;
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
                status => return Err(format!("original exact Step failed: {status:?}")),
            }
        }
        self.settle()?;
        assert_eq!(self.engine_tick()?, tick + 1);
        Ok(())
    }

    fn close(mut self, role: &str) -> Result<WinitEventLoop, String> {
        let result = self.retire();
        match result {
            Ok(event_loop) => {
                println!(
                    "two_level_desktop_reconstruction_retired: role={role} native_destroyed=true surface_retired=true provider_released=true physical_loop_recovered=true"
                );
                Ok(event_loop)
            }
            Err(error) => {
                std::mem::forget(self);
                Err(format!(
                    "{role} cleanup remains retained until process exit: {error}"
                ))
            }
        }
    }

    fn retire(&mut self) -> Result<WinitEventLoop, String> {
        self.run.request_close();
        if self.run.capture(|_| ()).is_ok() {
            return Err("Close failed to fence capture".into());
        }
        let deadline = Instant::now() + TIMEOUT;
        let mut retried = false;
        loop {
            if Instant::now() >= deadline {
                return Err("original desktop cleanup deadline elapsed".into());
            }
            match self.run.poll(POLL) {
                DesktopRunProgress::Completed(AppExit::Requested) => break,
                DesktopRunProgress::Retiring => {
                    if self.run.take_event_loop().is_some() {
                        return Err("physical event loop escaped before native retirement".into());
                    }
                }
                DesktopRunProgress::CleanupIncomplete if !retried => {
                    retried = true;
                    let report = self.run.execute();
                    if report.outcome() == DesktopRunOutcome::Failed {
                        return Err(format!("original native retirement failed: {report:?}"));
                    }
                }
                progress => return Err(format!("original native Close failed: {progress:?}")),
            }
        }
        let target = self
            .presented
            .borrow()
            .target
            .clone()
            .ok_or("no original native registry")?;
        let snapshot = target
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        if snapshot.phase != WindowTargetPhase::NativeDestroyed
            || snapshot.fault.is_some()
            || snapshot.surface_active
            || snapshot.provider_present
            || !snapshot.native_destroyed
        {
            return Err(format!(
                "original native target has incomplete physical retirement: {snapshot:?}"
            ));
        }
        let event_loop = self
            .run
            .take_event_loop()
            .ok_or("physical event loop was not recovered")?;
        if self.run.take_event_loop().is_some() {
            return Err("original physical event loop was recovered twice".into());
        }
        Ok(event_loop)
    }
}

fn assert_meaningful(state: &ResumeState, marker_positions: [[f32; 2]; 3]) {
    state.validate().unwrap();
    assert_eq!(
        state.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );
    assert_eq!(state.progress.location.anchor.as_str(), "checkpoint");
    assert_ne!(state.player.translation, [-120.0, 0.0]);
    assert_eq!(state.player.rotation, 0.0);
    assert_eq!(state.player.scale, [1.0; 2]);
    assert_eq!(state.held, [false, true, false, false]);
    assert_eq!(state.progress.score, 30);
    let keys = [
        LevelKey::main(LevelId::Meadow),
        LevelKey::main(LevelId::Cavern),
        LevelKey::new(LevelId::Cavern, 1).unwrap(),
    ];
    assert_eq!(state.progress.collected.len(), 3);
    assert_eq!(
        state
            .progress
            .collected
            .iter()
            .map(|pickup| pickup.level)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(keys)
    );
    assert!(
        state
            .progress
            .collected
            .iter()
            .all(|pickup| pickup.entity.as_str() == "gem-near")
    );
    assert_eq!(state.progress.markers.len(), 3);
    for (index, ((marker, key), position)) in state
        .progress
        .markers
        .iter()
        .zip(keys)
        .zip(marker_positions)
        .enumerate()
    {
        assert_eq!(marker.id.get(), index as u64 + 1);
        assert_eq!(marker.level, key);
        assert_eq!(marker.position, position);
    }
    assert_eq!(state.progress.next_marker_id.get(), 4);
    assert_eq!(state.counters.travel_count, 2);
    assert_eq!(state.counters.restarts, 0);
    assert_eq!(state.counters.saves, 0);
    assert_eq!(state.counters.restores, 0);
    assert!(state.clock.ticks > 0);
    assert_eq!(state.clock.timestep_nanos, 20_000_000);
    assert_eq!(
        state.clock.elapsed_nanos,
        state.clock.ticks * state.clock.timestep_nanos
    );
    assert_eq!(state.runtime_time.pending_nanos, 13_000_000);
    assert_eq!(
        state.runtime_time.timestep_nanos,
        state.clock.timestep_nanos
    );
    assert_ne!(state.compatibility.configuration, [0; 32]);
    assert_ne!(state.compatibility.schema, [0; 32]);
    assert_eq!(
        state
            .compatibility
            .levels
            .iter()
            .map(|stamp| stamp.level)
            .collect::<Vec<_>>(),
        LevelId::ALL
    );
    assert!(
        state
            .compatibility
            .levels
            .iter()
            .all(|stamp| stamp.manifest != [0; 32] && stamp.content != [0; 32])
    );
}

fn advance(state: &mut ResumeState, move_right: bool) {
    if move_right {
        let delta = Duration::from_nanos(state.clock.timestep_nanos).as_secs_f32();
        state.player.translation[0] =
            (state.player.translation[0] + 160.0 * delta).clamp(-280.0, 280.0);
    }
    state.clock.ticks += 1;
    state.clock.elapsed_nanos += state.clock.timestep_nanos;
}

fn simple(id: &str) -> Result<GameplayCommandDraft, String> {
    Ok(GameplayCommandDraft::new(
        GameplayCommandTypeId::new(id).map_err(|error| error.to_string())?,
    ))
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
