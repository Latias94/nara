//! Read-only preparation owns no publication Host or Workspace authority.

use std::sync::{Arc, Mutex, TryLockError};

use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest, TaskTerminal,
};

use crate::project_host::persistence::{
    PrefabReopenRequest, PreparedPrefabReopen, PreparedSceneReopen, SceneReopenRequest,
};

use super::*;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

pub(super) struct ReopenAdmission {
    document: EditorDocumentId,
    kind: EditorDocumentKind,
    revision: SceneAuthoringRevision,
    binding: Option<EditorDocumentSourceBinding>,
    project_path: Arc<str>,
    schema: Option<AuthoringSchemaBasis>,
}

enum ReopenRequest {
    Scene(SceneReopenRequest),
    Prefab(PrefabReopenRequest),
    Extension(super::super::extension_documents::ExtensionReopenRequest),
}

pub(super) enum ReopenCompletion {
    Scene(PreparedSceneReopen),
    Prefab(PreparedPrefabReopen),
    Extension(
        Result<
            super::super::extension_documents::PreparedExtensionReopen,
            super::super::extension_documents::ExtensionReopenFailure,
        >,
    ),
    Unavailable,
}

pub(in crate::project_host::runtime::editor) struct PendingReopen {
    admission: ReopenAdmission,
    request: ReopenRequest,
}

struct ReopenWork {
    request: Option<ReopenRequest>,
    prepared: Option<ReopenCompletion>,
    #[cfg(test)]
    gate: Option<ReopenTestGate>,
}

pub(in crate::project_host::runtime::editor) struct BackgroundReopen {
    admission: ReopenAdmission,
    work: Arc<Mutex<Option<ReopenWork>>>,
    task: Option<TaskHandle<()>>,
}

impl BackgroundReopen {
    pub(super) fn start(
        pending: PendingReopen,
        pools: &TaskPools,
        #[cfg(test)] gate: Option<ReopenTestGate>,
    ) -> Self {
        let work = Arc::new(Mutex::new(Some(ReopenWork {
            request: Some(pending.request),
            prepared: None,
            #[cfg(test)]
            gate,
        })));
        let retained = Arc::clone(&work);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x7265_6f70_656e)),
                move |_| {
                    let mut slot = retained.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(work) = slot.as_mut() else { return };
                    #[cfg(test)]
                    work.wait_at(ReopenTestPhase::BeforeRead);
                    let request = work.request.take().expect("a Reopen request runs once");
                    work.prepared = Some(match request {
                        ReopenRequest::Scene(request) => ReopenCompletion::Scene(request.prepare()),
                        ReopenRequest::Prefab(request) => {
                            ReopenCompletion::Prefab(request.prepare())
                        }
                        ReopenRequest::Extension(request) => {
                            ReopenCompletion::Extension(request.prepare())
                        }
                    });
                    #[cfg(test)]
                    work.wait_at(ReopenTestPhase::AfterPreparation);
                },
            )
            .into_handle()
            .ok();
        Self {
            admission: pending.admission,
            work,
            task,
        }
    }

    pub(super) fn admission(&self) -> &ReopenAdmission {
        &self.admission
    }

    pub(super) fn poll(&mut self) -> Option<ReopenCompletion> {
        if self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return None;
        }
        // Cancellation can publish a terminal before the read returns. This mutex, held
        // through preparation, prevents a logical terminal from certifying physical return.
        let mut slot = match self.work.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let work = slot.take()?;
        // Unlike Save, a completed read has no irreversible publication to preserve.
        // A cancelled or failed attempt cannot gain publication authority from leftover data.
        let completed = matches!(
            self.task.as_mut().and_then(|task| task.try_take()),
            Some(TaskTerminal::Completed(()))
        );
        Some(if completed {
            work.prepared.unwrap_or(ReopenCompletion::Unavailable)
        } else {
            ReopenCompletion::Unavailable
        })
    }
}

