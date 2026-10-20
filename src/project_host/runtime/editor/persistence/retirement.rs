//! Runtime-owned file cleanup custody survives abandonment of the Editor session.

use std::sync::{Arc, Mutex, TryLockError};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_ecs::Resource;

use super::background::SharedSaveWork;

/// Keeps the one admitted file Host reachable independently of its task and Editor caller.
#[derive(Clone, Default, Resource)]
pub(in crate::project_host::runtime::editor) struct SaveCustody {
    work: Arc<Mutex<Option<SharedSaveWork>>>,
}

impl SaveCustody {
    /// Retains a new operation only after the previous operation returned its Host.
    pub(super) fn retain(&self, work: SharedSaveWork) -> Result<(), SharedSaveWork> {
        let mut retained = match self.work.try_lock() {
            Ok(retained) => retained,
            Err(TryLockError::WouldBlock) => return Err(work),
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        if let Some(previous) = retained.as_ref() {
            let retired = match previous.try_lock() {
                Ok(previous) => previous.is_none(),
                Err(TryLockError::WouldBlock) => false,
                Err(TryLockError::Poisoned(error)) => error.into_inner().is_none(),
            };
            if !retired {
                return Err(work);
            }
        }
        *retained = Some(work);
        Ok(())
    }

    fn poll_retirement(&self) -> RuntimeCloseProgress {
        let mut retained = match self.work.try_lock() {
            Ok(retained) => retained,
            Err(TryLockError::WouldBlock) => return RuntimeCloseProgress::Pending,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let Some(work) = retained.as_ref() else {
            return RuntimeCloseProgress::Complete;
        };
        let mut work = match work.try_lock() {
            Ok(work) => work,
            Err(TryLockError::WouldBlock) => return RuntimeCloseProgress::Pending,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        // The worker holds this mutex throughout every file call. Acquiring it proves that
        // the file Host is available, including after an unwind. Taking an unstarted request
        // also prevents its queued closure from beginning publication: it observes None.
        // The separate TaskPlugin participant still proves closure/capture/worker retirement.
        if work.as_mut().is_some_and(|work| !work.retry_cleanup()) {
            return RuntimeCloseProgress::Pending;
        }
        *work = None;
        drop(work);
        *retained = None;
        RuntimeCloseProgress::Complete
    }
}

const SAVE_CUSTODY_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.editor.persistence.save-custody");
const SAVE_CUSTODY_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.editor.persistence.save-custody");
const SAVE_CUSTODY_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.editor.persistence.save-custody"),
    PluginCategory::Service,
)
.requires_plugins(&[nara_tasks::TASK_PLUGIN_ID])
.shutdown_obligations(&[SAVE_CUSTODY_OBLIGATION]);

pub(in crate::project_host::runtime::editor) struct SaveCustodyPlugin;

impl Plugin for SaveCustodyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SAVE_CUSTODY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let custody = SaveCustody::default();
        app.register_plugin_runtime_close_participant(
            SAVE_CUSTODY_OBLIGATION,
            SAVE_CUSTODY_PARTICIPANT,
            SaveCustodyCloseParticipant(custody.clone()),
        )?;
        app.insert_resource(custody)?;
        Ok(())
    }
}

struct SaveCustodyCloseParticipant(SaveCustody);

impl RuntimeCloseParticipant for SaveCustodyCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.0.poll_retirement())
    }
}
