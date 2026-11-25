use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    num::NonZeroU64,
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, RealTime, RuntimeState, RuntimeTimeSettings},
    project_host::{HeadlessSession, HeadlessSessionError},
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, Scenario, ServerCommandRejections, SideRoomPhase,
    capture_processed_resume, claim_processed_checkpoint,
    command::{CommandSchedule, CommandStats, ServerCommand},
    levels::{LevelId, LevelKey},
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
const DRAIN_STALL_TIMEOUT: Duration = Duration::from_secs(10);
const SCENE_POLL: Duration = Duration::from_millis(50);
const SHORT_FRAME_WAIT: Duration = Duration::from_millis(10);

pub(super) fn run(
    session: &mut HeadlessSession,
    options: ServeOptions,
    startup_deadline: Instant,
    output: &Output,
    report: &mut Report,
) -> Result<(), Failure> {
    report.stage = "service_startup";
    let checkpoint_configured = options.save_path.is_some();
    let (fixed, time) = session.observe(|world| {
        (
            *world.resource::<FixedTime>(),
            *world.resource::<RuntimeTimeSettings>(),
        )
    })?;
    let paused = options.paused || time.paused();
    let restore_paths = if let Some(path) = options.resume_state {
        Some((path, options.save_path))
    } else {
        let launch_options = RunOptions {
            scenario: Scenario::Interactive,
            save_path: options.save_path,
        };
        if let Some(launch) =
            two_level_game::player_launch(launch_options).map_err(Failure::Rejected)?
        {
            launch.submit_headless(session)?;
            if session.pause()?.is_some() || session.step()?.is_some() {
                return Err("Server exited before its checkpoint launch was applied".into());
            }
            session
                .observe(|world| {
                    two_level_game::player_launch_result(world)
                        .map(|result| result.map_err(str::to_owned))
                })?
                .ok_or("Server did not apply its first-tick checkpoint launch")?
                .map_err(Failure::Rejected)?;
            if !paused && session.resume()?.is_some() {
                return Err("Server exited before starting its live clock".into());
            }
        }
        None
    };
    if paused && session.state() != Some(RuntimeState::Paused) && session.pause()?.is_some() {
        return Err("Server exited before paused startup completed".into());
    }
    let restored = restore_paths.is_some();
    let (sequence, checkpoint_pending) = match restore_paths {
        Some((resume_path, checkpoint_path)) => {
            let resume = read_resume_state(&resume_path)?;
            restore_processed_resume(
                session,
                &resume,
                checkpoint_path.as_deref(),
                startup_deadline,
            )
            .map_err(|error| Failure::Rejected(format!("restore Server resume state: {error}")))?;
            (resume.pending.len() as u64, resume.checkpoint.is_some())
        }
        None => (0, false),
    };
    let initial = Observation::read(session)?;
    let now = Instant::now();
    if now >= startup_deadline {
        return Err("Server startup finished after its readiness deadline".into());
    }
    let (input, reader) = control::spawn()
        .map_err(|error| Failure::Error(format!("start Server input reader: {error}")))?;
    report.ready = true;
    report.stage = "service";
    report.state = Some(state(&initial.game));
    let mut service = Service {
        session,
        output,
        clock: FrameClock::new(now, fixed, time),
        checkpoint_configured,
        checkpoint_pending,
        paused,
        sequence,
        visited_levels: initial
            .game
            .level
            .map(LevelKey::level)
            .into_iter()
            .collect(),
    };
    let stop = output
        .send(
            READY,
            json!({
                "version": 1,
                "state": report.state,
                "timestep_nanos": fixed.timestep().as_nanos(),
                "checkpoint_configured": checkpoint_configured,
                "checkpoint_pending": checkpoint_pending,
                "paused": paused,
                "restored": restored,
                "commands": initial.commands(),
                "time": initial.time(),
            }),
        )
        .map_err(Failure::from)
        .and_then(|()| service.drive(&input));
    drop(input);

    let reader_sent_stop = matches!(
        &stop,
        Ok(Stop::Shutdown | Stop::Retired | Stop::Eof | Stop::InputError(_))
    );
    let reader_joined = if reader_sent_stop || reader.is_finished() {
        reader.join().is_ok()
    } else {
        false
    };
    let drain = if !service.paused
        && matches!(&stop, Ok(Stop::Shutdown | Stop::Eof | Stop::InputError(_)))
    {
        service.drain()
    } else {
        Ok(())
    };
    let observation = Observation::read(service.session);
    report.visited_levels = json!(service.visited_levels);
    if let Ok(observation) = &observation {
        report.state = Some(state(&observation.game));
        report.server_evidence = Some(json!({
            "stop": stop.as_ref().map_or("error", Stop::name),
            "commands": observation.commands(),
            "gameplay_rejections": observation.rejections,
            "time": observation.time(),
            "paused": service.paused,
            "input_reader_joined": reader_joined,
        }));
    }
    let stop = stop?;
    drain?;
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

struct Service<'a> {
    session: &'a mut HeadlessSession,
    output: &'a Output,
    clock: FrameClock,
    checkpoint_configured: bool,
    checkpoint_pending: bool,
    paused: bool,
    sequence: u64,
    visited_levels: BTreeSet<LevelId>,
}