impl EditorProjectSession {
    pub(super) fn capture_reopen(
        &self,
        document: EditorDocumentId,
    ) -> Result<PendingReopen, EditorPersistenceRejection> {
        let kind = self
            .workspace
            .document_kind(document)
            .ok_or(EditorPersistenceRejection::MissingDocument)?;
        let (admission, request) = match kind {
            EditorDocumentKind::Scene => {
                let slot = self
                    .workspace
                    .scene(document)
                    .ok_or(EditorPersistenceRejection::MissingDocument)?;
                let lane = self
                    .scene_lane
                    .as_ref()
                    .filter(|lane| lane.document == document)
                    .ok_or(EditorPersistenceRejection::SourceBindingMismatch)?;
                (
                    ReopenAdmission {
                        document,
                        kind,
                        revision: slot.revision(),
                        binding: slot.source_binding().cloned(),
                        project_path: Arc::clone(&lane.project_path),
                        schema: slot.session().authoring_schema_basis(),
                    },
                    ReopenRequest::Scene(
                        lane.host
                            .try_with(|host| {
                                host.prepare_reopen(slot.session().authoring_readmission())
                            })
                            .ok_or(EditorPersistenceRejection::Busy)??,
                    ),
                )
            }
            EditorDocumentKind::Prefab => {
                let slot = self
                    .workspace
                    .prefab(document)
                    .ok_or(EditorPersistenceRejection::MissingDocument)?;
                let lane = self
                    .prefab_lane
                    .as_ref()
                    .filter(|lane| lane.document == document)
                    .ok_or(EditorPersistenceRejection::SourceBindingMismatch)?;
                (
                    ReopenAdmission {
                        document,
                        kind,
                        revision: slot.revision(),
                        binding: Some(slot.source_binding().clone()),
                        project_path: Arc::clone(&lane.project_path),
                        schema: slot.session().authoring_schema_basis(),
                    },
                    ReopenRequest::Prefab(
                        lane.host
                            .try_with(|host| {
                                host.prepare_reopen(slot.session().authoring_readmission())
                            })
                            .ok_or(EditorPersistenceRejection::Busy)??,
                    ),
                )
            }
            EditorDocumentKind::Extension(_) => {
                let lane = self
                    .extension_documents
                    .lane(document)
                    .ok_or(EditorPersistenceRejection::SourceBindingMismatch)?;
                (
                    ReopenAdmission {
                        document,
                        kind,
                        revision: self
                            .workspace
                            .document_revision(document)
                            .ok_or(EditorPersistenceRejection::MissingDocument)?,
                        binding: Some(lane.source.clone()),
                        project_path: Arc::from(lane.source.locator()),
                        schema: None,
                    },
                    ReopenRequest::Extension(lane.capture_reopen()?),
                )
            }
        };
        Ok(PendingReopen { admission, request })
    }

    fn validate_reopen_admission(
        &self,
        admission: &ReopenAdmission,
    ) -> Result<(), EditorPersistenceRejection> {
        let document = admission.document;
        if self.workspace.document_kind(document) != Some(admission.kind) {
            return Err(EditorPersistenceRejection::MissingDocument);
        }
        if self.workspace.document_revision(document) != Some(admission.revision) {
            return Err(EditorPersistenceRejection::StaleRevision);
        }
        let (binding, schema, lane_matches) = match admission.kind {
            EditorDocumentKind::Scene => {
                let slot = self
                    .workspace
                    .scene(document)
                    .ok_or(EditorPersistenceRejection::MissingDocument)?;
                (
                    slot.source_binding().cloned(),
                    slot.session().authoring_schema_basis(),
                    self.scene_lane.as_ref().is_some_and(|lane| {
                        lane.document == document && lane.project_path == admission.project_path
                    }),
                )
            }
            EditorDocumentKind::Prefab => {
                let slot = self
                    .workspace
                    .prefab(document)
                    .ok_or(EditorPersistenceRejection::MissingDocument)?;
                (
                    Some(slot.source_binding().clone()),
                    slot.session().authoring_schema_basis(),
                    self.prefab_lane.as_ref().is_some_and(|lane| {
                        lane.document == document && lane.project_path == admission.project_path
                    }),
                )
            }
            EditorDocumentKind::Extension(_) => (
                self.workspace.document_source_binding(document).cloned(),
                None,
                self.extension_documents
                    .lane(document)
                    .is_some_and(|lane| lane.source.locator() == admission.project_path.as_ref()),
            ),
        };
        if !lane_matches || binding != admission.binding || schema != admission.schema {
            return Err(EditorPersistenceRejection::SourceBindingMismatch);
        }
        if admission.kind == EditorDocumentKind::Scene && self.scene_runtime_blocks_reopen() {
            return Err(EditorPersistenceRejection::RuntimeActive);
        }
        Ok(())
    }

