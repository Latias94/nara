use super::*;

use nara_asset::{AssetPath, AssetRef};
use nara_scene::SceneEntityId;
use nara_tooling::EditorDocumentSourceBinding;

use crate::project_host::persistence::{
    OpenedPrefabPersistence, OpenedPrefabReopen, PersistenceSaveOutcome, PrefabReopenOutcome,
    PrefabSaveCandidate,
};

impl EditorProjectSession {
    /// Opens or activates the one path-bound Prefab lane owned by this Editor session.
    #[doc(hidden)]
    pub(super) fn open_prefab_source(
        &mut self,
        document: EditorDocumentId,
        base_revision: nara_scene::SceneAuthoringRevision,
        entity: &SceneEntityId,
    ) -> EditorWorkspaceCommandReport {
        let source_binding = match self.prefab_source_binding(document, base_revision, entity) {
            Ok(source_binding) => source_binding,
            Err(code) => {
                return editor_workspace_command_rejected(self.workspace.active_document(), code);
            }
        };
        if let Some(document) = self.workspace.document_for_source_binding(&source_binding) {
            return self.workspace.activate_document(document);
        }
        if self.prefab_persistence.is_some() {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-replacement-requires-close",
            );
        }
        if self.authoring_project().is_some() {
            return self.open_authoring_prefab_source(source_binding);
        }
        if !cfg!(feature = "runtime-ui") {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.ui-authoring-unavailable",
            );
        }
        let Some(lane) = self.trusted_compiled() else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-runtime-lane-unavailable",
            );
        };
        let opened = PrefabPersistenceHost::open(
            &lane.prefab_root,
            source_binding.clone(),
            lane.plan.schema_validation().registry(),
        );
        let OpenedPrefabPersistence {
            host,
            session,
            digest,
        } = match opened {
            Ok(opened) => opened,
            Err(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        if !prefab_contains_ui_root(session.document()) {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-source-not-ui",
            );
        }
        let title = source_binding.locator().to_owned();
        let report = match self
            .workspace
            .open_prefab_session(title, source_binding, session)
        {
            Ok(report) => report,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-publication-failed",
                );
            }
        };
        let Some(document) = report.opened_document else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-publication-failed",
            );
        };
        let binding = self.bind_opened_prefab_source_digest(document, digest);
        if !binding.applied {
            let _ = self.workspace.__discard_document(document);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-persistence-binding-failed",
            );
        }
        self.prefab_persistence = Some(host);
        report
    }

    fn open_authoring_prefab_source(
        &mut self,
        source_binding: EditorDocumentSourceBinding,
    ) -> EditorWorkspaceCommandReport {
        let path = match AssetPath::new(source_binding.locator().to_owned()) {
            Ok(path) => path,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-source-path-invalid",
                );
            }
        };
        let (project_root, prefab_directory, snapshot_observation, published) = {
            let Some(snapshot) = self.authoring_project() else {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-authoring-lane-unavailable",
                );
            };
            let Some(prefab) = snapshot.prefab(&path) else {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-source-outside-authoring-closure",
                );
            };
            (
                snapshot.project_root(),
                snapshot.settings().paths.prefabs.clone(),
                AuthoringPrefabSourceObservation::new(
                    prefab.source().identity(),
                    prefab.source().digest(),
                ),
                snapshot
                    .fork_prefab(&path)
                    .expect("the admitted Prefab source remains in the immutable snapshot"),
            )
        };
        let observation = self
            .authoring_prefab_source_observations
            .get(&path)
            .copied()
            .unwrap_or(snapshot_observation);
        let directory_path = match RelativePath::new(prefab_directory.as_str()) {
            Ok(path) => path,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefabs-path-invalid",
                );
            }
        };
        let prefabs = match project_root.open_directory(&directory_path) {
            Ok(prefabs) => prefabs,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefabs-open-failed",
                );
            }
        };
        let (mut host, _) = match PrefabPersistenceHost::bind_authoring_source(
            &prefabs,
            source_binding.clone(),
            observation.identity,
            observation.digest,
        ) {
            Ok(bound) => bound,
            Err(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        let diagnostics = published.diagnostics().clone();
        let basis =
            nara_scene::PrefabAuthoringSession::from_published_authoring_document(published);
        let reopened = match host.reopen_authoring(&basis) {
            PrefabReopenOutcome::Opened(opened) => opened,
            PrefabReopenOutcome::Rejected(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-preservation-mismatch",
                );
            }
            PrefabReopenOutcome::Failed(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        let OpenedPrefabReopen {
            session,
            digest: opened_digest,
            identity,
            content_digest,
        } = *reopened;
        host.commit_reopen(identity, content_digest);
        let title = source_binding.locator().to_owned();
        let report = match self
            .workspace
            .open_prefab_session(title, source_binding, session)
        {
            Ok(report) => report,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-publication-failed",
                );
            }
        };
        let Some(document) = report.opened_document else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-publication-failed",
            );
        };
        let binding = self.bind_opened_prefab_source_digest(document, opened_digest);
        if !binding.applied {
            let _ = self.workspace.__discard_document(document);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-persistence-binding-failed",
            );
        }
        let _ = self.diagnostics.extend(diagnostics);
        self.authoring_prefab_source_observations.insert(
            path,
            AuthoringPrefabSourceObservation::new(identity, content_digest),
        );
        self.prefab_persistence = Some(host);
        report
    }

    fn prefab_source_binding(
        &self,
        document: EditorDocumentId,
        base_revision: nara_scene::SceneAuthoringRevision,
        entity: &SceneEntityId,
    ) -> Result<EditorDocumentSourceBinding, &'static str> {
        let scene = self
            .workspace
            .scene(document)
            .ok_or("project.editor.prefab-source-requires-scene")?;
        if scene.revision() != base_revision {
            return Err("project.editor.prefab-source-stale-scene");
        }
        if scene.selection().top_entity() != Some(entity) {
            return Err("project.editor.prefab-source-not-selected");
        }
        let record = scene
            .session()
            .document()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .ok_or("project.editor.prefab-source-entity-missing")?;
        let instance = record
            .prefab
            .as_ref()
            .ok_or("project.editor.prefab-source-not-an-anchor")?;
        let AssetRef::Path(source) = &instance.source else {
            return Err("project.editor.prefab-source-stable-id-unsupported");
        };
        let source_is_admitted = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.prefab(source).is_some(),
            EditorExecutionLane::TrustedCompiled(lane) => lane
                .snapshot
                .prefabs()
                .iter()
                .any(|prefab| prefab.path() == source),
        };
        if !source_is_admitted {
            return Err("project.editor.prefab-source-outside-content-closure");
        }
        Ok(EditorDocumentSourceBinding::new(source.as_str().to_owned()))
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_valid_persistence_receipt(&mut self) {
        self.persistence.test_reject_next_valid_receipt();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.persistence.test_fail_next_save(stage);
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_prefab_source_binding(&mut self) {
        self.reject_next_prefab_source_binding = true;
    }

    fn bind_opened_prefab_source_digest(
        &mut self,
        document: EditorDocumentId,
        digest: nara_tooling::EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        #[cfg(test)]
        if std::mem::take(&mut self.reject_next_prefab_source_binding) {
            return EditorWorkspaceCommandReport::default();
        }
        self.persistence_authority
            .bind_opened_source_digest(&mut self.workspace, document, digest)
    }

    #[must_use]
    #[doc(hidden)]
    pub(crate) const fn persistence_view(&self) -> EditorPersistenceView {
        EditorPersistenceView::new(self.persistence_operation, self.persistence_result)
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn last_persistence_receipt(&self) -> Option<EditorPersistenceReceipt> {
        self.last_persistence_receipt
    }

    pub(crate) fn request_persistence(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        if command != EditorPersistenceCommand::AcknowledgeResult
            && self.pending_workspace_intent.is_some()
        {
            return EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy);
        }
        self.request_persistence_after_workspace_arbitration(command)
    }

    pub(super) fn request_persistence_for_workspace_intent(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        self.request_persistence_after_workspace_arbitration(command)
    }

    fn request_persistence_after_workspace_arbitration(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        if command == EditorPersistenceCommand::AcknowledgeResult {
            return if self.pending_persistence.is_some() {
                EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy)
            } else if self.persistence_result.take().is_some() {
                EditorPersistenceRequestResult::Accepted
            } else {
                EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::ResultPending)
            };
        }
        if self.pending_persistence.is_some() {
            return EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy);
        }
        let (request, requested_document) = match command {
            EditorPersistenceCommand::Save { document } => {
                let Some(document) = document.or(self.workspace.active_document()) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: None,
                        reason: EditorPersistenceRejection::NoActiveDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::NoActiveDocument,
                    );
                };
                (EditorPersistenceRequestKind::Save, document)
            }
            EditorPersistenceCommand::Reopen { document } => (
                EditorPersistenceRequestKind::Reopen,
                document
                    .or(self.workspace.active_document())
                    .unwrap_or(self.source_document),
            ),
            EditorPersistenceCommand::AcknowledgeResult => unreachable!(),
        };
        if self.persistence_result.is_some_and(|result| {
            !persistence_result_allows_retry(result, request, requested_document)
        }) {
            return EditorPersistenceRequestResult::Rejected(
                EditorPersistenceRejection::ResultPending,
            );
        }

        let runtime_blocks_reopen = self.trusted_compiled().is_some_and(|lane| {
            !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
                || lane.runtime_host.has_cleanup_owner()
        });
        if matches!(command, EditorPersistenceCommand::Reopen { .. })
            && self.workspace.document_kind(requested_document) != Some(EditorDocumentKind::Prefab)
            && runtime_blocks_reopen
        {
            self.persistence_result = Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(requested_document),
                reason: EditorPersistenceRejection::RuntimeActive,
            });
            return EditorPersistenceRequestResult::Rejected(
                EditorPersistenceRejection::RuntimeActive,
            );
        }

        match command {
            EditorPersistenceCommand::Save { .. } => {
                let Some(kind) = self.workspace.document_kind(requested_document) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::MissingDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::MissingDocument,
                    );
                };
                if !self
                    .workspace
                    .document_is_dirty(requested_document)
                    .unwrap_or(false)
                {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::NotDirty,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::NotDirty,
                    );
                }
                let Some(checkpoint) = self
                    .persistence_authority
                    .capture(&self.workspace, requested_document)
                else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::StaleRevision,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::StaleRevision,
                    );
                };
                let revision = checkpoint.revision();
                self.pending_persistence = Some(PendingPersistence::Save(match kind {
                    EditorDocumentKind::Scene => {
                        PendingSave::Scene(SceneSaveCandidate { checkpoint })
                    }
                    EditorDocumentKind::Prefab => {
                        PendingSave::Prefab(PrefabSaveCandidate { checkpoint })
                    }
                }));
                self.persistence_operation = EditorPersistenceOperation::Saving {
                    document: requested_document,
                    captured_revision: revision,
                };
            }
            EditorPersistenceCommand::Reopen { .. } => {
                let base_revision =
                    self.workspace
                        .document_kind(requested_document)
                        .and_then(|kind| match kind {
                            EditorDocumentKind::Scene => self
                                .workspace
                                .scene(requested_document)
                                .map(|slot| slot.revision()),
                            EditorDocumentKind::Prefab => self
                                .workspace
                                .prefab(requested_document)
                                .map(|slot| slot.revision()),
                        });
                self.pending_persistence = Some(PendingPersistence::Reopen {
                    document: requested_document,
                    base_revision,
                });
                self.persistence_operation = EditorPersistenceOperation::Opening {
                    document: requested_document,
                };
            }
            EditorPersistenceCommand::AcknowledgeResult => unreachable!(),
        }
        EditorPersistenceRequestResult::Accepted
    }

    pub(super) fn advance_workspace_persistence(&mut self) {
        let Some(mut pending) = self.pending_workspace_intent.take() else {
            return;
        };
        if pending.phase() != EditorWorkspaceIntentPhase::Saving {
            self.pending_workspace_intent = Some(pending);
            return;
        }
        let Some(pending_document) = pending.decision_document() else {
            self.pending_workspace_intent = Some(pending);
            return;
        };
        match self.persistence_result {
            Some(EditorPersistenceResult::Saved { document, .. })
                if document == pending_document =>
            {
                match &mut pending {
                    PendingWorkspaceIntent::CloseDocument(close) => {
                        close.phase = EditorWorkspaceIntentPhase::RetiringRuntime;
                    }
                    PendingWorkspaceIntent::Exit(exit) => {
                        self.persistence_result = None;
                        exit.cursor = exit.cursor.saturating_add(1);
                        self.prepare_current_exit_document(exit);
                    }
                }
            }
            Some(
                EditorPersistenceResult::Rejected { .. }
                | EditorPersistenceResult::Failed { .. }
                | EditorPersistenceResult::PersistenceUncertain { .. },
            ) => {
                pending.set_phase(EditorWorkspaceIntentPhase::AwaitingDecision);
            }
            Some(EditorPersistenceResult::Opened { .. })
            | Some(EditorPersistenceResult::Saved { .. }) => {
                pending.set_phase(EditorWorkspaceIntentPhase::AwaitingDecision);
            }
            None => {}
        }
        self.pending_workspace_intent = Some(pending);
    }

    pub(super) fn complete_save(&mut self, save: PendingSave) -> EditorPersistenceResult {
        let (lane, document, refresh_runtime_content, outcome) = match save {
            PendingSave::Scene(save) => {
                let document = save.checkpoint.document();
                let outcome = if let Some(slot) = self
                    .workspace
                    .scene(document)
                    .filter(|slot| slot.authoring_readiness().is_some())
                {
                    self.persistence.save_authoring(save, slot.session())
                } else {
                    self.persistence.save_native(save)
                };
                (PersistenceLane::Scene, document, false, outcome)
            }
            PendingSave::Prefab(save) => {
                let document = save.checkpoint.document();
                let Some(host) = self.prefab_persistence.as_mut() else {
                    return EditorPersistenceResult::Failed {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(document),
                        stage: EditorPersistenceFailureStage::OpenTarget,
                    };
                };
                let refresh_runtime_content = host.publishes_runtime_content();
                let outcome = if let Some(slot) = self
                    .workspace
                    .prefab(document)
                    .filter(|slot| slot.authoring_readiness().is_some())
                {
                    host.save_authoring(save, slot.session())
                } else {
                    host.save_native(save)
                };
                (
                    PersistenceLane::Prefab,
                    document,
                    refresh_runtime_content,
                    outcome,
                )
            }
        };
        match outcome {
            PersistenceSaveOutcome::Saved { commit, evidence } => {
                let report = self
                    .persistence_authority
                    .commit(&mut self.workspace, commit);
                if !report.applied {
                    self.mark_persistence_uncertain(lane);
                    self.last_persistence_receipt = Some(evidence);
                    return EditorPersistenceResult::PersistenceUncertain {
                        document: evidence.document(),
                        revision: evidence.revision(),
                        digest: evidence.digest(),
                    };
                }
                self.last_persistence_receipt = Some(evidence);
                if refresh_runtime_content {
                    self.schedule_prefab_content_refresh(evidence);
                } else if matches!(lane, PersistenceLane::Prefab) {
                    self.commit_authoring_prefab_source_observation(document, evidence);
                }
                EditorPersistenceResult::Saved {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
            PersistenceSaveOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason,
            },
            PersistenceSaveOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                stage,
            },
            PersistenceSaveOutcome::PersistenceUncertain { evidence } => {
                self.last_persistence_receipt = Some(evidence);
                EditorPersistenceResult::PersistenceUncertain {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
        }
    }

    fn mark_persistence_uncertain(&mut self, lane: PersistenceLane) {
        match lane {
            PersistenceLane::Scene => self.persistence.mark_uncertain(),
            PersistenceLane::Prefab => {
                if let Some(host) = self.prefab_persistence.as_mut() {
                    host.mark_uncertain();
                }
            }
        }
    }

    pub(super) fn complete_reopen(
        &mut self,
        document: EditorDocumentId,
        base_revision: Option<nara_scene::SceneAuthoringRevision>,
    ) -> EditorPersistenceResult {
        if self.workspace.document_kind(document) == Some(EditorDocumentKind::Prefab) {
            return self.complete_prefab_reopen(document, base_revision);
        }
        self.complete_scene_reopen(document, base_revision)
    }

    fn complete_scene_reopen(
        &mut self,
        document: EditorDocumentId,
        base_revision: Option<nara_scene::SceneAuthoringRevision>,
    ) -> EditorPersistenceResult {
        let reopened = match self.workspace.scene(document) {
            Some(slot) if slot.authoring_readiness().is_some() => {
                self.persistence.reopen_authoring(slot.session())
            }
            _ => match self.trusted_compiled() {
                Some(lane) => self
                    .persistence
                    .reopen_native(lane.plan.schema_validation().registry()),
                None => match self.authoring_project() {
                    Some(snapshot) => {
                        let basis =
                            nara_scene::SceneAuthoringSession::from_published_authoring_document(
                                snapshot.fork_startup_scene(),
                            );
                        self.persistence.reopen_authoring(&basis)
                    }
                    None => SceneReopenOutcome::Rejected(
                        EditorPersistenceRejection::PreservationMismatch,
                    ),
                },
            },
        };
        match reopened {
            SceneReopenOutcome::Opened(opened) => {
                let OpenedSceneReopen {
                    session,
                    digest,
                    identity,
                    content_digest,
                } = *opened;
                let replacing_existing = base_revision.is_some();
                let report = if let Some(base_revision) = base_revision {
                    match self.persistence_authority.publish_reopened_session(
                        &mut self.workspace,
                        document,
                        base_revision,
                        session,
                        digest,
                    ) {
                        Ok(report) => report,
                        Err(_) => {
                            return EditorPersistenceResult::Rejected {
                                request: EditorPersistenceRequestKind::Reopen,
                                document: Some(document),
                                reason: EditorPersistenceRejection::StaleRevision,
                            };
                        }
                    }
                } else if self.workspace.is_empty() {
                    match self
                        .workspace
                        .open_scene_session(self.scene_title.clone(), session)
                    {
                        Ok(report) => report,
                        Err(_) => {
                            return EditorPersistenceResult::Failed {
                                request: EditorPersistenceRequestKind::Reopen,
                                document: Some(document),
                                stage: EditorPersistenceFailureStage::Validate,
                            };
                        }
                    }
                } else {
                    return EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(document),
                        reason: EditorPersistenceRejection::MissingDocument,
                    };
                };
                let opened_document = report.opened_document.unwrap_or(document);
                let revision = report
                    .revision
                    .expect("a reopened document reports its authoring revision");
                if !replacing_existing {
                    let binding = self.persistence_authority.bind_opened_source_digest(
                        &mut self.workspace,
                        opened_document,
                        digest,
                    );
                    if !binding.applied {
                        return EditorPersistenceResult::Failed {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(opened_document),
                            stage: EditorPersistenceFailureStage::Validate,
                        };
                    }
                }
                self.persistence.commit_reopen(identity, content_digest);
                self.source_document = opened_document;
                EditorPersistenceResult::Opened {
                    document: opened_document,
                    revision,
                    digest,
                }
            }
            SceneReopenOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason,
            },
            SceneReopenOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage,
            },
        }
    }

    fn complete_prefab_reopen(
        &mut self,
        document: EditorDocumentId,
        base_revision: Option<nara_scene::SceneAuthoringRevision>,
    ) -> EditorPersistenceResult {
        let Some(host) = self.prefab_persistence.as_ref() else {
            return EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage: EditorPersistenceFailureStage::OpenTarget,
            };
        };
        let Some(base_revision) = base_revision else {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::MissingDocument,
            };
        };
        let source_binding = match self.workspace.prefab(document) {
            Some(slot) => slot.source_binding().clone(),
            None => {
                return EditorPersistenceResult::Rejected {
                    request: EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    reason: EditorPersistenceRejection::MissingDocument,
                };
            }
        };
        let refresh_runtime_content = host.publishes_runtime_content();
        let reopened = match self.workspace.prefab(document) {
            Some(slot) if slot.authoring_readiness().is_some() => {
                host.reopen_authoring(slot.session())
            }
            _ => match self.trusted_compiled() {
                Some(lane) => host.reopen_native(lane.plan.schema_validation().registry()),
                None => {
                    PrefabReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
                }
            },
        };
        match reopened {
            PrefabReopenOutcome::Opened(opened) => {
                let OpenedPrefabReopen {
                    session,
                    digest,
                    identity,
                    content_digest,
                } = *opened;
                let report = match self.persistence_authority.publish_reopened_prefab_session(
                    &mut self.workspace,
                    document,
                    base_revision,
                    source_binding,
                    session,
                    digest,
                ) {
                    Ok(report) => report,
                    Err(_) => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::StaleRevision,
                        };
                    }
                };
                let revision = report
                    .revision
                    .expect("a reopened Prefab reports its authoring revision");
                if let Some(host) = self.prefab_persistence.as_mut() {
                    host.commit_reopen(identity, content_digest);
                }
                if refresh_runtime_content {
                    let path = AssetPath::new(
                        self.workspace
                            .prefab(document)
                            .expect("a reopened Prefab remains in its exact workspace slot")
                            .source_binding()
                            .locator()
                            .to_owned(),
                    )
                    .expect("a Host-normalized Prefab source remains a valid asset path");
                    self.schedule_reopened_prefab_content_refresh(
                        document,
                        revision,
                        path,
                        digest,
                        content_digest,
                    );
                } else if let Some(path) = self.workspace.prefab(document).and_then(|slot| {
                    AssetPath::new(slot.source_binding().locator().to_owned()).ok()
                }) {
                    self.authoring_prefab_source_observations.insert(
                        path,
                        AuthoringPrefabSourceObservation::new(identity, content_digest),
                    );
                }
                EditorPersistenceResult::Opened {
                    document,
                    revision,
                    digest,
                }
            }
            PrefabReopenOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason,
            },
            PrefabReopenOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage,
            },
        }
    }

    fn commit_authoring_prefab_source_observation(
        &mut self,
        document: EditorDocumentId,
        evidence: EditorPersistenceReceipt,
    ) {
        let Some(path) = self
            .workspace
            .prefab(document)
            .and_then(|slot| AssetPath::new(slot.source_binding().locator().to_owned()).ok())
        else {
            return;
        };
        let (Some(identity), Some(digest)) =
            (evidence.published_identity(), evidence.published_digest())
        else {
            return;
        };
        self.authoring_prefab_source_observations.insert(
            path,
            AuthoringPrefabSourceObservation::new(
                identity,
                ContentDigest::from_parts(digest.encoded_bytes(), digest.hash()),
            ),
        );
    }
}

