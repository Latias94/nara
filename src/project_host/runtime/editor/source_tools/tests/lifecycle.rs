use nara_scene::{SceneComponentRecord, SceneDocument};
use nara_tooling::EditorWorkspaceCommand;

use super::*;
use crate::project_host::EditorProjectCloseProgress;
use crate::project_host::runtime::editor::authoring_runtime::EditorAuthoringRuntimeCloseProgress;

#[test]
fn reviewed_attach_publishes_once_through_the_real_document_history() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let before = snapshot(editor);
        let persisted = project.persisted();
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let review = editor.source_tool_review().unwrap();
        assert_eq!(review.request(), &request);
        assert_eq!(review.title(), "Set the finite payload");
        assert!(review.created_entity().is_none());
        assert!(!review.patch().operations.is_empty());
        assert!(!review.inverse().operations.is_empty());
        assert_eq!(snapshot(editor), before);
        assert_eq!(project.persisted(), persisted);

        let committed = editor.accept_source_tool_review(&request).unwrap();
        assert!(committed.report.applied);
        assert!(committed.created_entity.is_none());
        assert!(editor.source_tool_review().is_none());
        let expected =
            SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(target())
                .with_component(
                    component(),
                    SceneComponentRecord::new(
                        nara_reflect::ComponentSchemaVersion::ONE,
                        ComponentValue::map([("amount", ComponentValue::I64(7))]),
                    )
                    .with_owner(project.owner.owner_id().clone()),
                )]);
        assert_eq!(snapshot(editor).source, expected);
        let accepted = snapshot(editor);
        assert_eq!(accepted.history.undo_depth, before.history.undo_depth + 1);
        assert_eq!(accepted.history.redo_depth, 0);
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::Busy),
        ));
        assert_eq!(snapshot(editor), accepted);
        let document = document(editor);
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision,
                })
                .applied
        );
        assert_eq!(snapshot(editor).source, before.source);
        assert_eq!(
            snapshot(editor).history.undo_depth,
            before.history.undo_depth
        );
        assert_eq!(snapshot(editor).history.redo_depth, 1);
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Redo {
                    document,
                    base_revision,
                })
                .applied
        );
        assert_eq!(snapshot(editor).source, expected);
        assert_eq!(project.persisted(), persisted);
    });
}

