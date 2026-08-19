use super::*;

impl EditorProjectSession {
    #[must_use]
    #[doc(hidden)]
    pub(crate) fn play_view(&self) -> EditorPlayView {
        let current_revision = self
            .workspace
            .active_scene()
            .map(nara_tooling::EditorSceneSlot::revision);
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayView::new(
                EditorPlayState::Empty,
                None,
                None,
                current_revision,
                false,
                self.play_result,
            );
        };
        let (state, source_revision) = match &lane.runtime_owner {
            EditorRuntimeOwnerState::Empty => (EditorPlayState::Empty, None),
            EditorRuntimeOwnerState::Preparing { revision, .. } => {
                (EditorPlayState::PreparingPlay, Some(*revision))
            }
            EditorRuntimeOwnerState::Starting { revision, .. } => {
                (EditorPlayState::Starting, Some(*revision))
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget { revision, .. }
            | EditorRuntimeOwnerState::CommittingGameTarget { revision, .. } => {
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
                let state = match lane.runtime_host.running_runtime_state() {
                    Some(RuntimeState::Running) => EditorPlayState::Running,
                    Some(RuntimeState::Paused) => EditorPlayState::Paused,
                    Some(RuntimeState::Stepping) => EditorPlayState::Stepping,
                    Some(RuntimeState::Faulted) => EditorPlayState::Faulted,
                    Some(RuntimeState::Stopping) => EditorPlayState::Stopping,
                    Some(RuntimeState::CloseIncomplete) => EditorPlayState::CloseIncomplete,
                    Some(RuntimeState::Stopped) | None => EditorPlayState::Empty,
                };
                let projected =
                    lane.pending_control
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
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                let projected = if lane.pending_step_presentation.is_some() {
                    EditorPlayState::Stepping
                } else {
                    projected
                };
                (projected, Some(*source_revision))
            }
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("editor runtime state is not observed during a transition")
            }
        };
        let running_content_revision = lane.runtime_host.running_content_revision();
        let out_of_date = running_content_revision.is_some()
            && (source_revision
                .zip(current_revision)
                .is_some_and(|(source, current)| source != current)
                || running_content_revision != Some(lane.snapshot.revision())
                || !self.play_source_is_current());
        EditorPlayView::new(
            state,
            lane.runtime_host.running_runtime_generation(),
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
        if self.trusted_compiled().is_none() {
            let reason = EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            );
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation: play_operation(command),
                reason,
            });
            return EditorPlayRequestResult::Rejected(reason);
        }
        if let Some(pending) = self.pending_workspace_intent.as_ref() {
            let retry_matches_owner = matches!(
                (pending.phase(), command, self.play_view().state()),
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

        if matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::Restart
        ) && !self.play_source_is_current()
        {
            let operation = match command {
                EditorPlayCommand::Play => EditorPlayOperation::Play,
                EditorPlayCommand::Restart => EditorPlayOperation::Restart,
                _ => unreachable!(),
            };
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeOutOfDate);
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
                let Some(lane) = self.trusted_compiled_mut() else {
                    return EditorPlayRequestResult::Rejected(
                        EditorPlayRejection::RuntimeUnavailable(
                            EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                        ),
                    );
                };
                let state = std::mem::replace(
                    &mut lane.runtime_owner,
                    EditorRuntimeOwnerState::Transitioning,
                );
                match state {
                    EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                        lane.runtime_owner = EditorRuntimeOwnerState::RetiringPlay { terminal };
                        self.play_result = Some(EditorPlayOperationResult::Pending {
                            operation: EditorPlayOperation::RetryRetirement,
                        });
                        EditorPlayRequestResult::Accepted
                    }
                    other => {
                        lane.runtime_owner = other;
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
        if self.trusted_compiled().is_none() {
            let reason = EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            );
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation: EditorPlayOperation::Play,
                reason,
            });
            return EditorPlayRequestResult::Rejected(reason);
        }
        if self.pending_workspace_intent.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
            || lane.pending_control.is_some()
        {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        if !self.play_source_is_current() {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation: EditorPlayOperation::Play,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeOutOfDate);
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
        let Some(lane) = self.trusted_compiled_mut() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        lane.runtime_owner = EditorRuntimeOwnerState::Preparing {
            document,
            revision,
            operation: EditorPlayOperation::Play,
        };
        self.play_result = Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Play,
        });
        EditorPlayRequestResult::Accepted
    }

    pub(super) fn play_source_is_current(&self) -> bool {
        self.project_content.is_current() && !self.workspace.has_dirty_prefab()
    }
}

fn play_operation(command: EditorPlayCommand) -> EditorPlayOperation {
    match command {
        EditorPlayCommand::Play => EditorPlayOperation::Play,
        EditorPlayCommand::Cancel => EditorPlayOperation::Cancel,
        EditorPlayCommand::Pause => EditorPlayOperation::Pause,
        EditorPlayCommand::Resume => EditorPlayOperation::Resume,
        EditorPlayCommand::StepFixedTick => EditorPlayOperation::StepFixedTick,
        EditorPlayCommand::Stop => EditorPlayOperation::Stop,
        EditorPlayCommand::Restart => EditorPlayOperation::Restart,
        EditorPlayCommand::RetryClose => EditorPlayOperation::RetryClose,
        EditorPlayCommand::RetryRetirement => EditorPlayOperation::RetryRetirement,
        EditorPlayCommand::AcknowledgeResult => {
            unreachable!("acknowledgement is handled before operation projection")
        }
    }
}