fn prefab_contains_ui_root(document: &nara_scene::PrefabDocument) -> bool {
    #[cfg(feature = "runtime-ui")]
    {
        nara_ui::__private::prefab_contains_ui_root(document)
    }
    #[cfg(not(feature = "runtime-ui"))]
    {
        let _ = document;
        false
    }
}

fn prefab_open_failure_code(stage: EditorPersistenceFailureStage) -> &'static str {
    match stage {
        EditorPersistenceFailureStage::OpenTarget => "project.editor.prefab-open-failed",
        EditorPersistenceFailureStage::ReadTarget => "project.editor.prefab-read-failed",
        EditorPersistenceFailureStage::Decode => "project.editor.prefab-decode-failed",
        EditorPersistenceFailureStage::Validate => "project.editor.prefab-validation-failed",
        EditorPersistenceFailureStage::Encode
        | EditorPersistenceFailureStage::CreateTemporary
        | EditorPersistenceFailureStage::WriteTemporary
        | EditorPersistenceFailureStage::SyncTemporary
        | EditorPersistenceFailureStage::ReplaceTarget => {
            "project.editor.prefab-open-internal-failed"
        }
    }
}

fn persistence_result_allows_retry(
    result: EditorPersistenceResult,
    request: EditorPersistenceRequestKind,
    document: EditorDocumentId,
) -> bool {
    match result {
        EditorPersistenceResult::Rejected {
            request: failed_request,
            document: failed_document,
            ..
        }
        | EditorPersistenceResult::Failed {
            request: failed_request,
            document: failed_document,
            ..
        } => {
            failed_request == request
                && failed_document.is_none_or(|failed_document| failed_document == document)
        }
        EditorPersistenceResult::PersistenceUncertain {
            document: uncertain_document,
            ..
        } => request == EditorPersistenceRequestKind::Reopen && uncertain_document == document,
        EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. } => false,
    }
}
