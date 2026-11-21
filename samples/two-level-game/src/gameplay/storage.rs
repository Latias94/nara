use std::{
    path::Path,
    sync::{Arc, Mutex, TryLockError},
};

use nara::{
    app::{
        PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    prelude::{App, PluginError, Resource},
};

use super::PLUGIN_ID;
use crate::save::{
    Checkpoint,
    io::{CheckpointBinding, SaveIoError, SaveSlot},
};

pub(super) const OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("two-level-game.checkpoint");

#[derive(Resource, Clone)]
pub(super) struct Storage(Arc<Mutex<Option<SaveSlot>>>);
impl Storage {
    pub(super) fn is_unconfigured(&self) -> bool {
        self.0.try_lock().is_ok_and(|slot| slot.is_none())
    }

    pub(super) fn open_slot(&self, path: Option<&Path>, served: bool) -> Result<(), String> {
        let mut slot = self
            .0
            .try_lock()
            .map_err(|_| "the application save slot is unavailable")?;
        if slot.is_some() {
            return Err("the application save slot is already configured".into());
        }
        let opened = path
            .map(if served {
                SaveSlot::open_server
            } else {
                SaveSlot::open
            })
            .transpose()
            .map_err(|error| error.to_string())?;
        *slot = opened;
        Ok(())
    }

    pub(super) fn handoff_stamp(&self) -> Result<Option<[u8; 32]>, SaveIoError> {
        self.0
            .try_lock()
            .map_err(|_| SaveIoError::HandoffUnavailable)?
            .as_ref()
            .map(SaveSlot::handoff_stamp)
            .transpose()
    }

    pub(super) fn stage_server_slot(&self, candidate: SaveSlot) -> Result<(), SaveIoError> {
        let mut slot = self
            .0
            .try_lock()
            .map_err(|_| SaveIoError::HandoffUnavailable)?;
        if slot.is_some() {
            return Err(SaveIoError::HandoffUnavailable);
        }
        *slot = Some(candidate);
        Ok(())
    }

    pub(super) fn claim_server_slot(&self) -> Result<(), SaveIoError> {
        self.0
            .try_lock()
            .map_err(|_| SaveIoError::HandoffUnavailable)?
            .as_mut()
            .ok_or(SaveIoError::HandoffUnavailable)?
            .claim_server_handoff()
    }

    pub fn save(&self, checkpoint: &Checkpoint) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
            .ok_or("no application save path was supplied")?
            .save(checkpoint)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    pub(super) fn prepare_load(&self) -> Result<(Checkpoint, CheckpointBinding), String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
            .ok_or("no application save path was supplied")?
            .prepare_load()
            .map(|(decoded, binding)| (decoded.checkpoint, binding))
            .map_err(|error| error.to_string())
    }

    pub(super) fn accept_load(&self, binding: CheckpointBinding) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
            .expect("a prepared checkpoint read has the original application save slot")
            .accept_load(binding);
    }
}

pub(super) fn install(app: &mut App, path: Option<&Path>) -> Result<(), PluginError> {
    let slot = path
        .map(SaveSlot::open)
        .transpose()
        .map_err(|error: SaveIoError| PluginError::SetupFailed {
            plugin: PLUGIN_ID,
            message: error.to_string(),
        })?;
    let storage = Storage(Arc::new(Mutex::new(slot)));
    app.register_plugin_runtime_close_participant(
        OBLIGATION,
        RuntimeCloseParticipantId::new("two-level-game.checkpoint"),
        Close(storage.clone()),
    )?;
    app.insert_resource(storage)?;
    Ok(())
}

struct Close(Storage);
impl Close {
    fn retry(&self) -> RuntimeCloseProgress {
        let mut slot = match self.0.0.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return RuntimeCloseProgress::Pending,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        if slot.as_mut().is_none_or(SaveSlot::retry_close) {
            *slot = None;
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }
}
impl RuntimeCloseParticipant for Close {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.retry())
    }
    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.retry())
    }
}
