use super::*;

use crate::project_host::authoring::{AdmittedAuthoringDocument, AuthoringDocumentKind};

use super::persistence::RetainedHost;
use super::source_admission::SourceAdmissionError;
use nara_fs::FileCapability;
use nara_tooling::{EditorDocumentDigest, EditorDocumentSourceBinding};
use nara_tooling::{
    EditorDocumentOpenId, EditorDocumentOpenPhase, EditorDocumentOpenResult, EditorDocumentOpenView,
};

mod inspection;
pub(super) mod preparation;

pub use inspection::EditorInspectedAuthoringSource;
use preparation::{DocumentOpenInput, DocumentOpenPreparation, PreparedDocumentRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorDocumentOpenRequestResult {
    Preparing {
        request: EditorDocumentOpenId,
    },
    Activated {
        document: EditorDocumentId,
    },
    #[cfg(test)]
    Opened {
        document: EditorDocumentId,
    },
    Rejected {
        code: &'static str,
    },
}

enum PreparedDocument {
    Scene {
        source: crate::project_host::authoring::document::AuthoringDocumentSourceFact,
        relative_path: Arc<str>,
        digest: EditorDocumentDigest,
        host: RetainedHost<ScenePersistenceHost>,
        document: nara_scene::PublishedAuthoringSceneDocument,
    },
    Prefab {
        source: crate::project_host::authoring::document::AuthoringDocumentSourceFact,
        relative_path: Arc<str>,
        digest: EditorDocumentDigest,
        host: RetainedHost<PrefabPersistenceHost>,
        document: nara_scene::PublishedAuthoringPrefabDocument,
    },
}

enum PreparedHost {
    Scene(RetainedHost<ScenePersistenceHost>),
    Prefab(RetainedHost<PrefabPersistenceHost>),
}

/// Retires an unpublished Host on every refusal, cancellation and disposable-result path.
pub(super) struct PreparedAuthoringDocumentOpen {
    document: Option<PreparedDocument>,
    host: Option<PreparedHost>,
}

impl PreparedAuthoringDocumentOpen {
    fn source(&self) -> &crate::project_host::authoring::document::AuthoringDocumentSourceFact {
        match self
            .document
            .as_ref()
            .expect("the prepared document has not published")
        {
            PreparedDocument::Scene { source, .. } | PreparedDocument::Prefab { source, .. } => {
                source
            }
        }
    }

    fn current_source(&self) -> Result<FileCapability, &'static str> {
        let source = match self
            .document
            .as_ref()
            .expect("the prepared document has not published")
        {
            PreparedDocument::Scene { host, .. } => host.try_with(|host| host.read_source()),
            PreparedDocument::Prefab { host, .. } => host.try_with(|host| host.read_source()),
        };
        source
            .ok_or(SourceAdmissionError::HostUnavailable.code())?
            .open()
            .map_err(|_| SourceAdmissionError::SourceRead.code())
    }

    fn validate_source_identity(&self, file: &FileCapability) -> Result<(), &'static str> {
        let admitted = self.source();
        if file.identity() != admitted.identity()
            || file.parent_identity() != admitted.parent_identity()
        {
            return Err("project.editor.document-source-identity-changed");
        }
        Ok(())
    }
}

impl Drop for PreparedAuthoringDocumentOpen {
    fn drop(&mut self) {
        if let Some(host) = self.host.take() {
            match host {
                PreparedHost::Scene(host) => {
                    host.retire();
                }
                PreparedHost::Prefab(host) => {
                    host.retire();
                }
            }
        }
    }
}

pub(super) struct OpenAdmission {
    request: EditorDocumentOpenId,
    path: Arc<str>,
    index: Arc<ProjectSourceIndexSnapshot>,
}

pub(super) struct PendingDocumentOpen {
    admission: OpenAdmission,
    worker: DocumentOpenPreparation,
    inspecting: bool,
    cancelled: bool,
}

pub(super) enum CompletedDocumentOpen {
    Outcome(EditorDocumentOpenResult),
    Inspected {
        admission: OpenAdmission,
        document: AdmittedAuthoringDocument,
    },
}

