//! Main-thread native ownership, finite physical trajectory and unconditional original cleanup.

#[path = "observation.rs"]
mod observation;
#[path = "window.rs"]
mod window;

use std::{
    any::Any,
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{AppExit, RuntimeState},
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunIntent, DesktopRunOptions,
        DesktopRunOutcome, DesktopRunProgress,
    },
    window::{WindowId, backend::WindowTargetPhase},
};
use two_level_game::{
    flow::{self, FlowPhase},
    levels::{LevelId, LevelKey},
};

use observation::{Observations, Presented, Stability};
use window::{D, E, ENTER, ESCAPE, Input, T, Window, X};

const POLL: Duration = Duration::from_millis(2);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
const TRAJECTORY_TIMEOUT: Duration = Duration::from_secs(75);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);
const STABLE_INTERVAL: Duration = Duration::from_millis(200);

struct Game {
    run: DesktopRun,
    input: Input,
    window: Option<Window>,
    observations: Rc<RefCell<Observations>>,
    deadline: Instant,
}

pub(super) fn run() -> ExitCode {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: play_flow_check");
        return ExitCode::FAILURE;
    }
    if std::env::var("WGPU_BACKEND").as_deref() != Ok("dx12")
        || std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() != Ok("0")
    {
        eprintln!("play_flow_check requires WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0");
        return ExitCode::FAILURE;
    }
    let input = Input::new();
    let root = match two_level_game::open_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("play_flow_check source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let recipe = match flow::with_play_flow(
        two_level_game::product_recipe(),
        flow::PlayFlowControls::Keyboard,
    )
    .and_then(flow::with_play_flow_hud)
    {
        Ok(recipe) => recipe,
        Err(error) => {
            eprintln!("play_flow_check recipe: {error}");
            return ExitCode::FAILURE;
        }
    };
    let observations = Rc::new(RefCell::new(Observations::default()));
    let observed = Rc::clone(&observations);
    let deadline = Instant::now() + TRAJECTORY_TIMEOUT;
    let options = DesktopRunOptions::new().with_presentation_observer(move |observation| {
        let mut observed = observed.borrow_mut();
        if observed.failure.is_none()
            && let Err(error) = observed.observe(observation)
        {
            observed.failure = Some(error);
        }
        if observed.failure.is_none() && Instant::now() >= deadline {
            observed.failure = Some("original native trajectory exceeded 75 seconds".into());
        }
        if observed.failure.is_some() {
            DesktopPresentationControl::Exit(AppExit::Requested)
        } else {
            DesktopPresentationControl::Continue
        }
    });
    let mut game = Game {
        run: DesktopRun::new(
            root,
            DesktopRunIntent::new()
                .with_profile("play-flow")
                .with_recipe(recipe)
                .with_execution(options),
        ),
        input,
        window: None,
        observations,
        deadline,
    };
    let trajectory = catch_unwind(AssertUnwindSafe(|| game.qualify())).unwrap_or_else(|payload| {
        Err(format!(
            "trajectory assertion: {}",
            panic_message(&*payload)
        ))
    });
    let cleanup = catch_unwind(AssertUnwindSafe(|| game.close()))
        .unwrap_or_else(|payload| Err(format!("cleanup assertion: {}", panic_message(&*payload))));
    let input_cleanup = game.input.finish();
    let mut success = true;
    for result in [trajectory, cleanup, input_cleanup] {
        if let Err(error) = result {
            success = false;
            eprintln!("play_flow_check: {error}");
        }
    }
    if !matches!(
        game.run.progress(),
        DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
    ) {
        std::mem::forget(game);
        eprintln!("play_flow_check: original native owners remain retained at process exit");
        return ExitCode::FAILURE;
    }
    if success {
        println!(
            "two_level_play_flow: ok physical_input=true presented_ui=true paused_clock=true released_hold=true new_round=true original_native_retired=true"
        );
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

impl Game {
    fn qualify(&mut self) -> Result<(), String> {
        assert_eq!(self.run.progress(), DesktopRunProgress::NotStarted);
        let title = self.wait_phase(FlowPhase::Title, 0, 0)?;
        assert_eq!(title.game.logical_tick, 0);
        assert_eq!(title.game.logical_elapsed_nanos, 0);
        assert_eq!(title.time_scale, 0.0);
        self.bind_window()?;
        title.trace("title");
        println!(
            "two_level_play_flow_adapter: {:?}",
            self.observations.borrow().adapter.as_ref().unwrap()
        );

        self.input.press(ENTER)?;
        let first = self.wait_phase(FlowPhase::Playing, 1, title.serial)?;
        self.release(ENTER)?;
        assert_new_round(&first);
        assert_ne!(first.game.current_instance, title.game.current_instance);
        first.trace("new-round-1");

        self.input.press(D)?;
        let walked = self.wait_for("physical D movement", first.serial, |frame| {
            frame.matches(FlowPhase::Playing, 1)
                && frame.game.held == [false, true, false, false]
                && frame.game.player.x > first.game.player.x + 3.0
                && frame.game.logical_tick > first.game.logical_tick
        })?;
        assert!(self.input.down(D)?);
        assert_eq!(walked.game.current_instance, first.game.current_instance);
        walked.trace("walk-held-d");

        self.input.press(ESCAPE)?;
        let paused = self.wait_phase(FlowPhase::Paused, 1, walked.serial)?;
        assert!(self.input.down(D)?);
        assert_eq!(paused.time_scale, 0.0);
        assert_eq!(paused.game.held, [false; 4]);
        self.release(ESCAPE)?;
        let paused = self.still(&paused, true)?;
        assert!(self.input.down(D)?);
        paused.trace("paused-held-d");

        self.release(D)?;
        let released = self.still(&paused, true)?;
        assert!(!self.input.down(D)?);
        released.trace("paused-released-d");

        self.input.press(ENTER)?;
        let continued = self.wait_phase(FlowPhase::Playing, 1, released.serial)?;
        self.release(ENTER)?;
        assert_eq!(continued.game.player, released.game.player);
        assert_eq!(continued.game.held, [false; 4]);
        assert!(continued.time_scale > 0.0);
        let continued = self.still(&continued, false)?;
        continued.trace("continued-idle");

        self.input.press(D)?;
        let fresh = self.wait_for("fresh D after continue", continued.serial, |frame| {
            frame.matches(FlowPhase::Playing, 1)
                && frame.game.held == [false, true, false, false]
                && frame.game.player.x > continued.game.player.x + 3.0
        })?;
        assert!(self.input.down(D)?);
        fresh.trace("fresh-d");
        let scored = self.wait_for("ordinary first Meadow pickup", continued.serial, |frame| {
            frame.matches(FlowPhase::Playing, 1)
                && frame.game.score == 10
                && frame.game.collected == 1
                && frame.flow.meadow_collected
                && !frame.flow.cavern_collected
        })?;
        self.release(D)?;
        scored.trace("scored-round");

        self.input.press(E)?;
        let traveled = self.wait_for("ordinary E Scene travel", scored.serial, |frame| {
            frame.matches(FlowPhase::Playing, 1)
                && frame.game.level == Some(LevelKey::main(LevelId::Cavern))
                && frame.game.travel_count == 1
                && frame.game.current_instance != first.game.current_instance
        })?;
        self.release(E)?;
        assert_eq!(traveled.game.score, 10);
        traveled.trace("traveled-round");

        self.input.press(ESCAPE)?;
        let paused = self.wait_phase(FlowPhase::Paused, 1, traveled.serial)?;
        self.release(ESCAPE)?;
        self.input.press(T)?;
        let returned = self.wait_phase(FlowPhase::Title, 1, paused.serial)?;
        self.release(T)?;
        assert_eq!(returned.time_scale, 0.0);
        assert_eq!(returned.game.score, 10);
        assert_eq!(returned.game.travel_count, 1);
        returned.trace("returned-title");

        self.input.press(ENTER)?;
        let second = self.wait_phase(FlowPhase::Playing, 2, returned.serial)?;
        self.release(ENTER)?;
        assert_new_round(&second);
        assert_ne!(second.game.current_instance, first.game.current_instance);
        assert_ne!(second.game.current_instance, returned.game.current_instance);
        assert_eq!(second.runtime, first.runtime);
        second.trace("new-round-2");

        self.input.press(X)?;
        self.input.release(X)?;
        let progress = self.run.poll(POLL);
        let release = self.input.release_owned();
        self.await_exit(progress)?;
        release?;
        Ok(())
    }

    fn bind_window(&mut self) -> Result<(), String> {
        let deadline = (Instant::now() + Duration::from_secs(30)).min(self.deadline);
        let window = loop {
            if let Some(window) = Window::find_owned()? {
                break window;
            }
            self.poll(deadline)?;
        };
        let result = (|| {
            self.input.bind(window)?;
            if !self.input.focused() {
                println!(
                    "two_level_play_flow_focus: waiting for normal activation of the exact test window"
                );
            }
            while !self.input.focused() {
                self.poll(deadline)?;
            }
            self.input.check()
        })();
        self.window = Some(window);
        result?;
        println!("two_level_play_flow_window: {window:?}");
        Ok(())
    }

    fn operation_deadline(&self) -> Instant {
        (Instant::now() + OPERATION_TIMEOUT).min(self.deadline)
    }

    fn poll(&mut self, deadline: Instant) -> Result<(), String> {
        if Instant::now() >= deadline {
            let observations = self.observations.borrow();
            let latest = observations.latest.as_ref().map(|frame| {
                (
                    frame.flow.phase,
                    frame.flow.round,
                    frame.app_frame,
                    frame.render_frame,
                    frame.game.logical_tick,
                    frame.game.player,
                )
            });
            return Err(format!(
                "native operation deadline elapsed; latest={latest:?}"
            ));
        }
        let progress = self.run.poll(POLL);
        if let Some(error) = &self.observations.borrow().failure {
            return Err(error.clone());
        }
        if progress != DesktopRunProgress::Running {
            return Err(format!(
                "original desktop stopped during physical trajectory: {progress:?}"
            ));
        }
        assert_eq!(self.run.state(), Some(RuntimeState::Running));
        if self.window.is_some() {
            self.input.check()?;
        }
        Ok(())
    }

    fn wait_for(
        &mut self,
        operation: &str,
        after: u64,
        predicate: impl Fn(&Presented) -> bool,
    ) -> Result<Presented, String> {
        let deadline = self.operation_deadline();
        loop {
            self.poll(deadline)
                .map_err(|error| format!("{operation}: {error}"))?;
            if let Some(frame) = &self.observations.borrow().latest
                && frame.serial > after
                && predicate(frame)
            {
                return Ok(frame.clone());
            }
        }
    }

    fn wait_phase(
        &mut self,
        phase: FlowPhase,
        round: u32,
        after: u64,
    ) -> Result<Presented, String> {
        self.wait_for(
            &format!("presented {phase:?} round {round}"),
            after,
            |frame| frame.matches(phase, round),
        )
    }

    fn release(&mut self, scan: u16) -> Result<(), String> {
        let after = self
            .observations
            .borrow()
            .latest
            .as_ref()
            .map_or(0, |frame| frame.serial);
        self.input.release(scan)?;
        let deadline = self.operation_deadline();
        loop {
            self.poll(deadline)?;
            let presented = self
                .observations
                .borrow()
                .latest
                .as_ref()
                .is_some_and(|frame| frame.serial > after);
            if presented && !self.input.down(scan)? {
                return self.input.acknowledge_release(scan);
            }
        }
    }

    fn still(&mut self, baseline: &Presented, frozen_clock: bool) -> Result<Presented, String> {
        let after = {
            let mut observations = self.observations.borrow_mut();
            observations.stability = Some(Stability {
                flow: baseline.flow.clone(),
                game: baseline.game.clone(),
                frozen_clock,
            });
            observations
                .latest
                .as_ref()
                .map_or(baseline.serial, |frame| frame.serial)
        };
        let started = Instant::now();
        let result = self.wait_for("multiple stable native presentations", after, |frame| {
            frame.matches(baseline.flow.phase, baseline.flow.round)
                && frame.serial >= after + 5
                && frame.app_frame > baseline.app_frame
                && started.elapsed() >= STABLE_INTERVAL
                && (frozen_clock
                    || (frame.game.logical_tick >= baseline.game.logical_tick + 5
                        && frame.game.logical_elapsed_nanos > baseline.game.logical_elapsed_nanos))
        });
        self.observations.borrow_mut().stability = None;
        result
    }

    fn await_exit(&mut self, mut progress: DesktopRunProgress) -> Result<(), String> {
        let deadline = self.operation_deadline();
        loop {
            if let Some(error) = &self.observations.borrow().failure {
                return Err(error.clone());
            }
            match progress {
                DesktopRunProgress::Completed(AppExit::Requested) => return Ok(()),
                DesktopRunProgress::Running | DesktopRunProgress::Retiring => {}
                DesktopRunProgress::CleanupIncomplete => {
                    self.run.execute();
                }
                progress => {
                    return Err(format!(
                        "physical X did not retire the original run: {progress:?}"
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err("physical X exit did not finish within its native deadline".into());
            }
            progress = self.run.poll(POLL);
        }
    }

    fn close(&mut self) -> Result<(), String> {
        self.observations.borrow_mut().stability = None;
        let release = self.input.release_owned();
        if !matches!(
            self.run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            self.run.request_close();
        }
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            match self.run.progress() {
                DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed => break,
                DesktopRunProgress::CleanupIncomplete => {
                    self.run.execute();
                }
                _ => {
                    self.run.poll(POLL);
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    "original DesktopRun still owns incomplete cleanup at its deadline".into(),
                );
            }
        }
        let report = self.run.execute();
        if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
            || report.diagnostics().has_errors()
        {
            return Err(format!("original desktop close failed: {report:?}"));
        }
        let handles = self
            .observations
            .borrow_mut()
            .target
            .take()
            .ok_or("no original native target was observed")?;
        let target = handles
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        drop(handles);
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert_eq!(target.fault, None);
        assert!(!target.surface_active && !target.provider_present && target.native_destroyed);
        assert!(
            !self
                .window
                .ok_or("no exact original HWND was bound")?
                .exists()
        );
        assert!(Window::find_owned()?.is_none());
        assert_eq!(self.run.runtime_generation(), None);
        assert_eq!(self.run.state(), None);
        assert!(self.run.take_event_loop().is_some());
        assert!(self.run.take_event_loop().is_none());
        release?;
        println!(
            "two_level_play_flow_checkpoint: exited-cleanly original_hwnd_destroyed=true original_runtime_retired=true original_pump_returned=true"
        );
        Ok(())
    }
}

fn assert_new_round(frame: &Presented) {
    assert_eq!(frame.game.level, Some(LevelKey::main(LevelId::Meadow)));
    assert!(frame.game.current_instance.is_some());
    assert_eq!(frame.game.player.to_array(), [-120.0, 0.0]);
    assert_eq!(frame.game.score, 0);
    assert_eq!(frame.game.collected, 0);
    assert_eq!(frame.game.visible_pickups, 2);
    assert!(frame.game.markers.is_empty());
    assert_eq!(frame.game.travel_count, 0);
    assert_eq!(frame.game.restarts, 0);
    assert_eq!(frame.game.saves, 0);
    assert_eq!(frame.game.restores, 0);
    assert_eq!(frame.game.held, [false; 4]);
    assert!(!frame.flow.meadow_collected && !frame.flow.cavern_collected);
    assert!(frame.time_scale > 0.0);
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload")
}
