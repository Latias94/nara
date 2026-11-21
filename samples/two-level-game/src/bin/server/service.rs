use std::{
    fs::{self, File},
    io::Read,
    num::NonZeroU64,
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, RealTime, RuntimeState, RuntimeTimeSettings},
    gameplay::{GameplayCommandQueue, GameplayCommandQueueStats},
    project_host::HeadlessSession,
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, Scenario, ServerCommandRejections, SideRoomPhase,
    capture_processed_resume,
    command::ServerCommand,
    restore_processed_resume,
    resume::processed::{
        MAX_PROCESSED_RESUME_BYTES, ProcessedResumeState, decode_processed_resume,
        encode_processed_resume,
    },
};

use super::{
    Report, ServeOptions,
    clock::FrameClock,
    control::{self, CloseRequest, Event, Request},
    output::Output,
    runtime::{Failure, state, submit_external_after},
};

const READY: &str = "TWO_LEVEL_SERVER_READY";
const REPLY: &str = "TWO_LEVEL_SERVER_REPLY";
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const PAUSED_POLL: Duration = Duration::from_millis(50);

pub(super) fn run(
    session: &mut HeadlessSession,
    options: ServeOptions,
    timestep: Duration,
    startup_deadline: Instant,
    output: &Output,
    report: &mut Report,
) -> Result<(), Failure> {
    report.stage = "service_startup";
    let checkpoint_configured = options.save_path.is_some();
    let max_delta = session.observe(|world| world.resource::<RuntimeTimeSettings>().max_delta())?;
    let mut clock = FrameClock::new(Instant::now(), timestep, max_delta);
    let launch_options = RunOptions {
        scenario: Scenario::Interactive,
        save_path: options.save_path,
    };
    if let Some(launch) =
        two_level_game::player_launch(launch_options).map_err(Failure::Rejected)?
    {
        session.submit(launch)?;
        apply_launch(session, &mut clock, startup_deadline)?;
    }
    if options.paused && session.pause()?.is_some() {
        return Err("Server exited before paused startup completed".into());
    }
    let (restored, sequence) = if let Some(path) = options.resume_state.as_deref() {
        let resume = read_resume_state(path)?;
        restore_processed_resume(session, &resume, startup_deadline)
            .map_err(|error| Failure::Rejected(format!("restore Server resume state: {error}")))?;
        (true, resume.pending.len() as u64)
    } else {
        (false, 0)
    };
    let initial = Observation::read(session)?;
    let (input, reader) = control::spawn()
        .map_err(|error| Failure::Error(format!("start Server input reader: {error}")))?;
    let now = Instant::now();
    if now >= startup_deadline {
        return Err("Server startup finished after its readiness deadline".into());
    }
    if !options.paused {
        clock.elapsed(now)?;
    }
    report.ready = true;
    report.stage = "service";
    report.state = Some(state(&initial.game));
    let mut service = Service {
        session,
        output,
        clock,
        checkpoint_configured,
        paused: options.paused,
        sequence,
    };
    let outcome = output
        .send(
            READY,
            json!({
                "version": 1,
                "state": report.state,
                "timestep_nanos": timestep.as_nanos(),
                "checkpoint_configured": checkpoint_configured,
                "paused": options.paused,
                "restored": restored,
                "commands": initial.commands(),
                "time": initial.time(),
            }),
        )
        .map_err(Failure::from)
        .and_then(|()| service.drive(&input));
    drop(input);

    let reader_sent_stop = matches!(
        &outcome,
        Ok(Stop::Shutdown | Stop::Retired | Stop::Eof | Stop::InputError(_))
    );
    let reader_joined = if reader_sent_stop || reader.is_finished() {
        reader.join().is_ok()
    } else {
        false
    };
    let observation = Observation::read(service.session);
    if let Ok(observation) = &observation {
        report.state = Some(state(&observation.game));
        report.server_evidence = Some(json!({
            "stop": outcome.as_ref().map_or("error", Stop::name),
            "commands": observation.commands(),
            "gameplay_rejections": observation.rejections,
            "time": observation.time(),
            "paused": service.paused,
            "input_reader_joined": reader_joined,
        }));
    }
    let stop = outcome?;
    if reader_sent_stop && !reader_joined {
        return Err("Server input reader failed to retire after orderly stop".into());
    }
    let observation = observation?;
    match stop {
        Stop::Retired => Ok(()),
        Stop::InputError(message) => Err(Failure::Error(message)),
        Stop::Eof if service.paused && !settled(service.session)? => Err(Failure::Rejected(
            "Server stdin closed while paused with unsettled work. Send retire before closing stdin to hand off"
                .into(),
        )),
        _ if observation.queue.retained_commands != 0 => {
            Err("Server exited with admitted gameplay commands still pending".into())
        }
        _ => Ok(()),
    }
}

