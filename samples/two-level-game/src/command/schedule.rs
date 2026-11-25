use std::{collections::BTreeMap, num::NonZeroU64};

#[cfg(feature = "desktop")]
use nara::project_host::{DesktopControlError, DesktopRun};
use nara::{
    app::{FixedTime, RealTime, RuntimeFault},
    ecs::{Command, Res, ResMut, Resource, SystemSet, World, bevy_ecs::error::BevyError},
    project_host::{HeadlessSession, HeadlessSessionError},
};
use serde::{Deserialize, Deserializer, Serialize};

use super::GameCommand;
use crate::resume::MAX_RESUME_BYTES;

pub type CommandTick = NonZeroU64;
pub type CommandSequence = NonZeroU64;
pub const MAX_SCHEDULED_COMMANDS: usize = 4_096;
pub const MAX_COMMAND_TICK_OFFSET: u64 = 600;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct CommandSourceId(Box<str>);

impl CommandSourceId {
    pub fn new(id: impl Into<Box<str>>) -> Result<Self, CommandRejection> {
        let id = id.into();
        if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
            return Err(CommandRejection::InvalidSource);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CommandSourceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Box::<str>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CommandSource {
    LocalAction,
    Test { driver: CommandSourceId },
    Replay { stream: CommandSourceId },
    External { producer: CommandSourceId },
}

impl CommandSource {
    pub fn test(id: impl Into<Box<str>>) -> Result<Self, CommandRejection> {
        Ok(Self::Test {
            driver: CommandSourceId::new(id)?,
        })
    }

    pub fn replay(id: impl Into<Box<str>>) -> Result<Self, CommandRejection> {
        Ok(Self::Replay {
            stream: CommandSourceId::new(id)?,
        })
    }

    pub fn external(id: impl Into<Box<str>>) -> Result<Self, CommandRejection> {
        Ok(Self::External {
            producer: CommandSourceId::new(id)?,
        })
    }

    pub fn id(&self) -> Option<&CommandSourceId> {
        match self {
            Self::LocalAction => None,
            Self::Test { driver } => Some(driver),
            Self::Replay { stream } => Some(stream),
            Self::External { producer } => Some(producer),
        }
    }

    pub(crate) fn is_external(&self, id: &str) -> bool {
        matches!(self, Self::External { producer } if producer.as_str() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandKey {
    tick: CommandTick,
    source: CommandSource,
    source_sequence: CommandSequence,
}

impl CommandKey {
    pub const fn new(
        tick: CommandTick,
        source: CommandSource,
        source_sequence: CommandSequence,
    ) -> Self {
        Self {
            tick,
            source,
            source_sequence,
        }
    }

    pub const fn tick(&self) -> CommandTick {
        self.tick
    }

    pub const fn source(&self) -> &CommandSource {
        &self.source
    }

    pub const fn source_sequence(&self) -> CommandSequence {
        self.source_sequence
    }
}

#[derive(Clone, Debug)]
pub struct ScheduledCommand {
    key: CommandKey,
    intent: GameCommand,
}

impl ScheduledCommand {
    pub const fn new(
        tick: CommandTick,
        source: CommandSource,
        sequence: CommandSequence,
        intent: GameCommand,
    ) -> Self {
        Self {
            key: CommandKey::new(tick, source, sequence),
            intent,
        }
    }

    pub const fn key(&self) -> &CommandKey {
        &self.key
    }

    pub const fn tick(&self) -> CommandTick {
        self.key.tick()
    }

    pub const fn source(&self) -> &CommandSource {
        self.key.source()
    }

    pub const fn source_sequence(&self) -> CommandSequence {
        self.key.source_sequence()
    }

    pub const fn intent(&self) -> &GameCommand {
        &self.intent
    }

    pub fn into_parts(self) -> (CommandKey, GameCommand) {
        (self.key, self.intent)
    }

    pub fn submit_headless(
        self,
        session: &mut HeadlessSession,
    ) -> Result<CommandKey, CommandSubmitError> {
        let result = session.apply_command(self)?;
        finish_submit(result.output, result.post_operation_fault)
    }

    #[cfg(feature = "desktop")]
    pub fn submit_desktop(self, run: &mut DesktopRun) -> Result<CommandKey, CommandSubmitError> {
        let result = run.apply_command(self)?;
        finish_submit(result.output, result.post_operation_fault)
    }
}

impl Command for ScheduledCommand {
    type Out = Result<CommandKey, CommandRejection>;

    fn apply(self, world: &mut World) -> Self::Out {
        world
            .get_resource_mut::<CommandSchedule>()
            .ok_or(CommandRejection::Unavailable)?
            .schedule(self)
    }
}

fn finish_submit(
    output: Result<CommandKey, CommandRejection>,
    fault: Option<RuntimeFault>,
) -> Result<CommandKey, CommandSubmitError> {
    match fault {
        Some(fault) => Err(CommandSubmitError::Runtime { output, fault }),
        None => output.map_err(Into::into),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandSubmitError {
    #[error(transparent)]
    Headless(#[from] HeadlessSessionError),
    #[cfg(feature = "desktop")]
    #[error(transparent)]
    Desktop(#[from] DesktopControlError),
    #[error(transparent)]
    Rejected(#[from] CommandRejection),
    #[error("the runtime faulted after game command admission: {fault:?}")]
    Runtime {
        output: Result<CommandKey, CommandRejection>,
        fault: RuntimeFault,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandRejection {
    #[error("the game command schedule is unavailable")]
    Unavailable,
    #[error("game command source ID is invalid")]
    InvalidSource,
    #[error("game command source is excluded by the replay session")]
    UnexpectedSource { actual: CommandSource },
    #[error("game command targets a closed tick")]
    Late { target: u64, closed_through: u64 },
    #[error("game command exceeds the future tick horizon")]
    TooFarFuture { target: u64 },
    #[error("game command key is already scheduled for this tick")]
    Duplicate,
    #[error("game resume command exceeds the resume format's byte limit")]
    ResumeTooLarge,
    #[error("game command schedule is full")]
    Full,
    #[error("the next game command tick or sequence is exhausted")]
    Exhausted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandStats {
    pub accepted: u64,
    pub admitted: u64,
    pub acknowledged: u64,
    pub rejected: u64,
    pub pending_commands: usize,
    pub active_commands: usize,
    pub retained_commands: usize,
    pub closed_through_tick: u64,
    pub acknowledged_through_tick: u64,
    pub furthest_pending_tick: Option<u64>,
}

#[derive(Default, Resource)]
pub struct CommandSchedule {
    pending: BTreeMap<CommandKey, GameCommand>,
    current: Vec<ScheduledCommand>,
    progress: CommandStats,
    local_sequence: u64,
    replay_source: Option<CommandSource>,
    input_frame: Option<u64>,
}

impl CommandSchedule {
    #[cfg(feature = "development")]
    pub(crate) fn replay_only(source: CommandSource) -> Self {
        Self {
            replay_source: Some(source),
            ..Self::default()
        }
    }

    pub fn schedule(&mut self, command: ScheduledCommand) -> Result<CommandKey, CommandRejection> {
        let (key, intent) = command.into_parts();
        let target = key.tick().get();
        let closed_through = self.progress.closed_through_tick;
        let refusal = if self
            .replay_source
            .as_ref()
            .is_some_and(|source| source != key.source())
        {
            Some(CommandRejection::UnexpectedSource {
                actual: key.source().clone(),
            })
        } else if target <= closed_through {
            Some(CommandRejection::Late {
                target,
                closed_through,
            })
        } else if target - closed_through > MAX_COMMAND_TICK_OFFSET {
            Some(CommandRejection::TooFarFuture { target })
        } else if self.pending.contains_key(&key) {
            Some(CommandRejection::Duplicate)
        } else if matches!(&intent, GameCommand::Resume(bytes) if bytes.len() > MAX_RESUME_BYTES) {
            Some(CommandRejection::ResumeTooLarge)
        } else if self.pending.len() + self.current.len() == MAX_SCHEDULED_COMMANDS {
            Some(CommandRejection::Full)
        } else {
            None
        };
        if let Some(error) = refusal {
            self.progress.rejected = self.progress.rejected.saturating_add(1);
            return Err(error);
        }
        self.pending.insert(key.clone(), intent);
        self.progress.accepted = self.progress.accepted.saturating_add(1);
        Ok(key)
    }

    pub(crate) fn push_local(&mut self, intent: GameCommand) -> Result<(), CommandRejection> {
        let tick = self
            .progress
            .closed_through_tick
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(CommandRejection::Exhausted)?;
        let sequence = self
            .local_sequence
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(CommandRejection::Exhausted)?;
        self.schedule(ScheduledCommand::new(
            tick,
            CommandSource::LocalAction,
            sequence,
            intent,
        ))?;
        self.local_sequence = sequence.get();
        Ok(())
    }

    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
            && self.current.is_empty()
            && self.progress.closed_through_tick == self.progress.acknowledged_through_tick
    }

    pub fn stats(&self) -> CommandStats {
        CommandStats {
            pending_commands: self.pending.len(),
            active_commands: self.current.len(),
            retained_commands: self.pending.len() + self.current.len(),
            furthest_pending_tick: self
                .pending
                .last_key_value()
                .map(|(key, _)| key.tick().get()),
            ..self.progress
        }
    }

    pub(crate) fn pending_commands(&self) -> impl Iterator<Item = (&CommandKey, &GameCommand)> {
        self.pending.iter()
    }

    pub fn current(&self) -> &[ScheduledCommand] {
        &self.current
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum CommandSet {
    FrameInput,
    Consume,
    Capture,
}

pub(crate) fn needs_frame_input(time: Res<RealTime>, schedule: Res<CommandSchedule>) -> bool {
    schedule.input_frame != Some(time.frame)
}

pub(crate) fn mark_frame_input(time: Res<RealTime>, mut schedule: ResMut<CommandSchedule>) {
    schedule.input_frame = Some(time.frame);
}

pub(crate) fn prepare_commands(
    time: Res<FixedTime>,
    mut schedule: ResMut<CommandSchedule>,
) -> Result<(), BevyError> {
    let tick = time.tick();
    if schedule.progress.acknowledged_through_tick.checked_add(1) != Some(tick)
        || schedule.progress.closed_through_tick != schedule.progress.acknowledged_through_tick
    {
        return Err(BevyError::error(
            "game command schedule did not finish the preceding fixed tick",
        ));
    }
    schedule.progress.closed_through_tick = tick;
    while schedule
        .pending
        .first_key_value()
        .is_some_and(|(key, _)| key.tick().get() == tick)
    {
        let (key, intent) = schedule
            .pending
            .pop_first()
            .expect("tick has a pending command");
        schedule.current.push(ScheduledCommand { key, intent });
    }
    schedule.progress.admitted = schedule
        .progress
        .admitted
        .saturating_add(schedule.current.len() as u64);
    Ok(())
}

pub(crate) fn acknowledge_commands(time: Res<FixedTime>, mut schedule: ResMut<CommandSchedule>) {
    schedule.progress.acknowledged = schedule
        .progress
        .acknowledged
        .saturating_add(schedule.current.len() as u64);
    schedule.progress.acknowledged_through_tick = time.tick();
    schedule.current.clear();
}

#[cfg(all(test, feature = "local-player"))]
pub(crate) fn with_test_script(
    recipe: nara::ProductRecipe,
    commands: Vec<ScheduledCommand>,
) -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    use nara::app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, StartupStage,
    };

    struct TestScript(Vec<ScheduledCommand>);

    impl Plugin for TestScript {
        fn declaration() -> &'static PluginDeclaration {
            const DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("two-level-game.test-command-script"),
                PluginCategory::Runtime,
            )
            .requires_plugins(&[crate::gameplay::PLUGIN_ID]);
            &DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            let commands = self.0.clone();
            app.add_systems(
                StartupStage::Tooling,
                move |mut schedule: ResMut<CommandSchedule>| -> Result<(), BevyError> {
                    for command in commands.iter().cloned() {
                        schedule.schedule(command).map_err(BevyError::error)?;
                    }
                    Ok(())
                },
            )?;
            Ok(())
        }
    }

    recipe.add_configured_plugin(format!("{commands:?}").into_bytes(), move || {
        TestScript(commands.clone())
    })
}
