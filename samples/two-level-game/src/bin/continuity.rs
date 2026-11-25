//! Game-owned finite source consumer for the actual Rust rebuild/restore Host.
//!
//! Usage: continuity --project-root ABS --control-dir ABS --session STRING --generation STRING
//!                   [--placement headless|desktop|audio-desktop]
//! The Host creates a PRIVATE control directory under its temporary root and owns Child
//! wait/kill. stdout, stderr, stdin and process-local Runtime generations grant no authority.

#[path = "continuity/control.rs"]
mod control;
#[cfg(feature = "desktop")]
#[path = "continuity/desktop.rs"]
mod desktop;
#[path = "continuity/request_watch.rs"]
mod request_watch;
use two_level_game::development::protocol;

#[cfg(test)]
#[path = "continuity/tests.rs"]
mod tests;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::RuntimeState,
    project_host::{HeadlessSession, HeadlessSessionClose},
    tasks::{TaskPools, TaskReturnState},
};
use two_level_game::{
    GamePhase, GameSnapshot, capture_resume,
    command::{
        CommandSchedule, CommandSequence, CommandSource, CommandSubmitError, CommandTick,
        GameCommand, ScheduledCommand,
    },
    request_resume,
};

use control::{Arguments, ControlDirectory};
use protocol::{
    Command, ControlServiceObservation, DevelopmentPlacement, MAX_STEP_COMMANDS, Observation,
    READY_FILE, Reply, SemanticCommand,
};
use request_watch::RequestWatch;

const MAX_REQUESTS: usize = 512;
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(2);
const IDLE_RECONCILE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Default)]
struct ControlService {
    inbox_checks: u64,
    idle_wait_calls: u64,
    idle_wait_nanos: u64,
    counters_saturated: bool,
}

impl ControlService {
    fn checked_inbox(&mut self) {
        add_counter(&mut self.inbox_checks, 1, &mut self.counters_saturated);
    }

    fn waited(&mut self, elapsed: Duration) {
        add_counter(&mut self.idle_wait_calls, 1, &mut self.counters_saturated);
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or_else(|_| {
            self.counters_saturated = true;
            u64::MAX
        });
        add_counter(
            &mut self.idle_wait_nanos,
            nanos,
            &mut self.counters_saturated,
        );
    }

    fn observation(&self, watch: &RequestWatch) -> ControlServiceObservation {
        let watch = watch.snapshot();
        ControlServiceObservation {
            inbox_checks: self.inbox_checks,
            idle_wait_calls: self.idle_wait_calls,
            idle_wait_nanos: self.idle_wait_nanos,
            hints: watch.hints,
            callbacks: watch.callbacks,
            coalesced_hints: watch.coalesced_hints,
            native_wake_delivery_failures: watch.native_wake_delivery_failures,
            watcher_started: watch.watcher_started,
            watcher_closed: watch.watcher_closed,
            counters_saturated: self.counters_saturated || watch.counters_saturated,
        }
    }
}

