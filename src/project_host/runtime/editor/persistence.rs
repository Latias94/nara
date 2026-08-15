use super::*;

impl EditorProjectSession {
    #[cfg(test)]
    pub(crate) fn test_reject_next_valid_persistence_receipt(&mut self) {
        self.persistence.test_reject_next_valid_receipt();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.persistence.test_fail_next_save(stage);
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

        if matches!(command, EditorPersistenceCommand::Reopen { .. })
            && (!matches!(self.runtime_owner, EditorRuntimeOwnerState::Empty)
                || self.runtime_host.has_cleanup_owner())
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
                let Some(slot) = self.workspace.scene(requested_document) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::MissingDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::MissingDocument,
                    );
                };
                if !slot.is_dirty() {
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
                self.pending_persistence =
                    Some(PendingPersistence::Save(SceneSaveCandidate { checkpoint }));
                self.persistence_operation = EditorPersistenceOperation::Saving {
                    document: requested_document,
                    captured_revision: revision,
                };
            }
            EditorPersistenceCommand::Reopen { .. } => {
                let base_revision = self
                    .workspace
                    .scene(requested_document)
                    .map(|slot| slot.revision());
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
        let Some(pending) = self.pending_workspace_intent else {
            return;
        };
        if pending.phase != EditorWorkspaceIntentPhase::Saving {
            return;
        }
        match self.persistence_result {
            Some(EditorPersistenceResult::Saved { document, .. })
                if document == pending.document =>
            {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::RetiringRuntime,
                    ..pending
                });
            }
            Some(
                EditorPersistenceResult::Rejected { .. }
                | EditorPersistenceResult::Failed { .. }
                | EditorPersistenceResult::PersistenceUncertain { .. },
            ) => {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::AwaitingDecision,
                    ..pending
                });
            }
            Some(EditorPersistenceResult::Opened { .. })
            | Some(EditorPersistenceResult::Saved { .. }) => {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::AwaitingDecision,
                    ..pending
                });
            }
            None => {}
        }
    }

    pub(super) fn complete_save(&mut self, save: SceneSaveCandidate) -> EditorPersistenceResult {
        let document = save.checkpoint.document();
        match self.persistence.save(save) {
            SceneSaveOutcome::Saved { commit, evidence } => {
                let report = self
                    .persistence_authority
                    .commit(&mut self.workspace, commit);
                if !report.applied {
                    self.persistence.mark_uncertain();
                    self.last_persistence_receipt = Some(evidence);
                    return EditorPersistenceResult::PersistenceUncertain {
                        document: evidence.document(),
                        revision: evidence.revision(),
                        digest: evidence.digest(),
                    };
                }
                self.last_persistence_receipt = Some(evidence);
                EditorPersistenceResult::Saved {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
            SceneSaveOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason,
            },
            SceneSaveOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                stage,
            },
            SceneSaveOutcome::PersistenceUncertain { evidence } => {
                self.last_persistence_receipt = Some(evidence);
                EditorPersistenceResult::PersistenceUncertain {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
        }
    }

    pub(super) fn complete_reopen(
        &mut self,
        document: EditorDocumentId,
        base_revision: Option<nara_scene::SceneAuthoringRevision>,
    ) -> EditorPersistenceResult {
        match self
            .persistence
            .reopen(self.plan.schema_validation().registry())
        {
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