#[test]
fn integer_property_actions_use_the_admitted_parameter_and_publish_the_exact_review_origin() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorSourceToolAction,
        EditorSourceToolActionOutcome,
    };

    let project = Project::new();
    project.run(|editor| {
        let original = activate(editor, &project);
        let attached = attach(editor, &original);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        editor.accept_source_tool_review(&attached).unwrap();
        editor
            .enable_source_tool(project.tool_with(
                project.facet_description(),
                Output::IntegerProposal,
                Duration::from_secs(10),
            ))
            .unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let activation = frame.source_tool().active.as_ref().unwrap().id().clone();
        let document = document(editor);
        let basis = editor.document_query_basis(document).unwrap();
        let target = EditorPropertyTarget {
            basis: basis.clone(),
            entity: target(),
            component: component(),
            property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
        };
        assert!(
            frame
                .source_tool()
                .active
                .as_ref()
                .unwrap()
                .matches_integer_property(&target)
        );
        let before = snapshot(editor);
        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::SourceTool(
                    EditorSourceToolAction::IntegerProperty {
                        activation: activation.clone(),
                        target,
                        value: 5,
                    },
                )],
            ))
            .unwrap();
        let [
            EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Requested(Ok(request))),
        ] = report.outcomes()
        else {
            panic!("the integer property action did not reach the production owner: {report:?}");
        };
        let request = request.clone();
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let reviewed_frame = editor.current_frame();
        let summary = reviewed_frame.source_tool().review.as_ref().unwrap();
        assert_eq!(summary.request, request);
        assert_eq!(summary.basis, basis);
        assert_eq!(
            summary.intent,
            EditorSourceToolIntent::Edit {
                entity: fixture::target()
            }
        );
        assert_eq!(summary.title.as_ref(), "Apply the admitted bonus");
        let review = editor.source_tool_review().unwrap();
        assert_eq!(summary.operations, review.patch().operations.len());
        assert_eq!(summary.created_entity.as_ref(), review.created_entity());
        assert_eq!(snapshot(editor), before);
        let next_frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(next_frame.source_tool(), reviewed_frame.source_tool());
        assert!(Arc::ptr_eq(
            &summary.title,
            &next_frame.source_tool().review.as_ref().unwrap().title
        ));

        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::SourceTool(EditorSourceToolAction::Accept {
                    request: request.clone(),
                })],
            ))
            .unwrap();
        let [
            EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Accepted(Ok(committed))),
        ] = report.outcomes()
        else {
            panic!("the retained review did not commit through its ordinary action: {report:?}");
        };
        assert!(committed.report.applied);
        let accepted = snapshot(editor);
        let entity = accepted
            .source
            .entities
            .iter()
            .find(|entity| entity.id == fixture::target())
            .unwrap();
        assert_eq!(
            entity.components[&component()]
                .value
                .field_i64("amount")
                .unwrap(),
            12
        );
        assert_eq!(accepted.history.undo_depth, before.history.undo_depth + 1);
        let disabled = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::SourceTool(EditorSourceToolAction::Disable {
                    activation,
                })],
            ))
            .unwrap();
        assert!(matches!(
            disabled.outcomes(),
            [EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Disabled(Ok(()))
            )]
        ));
        assert!(editor.source_tool_view().active.is_none());
    });
}

#[test]
fn superseded_request_keeps_physical_custody_without_overwriting_its_successor() {
    for point in [GatePoint::BeforeSpawn, GatePoint::BeforeIoReturn] {
        let project = Project::new();
        project.run(|editor| {
            let activation = activate(editor, &project);
            let before = snapshot(editor);
            let held = HeldProcess::arm(editor, point);
            let first = attach(editor, &activation);
            let returned = editor
                .source_tools
                .invocation
                .as_ref()
                .unwrap()
                .task
                .return_observer();
            held.wait_until_entered();
            let basis = editor.document_query_basis(document(editor)).unwrap();
            let second = held
                .poll(|| {
                    editor.request_source_tool(
                        &activation,
                        &basis,
                        EditorSourceToolIntent::Spawn {
                            name: "Successor only".into(),
                        },
                        BTreeMap::new(),
                    )
                })
                .unwrap();
            assert_ne!(first, second);
            let retiring = editor.source_tools.retiring.last_mut().unwrap();
            assert!(retiring.is_finished());
            assert!(matches!(
                retiring.try_take(),
                Some(TaskTerminal::Cancelled(_))
            ));
            held.assert_pending(&returned);

            // The other IO worker can finish the successor while the predecessor stays held.
            wait(editor, |editor| editor.source_tool_view().review.is_some());
            let review = editor.source_tool_review().unwrap();
            assert_eq!(review.request(), &second);
            let created = review.created_entity().unwrap().clone();
            let patch = review.patch().clone();
            assert_eq!(snapshot(editor), before);
            assert!(matches!(
                editor.accept_source_tool_review(&first),
                Err(EditorSourceToolError::StaleRequest),
            ));
            assert_eq!(editor.source_tool_review().unwrap().patch(), &patch);
            held.assert_pending(&returned);
            let committed = held
                .poll(|| editor.accept_source_tool_review(&second))
                .unwrap();
            assert!(committed.report.applied);
            assert_eq!(committed.created_entity, Some(created.clone()));
            let accepted = snapshot(editor);
            assert_eq!(accepted.source.entities.len(), 2);
            let original = accepted
                .source
                .entities
                .iter()
                .find(|entity| entity.id == target())
                .unwrap();
            assert!(!original.components.contains_key(&component()));
            let successor = accepted
                .source
                .entities
                .iter()
                .find(|entity| entity.id == created)
                .unwrap();
            assert_eq!(
                successor.components[&ComponentTypeId::new("nara.scene.Name")].value,
                ComponentValue::String("Successor only".into()),
            );
            for _ in 0..4 {
                held.poll(|| editor.drive_editor_frame(Duration::ZERO));
                held.assert_pending(&returned);
                assert!(editor.source_tool_view().retiring);
                assert_eq!(snapshot(editor), accepted);
            }
            held.release();
            wait(editor, |editor| !editor.source_tool_view().retiring);
            assert_eq!(returned.state(), TaskReturnState::Returned);
            assert_eq!(snapshot(editor), accepted);
            assert_eq!(editor.source_tool_view().request.as_ref(), Some(&second));
            assert!(editor.source_tool_view().request_error.is_none());
            assert!(editor.source_tool_review().is_none());
        });
    }
}