fn add_counter(counter: &mut u64, amount: u64, saturated: &mut bool) {
    match counter.checked_add(amount) {
        Some(value) => *counter = value,
        None => {
            *counter = u64::MAX;
            *saturated = true;
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("continuity: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = Arguments::parse()?;
    let mut control = ControlDirectory::open(&arguments)?;
    let root = two_level_game::open_explicit_project_root(&arguments.project_root)
        .map_err(|error| format!("open source project: {error}"))?;
    let mut game = match arguments.placement {
        DevelopmentPlacement::SourceHeadless => ChildGame::Headless(Game {
            session: HeadlessSession::new(root, two_level_game::product_recipe()),
            sequence: 0,
            execution_failed: false,
        }),
        #[cfg(feature = "desktop")]
        DevelopmentPlacement::SourceDesktop | DevelopmentPlacement::SourceDesktopAudio => {
            ChildGame::Desktop(desktop::DesktopGame::new(
                root,
                arguments.placement == DevelopmentPlacement::SourceDesktopAudio,
            )?)
        }
        #[cfg(not(feature = "desktop"))]
        DevelopmentPlacement::SourceDesktop | DevelopmentPlacement::SourceDesktopAudio => {
            return Err("source-desktop continuity requires the desktop feature".into());
        }
    };
    let mut watch = RequestWatch::new();
    let mut service = ControlService::default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        serve(
            &arguments,
            &mut control,
            &mut game,
            &mut watch,
            &mut service,
        )
    }))
    .unwrap_or_else(|_| Err("continuity panicked; retaining original physical cleanup".into()));
    let watch_cleanup = catch_unwind(AssertUnwindSafe(|| watch.close()))
        .unwrap_or_else(|_| Err("original request watcher close panicked".into()));
    let cleanup = if game.is_closed() {
        Ok(Reply::Closed { failed: false })
    } else {
        catch_unwind(AssertUnwindSafe(|| {
            game.close(Instant::now() + OPERATION_TIMEOUT)
        }))
        .unwrap_or_else(|_| Err("original physical close panicked".into()))
    };
    if !game.is_closed() {
        return Err(format!(
            "{result:?}; original cleanup did not complete: game={cleanup:?}; watcher={watch_cleanup:?}"
        ));
    }
    if let Err(error) = watch_cleanup {
        return Err(format!(
            "{result:?}; original request watcher close failed: {error}"
        ));
    }
    match cleanup {
        Ok(Reply::Closed { failed: false } | Reply::AudioClosed { failed: false, .. }) => result,
        Ok(Reply::Closed { failed: true } | Reply::AudioClosed { failed: true, .. }) => Err(
            format!("{result:?}; original game owner closed with failure"),
        ),
        cleanup => Err(format!(
            "{result:?}; physical close did not report completion: {cleanup:?}"
        )),
    }
}

fn serve(
    arguments: &Arguments,
    control: &mut ControlDirectory,
    game: &mut ChildGame,
    watch: &mut RequestWatch,
    service: &mut ControlService,
) -> Result<(), String> {
    let total_deadline = Instant::now() + TOTAL_TIMEOUT;
    watch.start(&arguments.control_dir)?;
    let startup_budget = match arguments.placement {
        DevelopmentPlacement::SourceHeadless => OPERATION_TIMEOUT,
        DevelopmentPlacement::SourceDesktop | DevelopmentPlacement::SourceDesktopAudio => {
            Duration::from_secs(60)
        }
    };
    game.initialize(Instant::now() + startup_budget)?;
    game.bind_request_watch(watch)?;
    let ready = game.observed()?;
    control.publish(
        READY_FILE,
        &arguments.response(0, Ok(ready), service.observation(watch)),
    )?;
    let mut previous_request = 0;
    let mut activity = Instant::now();
    let mut consumed = 0;
    let mut probe_due = true;
    loop {
        if Instant::now() >= total_deadline || activity.elapsed() >= IDLE_TIMEOUT {
            return Err(
                "finite continuity process reached its total or inactivity deadline".into(),
            );
        }
        if consumed == MAX_REQUESTS {
            return Err("finite continuity process exhausted its 512 request budget".into());
        }
        game.poll_idle(Duration::ZERO)?;
        if !probe_due && !watch.pending() {
            let now = Instant::now();
            let wait = IDLE_RECONCILE_INTERVAL
                .min(total_deadline.saturating_duration_since(now))
                .min((activity + IDLE_TIMEOUT).saturating_duration_since(now));
            let started = Instant::now();
            game.wait_request(watch, wait)?;
            service.waited(started.elapsed());
            probe_due = true;
            continue;
        }
        probe_due = false;
        watch.acknowledge()?;
        service.checked_inbox();
        let Some(bytes) = control.receive()? else {
            continue;
        };
        activity = Instant::now();
        consumed += 1;
        let deadline = (activity + OPERATION_TIMEOUT).min(total_deadline);
        let (number, request) = control::decode_request(&bytes);
        if !control.response_available(number)? {
            eprintln!("continuity: refused request {number}: reply destination already exists");
            continue;
        }
        let mut closing = false;
        let result = request.and_then(|request| {
            arguments.validate(&request, previous_request)?;
            previous_request = request.request;
            validate_command(&request.command)?;
            closing = matches!(&request.command, Command::Close);
            let watch_close = if closing { watch.close() } else { Ok(()) };
            let result = game.execute(request.command, deadline);
            match watch_close {
                Ok(()) => result,
                Err(error) => Err(format!("{result:?}; request watcher close failed: {error}")),
            }
        });
        let close_result = if closing {
            match &result {
                Ok(Reply::Closed { failed: false } | Reply::AudioClosed { failed: false, .. }) => {
                    Some(Ok(()))
                }
                Ok(Reply::Closed { failed: true } | Reply::AudioClosed { failed: true, .. }) => {
                    Some(Err("original game owner closed with failure".into()))
                }
                _ => None,
            }
        } else {
            None
        };
        control.publish(
            &format!("response-{number}.json"),
            &arguments.response(number, result, service.observation(watch)),
        )?;
        if game.execution_failed() && !closing {
            return Err(
                "the operation failed after command admission; the original game must retire"
                    .into(),
            );
        }
        if let Some(result) = close_result {
            return result;
        }
    }
}