impl CompletedDocumentOpen {
    fn view(&self) -> EditorDocumentOpenResult {
        match self {
            Self::Outcome(result) => *result,
            Self::Inspected { admission, .. } => EditorDocumentOpenResult::Inspected {
                request: admission.request,
            },
        }
    }
}

impl EditorProjectSession {
    #[cfg(test)]
    pub(crate) fn document_open_return_observer(&self) -> Option<nara_tasks::TaskReturnObserver> {
        self.pending_document_open
            .as_ref()?
            .worker
            .return_observer()
    }

    #[cfg(test)]
    pub(crate) fn hold_next_document_open(
        &mut self,
        phase: preparation::OpenTestPhase,
    ) -> preparation::OpenTestControl {
        let (gate, control) = preparation::OpenTestGate::new(phase);
        self.next_document_open_gate = Some(gate);
        control
    }

    /// Owner-conformance tests retain their synchronous observation helper. Product consumers
    /// must use dispatch and the explicit pending/result projection.
    #[cfg(test)]
    pub(crate) fn open_indexed_authoring_document(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        path: &str,
        kind: AuthoringDocumentKind,
    ) -> EditorDocumentOpenRequestResult {
        self.document_open_result = None;
        let result = self.request_indexed_authoring_document(index, path, kind);
        let EditorDocumentOpenRequestResult::Preparing { request } = result else {
            return result;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            self.drive_editor_frame(Duration::ZERO);
            if let Some(result) = self.document_open_result.take() {
                return match result.view() {
                    EditorDocumentOpenResult::Opened { document, .. } => {
                        EditorDocumentOpenRequestResult::Opened { document }
                    }
                    EditorDocumentOpenResult::Inspected { .. } => unreachable!(),
                    EditorDocumentOpenResult::Rejected { code, .. } => rejected(code),
                    EditorDocumentOpenResult::Cancelled { .. } => {
                        rejected("project.editor.document-open-cancelled")
                    }
                };
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Open {request:?} did not settle"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(crate) fn request_indexed_authoring_document(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
        kind: AuthoringDocumentKind,
    ) -> EditorDocumentOpenRequestResult {
        self.request_indexed_authoring_source(index, relative_path, kind, false)
    }

    pub(super) fn inspect_indexed_authoring_source(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
        kind: AuthoringDocumentKind,
    ) -> EditorDocumentOpenRequestResult {
        self.request_indexed_authoring_source(index, relative_path, kind, true)
    }

    fn request_indexed_authoring_source(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
        kind: AuthoringDocumentKind,
        inspect: bool,
    ) -> EditorDocumentOpenRequestResult {
        if self.source_tools.reclamation_fenced() {
            return EditorDocumentOpenRequestResult::Rejected {
                code: "project.editor.document-open-busy",
            };
        }
        if !self.accepts_new_work()
            || self.authoring_operation_fenced()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || !inspect && self.persistence.has_pending_work()
            || self.document_open_result.is_some()
        {
            return rejected("project.editor.document-open-busy");
        }
        let Some(project) = self.authoring_project() else {
            return rejected("project.editor.document-open-requires-authoring-project");
        };
        let Some(current_index) = project.source_index_snapshot() else {
            return rejected("project.editor.document-index-unavailable");
        };
        if !Arc::ptr_eq(&current_index, &index) {
            return rejected("project.editor.document-index-stale");
        }

        if let Some(document) = self.exact_source_owner(relative_path) {
            if inspect {
                return rejected(SourceAdmissionError::AlreadyOpen.code());
            }
            let expected_kind = match kind {
                AuthoringDocumentKind::Scene => EditorDocumentKind::Scene,
                AuthoringDocumentKind::Prefab => EditorDocumentKind::Prefab,
            };
            if self.workspace.document_kind(document) != Some(expected_kind) {
                return rejected("project.editor.document-source-already-open");
            }
            let report = self
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document });
            return if report.applied {
                EditorDocumentOpenRequestResult::Activated { document }
            } else {
                rejected("project.editor.document-activation-failed")
            };
        }
        let relative_path: Arc<str> = Arc::from(relative_path);
        let captured = match project.capture_indexed_document(
            Arc::clone(&index),
            Arc::clone(&relative_path),
            kind,
        ) {
            Ok(captured) => captured,
            Err(error) => return rejected(error.code()),
        };
        if !inspect && let Err(error) = self.exclude_project_source_aliases(&relative_path) {
            return rejected(error.code());
        }
        let input = if inspect {
            DocumentOpenInput::Inspect(captured)
        } else {
            DocumentOpenInput::Open(captured)
        };
        self.start_document_open(index, relative_path, input)
    }

