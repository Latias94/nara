use super::*;

use crate::project_host::authoring::{AdmittedAuthoringDocument, AuthoringDocumentKind};

use super::persistence::RetainedHost;
use super::source_admission::SourceAdmissionError;
use nara_fs::FileCapability;
use nara_tooling::{EditorDocumentDigest, EditorDocumentSourceBinding};
use nara_tooling::{
    EditorDocumentOpenId, EditorDocumentOpenPhase, EditorDocumentOpenResult, EditorDocumentOpenView,
};

pub(super) mod preparation;
use preparation::DocumentOpenPreparation;

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
    fn current_source(&self) -> Result<(&str, FileCapability), SourceAdmissionError> {
        let (path, source) = match self
            .document
            .as_ref()
            .ok_or(SourceAdmissionError::HostUnavailable)?
        {
            PreparedDocument::Scene {
                relative_path,
                host,
                ..
            } => (
                relative_path.as_ref(),
                host.try_with(|host| host.read_source()),
            ),
            PreparedDocument::Prefab {
                relative_path,
                host,
                ..
            } => (
                relative_path.as_ref(),
                host.try_with(|host| host.read_source()),
            ),
        };
        let file = source
            .ok_or(SourceAdmissionError::HostUnavailable)?
            .open()
            .map_err(|_| SourceAdmissionError::SourceRead)?;
        Ok((path, file))
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

struct OpenAdmission {
    request: EditorDocumentOpenId,
    path: Arc<str>,
    index: Arc<ProjectSourceIndexSnapshot>,
}

pub(super) struct PendingDocumentOpen {
    admission: OpenAdmission,
    worker: DocumentOpenPreparation,
    cancelled: bool,
}

impl EditorProjectSession {
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
                return match result {
                    EditorDocumentOpenResult::Opened { document, .. } => {
                        EditorDocumentOpenRequestResult::Opened { document }
                    }
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
        if !self.accepts_new_work()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || self.pending_persistence.is_some()
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
        if let Err(error) = self.exclude_project_source_aliases(&relative_path) {
            return rejected(error.code());
        }
        let Some(next) = self.next_document_open_request.checked_add(1) else {
            return rejected("project.editor.document-open-id-exhausted");
        };
        self.next_document_open_request = next;
        let request = EditorDocumentOpenId::new(self.current_frame().revision(), next);
        let admission = OpenAdmission {
            request,
            path: relative_path,
            index,
        };
        let worker = DocumentOpenPreparation::start(
            captured,
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            #[cfg(test)]
            self.next_document_open_gate.take(),
        );
        self.pending_document_open = Some(PendingDocumentOpen {
            admission,
            worker,
            cancelled: false,
        });
        EditorDocumentOpenRequestResult::Preparing { request }
    }

    fn validate_document_open(
        &self,
        admission: &OpenAdmission,
        prepared: &PreparedAuthoringDocumentOpen,
    ) -> Result<(), &'static str> {
        if !self.accepts_new_work() {
            return Err("project.editor.document-open-busy");
        }
        let current = self
            .project_source_index_snapshot()
            .ok_or("project.editor.document-index-unavailable")?;
        if !Arc::ptr_eq(&current, &admission.index) {
            return Err("project.editor.document-index-stale");
        }
        let (path, current) = prepared
            .current_source()
            .map_err(SourceAdmissionError::code)?;
        self.exclude_source_aliases(path, &current)
            .map_err(SourceAdmissionError::code)?;
        let source = match prepared
            .document
            .as_ref()
            .expect("prepared Open retains its document")
        {
            PreparedDocument::Scene { source, .. } | PreparedDocument::Prefab { source, .. } => {
                source
            }
        };
        source
            .revalidate_native_source(&self.source_project_root())
            .map_err(|error| error.code())?;
        self.exclude_native_source_key(source.relative_path(), source.source_key())?;
        Ok(())
    }

    pub(super) fn document_open_view(&self) -> EditorDocumentOpenView {
        let pending = self.pending_document_open.as_ref().map(|pending| {
            (
                pending.admission.request,
                Arc::clone(&pending.admission.path),
                if pending.cancelled {
                    EditorDocumentOpenPhase::Cancelling
                } else {
                    EditorDocumentOpenPhase::Preparing
                },
            )
        });
        EditorDocumentOpenView::new(pending, self.document_open_result)
    }

    pub(super) fn acknowledge_document_open(&mut self, request: EditorDocumentOpenId) -> bool {
        if self
            .document_open_result
            .is_some_and(|result| result.request() == request)
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
            cancelled,
        }) = self.pending_document_open.take()
        else {
            return false;
        };
        let Some(completion) = worker.poll() else {
            self.pending_document_open = Some(PendingDocumentOpen {
                admission,
                worker,
                cancelled,
            });
            return false;
        };
        let request = admission.request;
        if cancelled || !self.accepts_new_work() {
            self.document_open_result = Some(EditorDocumentOpenResult::Cancelled { request });
            return true;
        }
        self.document_open_result = Some(
            match completion.and_then(|prepared| {
                self.publish_prepared_document_open(&admission, prepared)
                    .map_err(preparation::DocumentOpenFailure::from_code)
            }) {
                Ok(document) => EditorDocumentOpenResult::Opened { request, document },
                Err(failure) => {
                    let _ = self.diagnostics.push(failure.diagnostic);
                    EditorDocumentOpenResult::Rejected {
                        request,
                        code: failure.code,
                    }
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