impl Service<'_> {
    fn drive(&mut self, input: &Receiver<Event>) -> Result<Stop, Failure> {
        let mut next_maintenance = Instant::now() + SCENE_POLL;
        loop {
            self.output.check()?;
            let now = Instant::now();
            let wait = if self.checkpoint_pending {
                SCENE_POLL
            } else if self.paused {
                if now >= next_maintenance {
                    next_maintenance = now + SCENE_POLL;
                    let pending = self
                        .session
                        .observe(|world| scene_work_pending(world.resource::<GameSnapshot>()))?;
                    if pending {
                        if self.session.frame(Duration::ZERO)?.is_some() {
                            return Ok(Stop::AppExit);
                        }
                        self.record_visit()?;
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
            let event = if !self.paused && wait <= SHORT_FRAME_WAIT {
                match input.try_recv() {
                    Ok(event) => Ok(event),
                    Err(TryRecvError::Empty) => {
                        thread::sleep(wait);
                        continue;
                    }
                    Err(TryRecvError::Disconnected) => Err(RecvTimeoutError::Disconnected),
                }
            } else {
                input.recv_timeout(wait)
            };
            let stop = match event {
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
        if exit.is_some() {
            return Ok(true);
        }
        self.record_visit()?;
        Ok(false)
    }

    fn record_visit(&mut self) -> Result<(), Failure> {
        let level = self.session.observe(|world| {
            let game = world.resource::<GameSnapshot>();
            game.current_instance.and(game.level).map(LevelKey::level)
        })?;
        self.visited_levels.extend(level);
        Ok(())
    }

    fn drain(&mut self) -> Result<(), Failure> {
        self.clock.cap_period(SCENE_POLL);
        let mut status = DrainStatus::read(self.session)?;
        let mut last_progress = Instant::now();
        while !status.settled {
            let now = Instant::now();
            let remaining = DRAIN_STALL_TIMEOUT.saturating_sub(now.duration_since(last_progress));
            if remaining.is_zero() {
                return Err(
                    "Server made no progress settling admitted work during shutdown".into(),
                );
            }
            let Some(delta) = self.clock.take_due(now)? else {
                thread::sleep(self.clock.remaining(now).min(remaining));
                continue;
            };
            if self.drive_live_frame(delta)? && !settled(self.session)? {
                return Err("Server exited before admitted gameplay work settled".into());
            }
            let next = DrainStatus::read(self.session)?;
            if next.acknowledged > status.acknowledged
                || next.toward_pending_tick > status.toward_pending_tick
            {
                last_progress = Instant::now();
            }
            status = next;
        }
        Ok(())
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
        if self.checkpoint_pending && !request.allowed_while_checkpoint_pending() {
            reject(
                self.output,
                "Server checkpoint is pending. Send claim_checkpoint before driving or admitting gameplay commands",
            )?;
            return Ok(None);
        }
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
                    self.record_visit()?;
                }
                self.send_observation("paused")?;
            }
            Request::Resume {} => {
                if !self.paused {
                    reject(self.output, "Server resume requires a paused session")?;
                    return Ok(None);
                }
                let exit = self.session.resume()?;
                self.paused = false;
                if exit.is_some() {
                    return Ok(Some(Stop::AppExit));
                }
                self.clock.reset(Instant::now());
                self.record_visit()?;
                self.send_observation("resumed")?;
            }
            Request::Advance { elapsed_nanos } => {
                return self.advance(Duration::from_nanos(elapsed_nanos));
            }
            Request::Capture {} => {
                // ECS commands invalidate the host's paused capture boundary.
                if self.paused
                    && matches!(
                        self.session.capture(|_| ()),
                        Err(HeadlessSessionError::NotPaused)
                    )
                    && self.session.frame(Duration::ZERO)?.is_some()
                {
                    return Ok(Some(Stop::AppExit));
                }
                self.capture()?;
            }
            Request::ClaimCheckpoint {} => self.claim_checkpoint()?,
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
                "checkpoint_configured": self.checkpoint_configured,
                "checkpoint_pending": self.checkpoint_pending,
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
        if self.drive_live_frame(elapsed)? {
            self.paused = false;
            return Ok(Some(Stop::AppExit));
        }
        if self.session.pause()?.is_some() {
            return Ok(Some(Stop::AppExit));
        }
        self.record_visit()?;
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

    fn claim_checkpoint(&mut self) -> Result<(), Failure> {
        if !self.checkpoint_pending {
            return reject(self.output, "Server has no pending checkpoint claim");
        }
        if let Err(error) = claim_processed_checkpoint(self.session) {
            return reject(self.output, &error.to_string());
        }
        self.checkpoint_pending = false;
        self.send_observation("checkpoint_claimed")
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
        let intent = match command.into_intent() {
            Ok(intent) => intent,
            Err(message) => return reject(self.output, &message),
        };
        match submit_external_after(self.session, &mut self.sequence, intent, after_ticks) {
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
        world.resource::<CommandSchedule>().is_idle()
            && !scene_work_pending(world.resource::<GameSnapshot>())
    })?)
}

struct DrainStatus {
    settled: bool,
    acknowledged: u64,
    toward_pending_tick: Option<(u64, Duration)>,
}

impl DrainStatus {
    fn read(session: &HeadlessSession) -> Result<Self, Failure> {
        Ok(session.observe(|world| {
            let queue = world.resource::<CommandSchedule>();
            let fixed = world.resource::<FixedTime>();
            let stats = queue.stats();
            Self {
                settled: queue.is_idle() && !scene_work_pending(world.resource::<GameSnapshot>()),
                acknowledged: stats.acknowledged,
                toward_pending_tick: stats
                    .furthest_pending_tick
                    .filter(|&pending| pending > fixed.tick())
                    .map(|_| (fixed.tick(), fixed.remainder())),
            }
        })?)
    }
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
    queue: CommandStats,
    rejections: ServerCommandRejections,
    real: RealTime,
    fixed: FixedTime,
}

impl Observation {
    fn read(session: &HeadlessSession) -> Result<Self, Failure> {
        Ok(session.observe(|world| Self {
            game: world.resource::<GameSnapshot>().clone(),
            queue: world.resource::<CommandSchedule>().stats(),
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
