use nara_ecs::Resource;
use thiserror::Error;

use crate::GameplayCommandDraft;

/// Maximum semantic command drafts collected during one runtime frame.
pub const MAX_LOCAL_GAMEPLAY_COMMANDS: usize = 64;

/// Public frame-local collection point for domain adapters that lower semantic intents.
///
/// Producers register through [`crate::add_local_gameplay_command_systems`], coalesce their own
/// domain signals, and push only the drafts that should enter the next open authoritative tick.
/// The gameplay plugin drains the collection exactly once before command admission.
#[derive(Debug, Default, Resource)]
pub struct LocalGameplayCommands {
    commands: Vec<GameplayCommandDraft>,
}

impl LocalGameplayCommands {
    pub fn push(&mut self, command: GameplayCommandDraft) -> Result<(), LocalGameplayCommandError> {
        if self.commands.len() == MAX_LOCAL_GAMEPLAY_COMMANDS {
            return Err(LocalGameplayCommandError::CapacityExceeded {
                maximum: MAX_LOCAL_GAMEPLAY_COMMANDS,
            });
        }
        self.commands.push(command);
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = GameplayCommandDraft> + '_ {
        self.commands.drain(..)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LocalGameplayCommandError {
    #[error("local gameplay command collection reached its {maximum}-item capacity")]
    CapacityExceeded { maximum: usize },
}
