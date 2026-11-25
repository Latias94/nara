use std::{
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration,
    app::{
        FixedTime, PluginCategory, PluginDeclaration, PluginId, RuntimeGeneration, RuntimeState,
    },
    ecs::schedule::IntoScheduleConfigs,
    prelude::{App, CoreStage, Plugin, PluginError, Res, Resource},
    project_host::HeadlessSession,
    tasks::{TaskPools, TaskReturnState},
};

use super::{DEADLINE, ReplayError};
use crate::{
    GamePhase, GameSnapshot,
    command::{
        CommandSchedule, CommandSequence, CommandSet, CommandSource, CommandTick, ScheduledCommand,
        SemanticCommand,
    },
    levels::{LevelId, LevelKey},
    resume::ResumeState,
};

const RECORD_SOURCE: &str = "two-level-game.recording";
const REPLAY_SOURCE: &str = "two-level-game.playback";

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Record,
    Replay,
}

#[derive(Clone, Default, Resource)]
struct CaptureSlot(Arc<Mutex<Option<CapturedTick>>>);

struct CapturedTick {
    generation: u64,
    tick: u64,
    commands: Vec<ScheduledCommand>,
}

#[derive(Clone)]
struct Configuration {
    mode: Mode,
    slot: CaptureSlot,
}

impl ProductConfiguration for Configuration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(match self.mode {
            Mode::Record => b"two-level-game.exact-step-record/v1",
            Mode::Replay => b"two-level-game.exact-step-replay/v1",
        });
    }
}

struct CapturePlugin(Configuration);

impl Plugin for CapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.replay"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[crate::gameplay::PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if matches!(self.0.mode, Mode::Replay) {
            app.insert_resource(CommandSchedule::replay_only(
                CommandSource::replay(REPLAY_SOURCE).expect("fixed game Replay source"),
            ))?;
        }
        app.insert_resource(self.0.slot.clone())?;
        app.add_systems(CoreStage::FixedUpdate, capture.in_set(CommandSet::Capture))?;
        Ok(())
    }
}

fn capture(
    schedule: Res<CommandSchedule>,
    time: Res<FixedTime>,
    generation: Res<RuntimeGeneration>,
    slot: Res<CaptureSlot>,
) {
    *slot.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(CapturedTick {
        generation: generation.get(),
        tick: time.tick(),
        commands: schedule.current().to_vec(),
    });
}

pub(super) struct Game {
    pub(super) session: HeadlessSession,
    mode: Mode,
    slot: CaptureSlot,
    sequence: u64,
}

impl Game {
    pub(super) fn new(project: &Path, mode: Mode) -> Result<Self, ReplayError> {
        let root = crate::open_explicit_project_root(project)
            .map_err(|error| ReplayError::new(error.to_string()))?;
        let slot = CaptureSlot::default();
        let recipe = crate::product_recipe()
            .add_configured_plugin(
                Configuration {
                    mode,
                    slot: slot.clone(),
                },
                |configuration| CapturePlugin(configuration.clone()),
            )
            .map_err(|error| ReplayError::new(error.to_string()))?;
        Ok(Self {
            session: HeadlessSession::new(root, recipe),
            mode,
            slot,
            sequence: 0,
        })
    }

    pub(super) fn pause_at_startup(&mut self) -> Result<(), ReplayError> {
        if self.session.pause().map_err(runtime_error)?.is_some() {
            return Err(ReplayError::new("game exited before recording startup"));
        }
        self.settle(0)?;
        let state = self.capture()?;
        if state.clock.ticks != 0 || state.runtime_time.has_pending_time() {
            return Err(ReplayError::new(
                "recording requires fresh paused game time",
            ));
        }
        Ok(())
    }

    pub(super) fn capture(&self) -> Result<ResumeState, ReplayError> {
        crate::capture_resume(&self.session).map_err(runtime_error)
    }

    fn snapshot(&self) -> Result<GameSnapshot, ReplayError> {
        self.session
            .observe(|world| world.resource::<GameSnapshot>().clone())
            .map_err(runtime_error)
    }

    fn tick(&self) -> Result<u64, ReplayError> {
        self.session
            .observe(|world| world.resource::<FixedTime>().tick())
            .map_err(runtime_error)
    }

