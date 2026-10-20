use super::*;

use crate::project_host::authoring::{AdmittedAuthoringDocument, AuthoringDocumentKind};

use nara_tooling::{EditorDocumentDigest, EditorDocumentSourceBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorDocumentOpenRequestResult {
    Activated { document: EditorDocumentId },
    Opened { document: EditorDocumentId },
    AwaitingClose { document: EditorDocumentId },
    Rejected { code: &'static str },
}

pub(super) enum PreparedAuthoringDocumentOpen {
    Scene {
        relative_path: Arc<str>,
        digest: EditorDocumentDigest,
        host: ScenePersistenceHost,
        document: nara_scene::PublishedAuthoringSceneDocument,
    },
    Prefab {
        relative_path: Arc<str>,
        digest: EditorDocumentDigest,
        host: PrefabPersistenceHost,
        document: nara_scene::PublishedAuthoringPrefabDocument,
    },
}

impl PreparedAuthoringDocumentOpen {
    const fn kind(&self) -> AuthoringDocumentKind {
        match self {
            Self::Scene { .. } => AuthoringDocumentKind::Scene,
            Self::Prefab { .. } => AuthoringDocumentKind::Prefab,
        }
    }
}

impl EditorProjectSession {
    pub(crate) fn open_indexed_authoring_document(
        &mut self,
        index: Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
        kind: AuthoringDocumentKind,
    ) -> EditorDocumentOpenRequestResult {
        if !self.accepts_new_work()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || self.pending_persistence.is_some()
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

        let admitted = match project.admit_indexed_document(&index, relative_path, kind) {
            Ok(admitted) => admitted,
            Err(error) => return rejected(error.code()),
        };
        let prepared = match self.prepare_document_open(admitted) {
            Ok(prepared) => prepared,
            Err(code) => return rejected(code),
        };
        debug_assert_eq!(prepared.kind(), kind);
        let replacing = match kind {
            AuthoringDocumentKind::Scene => self.scene_lane.as_ref().map(|lane| lane.document),
            AuthoringDocumentKind::Prefab => self.prefab_lane.as_ref().map(|lane| lane.document),
        };
        if let Some(document) = replacing {
            self.pending_document_open = Some(prepared);
            let requested =
                self.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document });
            if requested != EditorWorkspaceIntentRequestResult::Accepted {
                self.pending_document_open = None;
                return rejected("project.editor.document-replacement-close-rejected");
            }
            return match self.bound_document(relative_path, kind) {
                Some(document) => EditorDocumentOpenRequestResult::Opened { document },
                None => EditorDocumentOpenRequestResult::AwaitingClose { document },
            };
        }

        self.publish_prepared_document_open(prepared)
            .map_or_else(rejected, |document| {
                EditorDocumentOpenRequestResult::Opened { document }
            })
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

    fn prepare_document_open(
        &self,
        admitted: AdmittedAuthoringDocument,
    ) -> Result<PreparedAuthoringDocumentOpen, &'static str> {
        let root = self
            .authoring_project()
            .expect("indexed authoring admission requires the authoring execution lane")
            .project_root();
        let source = admitted.source();
        let relative_path: Arc<str> = Arc::from(source.relative_path());
        let digest = editor_document_digest(source.digest());
        match admitted {
            AdmittedAuthoringDocument::Scene { source, document } => {
                let (host, opened_digest) = ScenePersistenceHost::bind_authoring_source(
                    &root,
                    source.relative_path(),
                    source.identity(),
                    source.digest(),
                )
                .map_err(document_open_failure_code)?;
                if opened_digest != digest {
                    return Err("project.editor.document-persistence-digest-mismatch");
                }
                Ok(PreparedAuthoringDocumentOpen::Scene {
                    relative_path,
                    digest,
                    host,
                    document,
                })
            }
            AdmittedAuthoringDocument::Prefab { source, document } => {
                let source_binding = EditorDocumentSourceBinding::new(source.relative_path());
                let (host, opened_digest) = PrefabPersistenceHost::bind_authoring_source(
                    &root,
                    source_binding,
                    source.identity(),
                    source.digest(),
                )
                .map_err(document_open_failure_code)?;
                if opened_digest != digest {
                    return Err("project.editor.document-persistence-digest-mismatch");
                }
                Ok(PreparedAuthoringDocumentOpen::Prefab {
                    relative_path,
                    digest,
                    host,
                    document,
                })
            }
        }
    }

    pub(super) fn publish_pending_document_open(&mut self) {
        let Some(prepared) = self.pending_document_open.take() else {
            return;
        };
        if let Err(code) = self.publish_prepared_document_open(prepared) {
            let _ = self.diagnostics.extend(single_error(
                code,
                "the admitted authoring document could not enter its Editor persistence lane",
            ));
        }
    }

    pub(super) fn cancel_pending_document_open(&mut self) {
        self.pending_document_open = None;
    }

    fn publish_prepared_document_open(
        &mut self,
        prepared: PreparedAuthoringDocumentOpen,
    ) -> Result<EditorDocumentId, &'static str> {
        if !self.accepts_new_work() {
            return Err("project.editor.document-open-busy");
        }
        match prepared {
            PreparedAuthoringDocumentOpen::Scene {
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
                let host = self
                    .authoring_runtime
                    .host_custody()
                    .retain_scene(host)
                    .ok_or("project.editor.document-open-busy")?;
                self.scene_lane = Some(EditorScenePersistenceLane {
                    project_path: relative_path,
                    document,
                    host,
                });
                Ok(document)
            }
            PreparedAuthoringDocumentOpen::Prefab {
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
                let host = self
                    .authoring_runtime
                    .host_custody()
                    .retain_prefab(host)
                    .ok_or("project.editor.document-open-busy")?;
                self.prefab_lane = Some(EditorPrefabPersistenceLane {
                    project_path: relative_path,
                    document,
                    host,
                });
                Ok(document)
            }
        }
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
