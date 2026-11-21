use std::{
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, RealTime, RuntimeTimeSettings},
    gameplay::{GameplayCommandQueue, GameplayCommandQueueStats},
    project_host::HeadlessSession,
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, ServerCommandRejections, SideRoomPhase,
    command::ServerCommand,
};

use super::{
    Report,
    clock::FrameClock,
    control::{self, Event, Request},
    output::Output,
    runtime::{Failure, state, submit_external},
};

const READY: &str = "TWO_LEVEL_SERVER_READY";
const REPLY: &str = "TWO_LEVEL_SERVER_REPLY";
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn run(
    session: &mut HeadlessSession,
    options: RunOptions,
    timestep: Duration,
    startup_deadline: Instant,
    output: &Output,
    report: &mut Report,
) -> Result<(), Failure> {
    report.stage = "service_startup";
    let checkpoint_configured = options.save_path.is_some();
    let max_delta = session.observe(|world| world.resource::<RuntimeTimeSettings>().max_delta())?;
    let mut clock = FrameClock::new(Instant::now(), timestep, max_delta);
    if let Some(launch) = two_level_game::player_launch(options).map_err(Failure::Rejected)? {
        session.submit(launch)?;
        apply_launch(session, &mut clock, startup_deadline)?;
    }
    let initial = Observation::read(session)?;
    let (input, reader) = control::spawn()
        .map_err(|error| Failure::Error(format!("start Server input reader: {error}")))?;
    let now = Instant::now();
    if now >= startup_deadline {
        return Err("Server startup finished after its readiness deadline".into());
    }
    clock.elapsed(now)?;
    report.ready = true;
    report.stage = "service";
    report.state = Some(state(&initial.game));
    let outcome = output
        .send(
            READY,
            json!({
                "version": 1,
                "state": report.state,
                "timestep_nanos": timestep.as_nanos(),
                "checkpoint_configured": checkpoint_configured,
            }),
        )
        .map_err(Failure::from)
        .and_then(|()| drive(session, &mut clock, &input, output, checkpoint_configured));
    drop(input);

    let reader_sent_stop = matches!(
        &outcome,
        Ok(Stop::Shutdown | Stop::Eof | Stop::InputError(_))
    );
    let reader_joined = if reader_sent_stop || reader.is_finished() {
        reader.join().is_ok()
    } else {
        false
    };
    let observation = Observation::read(session);
    if let Ok(observation) = &observation {
        report.state = Some(state(&observation.game));
        report.server_evidence = Some(json!({
            "stop": outcome.as_ref().map_or("error", Stop::name),
            "commands": observation.commands(),
            "gameplay_rejections": observation.rejections,
            "time": observation.time(),
            "input_reader_joined": reader_joined,
        }));
    }
    let stop = outcome?;
    if reader_sent_stop && !reader_joined {
        return Err("Server input reader failed to retire after orderly stop".into());
    }
    if observation?.queue.retained_commands != 0 {
        return Err("Server exited with admitted gameplay commands still pending".into());
    }
    match stop {
        Stop::InputError(message) => Err(Failure::Error(message)),
        _ => Ok(()),
    }
}

fn apply_launch(
    session: &mut HeadlessSession,
    clock: &mut FrameClock,
    deadline: Instant,
) -> Result<(), Failure> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(
                "Server did not apply its checkpoint launch before the startup deadline".into(),
            );
        }
        let Some(delta) = clock.take_due(now)? else {
            thread::sleep(clock.remaining(now).min(deadline - now));
            continue;
        };
        if session.frame(delta)?.is_some() {
            return Err("Server exited before its checkpoint launch was applied".into());
        }
        if Instant::now() >= deadline {
            return Err("Server checkpoint launch finished after the startup deadline".into());
        }
        let launch = session.observe(|world| {
            two_level_game::player_launch_result(world).map(|result| result.map_err(str::to_owned))
        })?;
        if let Some(result) = launch {
            return result.map_err(Failure::Rejected);
        }
    }
}

fn drive(
    session: &mut HeadlessSession,
    clock: &mut FrameClock,
    input: &Receiver<Event>,
    output: &Output,
    checkpoint_configured: bool,
) -> Result<Stop, Failure> {
    let mut sequence = 0;
    loop {
        output.check()?;
        if let Some(delta) = clock.take_due(Instant::now())?
            && session.frame(delta)?.is_some()
        {
            return Ok(Stop::AppExit);
        }
        let stop = match input.recv_timeout(clock.remaining(Instant::now())) {
            Ok(Event::Request(request)) => respond(
                session,
                &mut sequence,
                request,
                output,
                checkpoint_configured,
            )?,
            Ok(Event::Rejected(message)) => {
                reject(output, &message)?;
                None
            }
            Ok(Event::Eof) => Some(Stop::Eof),
            Ok(Event::Failed(message)) => Some(Stop::InputError(message)),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Server input reader disconnected without a terminal event".into());
            }
        };
        if let Some(stop) = stop {
            drain(session, clock)?;
            return Ok(stop);
        }
    }
}