    pub(super) fn publish_reopen_completion(
        &mut self,
        admission: &ReopenAdmission,
        prepared: ReopenCompletion,
    ) -> EditorPersistenceResult {
        let rejected = |reason| EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(admission.document),
            reason,
        };
        if let Err(reason) = self.validate_reopen_admission(admission) {
            return rejected(reason);
        }
        // Recheck before invoking native code. The registry never crosses into the worker.
        let registry = self
            .trusted_compiled()
            .map(|lane| lane.plan.schema_validation().registry());
        match prepared {
            ReopenCompletion::Scene(prepared) => {
                let outcome = prepared.admit(registry);
                self.publish_scene_reopen(admission.document, admission.revision, outcome)
            }
            ReopenCompletion::Prefab(prepared) => {
                let outcome = prepared.admit(registry);
                let refresh = self.prefab_lane.as_ref().is_some_and(|lane| {
                    lane.host
                        .try_with(|host| host.publishes_runtime_content())
                        .unwrap_or(false)
                });
                self.publish_prefab_reopen(
                    admission.document,
                    admission.revision,
                    admission
                        .binding
                        .clone()
                        .expect("an admitted Prefab retains its source binding"),
                    refresh,
                    outcome,
                )
            }
            ReopenCompletion::Unavailable => rejected(EditorPersistenceRejection::Busy),
            ReopenCompletion::Extension(prepared) => {
                self.publish_extension_reopen(admission.document, admission.revision, prepared)
            }
        }
    }

    pub(in crate::project_host::runtime::editor) fn scene_runtime_blocks_reopen(&self) -> bool {
        self.trusted_compiled().is_some_and(|lane| {
            !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
                || lane.pending_control.is_some()
                || lane.runtime_host.has_cleanup_owner()
        })
    }

    pub(in crate::project_host::runtime::editor) fn reopen_pending(&self) -> bool {
        matches!(
            self.persistence_operation,
            EditorPersistenceOperation::Opening { .. }
        )
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReopenTestPhase {
    BeforeRead,
    AfterPreparation,
}

#[cfg(test)]
pub(in crate::project_host::runtime::editor) struct ReopenTestGate {
    phase: ReopenTestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
pub(crate) struct ReopenTestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl ReopenTestControl {
    pub(crate) fn wait_until_started(&self) -> bool {
        self.started.recv_timeout(Duration::from_secs(5)).is_ok()
    }
    pub(crate) fn release(mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
}

#[cfg(test)]
impl Drop for ReopenTestControl {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
impl ReopenWork {
    fn wait_at(&mut self, phase: ReopenTestPhase) {
        if self.gate.as_ref().is_some_and(|gate| gate.phase == phase) {
            let gate = self.gate.take().unwrap();
            let _ = gate.started.send(());
            gate.release
                .recv_timeout(Duration::from_secs(15))
                .expect("Reopen test release guard must retire");
        }
    }
}

#[cfg(test)]
impl EditorProjectSession {
    pub(crate) fn hold_next_reopen(&mut self, phase: ReopenTestPhase) -> ReopenTestControl {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        self.next_reopen_gate = Some(ReopenTestGate {
            phase,
            started: started_tx,
            release: release_rx,
        });
        ReopenTestControl {
            started,
            release: Some(release),
        }
    }
}