#[test]
fn disable_revokes_execution_immediately_but_retains_held_work_across_reenable() {
    for point in [GatePoint::BeforeSpawn, GatePoint::BeforeIoReturn] {
        let project = Project::new();
        project.run(|editor| {
            let activation = activate(editor, &project);
            let before = snapshot(editor);
            let held = HeldProcess::arm(editor, point);
            let first = attach(editor, &activation);
            let returned = editor
                .source_tools
                .invocation
                .as_ref()
                .unwrap()
                .task
                .return_observer();
            held.wait_until_entered();
            held.poll(|| editor.disable_source_tool(&activation))
                .unwrap();
            let view = editor.source_tool_view();
            assert!(
                view.active.is_none() && !view.working && !view.probing && view.review.is_none()
            );
            assert!(view.retiring);
            assert!(view.request.is_none());
            let basis = editor.document_query_basis(document(editor)).unwrap();
            assert!(matches!(
                editor.request_source_tool(
                    &activation,
                    &basis,
                    EditorSourceToolIntent::Attach { entity: target() },
                    BTreeMap::new(),
                ),
                Err(EditorSourceToolError::Disabled),
            ));
            assert!(matches!(
                editor.accept_source_tool_review(&first),
                Err(EditorSourceToolError::Disabled),
            ));
            assert!(matches!(
                editor.source_tools.retiring.last_mut().unwrap().try_take(),
                Some(TaskTerminal::Cancelled(_)),
            ));
            held.assert_pending(&returned);
            assert_eq!(snapshot(editor), before);

            let activation = activate(editor, &project);
            let second = attach(editor, &activation);
            assert_ne!(first, second);
            wait(editor, |editor| editor.source_tool_view().review.is_some());
            assert_eq!(editor.source_tool_review().unwrap().request(), &second);
            assert!(matches!(
                editor.accept_source_tool_review(&first),
                Err(EditorSourceToolError::StaleRequest),
            ));
            assert!(
                editor
                    .accept_source_tool_review(&second)
                    .unwrap()
                    .report
                    .applied
            );
            let accepted = snapshot(editor);
            for poll in 0..4 {
                let disabled = held.poll(|| editor.disable_source_tool(&activation));
                assert_eq!(
                    disabled,
                    if poll == 0 {
                        Ok(())
                    } else {
                        Err(EditorSourceToolError::Disabled)
                    }
                );
                held.assert_pending(&returned);
                assert!(editor.source_tool_view().retiring);
            }
            held.release();
            wait(editor, |editor| !editor.source_tool_view().retiring);
            assert_eq!(returned.state(), TaskReturnState::Returned);
            assert_eq!(snapshot(editor), accepted);
            assert!(editor.source_tool_view().active.is_none());
            assert!(editor.source_tool_review().is_none());
        });
    }
}