enum ChildGame {
    Headless(Game),
    #[cfg(feature = "desktop")]
    Desktop(desktop::DesktopGame),
}

impl ChildGame {
    fn bind_request_watch(&self, watch: &RequestWatch) -> Result<(), String> {
        match self {
            Self::Headless(_) => {
                let _ = watch;
                Ok(())
            }
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => {
                let wake = game
                    .maintenance_wake()
                    .ok_or("original desktop request wake is unavailable")?;
                watch.bind_native_wake(wake);
                Ok(())
            }
        }
    }

    fn wait_request(&mut self, watch: &RequestWatch, max_wait: Duration) -> Result<(), String> {
        match self {
            Self::Headless(_) => watch.wait(max_wait),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) if !game.is_closed() => game.poll_idle(max_wait),
            #[cfg(feature = "desktop")]
            Self::Desktop(_) => watch.wait(max_wait),
        }
    }

    fn initialize(&mut self, deadline: Instant) -> Result<(), String> {
        match self {
            Self::Headless(game) => game.initialize(deadline),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.initialize(deadline),
        }
    }

    fn observed(&self) -> Result<Reply, String> {
        match self {
            Self::Headless(game) => game.observed(),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.observed(),
        }
    }

    fn execute(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        match self {
            Self::Headless(game) => game.execute(command, deadline),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.execute(command, deadline),
        }
    }

    fn poll_idle(&mut self, max_wait: Duration) -> Result<(), String> {
        match self {
            Self::Headless(_) => {
                if !max_wait.is_zero() {
                    thread::sleep(max_wait);
                }
                Ok(())
            }
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.poll_idle(max_wait),
        }
    }

    fn execution_failed(&self) -> bool {
        match self {
            Self::Headless(game) => game.execution_failed,
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.execution_failed(),
        }
    }

    fn is_closed(&self) -> bool {
        match self {
            Self::Headless(game) => game.session.is_closed(),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.is_closed(),
        }
    }

    fn close(&mut self, deadline: Instant) -> Result<Reply, String> {
        match self {
            Self::Headless(game) => game.close(deadline).map(|failed| Reply::Closed { failed }),
            #[cfg(feature = "desktop")]
            Self::Desktop(game) => game.close(deadline),
        }
    }
}

fn validate_command(command: &Command) -> Result<(), String> {
    match command {
        Command::Step { commands } => {
            if commands.len() > MAX_STEP_COMMANDS {
                return Err("step accepts at most eight semantic commands".into());
            }
            if commands.iter().any(|command| {
                matches!(command, SemanticCommand::Move { direction, .. } if *direction >= 4)
            }) {
                return Err("movement direction must be less than four".into());
            }
        }
        Command::Restore { state } => {
            two_level_game::resume::encode_resume(state)
                .map_err(|error| format!("resume state was refused before execution: {error}"))?;
        }
        _ => {}
    }
    Ok(())
}

struct Game {
    session: HeadlessSession,
    sequence: u64,
    execution_failed: bool,
}

