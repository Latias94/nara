//! The closed semantic vocabulary shared by development control and source-game replay.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};

mod schedule;
use crate::RunOptions;
#[cfg(all(test, feature = "local-player"))]
pub(crate) use schedule::with_test_script;
pub use schedule::{
    CommandKey, CommandRejection, CommandSchedule, CommandSequence, CommandSet, CommandSource,
    CommandSourceId, CommandStats, CommandSubmitError, CommandTick, MAX_COMMAND_TICK_OFFSET,
    MAX_SCHEDULED_COMMANDS, ScheduledCommand,
};
pub(crate) use schedule::{
    acknowledge_commands, mark_frame_input, needs_frame_input, prepare_commands,
};

/// External producer ID for commands submitted by the cooked Server.
pub const SERVER_COMMAND_SOURCE: &str = "two-level-game.server";

/// A game operation independent of its physical input binding or command ingress source.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticCommand {
    Move { direction: usize, pressed: bool },
    Marker,
    Travel,
    Instance,
    Retry,
}

impl<'de> Deserialize<'de> for SemanticCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Move { direction: usize, pressed: bool },
            Marker {},
            Travel {},
            Instance {},
            Retry {},
        }

        Ok(match Wire::deserialize(deserializer)? {
            Wire::Move { direction, pressed } => Self::Move { direction, pressed },
            Wire::Marker {} => Self::Marker,
            Wire::Travel {} => Self::Travel,
            Wire::Instance {} => Self::Instance,
            Wire::Retry {} => Self::Retry,
        })
    }
}

impl SemanticCommand {
    /// Checks the finite movement vocabulary before a command reaches the gameplay queue.
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Move { direction, .. } = self
            && *direction >= 4
        {
            return Err("movement direction must be in 0..4".into());
        }
        Ok(())
    }

    pub fn intent(&self) -> GameCommand {
        GameCommand::Player(ServerCommand::Gameplay(self.clone()))
    }

    #[cfg(any(feature = "development", test))]
    pub(crate) fn from_scheduled(command: &ScheduledCommand) -> Result<Self, String> {
        match command.intent() {
            GameCommand::Player(ServerCommand::Gameplay(command)) => {
                command.validate()?;
                Ok(command.clone())
            }
            _ => Err("recording contains an unsupported game operation".into()),
        }
    }
}

/// Commands available to an operator of the cooked dedicated server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerCommand {
    Gameplay(SemanticCommand),
    SideRoom,
    Lookout,
    Save,
    Restore,
}

impl ServerCommand {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Gameplay(command) => command.validate(),
            _ => Ok(()),
        }
    }

    pub fn into_intent(self) -> Result<GameCommand, String> {
        self.validate()?;
        Ok(GameCommand::Player(self))
    }
}

#[derive(Clone, Debug)]
pub enum GameCommand {
    Player(ServerCommand),
    RoundSave,
    Launch(RunOptions),
    Resume(Arc<[u8]>),
}