#[test]
fn close_stays_physically_incomplete_for_held_probe_and_invocation() {
    for point in [GatePoint::BeforeSpawn, GatePoint::BeforeIoReturn] {
        for probe in [false, true] {
            let project = Project::new();
            project.run(|editor| {
                let activation = (!probe).then(|| activate(editor, &project));
                let before = snapshot(editor);
                let persisted = project.persisted();
                let held = HeldProcess::arm(editor, point);
                let fallback_activation = EditorSourceToolActivationId::issue();
                let request = if probe {
                    editor.enable_source_tool(project.tool()).unwrap();
                    None
                } else {
                    Some(attach(editor, activation.as_ref().unwrap()))
                };
                let returned = if probe {
                    editor
                        .source_tools
                        .probe
                        .as_ref()
                        .unwrap()
                        .task
                        .return_observer()
                } else {
                    editor
                        .source_tools
                        .invocation
                        .as_ref()
                        .unwrap()
                        .task
                        .return_observer()
                };
                held.wait_until_entered();
                for _ in 0..4 {
                    let progress = held.poll(|| editor.advance_close());
                    assert!(matches!(
                        progress,
                        EditorProjectCloseProgress::Incomplete { .. }
                    ));
                    assert!(!progress.task_shutdown().is_complete());
                    assert!(!progress.task_shutdown().has_cleanup_failure());
                    let view = editor.source_tool_view();
                    assert!(
                        view.active.is_none()
                            && !view.working
                            && !view.probing
                            && view.review.is_none()
                    );
                    assert!(view.retiring);
                    held.assert_pending(&returned);
                    assert_eq!(snapshot(editor), before);
                }
                assert!(matches!(
                    editor.source_tools.retiring.last_mut().unwrap().try_take(),
                    Some(TaskTerminal::Cancelled(_)),
                ));
                assert!(matches!(
                    held.poll(|| editor.enable_source_tool(project.tool())),
                    Err(EditorSourceToolError::Closing),
                ));
                let basis = editor.document_query_basis(document(editor)).unwrap();
                assert!(matches!(
                    held.poll(|| editor.request_source_tool(
                        activation.as_ref().unwrap_or(&fallback_activation),
                        &basis,
                        EditorSourceToolIntent::Attach { entity: target() },
                        BTreeMap::new(),
                    )),
                    Err(EditorSourceToolError::Closing),
                ));
                if let Some(request) = request {
                    assert!(matches!(
                        editor.accept_source_tool_review(&request),
                        Err(EditorSourceToolError::Closing),
                    ));
                }
                assert_eq!(project.persisted(), persisted);
                held.release();
                let deadline = std::time::Instant::now() + WAIT;
                let receipt = loop {
                    if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
                        break receipt;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "released child did not close"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                };
                assert_eq!(returned.state(), TaskReturnState::Returned);
                assert!(receipt.task_shutdown().is_complete());
                assert!(!receipt.has_cleanup_failure());
                let closed = editor.source_tool_view();
                assert!(
                    closed.active.is_none()
                        && !closed.probing
                        && !closed.working
                        && !closed.retiring
                );
                assert_eq!(editor.source_tools.jobs(), 0);
                assert_eq!(snapshot(editor), before);
                assert_eq!(project.persisted(), persisted);
                assert!(editor.source_tool_review().is_none());
                assert_eq!(
                    editor.advance_close(),
                    EditorProjectCloseProgress::Complete(receipt)
                );
            });
        }
    }
}

