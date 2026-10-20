use super::*;

mod background;
mod retirement;
pub(super) use background::BackgroundSave;
use background::SaveLane;
#[cfg(test)]
pub(super) use background::SaveTestGate;
#[cfg(test)]
pub(crate) use background::{SaveTestControl, SaveTestPhase};
pub(super) use retirement::{SaveCustody, SaveCustodyPlugin};

use nara_asset::{AssetPath, AssetRef};
use nara_scene::SceneEntityId;
use nara_tooling::EditorDocumentSourceBinding;

use crate::project_host::persistence::{
    OpenedPrefabPersistence, OpenedPrefabReopen, PersistenceSaveOutcome, PrefabReopenOutcome,
    PrefabSaveCandidate,
};

impl EditorProjectSession {
    #[cfg(test)]
    pub(crate) fn hold_next_save(&mut self, phase: SaveTestPhase) -> SaveTestControl {
        let (gate, control) = SaveTestGate::new(phase);
        self.next_save_gate = Some(gate);
        control
    }

    pub(super) fn advance_persistence(&mut self) {
        let Some(pending) = self.pending_persistence.take() else {
            return;
        };
        let result = match pending {
            PendingPersistence::Save {
                candidate,
                authoring,
            } => match self.begin_background_save(candidate, authoring) {
                Ok(worker) => {
                    self.pending_persistence = Some(PendingPersistence::Saving(worker));
                    return;
                }
                Err(result) => result,
            },
            PendingPersistence::Saving(mut worker) => {
                let Some((lane, outcome)) = worker.poll() else {
                    self.pending_persistence = Some(PendingPersistence::Saving(worker));
                    return;
                };
                let document = lane.document();
                let (kind, refresh) = match lane {
                    SaveLane::Scene(lane) => {
                        debug_assert!(self.scene_lane.is_none());
                        self.scene_lane = Some(lane);
                        (PersistenceLane::Scene, false)
                    }
                    SaveLane::Prefab(lane) => {
                        let refresh = lane.host.publishes_runtime_content();
                        debug_assert!(self.prefab_lane.is_none());
                        self.prefab_lane = Some(lane);
                        (PersistenceLane::Prefab, refresh)
                    }
                };
                self.acknowledge_save(kind, document, refresh, outcome)
            }
            PendingPersistence::Reopen {
                document,
                base_revision,
            } => self.complete_reopen(document, base_revision),
        };
        self.persistence_result = Some(result);
        self.persistence_operation = EditorPersistenceOperation::Idle;
    }

