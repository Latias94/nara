use super::*;

use crate::project_host::authoring::{AdmittedAuthoringDocument, AuthoringDocumentKind};

use super::persistence::RetainedHost;
use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision};
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
    #[cfg(test)]
    AwaitingClose {
        document: EditorDocumentId,
    },
    Rejected {
        code: &'static str,
    },
}

enum PreparedDocument {
    Scene {
        relative_path: Arc<str>,
        digest: EditorDocumentDigest,
        host: RetainedHost<ScenePersistenceHost>,
        document: nara_scene::PublishedAuthoringSceneDocument,
    },
    Prefab {
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
    kind: AuthoringDocumentKind,
    index: Arc<ProjectSourceIndexSnapshot>,
    replacing: Option<ReplacingDocument>,
}

struct ReplacingDocument {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    binding: Option<EditorDocumentSourceBinding>,
    schema: Option<AuthoringSchemaBasis>,
}

pub(super) enum PendingDocumentOpen {
    Preparing {
        admission: OpenAdmission,
        worker: DocumentOpenPreparation,
        cancelled: bool,
    },
    AwaitingClose {
        admission: OpenAdmission,
        prepared: PreparedAuthoringDocumentOpen,
        retirement_admitted: bool,
    },
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
            if let Some(PendingDocumentOpen::AwaitingClose { admission, .. }) =
                &self.pending_document_open
            {
                return EditorDocumentOpenRequestResult::AwaitingClose {
                    document: admission.replacing.as_ref().unwrap().document,
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

        if let Some(document) = self.bound_document(relative_path, kind) {
            let report = self.workspace.activate_document(document);
            return if report.applied {
                EditorDocumentOpenRequestResult::Activated { document }
            } else {
                rejected("project.editor.document-activation-failed")
            };
        }
        if self.extension_documents.has_source(relative_path) {
            return rejected("project.editor.document-source-already-open");
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
        let Some(next) = self.next_document_open_request.checked_add(1) else {
            return rejected("project.editor.document-open-id-exhausted");
        };
        self.next_document_open_request = next;
        let request = EditorDocumentOpenId::new(self.current_frame().revision(), next);
        let admission = OpenAdmission {
            request,
            path: relative_path,
            kind,
            index,
            replacing: self.capture_replacing_document(kind),
        };
        let worker = DocumentOpenPreparation::start(
            captured,
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            #[cfg(test)]
            self.next_document_open_gate.take(),
        );
        self.pending_document_open = Some(PendingDocumentOpen::Preparing {
            admission,
            worker,
            cancelled: false,
        });
        EditorDocumentOpenRequestResult::Preparing { request }
    }

    fn bound_document(
        &self,
        relative_path: &str,
        kind: AuthoringDocumentKind,
    ) -> Option<EditorDocumentId> {
        match kind {
            AuthoringDocumentKind::Scene => self.scene_lane.as_ref().and_then(|lane| {
                (lane.project_path.as_ref() == relative_path).then_some(lane.document)
            }),
            AuthoringDocumentKind::Prefab => self.prefab_lane.as_ref().and_then(|lane| {
                (lane.project_path.as_ref() == relative_path).then_some(lane.document)
            }),
        }
    }

    fn capture_replacing_document(&self, kind: AuthoringDocumentKind) -> Option<ReplacingDocument> {
        let document = match kind {
            AuthoringDocumentKind::Scene => self.scene_lane.as_ref()?.document,
            AuthoringDocumentKind::Prefab => self.prefab_lane.as_ref()?.document,
        };
        let (binding, schema) = match kind {
            AuthoringDocumentKind::Scene => {
                let slot = self.workspace.scene(document)?;
                (
                    slot.source_binding().cloned(),
                    slot.session().authoring_schema_basis(),
                )
            }
            AuthoringDocumentKind::Prefab => {
                let slot = self.workspace.prefab(document)?;
                (
                    Some(slot.source_binding().clone()),
                    slot.session().authoring_schema_basis(),
                )
            }
        };
        Some(ReplacingDocument {
            document,
            revision: self.workspace.document_revision(document)?,
            binding,
            schema,
        })
    }

    fn validate_document_open(&self, admission: &OpenAdmission) -> Result<(), &'static str> {
        if !self.accepts_new_work() {
            return Err("project.editor.document-open-busy");
        }
        let current = self
            .project_source_index_snapshot()
            .ok_or("project.editor.document-index-unavailable")?;
        if !Arc::ptr_eq(&current, &admission.index) {
            return Err("project.editor.document-index-stale");
        }
        if self.extension_documents.has_source(&admission.path) {
            return Err("project.editor.document-source-already-open");
        }
        match (
            &admission.replacing,
            self.capture_replacing_document(admission.kind),
        ) {
            (None, None) => Ok(()),
            (Some(before), Some(now))
                if before.document == now.document
                    && before.revision == now.revision
                    && before.binding == now.binding
                    && before.schema == now.schema =>
            {
                Ok(())
            }
            _ => Err("project.editor.document-open-stale-document"),
        }
    }

    pub(super) fn document_open_view(&self) -> EditorDocumentOpenView {
        let pending = self
            .pending_document_open
            .as_ref()
            .map(|pending| match pending {
                PendingDocumentOpen::Preparing {
                    admission,
                    cancelled,
                    ..
                } => (
                    admission.request,
                    Arc::clone(&admission.path),
                    if *cancelled {
                        EditorDocumentOpenPhase::Cancelling
                    } else {
                        EditorDocumentOpenPhase::Preparing
                    },
                ),
                PendingDocumentOpen::AwaitingClose { admission, .. } => (
                    admission.request,
                    Arc::clone(&admission.path),
                    EditorDocumentOpenPhase::AwaitingClose {
                        document: admission
                            .replacing
                            .as_ref()
                            .expect("replacement has a predecessor")
                            .document,
                    },
                ),
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
            Some(PendingDocumentOpen::Preparing {
                admission,
                worker,
                cancelled,
            }) if admission.request == request => {
                *cancelled = true;
                worker.cancel();
                true
            }
            // Once dirty close is admitted, its existing prompt owns Save/Discard/Cancel.
            _ => false,
        }
    }

    pub(super) fn document_open_preparing(&self) -> bool {
        matches!(
            self.pending_document_open,
            Some(PendingDocumentOpen::Preparing { .. })
        )
    }

    pub(super) fn advance_document_open(&mut self) -> bool {
        let Some(PendingDocumentOpen::Preparing { .. }) = self.pending_document_open.as_ref()
        else {
            return false;
        };
        let Some(PendingDocumentOpen::Preparing {
            admission,
            mut worker,
            cancelled,
        }) = self.pending_document_open.take()
        else {
            unreachable!()
        };
        let Some(completion) = worker.poll() else {
            self.pending_document_open = Some(PendingDocumentOpen::Preparing {
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
        let prepared = match completion.and_then(|prepared| {
            self.validate_document_open(&admission)
                .map(|()| prepared)
                .map_err(preparation::DocumentOpenFailure::from_code)
        }) {
            Ok(prepared) => prepared,
            Err(failure) => {
                let _ = self.diagnostics.push(failure.diagnostic);
                self.document_open_result = Some(EditorDocumentOpenResult::Rejected {
                    request,
                    code: failure.code,
                });
                return true;
            }
        };
        if let Some(replacing) = &admission.replacing {
            let document = replacing.document;
            self.pending_document_open = Some(PendingDocumentOpen::AwaitingClose {
                admission,
                prepared,
                retirement_admitted: false,
            });
            if self.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document })
                != EditorWorkspaceIntentRequestResult::Accepted
            {
                self.reject_pending_document_open(
                    "project.editor.document-replacement-close-rejected",
                );
            }
        } else {
            self.document_open_result = Some(match self.publish_prepared_document_open(prepared) {
                Ok(document) => EditorDocumentOpenResult::Opened { request, document },
                Err(code) => EditorDocumentOpenResult::Rejected { request, code },
            });
        }
        true
    }

    /// Recheck owner facts and file aliases before any irreversible predecessor retirement.
    pub(super) fn admit_document_open_retirement(&mut self) -> bool {
        let Some(PendingDocumentOpen::AwaitingClose {
            admission,
            retirement_admitted,
            ..
        }) = self.pending_document_open.as_ref()
        else {
            return true;
        };
        if *retirement_admitted {
            return true;
        }
        if let Err(code) = self.validate_document_open(admission) {
            self.reject_pending_document_open(code);
            if let Some(pending) = self.pending_workspace_intent.take() {
                self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Rejected {
                    intent: pending.intent(),
                    reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                        EditorPersistenceRejection::StaleRevision,
                    ),
                });
            }
            return false;
        }
        if let Some(PendingDocumentOpen::AwaitingClose {
            retirement_admitted,
            ..
        }) = self.pending_document_open.as_mut()
        {
            *retirement_admitted = true;
        }
        true
    }

    fn reject_pending_document_open(&mut self, code: &'static str) {
        if let Some(PendingDocumentOpen::AwaitingClose { admission, .. }) =
            self.pending_document_open.take()
        {
            self.document_open_result = Some(EditorDocumentOpenResult::Rejected {
                request: admission.request,
                code,
            });
        }
    }

    pub(super) fn publish_pending_document_open(&mut self) {
        let Some(PendingDocumentOpen::AwaitingClose {
            admission,
            prepared,
            ..
        }) = self.pending_document_open.take()
        else {
            return;
        };
        let request = admission.request;
        self.document_open_result = Some(match self.publish_prepared_document_open(prepared) {
            Ok(document) => EditorDocumentOpenResult::Opened { request, document },
            Err(code) => EditorDocumentOpenResult::Rejected { request, code },
        });
    }

    pub(super) fn cancel_pending_document_open(&mut self) {
        if let Some(PendingDocumentOpen::Preparing {
            worker, cancelled, ..
        }) = self.pending_document_open.as_mut()
        {
            *cancelled = true;
            worker.cancel();
        } else if let Some(PendingDocumentOpen::AwaitingClose { admission, .. }) =
            self.pending_document_open.take()
        {
            self.document_open_result = Some(EditorDocumentOpenResult::Cancelled {
                request: admission.request,
            });
        }
    }

    fn publish_prepared_document_open(
        &mut self,
        mut prepared: PreparedAuthoringDocumentOpen,
    ) -> Result<EditorDocumentId, &'static str> {
        if !self.accepts_new_work() {
            return Err("project.editor.document-open-busy");
        }
        let result = match prepared
            .document
            .take()
            .expect("a prepared document publishes once")
        {
            PreparedDocument::Scene {
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
                    let _ = self.workspace.__discard_document(document);
                    return Err("project.editor.scene-persistence-binding-failed");
                }
                let _ = self.diagnostics.extend(diagnostics);
                self.scene_lane = Some(EditorScenePersistenceLane {
                    project_path: relative_path,
                    document,
                    host,
                });
                Ok(document)
            }
            PreparedDocument::Prefab {
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
                    let _ = self.workspace.__discard_document(document);
                    return Err("project.editor.prefab-persistence-binding-failed");
                }
                let _ = self.diagnostics.extend(diagnostics);
                self.prefab_lane = Some(EditorPrefabPersistenceLane {
                    project_path: relative_path,
                    document,
                    host,
                });
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
