//! Owned immutable reads and Host binding; no Workspace or predecessor-close authority.

use std::sync::Arc;

use super::*;
use crate::project_host::authoring::{
    AuthoringDocumentAdmissionError, OwnedAuthoringDocumentRequest,
};
use crate::project_host::runtime::{diagnostic_code, safe_summary};
use nara_diagnostic::Diagnostic;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
    TaskTerminal,
};

pub(in crate::project_host::runtime::editor) struct DocumentOpenPreparation {
    task: Option<TaskHandle<Result<PreparedAuthoringDocumentOpen, DocumentOpenFailure>>>,
}

pub(super) struct DocumentOpenFailure {
    pub(super) code: &'static str,
    pub(super) diagnostic: Diagnostic,
}

impl DocumentOpenFailure {
    pub(super) fn from_code(code: &'static str) -> Self {
        Self {
            code,
            diagnostic: Diagnostic::error(
                diagnostic_code(code),
                safe_summary("Editor document could not be opened"),
            ),
        }
    }

    pub(super) fn from_admission(error: AuthoringDocumentAdmissionError) -> Self {
        Self {
            code: error.code(),
            diagnostic: error.diagnostic(),
        }
    }
}

impl DocumentOpenPreparation {
    pub(super) fn start(
        request: OwnedAuthoringDocumentRequest,
        custody: persistence::HostCustody,
        pools: &TaskPools,
        #[cfg(test)] mut gate: Option<OpenTestGate>,
    ) -> Self {
        let input = PreparationInput {
            request: Some(request),
            custody,
        };
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x6272_6f77_7365_726f)),
                move |cancellation| {
                    let mut input = input;
                    #[cfg(test)]
                    wait_at(&mut gate, OpenTestPhase::BeforeRead);
                    if cancellation.is_cancelled() {
                        return Err(DocumentOpenFailure::from_code(
                            "project.editor.document-open-cancelled",
                        ));
                    }
                    let request = input.request.take().expect("Open prepares once");
                    let root = request.project_root();
                    let admitted = request
                        .prepare()
                        .map_err(DocumentOpenFailure::from_admission)?;
                    #[cfg(test)]
                    wait_at(&mut gate, OpenTestPhase::BeforeBinding);
                    if cancellation.is_cancelled() {
                        return Err(DocumentOpenFailure::from_code(
                            "project.editor.document-open-cancelled",
                        ));
                    }
                    let prepared = bind_document_open(&root, &input.custody, admitted)
                        .map_err(DocumentOpenFailure::from_code)?;
                    #[cfg(test)]
                    wait_at(&mut gate, OpenTestPhase::AfterPreparation);
                    if cancellation.is_cancelled() {
                        return Err(DocumentOpenFailure::from_code(
                            "project.editor.document-open-cancelled",
                        ));
                    }
                    Ok(prepared)
                },
            )
            .into_handle()
            .ok();
        Self { task }
    }

    pub(super) fn poll(
        &mut self,
    ) -> Option<Result<PreparedAuthoringDocumentOpen, DocumentOpenFailure>> {
        // Rejected spawn disposes its captures before returning, even without a task handle.
        if let Some(task) = &self.task {
            match task.return_observer().state() {
                TaskReturnState::Pending => return None,
                TaskReturnState::CleanupFailed => {
                    return Some(Err(DocumentOpenFailure::from_code(
                        "project.editor.document-open-preparation-unavailable",
                    )));
                }
                TaskReturnState::Returned => {}
            }
        }
        Some(match self.task.as_mut() {
            None => Err(DocumentOpenFailure::from_code(
                "project.editor.document-open-preparation-unavailable",
            )),
            Some(task) => match task.try_take()? {
                TaskTerminal::Completed(result) => result,
                TaskTerminal::Cancelled(_) | TaskTerminal::Failed(_) => {
                    Err(DocumentOpenFailure::from_code(
                        "project.editor.document-open-preparation-unavailable",
                    ))
                }
            },
        })
    }

    pub(super) fn cancel(&self) {
        if let Some(task) = &self.task {
            task.cancel();
        }
    }
}

impl Drop for DocumentOpenPreparation {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct PreparationInput {
    request: Option<OwnedAuthoringDocumentRequest>,
    custody: persistence::HostCustody,
}

fn bind_document_open(
    root: &DirectoryCapability,
    custody: &persistence::HostCustody,
    admitted: AdmittedAuthoringDocument,
) -> Result<PreparedAuthoringDocumentOpen, &'static str> {
    let path: Arc<str> = Arc::from(admitted.source().relative_path());
    let digest = editor_document_digest(admitted.source().digest());
    let (document, host) = match admitted {
        AdmittedAuthoringDocument::Scene { source, document } => {
            let (host, observed) = ScenePersistenceHost::bind_authoring_source(
                root,
                source.relative_path(),
                source.identity(),
                source.digest(),
            )
            .map_err(document_open_failure_code)?;
            if observed != digest {
                return Err("project.editor.document-persistence-digest-mismatch");
            }
            let host = custody
                .retain_scene(host)
                .ok_or("project.editor.document-open-busy")?;
            (
                PreparedDocument::Scene {
                    relative_path: path,
                    digest,
                    host: host.clone(),
                    document,
                    source,
                },
                PreparedHost::Scene(host),
            )
        }
        AdmittedAuthoringDocument::Prefab { source, document } => {
            let (host, observed) = PrefabPersistenceHost::bind_authoring_source(
                root,
                EditorDocumentSourceBinding::new(source.relative_path()),
                source.identity(),
                source.digest(),
            )
            .map_err(document_open_failure_code)?;
            if observed != digest {
                return Err("project.editor.document-persistence-digest-mismatch");
            }
            let host = custody
                .retain_prefab(host)
                .ok_or("project.editor.document-open-busy")?;
            (
                PreparedDocument::Prefab {
                    relative_path: path,
                    digest,
                    host: host.clone(),
                    document,
                    source,
                },
                PreparedHost::Prefab(host),
            )
        }
    };
    Ok(PreparedAuthoringDocumentOpen {
        document: Some(document),
        host: Some(host),
    })
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenTestPhase {
    BeforeRead,
    BeforeBinding,
    AfterPreparation,
}

#[cfg(test)]
pub(in crate::project_host::runtime::editor) struct OpenTestGate {
    phase: OpenTestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
pub(crate) struct OpenTestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl OpenTestControl {
    pub(crate) fn wait_until_started(&self) -> bool {
        self.started.recv_timeout(Duration::from_secs(5)).is_ok()
    }
    pub(crate) fn release(mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
#[cfg(test)]
impl Drop for OpenTestControl {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
#[cfg(test)]
impl OpenTestGate {
    pub(super) fn new(phase: OpenTestPhase) -> (Self, OpenTestControl) {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        (
            Self {
                phase,
                started: started_tx,
                release: release_rx,
            },
            OpenTestControl {
                started,
                release: Some(release),
            },
        )
    }
}
#[cfg(test)]
fn wait_at(gate: &mut Option<OpenTestGate>, phase: OpenTestPhase) {
    if !gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        return;
    }
    let gate = gate.take().expect("matching Open gate exists");
    let _ = gate.started.send(());
    let _ = gate.release.recv();
}