    pub(super) fn request_open_inspected_authoring_source(
        &mut self,
        request: EditorDocumentOpenId,
    ) -> EditorDocumentOpenRequestResult {
        let Some(CompletedDocumentOpen::Inspected {
            admission,
            document,
        }) = &self.document_open_result
        else {
            return rejected("project.editor.document-inspection-not-current");
        };
        if admission.request != request {
            return rejected("project.editor.document-inspection-not-current");
        }
        if self.source_tools.reclamation_fenced()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || self.persistence.has_pending_work()
        {
            return rejected("project.editor.document-open-busy");
        }
        if let Err(code) = self.validate_document_open_admission(admission) {
            return rejected(code);
        }
        if self
            .exact_source_owner(document.source().relative_path())
            .is_some()
        {
            return rejected(SourceAdmissionError::AlreadyOpen.code());
        }
        if self.next_document_open_request == u64::MAX {
            return rejected("project.editor.document-open-id-exhausted");
        }
        let Some(CompletedDocumentOpen::Inspected {
            admission,
            document,
        }) = self.document_open_result.take()
        else {
            unreachable!("inspection was checked before consumption");
        };
        self.start_document_open(
            admission.index,
            admission.path,
            DocumentOpenInput::OpenInspected(self.source_project_root(), document),
        )
    }

    fn start_document_open(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        path: Arc<str>,
        input: DocumentOpenInput,
    ) -> EditorDocumentOpenRequestResult {
        let Some(next) = self.next_document_open_request.checked_add(1) else {
            return rejected("project.editor.document-open-id-exhausted");
        };
        self.next_document_open_request = next;
        let request = EditorDocumentOpenId::new(self.current_frame().revision(), next);
        let admission = OpenAdmission {
            request,
            path,
            index,
        };
        let inspecting = matches!(&input, DocumentOpenInput::Inspect(_));
        let worker = DocumentOpenPreparation::start(
            input,
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            #[cfg(test)]
            self.next_document_open_gate.take(),
        );
        self.pending_document_open = Some(PendingDocumentOpen {
            admission,
            worker,
            inspecting,
            cancelled: false,
        });
        EditorDocumentOpenRequestResult::Preparing { request }
    }