    fn begin_background_save(
        &mut self,
        save: PendingSave,
        authoring: Option<nara_scene::AuthoringDocumentReadmission>,
    ) -> Result<BackgroundSave, EditorPersistenceResult> {
        let document = match &save {
            PendingSave::Scene(save) => save.checkpoint.document(),
            PendingSave::Prefab(save) => save.checkpoint.document(),
        };
        let failed = || EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(document),
            reason: EditorPersistenceRejection::SourceBindingMismatch,
        };
        let lane = match &save {
            PendingSave::Scene(_) => {
                if self
                    .scene_lane
                    .as_ref()
                    .is_none_or(|lane| lane.document != document)
                {
                    return Err(failed());
                }
                SaveLane::Scene(self.scene_lane.take().unwrap())
            }
            PendingSave::Prefab(_) => {
                if self
                    .prefab_lane
                    .as_ref()
                    .is_none_or(|lane| lane.document != document)
                {
                    return Err(failed());
                }
                SaveLane::Prefab(self.prefab_lane.take().unwrap())
            }
        };
        BackgroundSave::start(
            lane,
            save,
            authoring,
            self.authoring_runtime.persistence_tasks(),
            self.authoring_runtime.save_custody(),
            #[cfg(test)]
            self.next_save_gate.take(),
        )
        .map_err(|lane| {
            match lane {
                SaveLane::Scene(lane) => self.scene_lane = Some(lane),
                SaveLane::Prefab(lane) => self.prefab_lane = Some(lane),
            }
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason: EditorPersistenceRejection::Busy,
            }
        })
    }
    /// Opens or activates the one path-bound Prefab lane owned by this Editor session.
    #[doc(hidden)]
    pub(super) fn open_prefab_source(
        &mut self,
        document: EditorDocumentId,
        base_revision: nara_scene::SceneAuthoringRevision,
        entity: &SceneEntityId,
    ) -> EditorWorkspaceCommandReport {
        if self.pending_persistence.is_some() {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.document-open-busy",
            );
        }
        let source_binding = match self.prefab_source_binding(document, base_revision, entity) {
            Ok(source_binding) => source_binding,
            Err(code) => {
                return editor_workspace_command_rejected(self.workspace.active_document(), code);
            }
        };
        if let Some(document) = self.workspace.document_for_source_binding(&source_binding) {
            return self.workspace.activate_document(document);
        }
        if self.prefab_lane.is_some() {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-replacement-requires-close",
            );
        }
        if self.authoring_project().is_some() {
            return self.open_authoring_prefab_source(source_binding);
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
        let project_path = self
            .trusted_compiled()
            .map(|lane| lane.content_candidate.settings().paths.prefabs.as_str())
            .map_or_else(
                || Arc::from(source_binding.locator()),
                |root| Arc::from(format!("{root}/{}", source_binding.locator())),
            );
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
        self.prefab_lane = Some(EditorPrefabPersistenceLane {
            project_path,
            document,
            host,
        });
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
        let project_path: Arc<str> = Arc::from(format!(
            "{}/{}",
            prefab_directory.as_str(),
            source_binding.locator()
        ));
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
        self.prefab_lane = Some(EditorPrefabPersistenceLane {
            project_path,
            document,
            host,
        });
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
        let resolved_source;
        let source = match &instance.source {
            AssetRef::Path(source) => source,
            reference @ AssetRef::StableId(_) => {
                resolved_source = match &self.execution {
                    EditorExecutionLane::Authoring(snapshot) => snapshot
                        .asset_database()
                        .resolve_ref(reference)
                        .ok()
                        .map(|record| record.path().clone()),
                    EditorExecutionLane::TrustedCompiled(lane) => {
                        lane.snapshot.resolved_asset_path(reference).cloned()
                    }
                }
                .ok_or("project.editor.prefab-source-stable-id-unresolved")?;
                &resolved_source
            }
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
        self.scene_lane
            .as_mut()
            .expect("the persistence test requires an open Scene lane")
            .host
            .test_reject_next_valid_receipt();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.scene_lane
            .as_mut()
            .expect("the persistence test requires an open Scene lane")
            .host
            .test_fail_next_save(stage);
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save_after_write(&mut self) {
        self.scene_lane
            .as_mut()
            .expect("the test requires an open Scene lane")
            .host
            .test_fail_after_write();
    }

    #[cfg(test)]
    pub(crate) fn test_allow_save_cleanup(&mut self) {
        self.scene_lane
            .as_mut()
            .expect("the test requires an open Scene lane")
            .host
            .test_allow_cleanup();
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
    /// Immediate read-only Host state, including an admitted operation before frame publication.
    pub const fn persistence_view(&self) -> EditorPersistenceView {
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
            EditorPersistenceCommand::Reopen { document } => {
                let Some(document) = document.or(self.workspace.active_document()) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: None,
                        reason: EditorPersistenceRejection::NoActiveDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::NoActiveDocument,
                    );
                };
                (EditorPersistenceRequestKind::Reopen, document)
            }
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
                let authoring = match kind {
                    EditorDocumentKind::Scene => self
                        .workspace
                        .scene(requested_document)
                        .and_then(|slot| slot.session().authoring_readmission()),
                    EditorDocumentKind::Prefab => self
                        .workspace
                        .prefab(requested_document)
                        .and_then(|slot| slot.session().authoring_readmission()),
                };
                self.pending_persistence = Some(PendingPersistence::Save {
                    candidate: match kind {
                        EditorDocumentKind::Scene => {
                            PendingSave::Scene(SceneSaveCandidate { checkpoint })
                        }
                        EditorDocumentKind::Prefab => {
                            PendingSave::Prefab(PrefabSaveCandidate { checkpoint })
                        }
                    },
                    authoring,
                });
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
        // A retry has its own eventual result. The previous terminal must not drive
        // a Workspace close decision while this operation is still pending.
        self.persistence_result = None;
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
                if self.pending_document_open.is_some()
                    && matches!(pending, PendingWorkspaceIntent::CloseDocument(_))
                {
                    self.cancel_pending_document_open();
                    self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Cancelled {
                        intent: pending.intent(),
                    });
                    return;
                }
                pending.set_phase(EditorWorkspaceIntentPhase::AwaitingDecision);
            }
            Some(EditorPersistenceResult::Opened { .. })
            | Some(EditorPersistenceResult::Saved { .. }) => {
                pending.set_phase(EditorWorkspaceIntentPhase::AwaitingDecision);
            }
            None => {}
        }
        if pending.phase() == EditorWorkspaceIntentPhase::AwaitingDecision {
            self.refresh_workspace_close_prompt(&mut pending);
        }
        self.pending_workspace_intent = Some(pending);
    }

    fn acknowledge_save(
        &mut self,
        lane: PersistenceLane,
        document: EditorDocumentId,
        refresh_runtime_content: bool,
        outcome: PersistenceSaveOutcome,
    ) -> EditorPersistenceResult {
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
                self.refresh_current_saved_source_digest();
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
            PersistenceLane::Scene => {
                if let Some(lane) = self.scene_lane.as_mut() {
                    lane.host.mark_uncertain();
                }
            }
            PersistenceLane::Prefab => {
                if let Some(lane) = self.prefab_lane.as_mut() {
                    lane.host.mark_uncertain();
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
        let Some(base_revision) = base_revision else {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::MissingDocument,
            };
        };
        let Some(lane) = self.scene_lane.as_ref() else {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::MissingDocument,
            };
        };
        if lane.document != document {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            };
        }
        let Some(slot) = self.workspace.scene(document) else {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::MissingDocument,
            };
        };
        let reopened = if slot.authoring_readiness().is_some() {
            lane.host.reopen_authoring(slot.session())
        } else {
            match &self.execution {
                EditorExecutionLane::TrustedCompiled(runtime) => lane
                    .host
                    .reopen_native(runtime.plan.schema_validation().registry()),
                EditorExecutionLane::Authoring(_) => {
                    SceneReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
                }
            }
        };
        match reopened {
            SceneReopenOutcome::Opened(opened) => {
                let OpenedSceneReopen {
                    session,
                    digest,
                    identity,
                    content_digest,
                } = *opened;
                let report = match self.persistence_authority.publish_reopened_session(
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
                };
                let revision = report
                    .revision
                    .expect("a reopened document reports its authoring revision");
                self.scene_lane
                    .as_mut()
                    .expect("the exact Scene lane was verified before reopen")
                    .host
                    .commit_reopen(identity, content_digest);
                EditorPersistenceResult::Opened {
                    document,
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
        let Some(lane) = self.prefab_lane.as_ref() else {
            return EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage: EditorPersistenceFailureStage::OpenTarget,
            };
        };
        if lane.document != document {
            return EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            };
        }
        let host = &lane.host;
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
                if let Some(lane) = self.prefab_lane.as_mut() {
                    lane.host.commit_reopen(identity, content_digest);
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
            document: target,
            reason: EditorPersistenceRejection::RequiresReconcile,
            ..
        } => request == EditorPersistenceRequestKind::Reopen && target == Some(document),
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