impl Game {
    fn initialize(&mut self, deadline: Instant) -> Result<(), String> {
        self.session
            .start()
            .map_err(|error| format!("source startup failed: {error:?}"))?;
        check_deadline(deadline)?;
        if self
            .session
            .pause()
            .map_err(|error| format!("initial pause failed: {error:?}"))?
            .is_some()
        {
            return Err("source game exited during initial pause".into());
        }
        loop {
            check_deadline(deadline)?;
            let snapshot = self.snapshot()?;
            if let Some(error) = snapshot.error {
                return Err(format!("source game initialization failed: {error}"));
            }
            if snapshot.initialized {
                if snapshot.tick != 0 || snapshot.restoring || snapshot.phase != GamePhase::Running
                {
                    return Err(
                        "source startup did not reach a fresh initialized paused game".into(),
                    );
                }
                return self.wait_task_returns(deadline);
            }
            self.frame()?;
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn snapshot(&self) -> Result<GameSnapshot, String> {
        self.session
            .capture(|world| world.resource::<GameSnapshot>().clone())
            .map_err(|error| format!("paused observation failed: {error:?}"))
    }

    fn wait_task_returns(&self, deadline: Instant) -> Result<(), String> {
        loop {
            check_deadline(deadline)?;
            match self
                .session
                .capture(|world| world.resource::<TaskPools>().return_state())
                .map_err(|error| format!("paused task observation failed: {error:?}"))?
            {
                TaskReturnState::Returned => return Ok(()),
                TaskReturnState::CleanupFailed => return Err("original task cleanup failed".into()),
                TaskReturnState::Pending => thread::sleep(
                    POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                ),
            }
        }
    }

    fn observed(&self) -> Result<Reply, String> {
        let runtime_generation = self
            .session
            .runtime_generation()
            .ok_or("the original Runtime generation is unavailable")?;
        let (app_frame, snapshot) = self
            .session
            .capture(|world| {
                (
                    world.resource::<nara::app::FrameExecutionStart>().frame(),
                    world.resource::<GameSnapshot>().clone(),
                )
            })
            .map_err(|error| format!("paused observation failed: {error:?}"))?;
        Ok(observed_snapshot(runtime_generation, app_frame, snapshot))
    }

    fn execute(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        if self.execution_failed && !matches!(&command, Command::Close) {
            return Err("the original game has an unsettled failed operation".into());
        }
        let mutating = matches!(
            &command,
            Command::Step { .. } | Command::Frame | Command::Restore { .. } | Command::Close
        );
        check_deadline(deadline)?;
        match self.execute_inner(command, deadline) {
            Ok(reply) => {
                if let Err(error) = check_deadline(deadline) {
                    self.execution_failed |= mutating;
                    return Err(error);
                }
                Ok(reply)
            }
            Err(error) => {
                if mutating {
                    self.execution_failed |= self
                        .session
                        .observe(|world| {
                            world
                                .resource::<CommandSchedule>()
                                .stats()
                                .retained_commands
                                != 0
                                || world.resource::<GameSnapshot>().restoring
                        })
                        .unwrap_or(true);
                }
                Err(error)
            }
        }
    }

    fn execute_inner(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        let result = match command {
            Command::AudioStatus | Command::EnableAudio => {
                Err("headless placement has no resettable audio owner".into())
            }
            Command::ExecutionStatus | Command::Pause | Command::Resume => {
                Err("continuous execution controls require the explicit desktop placement".into())
            }
            Command::Observe => self.observed(),
            Command::Frame => {
                self.frame()?;
                self.observed()
            }
            Command::Step { commands } => {
                self.step(commands, deadline)?;
                self.observed()
            }
            Command::Capture => {
                self.wait_task_returns(deadline)?;
                capture_resume(&self.session)
                    .map(|state| Reply::Captured { state })
                    .map_err(|error| format!("game capture was refused: {error}"))
            }
            Command::Restore { state } => {
                let snapshot = self.snapshot()?;
                if !snapshot.initialized || snapshot.tick != 0 || snapshot.restoring {
                    return Err(
                        "restore requires a fresh initialized paused engine at tick zero".into(),
                    );
                }
                request_resume(&mut self.session, &state)
                    .map_err(|error| format!("game restore was refused: {error}"))?;
                loop {
                    check_deadline(deadline)?;
                    let snapshot = self.snapshot()?;
                    if let Some(error) = snapshot.error {
                        return Err(format!("game reconstruction failed: {error}"));
                    }
                    if !snapshot.restoring {
                        self.wait_task_returns(deadline)?;
                        break self.observed();
                    }
                    self.frame()?;
                    thread::sleep(POLL_INTERVAL);
                }
            }
            Command::IsolateInput => self.observed(),
            Command::ConfirmPresentation => {
                Err("headless placement has no native presentation".into())
            }
            Command::ActivateInput => {
                Err("headless placement has no native input activation".into())
            }
            Command::Close => self.close(deadline).map(|failed| Reply::Closed { failed }),
        }?;
        Ok(result)
    }

    fn frame(&mut self) -> Result<(), String> {
        if self.session.state() != Some(RuntimeState::Paused) {
            return Err("maintenance requires the original paused Runtime".into());
        }
        if self
            .session
            .frame(Duration::ZERO)
            .map_err(|error| format!("paused maintenance failed: {error:?}"))?
            .is_some()
        {
            return Err("game exited during paused maintenance".into());
        }
        if self.session.state() != Some(RuntimeState::Paused) {
            return Err("maintenance did not retain Pause".into());
        }
        Ok(())
    }

    fn step(&mut self, commands: Vec<SemanticCommand>, deadline: Instant) -> Result<(), String> {
        let tick = self
            .snapshot()?
            .tick
            .checked_add(1)
            .and_then(CommandTick::new)
            .ok_or("engine tick is exhausted")?;
        self.sequence
            .checked_add(commands.len() as u64)
            .ok_or("command source sequence is exhausted")?;
        let source = CommandSource::external("two-level-game.continuity")
            .map_err(|error| format!("command source is invalid: {error}"))?;
        let drafts = commands
            .into_iter()
            .map(command_draft)
            .collect::<Result<Vec<_>, _>>()?;
        let mut admitted = false;
        for draft in drafts {
            if let Err(error) = check_deadline(deadline) {
                self.execution_failed |= admitted;
                return Err(error);
            }
            self.sequence += 1;
            let sequence =
                CommandSequence::new(self.sequence).ok_or("command source sequence is zero")?;
            let submission = ScheduledCommand::new(tick, source.clone(), sequence, draft)
                .submit_headless(&mut self.session);
            if let Err(error) = submission {
                self.execution_failed |=
                    admitted || matches!(&error, CommandSubmitError::Runtime { output: Ok(_), .. });
                return Err(format!("semantic command admission failed: {error:?}"));
            }
            admitted = true;
        }
        self.complete_step(deadline)
    }

    fn complete_step(&mut self, deadline: Instant) -> Result<(), String> {
        let result = self.drive_step(deadline);
        self.execution_failed |= result.is_err();
        result
    }

    fn drive_step(&mut self, deadline: Instant) -> Result<(), String> {
        check_deadline(deadline)?;
        if self
            .session
            .step()
            .map_err(|error| format!("game step failed: {error:?}"))?
            .is_some()
        {
            return Err("game exited during its exact step".into());
        }
        Ok(())
    }

    fn close(&mut self, deadline: Instant) -> Result<bool, String> {
        loop {
            if Instant::now() >= deadline {
                return Err("Headless physical close remains incomplete at its deadline".into());
            }
            match self.session.close() {
                HeadlessSessionClose::Pending => thread::sleep(POLL_INTERVAL),
                HeadlessSessionClose::Complete => return Ok(false),
                HeadlessSessionClose::Failed => return Ok(true),
            }
        }
    }
}

fn observed_snapshot(runtime_generation: u64, app_frame: u64, snapshot: GameSnapshot) -> Reply {
    Reply::Observed {
        observation: observation(runtime_generation, app_frame, snapshot),
    }
}

fn observation(runtime_generation: u64, app_frame: u64, snapshot: GameSnapshot) -> Observation {
    Observation {
        runtime_generation,
        app_frame,
        engine_tick: snapshot.tick,
        logical_tick: snapshot.logical_tick,
        logical_elapsed_nanos: snapshot.logical_elapsed_nanos,
        player: snapshot.player.to_array(),
        held: snapshot.held,
        score: snapshot.score,
        collected: snapshot.collected,
        markers: snapshot.markers.iter().map(|marker| marker.get()).collect(),
        phase: match snapshot.phase {
            GamePhase::Running => "running",
            GamePhase::Preparing => "preparing",
            GamePhase::Reconstructing => "reconstructing",
            GamePhase::Degraded => "degraded",
        }
        .into(),
        restoring: snapshot.restoring,
        error: snapshot.error,
    }
}

fn command_draft(command: SemanticCommand) -> Result<GameCommand, String> {
    command.validate()?;
    Ok(command.intent())
}

fn check_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("continuity operation exceeded its cooperative deadline".into())
    } else {
        Ok(())
    }
}
