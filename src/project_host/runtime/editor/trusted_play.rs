use super::*;

impl EditorProjectSession {
    pub(super) fn request_start(
        &mut self,
        operation: EditorPlayOperation,
    ) -> EditorPlayRequestResult {
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
            ));
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
            || lane.pending_control.is_some()
        {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        let Some(document) = self.workspace.active_document() else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument);
        };
        let Some(slot) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument);
        };
        let revision = slot.revision();
        let Some(lane) = self.trusted_compiled_mut() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
            ));
        };
        lane.runtime_owner = EditorRuntimeOwnerState::Preparing {
            document,
            revision,
            operation,
        };
        self.play_result = Some(EditorPlayOperationResult::Pending { operation });
        EditorPlayRequestResult::Accepted
    }

    pub(super) fn request_start_cancel(&mut self) -> EditorPlayRequestResult {
        let Some(state) = self.take_trusted_runtime_owner() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
            ));
        };
        match state {
            EditorRuntimeOwnerState::Preparing { operation, .. } => {
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::Empty);
                self.play_result = Some(EditorPlayOperationResult::Cancelled { operation });
                EditorPlayRequestResult::Accepted
            }
            EditorRuntimeOwnerState::Starting {
                operation, attempt, ..
            } => {
                let _ =
                    self.install_trusted_runtime_owner(EditorRuntimeOwnerState::CancellingStart {
                        terminal: EditorPlayOperationResult::Cancelled { operation },
                        attempt,
                    });
                self.play_result = Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::Cancel,
                });
                EditorPlayRequestResult::Accepted
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                operation,
                prepared,
                ..
            } => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.runtime_host.abort_prepared_start(prepared);
                }
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::RetiringPlay {
                    terminal: EditorPlayOperationResult::Cancelled { operation },
                });
                self.play_result = Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::Cancel,
                });
                EditorPlayRequestResult::Accepted
            }
            other => {
                let _ = self.install_trusted_runtime_owner(other);
                EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
            }
        }
    }

    pub(super) fn request_active_control(
        &mut self,
        operation: EditorPlayOperation,
        control: RuntimeControl,
    ) -> EditorPlayRequestResult {
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
            ));
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Active { .. }) {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState);
        }
        let retires_runtime = matches!(
            operation,
            EditorPlayOperation::Stop | EditorPlayOperation::Restart
        );
        if lane.pending_apply_changes.is_some() && !retires_runtime {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        if lane.pending_control.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        let mut game_capture_suspended = false;
        #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
        let game_capture_suspended = false;
        if retires_runtime {
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            if matches!(
                self.trusted_compiled().map(|lane| &lane.runtime_owner),
                Some(EditorRuntimeOwnerState::Active { game: Some(_), .. })
            ) {
                if self
                    .set_game_view_source_active(false, "the Play Runtime is retiring")
                    .is_err()
                {
                    return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
                }
                game_capture_suspended = true;
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            if self.game_view_retirement_blocked() {
                if game_capture_suspended {
                    let _ = self.set_game_view_source_active(
                        true,
                        "the blocked Play retirement did not supersede Game View",
                    );
                }
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
            }
            self.cancel_pending_runtime_edit();
            self.cancel_pending_apply_changes();
        }
        let requested = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.request_runtime_control(control),
            None => {
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                    EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                ));
            }
        };
        match requested {
            Ok(ticket) => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_control = Some(PendingEditorControl { operation, ticket });
                }
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                EditorPlayRequestResult::Accepted
            }
            Err(fault) => {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                if game_capture_suspended {
                    let _ = self.set_game_view_source_active(
                        true,
                        "the rejected Play retirement did not supersede Game View",
                    );
                }
                #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
                let _ = game_capture_suspended;
                let _ = self.diagnostics.extend(fault.diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::InvalidState,
                });
                EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
            }
        }
    }

    pub(super) fn drive_play_owner(&mut self, real_delta: Duration) {
        let Some(state) = self.take_trusted_runtime_owner() else {
            return;
        };
        let next = match state {
            EditorRuntimeOwnerState::Empty => EditorRuntimeOwnerState::Empty,
            EditorRuntimeOwnerState::Preparing {
                document,
                revision,
                operation,
            } => self.drive_preparing(document, revision, operation),
            EditorRuntimeOwnerState::Starting {
                document,
                revision,
                operation,
                attempt,
            } => self.drive_starting(document, revision, operation, attempt),
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            } => EditorRuntimeOwnerState::AwaitingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            },
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::CommittingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            } => EditorRuntimeOwnerState::CommittingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            },
            EditorRuntimeOwnerState::CancellingStart { terminal, attempt } => {
                self.drive_cancelled_start(terminal, attempt)
            }
            EditorRuntimeOwnerState::RetiringPlay { terminal } => {
                self.drive_start_retirement(terminal)
            }
            EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
            EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            } => self.drive_active(
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
                real_delta,
            ),
            EditorRuntimeOwnerState::Transitioning => EditorRuntimeOwnerState::Transitioning,
        };
        let _ = self.install_trusted_runtime_owner(next);
    }

    fn drive_preparing(
        &mut self,
        document: EditorDocumentId,
        requested_revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
    ) -> EditorRuntimeOwnerState {
        if !self.play_source_is_current() {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorRuntimeOwnerState::Empty;
        }
        let Some(slot) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        };
        let revision = slot.revision();
        if revision != requested_revision {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::StaleDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        }
        let expanded = match self.trusted_compiled().map(|lane| {
            lane.snapshot.prepare_editor_startup_scene(
                slot.session().document(),
                lane.plan.schema_validation().registry(),
            )
        }) {
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
            Some(Ok(expanded)) => expanded,
            Some(Err(error)) => {
                let _ = self.diagnostics.extend(error.diagnostics().clone());
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Preparation,
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        let started = match self.trusted_compiled_mut() {
            Some(lane) => lane
                .runtime_host
                .begin_editor_start(
                    lane.snapshot.clone(),
                    lane.plan.clone(),
                    expanded,
                    Vec::new(),
                )
                .map(|mut attempt| {
                    let inputs = attempt
                        .inputs
                        .as_mut()
                        .expect("a fresh Editor start owns its inputs");
                    inputs.scene_preparation = Some((
                        lane.content_candidate.clone(),
                        Arc::clone(&lane.content_loader),
                    ));
                    attempt
                }),
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        match started {
            Ok(attempt) => EditorRuntimeOwnerState::Starting {
                document,
                revision,
                operation,
                attempt,
            },
            Err(fault) => {
                let _ = self.diagnostics.extend(fault.diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Preparation,
                });
                EditorRuntimeOwnerState::Empty
            }
        }
    }

    fn drive_starting(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
        mut attempt: super::super::RuntimeStartAttempt,
    ) -> EditorRuntimeOwnerState {
        let prepared = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.prepare_start(&mut attempt),
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        match prepared {
            Ok(prepared) => {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                if let Some(config) = self
                    .trusted_compiled()
                    .and_then(|lane| lane.game_view_config)
                {
                    let request = game_view::start_request(config, document, revision, &prepared);
                    return EditorRuntimeOwnerState::AwaitingGameTarget {
                        document,
                        revision,
                        operation,
                        request,
                        prepared,
                    };
                }

                let (diagnostics, generation) = match self.trusted_compiled_mut() {
                    Some(lane) => {
                        let diagnostics = lane.runtime_host.commit_prepared_start(prepared);
                        let generation = lane.runtime_host.running_runtime_generation();
                        (diagnostics, generation)
                    }
                    None => {
                        self.play_result = Some(EditorPlayOperationResult::Rejected {
                            operation,
                            reason: EditorPlayRejection::RuntimeUnavailable(
                                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                            ),
                        });
                        return EditorRuntimeOwnerState::Empty;
                    }
                };
                let _ = self.diagnostics.extend(diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Applied {
                    operation,
                    generation,
                });
                EditorRuntimeOwnerState::Active {
                    document,
                    source_revision: revision,
                    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                    game: None,
                }
            }
            Err(fault) => {
                let _ = self.diagnostics.extend(fault.diagnostics);
                let terminal = EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Start,
                };
                if self
                    .trusted_compiled()
                    .is_some_and(|lane| lane.runtime_host.has_cleanup_owner())
                {
                    self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                    EditorRuntimeOwnerState::RetiringPlay { terminal }
                } else {
                    self.play_result = Some(terminal);
                    EditorRuntimeOwnerState::Empty
                }
            }
        }
    }

    fn drive_start_retirement(
        &mut self,
        terminal: EditorPlayOperationResult,
    ) -> EditorRuntimeOwnerState {
        let Some(outcome) = self
            .trusted_compiled_mut()
            .map(|lane| lane.runtime_host.drive_cleanup_once())
        else {
            return EditorRuntimeOwnerState::Empty;
        };
        match outcome {
            super::super::CleanupDriveOutcome::Complete {
                failed,
                diagnostics,
            } => {
                let _ = self.diagnostics.extend(diagnostics);
                let cleanup_replaces_terminal =
                    failed && !matches!(terminal, EditorPlayOperationResult::Failed { .. });
                self.play_result = Some(if cleanup_replaces_terminal {
                    EditorPlayOperationResult::Failed {
                        operation: EditorPlayOperation::RetryRetirement,
                        failure: EditorPlayFailure::Retirement,
                    }
                } else {
                    terminal
                });
                EditorRuntimeOwnerState::Empty
            }
            super::super::CleanupDriveOutcome::Retiring => {
                EditorRuntimeOwnerState::RetiringPlay { terminal }
            }
            super::super::CleanupDriveOutcome::RetirementIncomplete => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
        }
    }

    fn drive_cancelled_start(
        &mut self,
        terminal: EditorPlayOperationResult,
        attempt: super::super::RuntimeStartAttempt,
    ) -> EditorRuntimeOwnerState {
        drop(attempt);
        self.play_result = Some(terminal);
        EditorRuntimeOwnerState::Empty
    }

    fn drive_active(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))] game: Option<
            ActiveEditorGameView,
        >,
        real_delta: Duration,
    ) -> EditorRuntimeOwnerState {
        let driven = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.drive_running_runtime(real_delta),
            None => return EditorRuntimeOwnerState::Empty,
        };
        if let Err(fault) = driven {
            let _ = self.diagnostics.extend(fault.diagnostics);
            self.cancel_pending_runtime_edit_without_port();
            let pending = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.pending_control.take());
            if let Some(pending) = pending {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Runtime,
                });
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            };
        }

        self.poll_runtime_edit();
        self.complete_pending_apply_changes(document, source_revision);

        let runtime_state = self
            .trusted_compiled()
            .and_then(|lane| lane.runtime_host.running_runtime_state());
        let (close_failed, close_diagnostics) =
            self.trusted_compiled_mut().map_or((false, None), |lane| {
                let failed = lane
                    .runtime_host
                    .running_runtime_has_terminal_close_failure();
                let generation = lane.runtime_host.running_runtime_generation();
                if failed && generation != lane.reported_close_failure_generation {
                    lane.reported_close_failure_generation = generation;
                    (
                        failed,
                        lane.runtime_host.running_runtime_close_failure_report(),
                    )
                } else {
                    (failed, None)
                }
            });
        // Shutdown is terminal even while a separate physical close participant waits.
        // Publish once for this generation, rather than on every frame or retry.
        if let Some(diagnostics) = close_diagnostics {
            self.diagnostics.extend(diagnostics);
        }
        if runtime_state == Some(RuntimeState::Stopped) {
            let (pending, control_failed, generation) = {
                let Some(lane) = self.trusted_compiled_mut() else {
                    return EditorRuntimeOwnerState::Empty;
                };
                let pending = lane.pending_control.take();
                let control_failed = pending.is_some_and(|pending| {
                    matches!(
                        lane.runtime_host.runtime_control_status(pending.ticket),
                        Some(RuntimeControlStatus::Failed(_))
                    )
                });
                let generation = lane.runtime_host.running_runtime_generation();
                let released = lane.runtime_host.release_stopped_runtime();
                debug_assert!(released);
                (pending, control_failed, generation)
            };
            let operation = pending.map_or(EditorPlayOperation::Stop, |pending| pending.operation);
            if control_failed || close_failed {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_step_presentation = None;
                    }
                }
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Close,
                });
                return EditorRuntimeOwnerState::Empty;
            }
            if operation == EditorPlayOperation::Restart {
                let Some(slot) = self.workspace.scene(document) else {
                    self.play_result = Some(EditorPlayOperationResult::Rejected {
                        operation,
                        reason: EditorPlayRejection::NoActiveDocument,
                    });
                    return EditorRuntimeOwnerState::Empty;
                };
                let revision = slot.revision();
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                return EditorRuntimeOwnerState::Preparing {
                    document,
                    revision,
                    operation,
                };
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            self.play_result = Some(EditorPlayOperationResult::Applied {
                operation,
                generation,
            });
            return EditorRuntimeOwnerState::Empty;
        }

        if runtime_state == Some(RuntimeState::CloseIncomplete) {
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            let pending = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.pending_control.take());
            if let Some(pending) = pending {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Close,
                });
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            };
        }

        let pending = self
            .trusted_compiled()
            .and_then(|lane| lane.pending_control);
        if let Some(pending) = pending {
            let (status, generation) = self.trusted_compiled().map_or((None, None), |lane| {
                (
                    lane.runtime_host.runtime_control_status(pending.ticket),
                    lane.runtime_host.running_runtime_generation(),
                )
            });
            match status {
                Some(RuntimeControlStatus::Applied) => {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_control = None;
                    }
                    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                    let requires_presentation =
                        pending.operation == EditorPlayOperation::StepFixedTick && game.is_some();
                    #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
                    let requires_presentation = false;
                    if requires_presentation {
                        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                        {
                            if let Some(lane) = self.trusted_compiled_mut() {
                                lane.pending_step_presentation = generation.map(|generation| {
                                    EditorStepPresentationState::AwaitingCandidate { generation }
                                });
                            }
                        }
                    } else {
                        self.play_result = Some(EditorPlayOperationResult::Applied {
                            operation: pending.operation,
                            generation,
                        });
                    }
                }
                Some(RuntimeControlStatus::Failed(_)) | None => {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_control = None;
                    }
                    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                    {
                        if let Some(lane) = self.trusted_compiled_mut() {
                            lane.pending_step_presentation = None;
                        }
                    }
                    self.play_result = Some(EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure: EditorPlayFailure::Runtime,
                    });
                }
                Some(RuntimeControlStatus::Pending) => {}
            }
        }
        EditorRuntimeOwnerState::Active {
            document,
            source_revision,
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            game,
        }
    }

    fn poll_runtime_edit(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        if !matches!(
            lane.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) || !matches!(
            lane.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return;
        }
        let Some(runtime) = lane.runtime_host.running_runtime_mut() else {
            return;
        };
        let Ok(nara_app::RuntimeScopeOutcome {
            output: Ok(EditorRuntimeBridgeOutput::Result(Some(result))),
            ..
        }) = runtime.with_driver_scope(|scope| {
            scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::TakeResult)
        })
        else {
            return;
        };
        lane.runtime_edit_result = Some(result);
    }

    pub(super) fn cancel_pending_runtime_edit(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        if !matches!(
            lane.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) {
            return;
        }
        let result = lane.runtime_host.running_runtime_mut().and_then(|runtime| {
            runtime
                .with_driver_scope(|scope| {
                    scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::Cancel)
                })
                .ok()
                .and_then(|outcome| outcome.output.ok())
        });
        match result {
            Some(EditorRuntimeBridgeOutput::Result(Some(result))) => {
                lane.runtime_edit_result = Some(result);
            }
            _ => self.cancel_pending_runtime_edit_without_port(),
        }
    }

    fn cancel_pending_runtime_edit_without_port(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        lane.runtime_edit_result = lane.runtime_edit_result.take().map(|result| match result {
            EditorRuntimeEditResult::Pending(request) => {
                EditorRuntimeEditResult::Cancelled(request)
            }
            terminal => terminal,
        });
    }

    fn complete_pending_apply_changes(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
    ) {
        let Some(pending) = self
            .trusted_compiled_mut()
            .and_then(|lane| lane.pending_apply_changes.take())
        else {
            return;
        };
        if !self.accepts_new_work() {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::Busy,
                    report: None,
                });
            }
            return;
        }
        let running_generation = self
            .trusted_compiled()
            .and_then(|lane| lane.runtime_host.running_runtime_generation());
        if pending.document != document
            || pending.source_revision != source_revision
            || running_generation != Some(pending.generation)
        {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleGeneration,
                    report: None,
                });
            }
            return;
        }
        let Some(slot) = self.workspace.scene(document) else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: None,
                });
            }
            return;
        };
        if slot.revision() != pending.document_revision {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: None,
                });
            }
            return;
        }
        let request = pending.request;
        let exported = self.trusted_compiled().map(|lane| {
            lane.runtime_host.with_running_world(|world| {
                __export_apply_changes_from_world(
                    world,
                    slot.session(),
                    lane.plan.schema_validation().registry(),
                    source_revision,
                    request.clone(),
                )
            })
        });
        let Some(exported) = exported else {
            return;
        };
        let Ok(mut report) = exported else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::InvalidRuntimeState,
                    report: None,
                });
            }
            return;
        };
        let Some(patch) = report.patch.clone() else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::Unsupported,
                    report: Some(report),
                });
            }
            return;
        };
        let patch_report = self.apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision: pending.document_revision,
            patch,
        });
        let Some(scene_patch_report) = patch_report.patch_report else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: Some(report),
                });
            }
            return;
        };
        report.applied = scene_patch_report.applied;
        report.current_revision = patch_report.revision.unwrap_or(report.current_revision);
        report.patch_report = Some(scene_patch_report.clone());
        let _ = report.diagnostics.extend(scene_patch_report.diagnostics);
        for component in &mut report.components {
            if component.status == SceneApplyChangesComponentStatus::Pending {
                component.status = if report.applied {
                    SceneApplyChangesComponentStatus::Applied
                } else {
                    SceneApplyChangesComponentStatus::Rejected
                };
            }
        }
        if let Some(lane) = self.trusted_compiled_mut() {
            lane.apply_changes_result = Some(if report.applied {
                EditorApplyChangesResult::Applied(report)
            } else {
                EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: Some(report),
                }
            });
        }
    }

    pub(super) fn cancel_pending_apply_changes(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        let Some(pending) = lane.pending_apply_changes.take() else {
            return;
        };
        lane.apply_changes_result = Some(EditorApplyChangesResult::Cancelled(pending.request));
    }
}