    fn settle(&mut self, tick: u64) -> Result<(), ReplayError> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let snapshot = self.snapshot()?;
            if let Some(error) = snapshot.error {
                return Err(ReplayError::new(format!(
                    "game refused step {tick}: {error}"
                )));
            }
            let (idle, returned) = self
                .session
                .observe(|world| {
                    (
                        world.resource::<CommandSchedule>().is_idle(),
                        world.resource::<TaskPools>().return_state(),
                    )
                })
                .map_err(runtime_error)?;
            if returned == TaskReturnState::CleanupFailed {
                return Err(ReplayError::new(
                    "game task cleanup failed while awaiting Scene readiness",
                ));
            }
            if snapshot.initialized
                && snapshot.phase == GamePhase::Running
                && !snapshot.restoring
                && idle
                && returned == TaskReturnState::Returned
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ReplayError::new(format!(
                    "game Scene-ready barrier timed out at step {tick}"
                )));
            }
            if self
                .session
                .frame(Duration::ZERO)
                .map_err(runtime_error)?
                .is_some()
                || self.session.state() != Some(RuntimeState::Paused)
                || self.tick()? != tick
            {
                return Err(ReplayError::new(
                    "Scene-ready maintenance changed fixed time or exited",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn step(
        &mut self,
        commands: &[SemanticCommand],
    ) -> Result<(u64, Vec<ScheduledCommand>, ResumeState), ReplayError> {
        let before = self.snapshot()?;
        let tick = self
            .tick()?
            .checked_add(1)
            .ok_or_else(|| ReplayError::new("fixed tick exhausted"))?;
        let generation = self.session.runtime_generation();
        self.slot
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        for command in commands {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or_else(|| ReplayError::new("recording sequence exhausted"))?;
            let source = match self.mode {
                Mode::Record => CommandSource::external(RECORD_SOURCE),
                Mode::Replay => CommandSource::replay(REPLAY_SOURCE),
            }
            .expect("fixed game command source");
            let submission = ScheduledCommand::new(
                CommandTick::new(tick).expect("successor tick is nonzero"),
                source,
                CommandSequence::new(self.sequence).expect("successor sequence is nonzero"),
                command.intent(),
            );
            submission
                .submit_headless(&mut self.session)
                .map_err(runtime_error)?;
        }
        if self.session.step().map_err(runtime_error)?.is_some() {
            return Err(ReplayError::new("game exited during the recorded step"));
        }
        let captured = self
            .slot
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .ok_or_else(|| {
                ReplayError::new("original command consumption did not reach successful Capture")
            })?;
        let acknowledged = self
            .session
            .observe(|world| {
                world
                    .resource::<CommandSchedule>()
                    .stats()
                    .acknowledged_through_tick
            })
            .map_err(runtime_error)?;
        if generation != Some(captured.generation)
            || generation != self.session.runtime_generation()
            || captured.tick != tick
            || self.tick()? != tick
            || acknowledged != tick
        {
            return Err(ReplayError::new(
                "original command generation/tick was not acknowledged",
            ));
        }
        let actual = captured
            .commands
            .iter()
            .map(SemanticCommand::from_scheduled)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ReplayError::new)?;
        if actual != commands {
            return Err(ReplayError::new(
                "actual consumed commands differ from the selected finite program",
            ));
        }
        self.settle(tick)?;
        confirm(&before, &self.snapshot()?, commands)?;
        Ok((tick, captured.commands, self.capture()?))
    }
}

fn confirm(
    before: &GameSnapshot,
    after: &GameSnapshot,
    commands: &[SemanticCommand],
) -> Result<(), ReplayError> {
    for command in commands {
        let Some(key) = before.level else {
            return Err(ReplayError::new("active game level is unavailable"));
        };
        let (target, travel, restarts) = match command {
            SemanticCommand::Travel => (
                LevelKey::main(if key.level() == LevelId::Meadow {
                    LevelId::Cavern
                } else {
                    LevelId::Meadow
                }),
                1,
                0,
            ),
            SemanticCommand::Instance => (
                LevelKey::new(key.level(), if key.instance() == 0 { 1 } else { 0 })
                    .expect("finite game instance"),
                1,
                0,
            ),
            SemanticCommand::Retry => (key, 0, 1),
            SemanticCommand::Marker => {
                if after.markers.len() != before.markers.len() + 1 {
                    return Err(ReplayError::new(
                        "marker command was acknowledged without the expected game result",
                    ));
                }
                continue;
            }
            SemanticCommand::Move { .. } => continue,
        };
        if after.level != Some(target)
            || after.current_instance == before.current_instance
            || after.travel_count != before.travel_count + travel
            || after.restarts != before.restarts + restarts
        {
            return Err(ReplayError::new(
                "Scene command was acknowledged without its matching activation and reconstruction",
            ));
        }
    }
    Ok(())
}

fn runtime_error(error: impl std::fmt::Display) -> ReplayError {
    ReplayError::new(error.to_string())
}

#[cfg(test)]
mod tests;