#[test]
fn physical_return_during_tasks_close_is_reconciled_before_the_complete_receipt() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let before = snapshot(editor);
        let held = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        attach(editor, &activation);
        let returned = editor
            .source_tools
            .invocation
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        held.wait_until_entered();
        assert!(matches!(
            held.poll(|| editor.advance_close()),
            EditorProjectCloseProgress::Incomplete { .. }
        ));
        held.assert_pending(&returned);
        let observed_return = returned.clone();
        editor.next_close_poll_gate = Some(Box::new(move |editor| {
            // The source-tool poll already observed Pending. Complete the physical owner in
            // this exact window, before advance_close reads the Tasks completion receipt.
            assert!(editor.source_tool_view().retiring);
            held.release();
            let deadline = std::time::Instant::now() + WAIT;
            loop {
                if matches!(
                    editor.authoring_runtime.shutdown_tasks(),
                    EditorAuthoringRuntimeCloseProgress::Complete(_)
                ) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "released tool did not physically close"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(observed_return.state(), TaskReturnState::Returned);
            assert!(editor.source_tool_view().retiring);
        }));
        let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() else {
            panic!("the completed Tasks owner must close in this poll");
        };
        assert!(receipt.task_shutdown().is_complete());
        assert!(!receipt.has_cleanup_failure());
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert!(!editor.source_tool_view().retiring);
        assert_eq!(editor.source_tools.jobs(), 0);
        assert_eq!(snapshot(editor), before);
    });
}

#[test]
fn disable_discards_a_ready_review_without_publishing_it() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let before = snapshot(editor);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        editor.disable_source_tool(&activation).unwrap();
        assert!(editor.source_tool_view().active.is_none());
        assert!(editor.source_tool_view().review.is_none());
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::Disabled),
        ));
        assert_eq!(snapshot(editor), before);
    });
}

#[test]
fn cancel_of_a_held_probe_cannot_late_activate_over_a_new_review() {
    for point in [GatePoint::BeforeSpawn, GatePoint::BeforeIoReturn] {
        let project = Project::new();
        project.run(|editor| {
            let before = snapshot(editor);
            let held = HeldProcess::arm(editor, point);
            let old_attempt = editor.enable_source_tool(project.tool()).unwrap();
            let returned = editor
                .source_tools
                .probe
                .as_ref()
                .unwrap()
                .task
                .return_observer();
            held.wait_until_entered();
            held.poll(|| editor.cancel_source_tool_attempt(&old_attempt))
                .unwrap();
            assert!(!editor.source_tool_view().probing);
            assert!(editor.source_tool_view().active.is_none());
            held.assert_pending(&returned);
            let activation = activate(editor, &project);
            let new_attempt = editor.source_tool_view().attempt.unwrap();
            assert_ne!(old_attempt, new_attempt);
            let request = attach(editor, &activation);
            wait(editor, |editor| editor.source_tool_view().review.is_some());
            let reviewed = editor.source_tool_review().unwrap().patch().clone();
            held.release();
            wait(editor, |editor| !editor.source_tool_view().retiring);
            assert_eq!(returned.state(), TaskReturnState::Returned);
            assert_eq!(
                editor.source_tool_view().attempt.as_ref(),
                Some(&new_attempt)
            );
            assert_eq!(editor.source_tool_review().unwrap().request(), &request);
            assert_eq!(editor.source_tool_review().unwrap().patch(), &reviewed);
            assert_eq!(snapshot(editor), before);
            assert!(
                editor
                    .accept_source_tool_review(&request)
                    .unwrap()
                    .report
                    .applied
            );
        });
    }
}