fn read_resume_state(path: &Path) -> Result<ProcessedResumeState, Failure> {
    let read_error = |error| Failure::Rejected(format!("read Server resume state: {error}"));
    let invalid_file = || Failure::Rejected("Server resume state must be a regular file".into());
    if !fs::metadata(path).map_err(read_error)?.is_file() {
        return Err(invalid_file());
    }
    let file = File::open(path).map_err(read_error)?;
    let limit = MAX_PROCESSED_RESUME_BYTES as u64;
    let metadata = file.metadata().map_err(read_error)?;
    if !metadata.is_file() {
        return Err(invalid_file());
    }
    if metadata.len() > limit {
        return Err(Failure::Rejected(format!(
            "Server resume state exceeds its {limit}-byte limit"
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut reader = file.take(limit);
    reader.read_to_end(&mut bytes).map_err(read_error)?;
    if reader.get_ref().metadata().map_err(read_error)?.len() != bytes.len() as u64 {
        return Err(Failure::Rejected(
            "Server resume state changed length while being read".into(),
        ));
    }
    decode_processed_resume(&bytes)
        .map_err(|error| Failure::Rejected(format!("decode Server resume state: {error}")))
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

struct Service<'a> {
    session: &'a mut HeadlessSession,
    output: &'a Output,
    clock: FrameClock,
    checkpoint_configured: bool,
    paused: bool,
    sequence: u64,
}

impl Service<'_> {
    fn drive(&mut self, input: &Receiver<Event>) -> Result<Stop, Failure> {
        let mut next_maintenance = Instant::now() + PAUSED_POLL;
        loop {
            self.output.check()?;
            let now = Instant::now();
            let wait = if self.paused {
                if now >= next_maintenance {
                    next_maintenance = now + PAUSED_POLL;
                    let pending = self
                        .session
                        .observe(|world| scene_work_pending(world.resource::<GameSnapshot>()))?;
                    if pending && self.session.frame(Duration::ZERO)?.is_some() {
                        return Ok(Stop::AppExit);
                    }
                }
                next_maintenance.saturating_duration_since(Instant::now())
            } else {
                if let Some(delta) = self.clock.take_due(now)?
                    && self.drive_live_frame(delta)?
                {
                    return Ok(Stop::AppExit);
                }
                self.clock.remaining(Instant::now())
            };
            let stop = match input.recv_timeout(wait) {
                Ok(Event::Request(request)) => self.respond(request)?,
                Ok(Event::Close {
                    request,
                    keep_reading,
                }) => self.handle_close(request, keep_reading)?,
                Ok(Event::Rejected(message)) => {
                    reject(self.output, &message)?;
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
                if !self.paused && !matches!(stop, Stop::AppExit) {
                    self.drain()?;
                }
                return Ok(stop);
            }
        }
    }

    fn drive_live_frame(&mut self, delta: Duration) -> Result<bool, Failure> {
        let exit = if self.session.state() == Some(RuntimeState::Paused) {
            self.session.resume_with_frame(delta)?
        } else {
            self.session.frame(delta)?
        };
        Ok(exit.is_some())
    }

    fn drain(&mut self) -> Result<(), Failure> {
        let scheduled_delay = self.session.observe(|world| {
            let fixed = world.resource::<FixedTime>();
            let queue = world.resource::<GameplayCommandQueue>().stats();
            let future_ticks = queue
                .furthest_pending_tick
                .unwrap_or(fixed.tick())
                .saturating_sub(fixed.tick())
                .saturating_sub(1);
            fixed
                .timestep()
                .saturating_mul(u32::try_from(future_ticks).unwrap_or(u32::MAX))
        })?;
        let deadline = Instant::now()
            .checked_add(DRAIN_TIMEOUT.saturating_add(scheduled_delay))
            .ok_or("Server command drain deadline cannot be represented")?;
        loop {
            let complete = settled(self.session)?;
            let now = Instant::now();
            if now >= deadline {
                return Err(
                    "Server could not settle admitted work before the shutdown deadline".into(),
                );
            }
            if complete {
                return Ok(());
            }
            if let Some(delta) = self.clock.take_due(now)? {
                if self.drive_live_frame(delta)? && !settled(self.session)? {
                    return Err("Server exited before admitted gameplay work settled".into());
                }
            } else {
                thread::sleep(self.clock.remaining(now).min(deadline - now));
            }
        }
    }

    fn handle_close(
        &self,
        request: CloseRequest,
        keep_reading: Sender<()>,
    ) -> Result<Option<Stop>, Failure> {
        let message = match request {
            CloseRequest::Shutdown if self.paused && !settled(self.session)? => {
                "Server shutdown requires settled work while paused. Advance, resume, or retire"
            }
            CloseRequest::Retire if !self.paused => "Server retire requires a paused session",
            CloseRequest::Shutdown => return Ok(Some(Stop::Shutdown)),
            CloseRequest::Retire => return Ok(Some(Stop::Retired)),
        };
        reject(self.output, message)?;
        keep_reading
            .send(())
            .map_err(|_| "Server input reader disconnected after a rejected close request")?;
        Ok(None)
    }

    fn respond(&mut self, request: Request) -> Result<Option<Stop>, Failure> {
        match request {
            Request::Observe {} => self.send_observation("observed")?,
            Request::Pause {} => {
                if !self.paused {
                    let elapsed = self.clock.elapsed(Instant::now())?;
                    if self.drive_live_frame(elapsed)? {
                        return Ok(Some(Stop::AppExit));
                    }
                    let exit = self.session.pause()?;
                    self.paused = true;
                    if exit.is_some() {
                        return Ok(Some(Stop::AppExit));
                    }
                }
                self.send_observation("paused")?;
            }
            Request::Resume {} => {
                if !self.paused {
                    reject(self.output, "Server resume requires a paused session")?;
                    return Ok(None);
                }
                self.clock.reset(Instant::now());
                loop {
                    let now = Instant::now();
                    if let Some(delta) = self.clock.take_due(now)? {
                        let exit = self.drive_live_frame(delta)?;
                        self.paused = false;
                        if exit {
                            return Ok(Some(Stop::AppExit));
                        }
                        break;
                    }
                    thread::sleep(self.clock.remaining(now));
                }
                self.send_observation("resumed")?;
            }
            Request::Advance { elapsed_nanos } => {
                return self.advance(Duration::from_nanos(elapsed_nanos));
            }
            Request::Capture {} => self.capture()?,
            Request::Command {
                command,
                after_ticks,
            } => {
                self.admit(ServerCommand::Gameplay(command), after_ticks)?;
            }
            Request::SideRoom {} => self.admit(ServerCommand::SideRoom, NonZeroU64::MIN)?,
            Request::Lookout {} => self.admit(ServerCommand::Lookout, NonZeroU64::MIN)?,
            Request::Save {} => self.admit(ServerCommand::Save, NonZeroU64::MIN)?,
            Request::Restore {} => self.admit(ServerCommand::Restore, NonZeroU64::MIN)?,
            Request::Shutdown {} | Request::Retire {} => {
                return Err("Server close request bypassed the input reader".into());
            }
        }
        Ok(None)
    }

    fn send_observation(&self, status: &str) -> Result<(), Failure> {
        let observation = Observation::read(self.session)?;
        self.output.send(
            REPLY,
            json!({
                "status": status,
                "paused": self.paused,
                "state": state(&observation.game),
                "commands": observation.commands(),
                "gameplay_rejections": observation.rejections,
                "time": observation.time(),
            }),
        )?;
        Ok(())
    }

    fn advance(&mut self, elapsed: Duration) -> Result<Option<Stop>, Failure> {
        if !self.paused {
            reject(self.output, "Server advance requires a paused session")?;
            return Ok(None);
        }
        let max_delta = self
            .session
            .observe(|world| world.resource::<RuntimeTimeSettings>().max_delta())?;
        if elapsed > max_delta {
            reject(
                self.output,
                &format!(
                    "Server advance elapsed_nanos exceeds the configured max_delta of {} ns",
                    max_delta.as_nanos(),
                ),
            )?;
            return Ok(None);
        }
        if self.session.resume_with_frame(elapsed)?.is_some() {
            self.paused = false;
            return Ok(Some(Stop::AppExit));
        }
        if self.session.pause()?.is_some() {
            return Ok(Some(Stop::AppExit));
        }
        self.send_observation("advanced")?;
        Ok(None)
    }

    fn capture(&self) -> Result<(), Failure> {
        if !self.paused {
            return reject(self.output, "Server capture requires a paused session");
        }
        let captured = match capture_processed_resume(self.session) {
            Ok(state) => state,
            Err(error) => return reject(self.output, &error.to_string()),
        };
        let encoded = match encode_processed_resume(&captured) {
            Ok(encoded) => encoded,
            Err(error) => return reject(self.output, &error.to_string()),
        };
        let wire: Value = serde_json::from_slice(&encoded).map_err(|error| {
            Failure::Error(format!("invalid encoded Server resume state: {error}"))
        })?;
        self.output
            .send(REPLY, json!({"status": "captured", "state": wire}))?;
        Ok(())
    }

    fn admit(&mut self, command: ServerCommand, after_ticks: NonZeroU64) -> Result<(), Failure> {
        if !self.checkpoint_configured
            && matches!(command, ServerCommand::Save | ServerCommand::Restore)
        {
            return reject(
                self.output,
                "Server requires --save to configure a checkpoint slot",
            );
        }
        let draft = match command.into_draft() {
            Ok(draft) => draft,
            Err(message) => return reject(self.output, &message),
        };
        match submit_external_after(self.session, &mut self.sequence, draft, after_ticks) {
            Ok(key) => self.output.send(
                REPLY,
                json!({
                    "status": "accepted",
                    "source": key.source().id().map(|source| source.as_str()),
                    "sequence": key.source_sequence().get(),
                    "tick": key.tick().get(),
                }),
            )?,
            Err(Failure::Rejected(message)) => reject(self.output, &message)?,
            Err(error) => return Err(error),
        }
        Ok(())
    }
}

fn scene_work_pending(game: &GameSnapshot) -> bool {
    matches!(game.phase, GamePhase::Preparing | GamePhase::Reconstructing)
        || game.side_room == SideRoomPhase::Preparing
        || game.lookout == SideRoomPhase::Preparing
        || game.restoring
}

fn settled(session: &HeadlessSession) -> Result<bool, Failure> {
    Ok(session.observe(|world| {
        world.resource::<GameplayCommandQueue>().is_idle()
            && !scene_work_pending(world.resource::<GameSnapshot>())
    })?)
}

fn reject(output: &Output, message: &str) -> Result<(), Failure> {
    let message = message.chars().take(2048).collect::<String>();
    output.send(REPLY, json!({"status": "rejected", "message": message}))?;
    Ok(())
}

enum Stop {
    Shutdown,
    Retired,
    Eof,
    AppExit,
    InputError(String),
}

impl Stop {
    fn name(&self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::Retired => "retired",
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
    fixed: FixedTime,
}

impl Observation {
    fn read(session: &HeadlessSession) -> Result<Self, Failure> {
        Ok(session.observe(|world| Self {
            game: world.resource::<GameSnapshot>().clone(),
            queue: world.resource::<GameplayCommandQueue>().stats(),
            rejections: world.resource::<ServerCommandRejections>().clone(),
            real: *world.resource::<RealTime>(),
            fixed: *world.resource::<FixedTime>(),
        })?)
    }

    fn commands(&self) -> Value {
        json!({
            "accepted": self.queue.accepted,
            "acknowledged": self.queue.acknowledged,
            "pending": self.queue.pending_commands,
            "retained": self.queue.retained_commands,
            "acknowledged_through_tick": self.queue.acknowledged_through_tick,
        })
    }

    fn time(&self) -> Value {
        json!({
            "elapsed_nanos": self.real.elapsed.as_nanos(),
            "frames": self.real.frame,
            "fixed_debt_nanos": self.fixed.debt().as_nanos(),
            "fixed_remainder_nanos": self.fixed.remainder().as_nanos(),
        })
    }
}
