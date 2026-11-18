//! The closed semantic vocabulary shared by development control and source-game replay.

use nara::gameplay::{GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandValue};
use serde::{Deserialize, Deserializer, Serialize};

use crate::gameplay::commands;

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

    /// Builds the existing gameplay command after semantic admission.
    pub(crate) fn draft(&self) -> GameplayCommandDraft {
        match self {
            Self::Move { direction, pressed } => commands::movement(*direction, *pressed),
            Self::Marker => commands::action(commands::MARKER),
            Self::Travel => commands::action(commands::TRAVEL),
            Self::Instance => commands::action(commands::INSTANCE),
            Self::Retry => commands::action(commands::RETRY),
        }
    }

    /// Decodes only exact game commands, with no target or unrepresented payload fields.
    pub(crate) fn from_envelope(envelope: &GameplayCommandEnvelope) -> Result<Self, String> {
        if envelope.command_type().as_str() == commands::MOVE {
            let (direction, pressed) = decode_movement(envelope)?;
            return Ok(Self::Move { direction, pressed });
        }
        if envelope.target().is_some() {
            return Err("semantic game commands cannot have a target".into());
        }
        let payload = envelope.payload();
        if !payload.is_empty() {
            return Err("semantic game actions require an empty payload".into());
        }
        match envelope.command_type().as_str() {
            commands::MARKER => Ok(Self::Marker),
            commands::TRAVEL => Ok(Self::Travel),
            commands::INSTANCE => Ok(Self::Instance),
            commands::RETRY => Ok(Self::Retry),
            kind => Err(format!("unsupported semantic game command {kind}")),
        }
    }
}

pub(crate) fn decode_movement(envelope: &GameplayCommandEnvelope) -> Result<(usize, bool), String> {
    if envelope.target().is_some() {
        return Err("semantic game commands cannot have a target".into());
    }
    let payload = envelope.payload();
    let (Some(GameplayCommandValue::I64(direction)), Some(GameplayCommandValue::Bool(pressed))) =
        (payload.get("direction"), payload.get("pressed"))
    else {
        return Err("movement requires an integer direction and boolean pressed".into());
    };
    if payload.len() != 2 {
        return Err("movement contains unrepresented payload fields".into());
    }
    let direction = usize::try_from(*direction)
        .ok()
        .filter(|direction| *direction < 4)
        .ok_or_else(|| "movement direction must be in 0..4".to_owned())?;
    Ok((direction, *pressed))
}