    fn validate_document_open_admission(
        &self,
        admission: &OpenAdmission,
    ) -> Result<(), &'static str> {
        if !self.accepts_new_work() || self.authoring_operation_fenced() {
            return Err("project.editor.document-open-busy");
        }
        let current = self
            .project_source_index_snapshot()
            .ok_or("project.editor.document-index-unavailable")?;
        if !Arc::ptr_eq(&current, &admission.index) {
            return Err("project.editor.document-index-stale");
        }
        Ok(())
    }

    fn validate_document_open(
        &self,
        admission: &OpenAdmission,
        prepared: &PreparedAuthoringDocumentOpen,
    ) -> Result<(), &'static str> {
        self.validate_document_open_admission(admission)?;
        let source = prepared.source();
        source
            .revalidate_native_source(&self.source_project_root())
            .map_err(|error| error.code())?;
        let current = prepared.current_source()?;
        self.exclude_source_aliases(source.relative_path(), &current)
            .map_err(SourceAdmissionError::code)?;
        prepared.validate_source_identity(&current)?;
        self.exclude_native_source_key(source.relative_path(), source.source_key())
    }

    pub(super) fn document_open_view(&self) -> EditorDocumentOpenView {
        let pending = self.pending_document_open.as_ref().map(|pending| {
            (
                pending.admission.request,
                Arc::clone(&pending.admission.path),
                if pending.cancelled {
                    EditorDocumentOpenPhase::Cancelling
                } else if pending.inspecting {
                    EditorDocumentOpenPhase::Inspecting
                } else {
                    EditorDocumentOpenPhase::Preparing
                },
            )
        });
        EditorDocumentOpenView::new(
            pending,
            self.document_open_result
                .as_ref()
                .map(CompletedDocumentOpen::view),
        )
    }

    /// Borrows the exact retained read-only result of an unopened-source inspection.
    #[must_use]
    pub fn inspected_authoring_source(
        &self,
        request: EditorDocumentOpenId,
    ) -> Option<EditorInspectedAuthoringSource<'_>> {
        match self.document_open_result.as_ref()? {
            CompletedDocumentOpen::Inspected {
                admission,
                document,
            } if admission.request == request => Some(EditorInspectedAuthoringSource { document }),
            _ => None,
        }
    }

    pub(super) fn document_open_may_publish(&self) -> bool {
        self.pending_document_open
            .as_ref()
            .is_some_and(|pending| !pending.inspecting)
    }

    pub(super) fn acknowledge_document_open(&mut self, request: EditorDocumentOpenId) -> bool {
        if self
            .document_open_result
            .as_ref()
            .is_some_and(|result| result.view().request() == request)
        {
            self.document_open_result = None;
            true
        } else {
            false
        }
    }

    pub(super) fn cancel_document_open_request(&mut self, request: EditorDocumentOpenId) -> bool {
        match self.pending_document_open.as_mut() {
            Some(PendingDocumentOpen {
                admission,
                worker,
                cancelled,
                ..
            }) if admission.request == request => {
                *cancelled = true;
                worker.cancel();
                true
            }
            _ => false,
        }
    }

    pub(super) fn advance_document_open(&mut self) -> bool {
        let Some(PendingDocumentOpen {
            admission,
            mut worker,
            inspecting,
            cancelled,
        }) = self.pending_document_open.take()
        else {
            return false;
        };
        let Some(completion) = worker.poll() else {
            self.pending_document_open = Some(PendingDocumentOpen {
                admission,
                worker,
                inspecting,
                cancelled,
            });
            return false;
        };
        let request = admission.request;
        if cancelled || !self.accepts_new_work() {
            self.document_open_result = Some(CompletedDocumentOpen::Outcome(
                EditorDocumentOpenResult::Cancelled { request },
            ));
            return true;
        }
        self.document_open_result = Some(
            match completion.and_then(|prepared| {
                self.finish_document_open(admission, prepared)
                    .map_err(preparation::DocumentOpenFailure::from_code)
            }) {
                Ok(result) => result,
                Err(failure) => {
                    let _ = self.diagnostics.push(failure.diagnostic);
                    CompletedDocumentOpen::Outcome(EditorDocumentOpenResult::Rejected {
                        request,
                        code: failure.code,
                    })
                }
            },
        );
        true
    }

    pub(super) fn cancel_pending_document_open(&mut self) {
        if let Some(PendingDocumentOpen {
            worker, cancelled, ..
        }) = self.pending_document_open.as_mut()
        {
            *cancelled = true;
            worker.cancel();
        }
    }

    pub(super) fn release_completed_source_inspection(&mut self) {
        if matches!(
            self.document_open_result,
            Some(CompletedDocumentOpen::Inspected { .. })
        ) {
            self.document_open_result = None;
        }
    }

    fn finish_document_open(
        &mut self,
        admission: OpenAdmission,
        prepared: PreparedDocumentRequest,
    ) -> Result<CompletedDocumentOpen, &'static str> {
        match prepared {
            PreparedDocumentRequest::Open(prepared) => {
                let request = admission.request;
                let document = self.publish_prepared_document_open(&admission, prepared)?;
                Ok(CompletedDocumentOpen::Outcome(
                    EditorDocumentOpenResult::Opened { request, document },
                ))
            }
            PreparedDocumentRequest::Inspected(document) => {
                self.validate_document_open_admission(&admission)?;
                let path = document.source().relative_path();
                self.exclude_native_source_key(path, document.source().source_key())?;
                self.exclude_source_aliases(path, document.file())
                    .map_err(SourceAdmissionError::code)?;
                Ok(CompletedDocumentOpen::Inspected {
                    admission,
                    document,
                })
            }
        }
    }

    fn publish_prepared_document_open(
        &mut self,
        admission: &OpenAdmission,
        mut prepared: PreparedAuthoringDocumentOpen,
    ) -> Result<EditorDocumentId, &'static str> {
        self.validate_document_open(admission, &prepared)?;
        let previous_active = self.workspace.active_document();
        let result = match prepared
            .document
            .take()
            .expect("a prepared document publishes once")
        {
            PreparedDocument::Scene {
                source,
                relative_path,
                digest,
                host,
                document,
            } => {
                let diagnostics = document.diagnostics().clone();
                let report = self
                    .workspace
                    .open_authoring_scene_document(
                        relative_path.as_ref(),
                        EditorDocumentSourceBinding::new(Arc::clone(&relative_path)),
                        document,
                    )
                    .map_err(|_| "project.editor.scene-publication-failed")?;
                let document = report
                    .opened_document
                    .ok_or("project.editor.scene-publication-failed")?;
                let binding = self.persistence_authority.bind_opened_source_digest(
                    &mut self.workspace,
                    document,
                    digest,
                );
                if !binding.applied {
                    self.discard_unpublished_document(document, previous_active);
                    return Err("project.editor.scene-persistence-binding-failed");
                }
                let _ = self.diagnostics.extend(diagnostics);
                self.scene_lanes.insert(
                    document,
                    EditorScenePersistenceLane {
                        project_path: relative_path,
                        source_key: source.source_key().clone(),
                        authoring_source: Some(source),
                        document,
                        host,
                    },
                );
                self.scene_preview_document = Some(document);
                Ok(document)
            }
            PreparedDocument::Prefab {
                source,
                relative_path,
                digest,
                host,
                document,
            } => {
                let diagnostics = document.diagnostics().clone();
                let report = self
                    .workspace
                    .open_authoring_prefab_document(
                        relative_path.as_ref(),
                        EditorDocumentSourceBinding::new(Arc::clone(&relative_path)),
                        document,
                    )
                    .map_err(|_| "project.editor.prefab-publication-failed")?;
                let document = report
                    .opened_document
                    .ok_or("project.editor.prefab-publication-failed")?;
                let binding = self.persistence_authority.bind_opened_source_digest(
                    &mut self.workspace,
                    document,
                    digest,
                );
                if !binding.applied {
                    self.discard_unpublished_document(document, previous_active);
                    return Err("project.editor.prefab-persistence-binding-failed");
                }
                let _ = self.diagnostics.extend(diagnostics);
                self.prefab_lanes.insert(
                    document,
                    EditorPrefabPersistenceLane {
                        project_path: relative_path,
                        source_key: source.source_key().clone(),
                        authoring_source: Some(source),
                        document,
                        host,
                    },
                );
                Ok(document)
            }
        };
        if result.is_ok() {
            prepared.host = None;
        }
        result
    }
}

fn document_open_failure_code(stage: EditorPersistenceFailureStage) -> &'static str {
    match stage {
        EditorPersistenceFailureStage::OpenTarget => "project.editor.document-source-open-failed",
        EditorPersistenceFailureStage::ReadTarget => "project.editor.document-source-read-failed",
        EditorPersistenceFailureStage::Decode | EditorPersistenceFailureStage::Validate => {
            "project.editor.document-source-changed"
        }
        EditorPersistenceFailureStage::Encode
        | EditorPersistenceFailureStage::CreateTemporary
        | EditorPersistenceFailureStage::WriteTemporary
        | EditorPersistenceFailureStage::SyncTemporary
        | EditorPersistenceFailureStage::ReplaceTarget => {
            "project.editor.document-persistence-binding-failed"
        }
    }
}

fn editor_document_digest(digest: ContentDigest) -> EditorDocumentDigest {
    EditorDocumentDigest::new(digest.length(), *digest.as_bytes())
}

const fn rejected(code: &'static str) -> EditorDocumentOpenRequestResult {
    EditorDocumentOpenRequestResult::Rejected { code }
}