fn drain(session: &mut HeadlessSession, clock: &mut FrameClock) -> Result<(), Failure> {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    loop {
        let complete = settled(session)?;
        let now = Instant::now();
        if now >= deadline {
            return Err(
                "Server could not settle admitted work before the shutdown deadline".into(),
            );
        }
        if complete {
            return Ok(());
        }
        if let Some(delta) = clock.take_due(now)? {
            if session.frame(delta)?.is_some() && !settled(session)? {
                return Err("Server exited before admitted gameplay work settled".into());
            }
        } else {
            thread::sleep(clock.remaining(now).min(deadline - now));
        }
    }
}

fn respond(
    session: &mut HeadlessSession,
    sequence: &mut u64,
    request: Request,
    output: &Output,
    checkpoint_configured: bool,
) -> Result<Option<Stop>, Failure> {
    let command = match request {
        Request::Observe {} => {
            let observation = Observation::read(session)?;
            output.send(
                REPLY,
                json!({
                    "status": "observed",
                    "state": state(&observation.game),
                    "commands": observation.commands(),
                    "gameplay_rejections": observation.rejections,
                    "time": observation.time(),
                }),
            )?;
            return Ok(None);
        }
        Request::Shutdown {} => return Ok(Some(Stop::Shutdown)),
        Request::Command { command } => ServerCommand::Gameplay(command),
        Request::SideRoom {} => ServerCommand::SideRoom,
        Request::Lookout {} => ServerCommand::Lookout,
        Request::Save {} => ServerCommand::Save,
        Request::Restore {} => ServerCommand::Restore,
    };
    if !checkpoint_configured && matches!(command, ServerCommand::Save | ServerCommand::Restore) {
        reject(
            output,
            "Server requires --save to configure a checkpoint slot",
        )?;
        return Ok(None);
    }
    let draft = match command.into_draft() {
        Ok(draft) => draft,
        Err(message) => {
            reject(output, &message)?;
            return Ok(None);
        }
    };
    match submit_external(session, sequence, draft) {
        Ok(key) => output.send(
            REPLY,
            json!({
                "status": "accepted",
                "source": key.source().id().map(|source| source.as_str()),
                "sequence": key.source_sequence().get(),
                "tick": key.tick().get(),
            }),
        )?,
        Err(Failure::Rejected(message)) => reject(output, &message)?,
        Err(error) => return Err(error),
    }
    Ok(None)
}

fn settled(session: &HeadlessSession) -> Result<bool, Failure> {
    Ok(session.observe(|world| {
        let game = world.resource::<GameSnapshot>();
        world.resource::<GameplayCommandQueue>().is_idle()
            && !matches!(game.phase, GamePhase::Preparing | GamePhase::Reconstructing)
            && game.side_room != SideRoomPhase::Preparing
            && game.lookout != SideRoomPhase::Preparing
            && !game.restoring
    })?)
}

fn reject(output: &Output, message: &str) -> Result<(), Failure> {
    let message = message.chars().take(2048).collect::<String>();
    output.send(REPLY, json!({"status": "rejected", "message": message}))?;
    Ok(())
}

enum Stop {
    Shutdown,
    Eof,
    AppExit,
    InputError(String),
}

impl Stop {
    fn name(&self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::Eof => "stdin_eof",
            Self::AppExit => "app_exit",
            Self::InputError(_) => "input_error",
        }
    }
}

struct Observation {
    game: GameSnapshot,
    queue: GameplayCommandQueueStats,
    rejections: ServerCommandRejections,
    real: RealTime,
    debt: Duration,
}

impl Observation {
    fn read(session: &HeadlessSession) -> Result<Self, Failure> {
        Ok(session.observe(|world| Self {
            game: world.resource::<GameSnapshot>().clone(),
            queue: world.resource::<GameplayCommandQueue>().stats(),
            rejections: world.resource::<ServerCommandRejections>().clone(),
            real: *world.resource::<RealTime>(),
            debt: world.resource::<FixedTime>().debt(),
        })?)
    }

    fn commands(&self) -> Value {
        json!({
            "accepted": self.queue.accepted,
            "acknowledged": self.queue.acknowledged,
            "retained": self.queue.retained_commands,
            "acknowledged_through_tick": self.queue.acknowledged_through_tick,
        })
    }

    fn time(&self) -> Value {
        json!({
            "elapsed_nanos": self.real.elapsed.as_nanos(),
            "frames": self.real.frame,
            "fixed_debt_nanos": self.debt.as_nanos(),
        })
    }
}
