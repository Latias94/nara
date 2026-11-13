//! Game-owned finite source/headless consumer for the actual Rust rebuild/restore Host.
//!
//! Usage: continuity --project-root ABS --control-dir ABS --session STRING --generation STRING
//! The Host creates a PRIVATE control directory under its temporary root and owns Child
//! wait/kill. stdout, stderr, stdin and process-local Runtime generations grant no authority.

#[path = "continuity/control.rs"]
mod control;
#[path = "continuity/protocol.rs"]
mod protocol;

#[cfg(test)]
#[path = "continuity/tests.rs"]
mod tests;

use std::{
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::RuntimeState,
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandPayload,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue,
    },
    project_host::{HeadlessSession, HeadlessSessionClose},
};
use two_level_game::{GamePhase, GameSnapshot, capture_resume, request_resume};

use control::{Arguments, ControlDirectory};
use protocol::{Command, MAX_STEP_COMMANDS, Observation, READY_FILE, Reply, SemanticCommand};

const MAX_REQUESTS: usize = 512;
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(2);

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
    let mut game = Game {
        session: HeadlessSession::new(root, two_level_game::product_recipe()),
        sequence: 0,
        execution_failed: false,
    };
    let result = serve(&arguments, &mut control, &mut game);
    if !game.session.is_closed() {
        match game.close(Instant::now() + OPERATION_TIMEOUT) {
            Ok(false) => {}
            Ok(true) => {
                return Err(format!(
                    "{result:?}; original Headless owner closed with failure"
                ));
            }
            Err(error) => return Err(format!("{result:?}; {error}")),
        }
    }
    result
}

fn serve(
    arguments: &Arguments,
    control: &mut ControlDirectory,
    game: &mut Game,
) -> Result<(), String> {
    let total_deadline = Instant::now() + TOTAL_TIMEOUT;
    game.initialize(Instant::now() + OPERATION_TIMEOUT)?;
    let ready = game.observed()?;
    control.publish(READY_FILE, &arguments.response(0, Ok(ready)))?;
    let mut previous_request = 0;
    let mut activity = Instant::now();
    let mut consumed = 0;
    loop {
        if Instant::now() >= total_deadline || activity.elapsed() >= IDLE_TIMEOUT {
            return Err(
                "finite continuity process reached its total or inactivity deadline".into(),
            );
        }
        if consumed == MAX_REQUESTS {
            return Err("finite continuity process exhausted its 512 request budget".into());
        }
        let Some(bytes) = control.receive()? else {
            thread::sleep(POLL_INTERVAL);
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
            game.execute(request.command, deadline)
        });
        let close_result = if closing {
            Some(match &result {
                Ok(Reply::Closed { failed: false }) => Ok(()),
                Ok(Reply::Closed { failed: true }) => {
                    Err("original Headless owner closed with failure".into())
                }
                Err(error) => Err(error.clone()),
                _ => Err("close did not observe terminal physical retirement".into()),
            })
        } else {
            None
        };
        control.publish(
            &format!("response-{number}.json"),
            &arguments.response(number, result),
        )?;
        if game.execution_failed {
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
                return Ok(());
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

    fn observed(&self) -> Result<Reply, String> {
        let runtime_generation = self
            .session
            .runtime_generation()
            .ok_or("the original Runtime generation is unavailable")?;
        let snapshot = self.snapshot()?;
        Ok(Reply::Observed {
            observation: Observation {
                runtime_generation,
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
            },
        })
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
                                .resource::<nara::gameplay::GameplayCommandQueue>()
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
            Command::Observe => self.observed(),
            Command::Frame => {
                self.frame()?;
                self.observed()
            }
            Command::Step { commands } => {
                self.step(commands, deadline)?;
                self.observed()
            }
            Command::Capture => capture_resume(&self.session)
                .map(|state| Reply::Captured { state })
                .map_err(|error| format!("game capture was refused: {error}")),
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
                        break self.observed();
                    }
                    self.frame()?;
                    thread::sleep(POLL_INTERVAL);
                }
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
            .and_then(GameplayCommandTick::new)
            .ok_or("engine tick is exhausted")?;
        self.sequence
            .checked_add(commands.len() as u64)
            .ok_or("command source sequence is exhausted")?;
        let source = GameplayCommandIngressSource::external("two-level-game.continuity")
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
            let sequence = GameplayCommandSourceSequence::new(self.sequence)
                .ok_or("command source sequence is zero")?;
            let submission = self.session.submit(GameplayCommandSubmission::new(
                tick,
                source.clone(),
                sequence,
                draft,
            ));
            if let Err(error) = submission {
                self.execution_failed |= admitted;
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

fn command_draft(command: SemanticCommand) -> Result<GameplayCommandDraft, String> {
    let (id, movement) = match command {
        SemanticCommand::Move { direction, pressed } => {
            ("two-level-game.move", Some((direction, pressed)))
        }
        SemanticCommand::Marker => ("two-level-game.marker", None),
        SemanticCommand::Travel => ("two-level-game.travel", None),
        SemanticCommand::Instance => ("two-level-game.instance", None),
        SemanticCommand::Retry => ("two-level-game.retry", None),
    };
    let id =
        GameplayCommandTypeId::new(id).map_err(|error| format!("invalid game command: {error}"))?;
    let mut draft = GameplayCommandDraft::new(id);
    if let Some((direction, pressed)) = movement {
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("direction", GameplayCommandValue::I64(direction as i64))
            .map_err(|error| format!("invalid movement direction: {error}"))?;
        payload
            .insert("pressed", GameplayCommandValue::Bool(pressed))
            .map_err(|error| format!("invalid movement intent: {error}"))?;
        draft = draft.with_payload(payload);
    }
    Ok(draft)
}

fn check_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("continuity operation exceeded its five-second deadline".into())
    } else {
        Ok(())
    }
}