#[test]
fn an_old_attempt_cannot_cancel_a_new_probe_or_disable_its_active_predecessor() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorSourceToolAction,
        EditorSourceToolActionOutcome,
    };

    let project = Project::new();
    project.run(|editor| {
        let initial = HeldProcess::arm(editor, GatePoint::BeforeSpawn);
        let old_attempt = editor.enable_source_tool(project.tool()).unwrap();
        initial.wait_until_entered();
        initial
            .poll(|| editor.cancel_source_tool_attempt(&old_attempt))
            .unwrap();
        initial.release();
        wait(editor, |editor| !editor.source_tool_view().retiring);

        let activation = activate(editor, &project);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let patch = editor.source_tool_review().unwrap().patch().clone();
        let source = snapshot(editor);
        let held = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        let attempt = editor.enable_source_tool(project.tool()).unwrap();
        let returned = editor
            .source_tools
            .probe
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        held.wait_until_entered();
        let before = editor.source_tool_view();
        let frame = editor.current_frame();
        let rejected = held
            .poll(|| {
                editor.dispatch_editor_actions(EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::SourceTool(
                        EditorSourceToolAction::CancelAttempt {
                            attempt: old_attempt.clone(),
                        },
                    )],
                ))
            })
            .unwrap();
        assert!(matches!(
            rejected.outcomes(),
            [EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Cancelled(Err(EditorSourceToolError::StaleAttempt))
            )],
        ));
        assert_eq!(editor.source_tool_view(), before);
        held.assert_pending(&returned);
        let cancelled = held
            .poll(|| {
                editor.dispatch_editor_actions(EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::SourceTool(
                        EditorSourceToolAction::CancelAttempt {
                            attempt: attempt.clone(),
                        },
                    )],
                ))
            })
            .unwrap();
        assert!(matches!(
            cancelled.outcomes(),
            [EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Cancelled(Ok(()))
            )],
        ));
        let view = editor.source_tool_view();
        assert_eq!(view.active.as_ref().unwrap().id(), &activation);
        assert_eq!(view.attempt.as_ref(), Some(&attempt));
        assert_eq!(
            view.attempt_error,
            Some(EditorSourceToolError::Process("cancelled"))
        );
        assert_eq!(view.request.as_ref(), Some(&request));
        assert!(view.review.is_some() && view.retiring && !view.probing);
        assert!(view.request_error.is_none());
        assert_eq!(editor.source_tool_review().unwrap().patch(), &patch);
        assert_eq!(snapshot(editor), source);
        held.assert_pending(&returned);
        held.release();
        wait(editor, |editor| !editor.source_tool_view().retiring);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert!(matches!(
            editor.cancel_source_tool_attempt(&attempt),
            Err(EditorSourceToolError::StaleAttempt),
        ));
        assert_eq!(editor.source_tool_review().unwrap().request(), &request);
        assert!(
            editor
                .accept_source_tool_review(&request)
                .unwrap()
                .report
                .applied
        );
    });
}

#[test]
fn two_held_retirements_bound_admission_until_actual_return() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let before = snapshot(editor);
        let first = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        attach(editor, &activation);
        let first_returned = editor
            .source_tools
            .invocation
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        first.wait_until_entered();
        let second = HeldProcess::arm(editor, GatePoint::BeforeSpawn);
        attach(editor, &activation);
        let second_returned = editor
            .source_tools
            .invocation
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        second.wait_until_entered();
        let basis = editor.document_query_basis(document(editor)).unwrap();
        assert!(matches!(
            second.poll(|| editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Attach { entity: target() },
                BTreeMap::new(),
            )),
            Err(EditorSourceToolError::Busy),
        ));
        second
            .poll(|| editor.disable_source_tool(&activation))
            .unwrap();
        assert_eq!(editor.source_tools.retiring.len(), 2);
        assert!(matches!(
            second.poll(|| editor.enable_source_tool(project.tool())),
            Err(EditorSourceToolError::Busy),
        ));
        first.assert_pending(&first_returned);
        second.assert_pending(&second_returned);
        assert_eq!(snapshot(editor), before);
        first.release();
        wait(editor, |editor| editor.source_tools.retiring.len() == 1);
        assert_eq!(first_returned.state(), TaskReturnState::Returned);
        second.assert_pending(&second_returned);
        let _activation = activate(editor, &project);
        assert!(editor.source_tool_view().active.is_some());
        second.release();
        wait(editor, |editor| !editor.source_tool_view().retiring);
        assert_eq!(second_returned.state(), TaskReturnState::Returned);
        assert_eq!(snapshot(editor), before);
    });
}
