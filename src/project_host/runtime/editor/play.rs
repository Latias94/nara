use super::*;

impl EditorProjectSession {
    #[must_use]
    #[doc(hidden)]
    pub(crate) fn play_view(&self) -> EditorPlayView {
        let current_revision = self
            .workspace
            .active_scene()
            .map(nara_tooling::EditorSceneSlot::revision);
        let (state, source_revision) = match &self.runtime_owner {
            EditorRuntimeOwnerState::Empty => (EditorPlayState::Empty, None),
            EditorRuntimeOwnerState::Preparing { revision, .. } => {
                (EditorPlayState::PreparingPlay, Some(*revision))
            }
            EditorRuntimeOwnerState::Starting { revision, .. } => {
                (EditorPlayState::Starting, Some(*revision))
            }
            EditorRuntimeOwnerState::CancellingStart { .. }
            | EditorRuntimeOwnerState::RetiringPlay { .. } => (EditorPlayState::RetiringPlay, None),
            EditorRuntimeOwnerState::RetirementIncomplete { .. } => {
                (EditorPlayState::RetirementIncomplete, None)
            }
            EditorRuntimeOwnerState::Active {
                source_revision, ..
            } => {
                let state = match self.runtime_host.running_runtime_state() {
                    Some(RuntimeState::Running) => EditorPlayState::Running,
                    Some(RuntimeState::Paused) => EditorPlayState::Paused,
                    Some(RuntimeState::Stepping) => EditorPlayState::Stepping,
                    Some(RuntimeState::Faulted) => EditorPlayState::Faulted,
                    Some(RuntimeState::Stopping) => EditorPlayState::Stopping,
                    Some(RuntimeState::CloseIncomplete) => EditorPlayState::CloseIncomplete,
                    Some(RuntimeState::Stopped) | None => EditorPlayState::Empty,
                };
                let projected =
                    self.pending_control
                        .map_or(state, |pending| match pending.operation {
                            EditorPlayOperation::StepFixedTick => EditorPlayState::Stepping,
                            EditorPlayOperation::Stop
                            | EditorPlayOperation::Restart
                            | EditorPlayOperation::RetryClose => EditorPlayState::Stopping,
                            EditorPlayOperation::Play
                            | EditorPlayOperation::Cancel
                            | EditorPlayOperation::Pause
                            | EditorPlayOperation::Resume
                            | EditorPlayOperation::RetryRetirement => state,
                        });
                (projected, Some(*source_revision))
            }
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("editor runtime state is not observed during a transition")
            }
        };
        let out_of_date = source_revision
            .zip(current_revision)
            .is_some_and(|(source, current)| source != current);
        EditorPlayView::new(
            state,
            self.runtime_host.running_runtime_generation(),
            source_revision,
            current_revision,
            out_of_date,
            self.play_result,
        )
    }

    pub(crate) fn request_play(&mut self, command: EditorPlayCommand) -> EditorPlayRequestResult {
        if command == EditorPlayCommand::AcknowledgeResult {
            return if self.play_result.take().is_some() {
                EditorPlayRequestResult::Accepted
            } else {
                EditorPlayRequestResult::Rejected(EditorPlayRejection::ResultPending)
            };
        }
        if let Some(pending) = self.pending_workspace_intent {
            let retry_matches_owner = matches!(
                (pending.phase, command, self.play_view().state()),
                (
                    EditorWorkspaceIntentPhase::RetiringRuntime,
                    EditorPlayCommand::RetryRetirement,
                    EditorPlayState::RetirementIncomplete,
                ) | (
                    EditorWorkspaceIntentPhase::RetiringRuntime,
                    EditorPlayCommand::RetryClose,
                    EditorPlayState::CloseIncomplete,
                )
            );
            if !retry_matches_owner {
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
            }
        }

        match command {
            EditorPlayCommand::Play => self.request_start(EditorPlayOperation::Play),
            EditorPlayCommand::Cancel => self.request_start_cancel(),
            EditorPlayCommand::Pause => {
                self.request_active_control(EditorPlayOperation::Pause, RuntimeControl::Pause)
            }
            EditorPlayCommand::Resume => {
                self.request_active_control(EditorPlayOperation::Resume, RuntimeControl::Resume)
            }
            EditorPlayCommand::StepFixedTick => self.request_active_control(
                EditorPlayOperation::StepFixedTick,
                RuntimeControl::StepFixedTick,
            ),
            EditorPlayCommand::Stop => {
                self.request_active_control(EditorPlayOperation::Stop, RuntimeControl::Stop)
            }
            EditorPlayCommand::Restart => {
                self.request_active_control(EditorPlayOperation::Restart, RuntimeControl::Stop)
            }
            EditorPlayCommand::RetryClose => self.request_active_control(
                EditorPlayOperation::RetryClose,
                RuntimeControl::RetryClose,
            ),
            EditorPlayCommand::RetryRetirement => {
                let state = std::mem::replace(
                    &mut self.runtime_owner,
                    EditorRuntimeOwnerState::Transitioning,
                );
                match state {
                    EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                        self.runtime_owner = EditorRuntimeOwnerState::RetiringPlay { terminal };
                        self.play_result = Some(EditorPlayOperationResult::Pending {
                            operation: EditorPlayOperation::RetryRetirement,
                        });
                        EditorPlayRequestResult::Accepted
                    }
                    other => {
                        self.runtime_owner = other;
                        EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
                    }
                }
            }
            EditorPlayCommand::AcknowledgeResult => unreachable!(),
        }
    }

    pub(super) fn request_play_from_frame(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
    ) -> EditorPlayRequestResult {
        if self.pending_workspace_intent.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        if !matches!(self.runtime_owner, EditorRuntimeOwnerState::Empty)
            || self.pending_control.is_some()
        {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        let Some(scene) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation: EditorPlayOperation::Play,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument);
        };
        if scene.revision() != revision {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation: EditorPlayOperation::Play,
                reason: EditorPlayRejection::StaleDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::StaleDocument);
        }
        self.runtime_owner = EditorRuntimeOwnerState::Preparing {
            document,
            revision,
            operation: EditorPlayOperation::Play,
        };
        self.play_result = Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Play,
        });
        EditorPlayRequestResult::Accepted
    }
}
