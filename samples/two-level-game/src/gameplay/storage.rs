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
    io::{SaveIoError, SaveSlot},
};

pub(super) const OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("two-level-game.checkpoint");

#[derive(Resource, Clone)]
pub(super) struct Storage(Arc<Mutex<Option<SaveSlot>>>);
impl Storage {
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
    pub fn load(&self) -> Result<Checkpoint, String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
            .ok_or("no application save path was supplied")?
            .load()
            .map(|decoded| decoded.checkpoint)
            .map_err(|error| error.to_string())
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
        if slot.as_mut().is_none_or(SaveSlot::retry_cleanup) {
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
