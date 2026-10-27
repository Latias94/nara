use nara_reflect::PersistentComponentSchemaOwnerId;
use nara_tooling::EditorDocumentQueryRejection;
use serde_json::json;

use super::*;

#[test]
fn a_dirty_close_prompt_allows_exact_revocation_while_retaining_physical_custody() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision,
        EditorSourceToolAction, EditorSourceToolActionOutcome, EditorWorkspaceIntent,
        EditorWorkspaceIntentPhase, EditorWorkspaceIntentRequestResult,
    };

    let project = Project::new();
    project.run(|editor| {
        let document = document(editor);
        let basis = editor.document_query_basis(document).unwrap();
        let mut draft = editor.begin_authoring_construction(&basis).unwrap();
        draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let prepared = editor
            .prepare_authoring_construction(&basis, &draft)
            .unwrap();
        assert!(
            editor
                .commit_authoring_construction(prepared)
                .unwrap()
                .report()
                .applied
        );
        assert_eq!(editor.workspace.document_is_dirty(document), Some(true));

        let activation = activate(editor, &project);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let retained = editor.source_tool_view();
        let patch = editor.source_tool_review().unwrap().patch().clone();
        let source = snapshot(editor);
        let persisted = project.persisted();
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
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document }),
            EditorWorkspaceIntentRequestResult::Accepted,
        );
        assert_eq!(
            editor.workspace_intent_view().phase(),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision),
        );
        let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
        let frame = editor.current_frame();
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
        let cancelled = editor.source_tool_view();
        assert_eq!(cancelled.active, retained.active);
        assert_eq!(cancelled.request.as_ref(), Some(&request));
        assert_eq!(cancelled.review, retained.review);
        assert_eq!(cancelled.attempt.as_ref(), Some(&attempt));
        assert_eq!(
            cancelled.attempt_error,
            Some(EditorSourceToolError::Process("cancelled"))
        );
        assert!(cancelled.request_error.is_none());
        assert!(!cancelled.probing && cancelled.retiring);
        assert_eq!(editor.source_tool_review().unwrap().patch(), &patch);
        held.assert_pending(&returned);

        let basis = editor.document_query_basis(document).unwrap();
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Attach { entity: target() },
                BTreeMap::new(),
            ),
            Err(EditorSourceToolError::Closing)
        ));
        assert!(matches!(
            editor.request_source_tool_integer_property(
                &activation,
                EditorPropertyTarget {
                    basis: basis.clone(),
                    entity: target(),
                    component: component(),
                    property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                },
                5,
            ),
            Err(EditorSourceToolError::Closing)
        ));
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::Closing)
        ));
        assert_eq!(editor.source_tool_view(), cancelled);

        let disabled = held
            .poll(|| {
                editor.dispatch_editor_actions(EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::SourceTool(EditorSourceToolAction::Disable {
                        activation: activation.clone(),
                    })],
                ))
            })
            .unwrap();
        assert!(matches!(
            disabled.outcomes(),
            [EditorActionOutcome::SourceTool(
                EditorSourceToolActionOutcome::Disabled(Ok(()))
            )],
        ));
        let disabled = editor.source_tool_view();
        assert!(
            disabled.active.is_none() && disabled.request.is_none() && disabled.review.is_none()
        );
        assert!(disabled.retiring && !disabled.probing && !disabled.working);
        assert!(editor.source_tool_review().is_none());
        assert_eq!(snapshot(editor), source);
        assert_eq!(project.persisted(), persisted);
        assert_eq!(
            editor.workspace_intent_view().decision_prompt(),
            Some(prompt)
        );
        held.assert_pending(&returned);

        assert_eq!(
            editor.resolve_workspace_intent(prompt, EditorCloseDecision::Cancel),
            EditorWorkspaceIntentRequestResult::Accepted,
        );
        let successor = HeldProcess::arm(editor, GatePoint::BeforeSpawn);
        let successor_attempt = editor.enable_source_tool(project.tool()).unwrap();
        successor.wait_until_entered();
        let pending = editor.source_tool_view();
        assert_ne!(attempt, successor_attempt);
        assert!(matches!(
            successor.poll(|| editor.cancel_source_tool_attempt(&attempt)),
            Err(EditorSourceToolError::StaleAttempt),
        ));
        assert_eq!(editor.source_tool_view(), pending);
        held.assert_pending(&returned);
        successor.release();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let successor_activation = editor.source_tool_view().active.unwrap().id().clone();
        let successor_request = attach(editor, &successor_activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let current = editor.source_tool_view();
        assert!(matches!(
            editor.disable_source_tool(&activation),
            Err(EditorSourceToolError::StaleActivation)
        ));
        assert!(matches!(
            editor.cancel_source_tool_attempt(&attempt),
            Err(EditorSourceToolError::StaleAttempt)
        ));
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::StaleRequest)
        ));
        assert_eq!(editor.source_tool_view(), current);
        assert_eq!(
            editor.source_tool_review().unwrap().request(),
            &successor_request
        );
        held.assert_pending(&returned);
        held.release();
        wait(editor, |editor| !editor.source_tool_view().retiring);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert_eq!(
            editor.source_tool_review().unwrap().request(),
            &successor_request
        );
        assert_eq!(snapshot(editor), source);
        assert_eq!(project.persisted(), persisted);
    });
}

#[test]
fn stale_activation_cannot_revoke_or_replace_a_successor_review() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorSourceToolAction,
        EditorSourceToolActionOutcome,
    };

    let project = Project::new();
    project.run(|editor| {
        let predecessor = activate(editor, &project);
        let predecessor_basis = editor.document_query_basis(document(editor)).unwrap();
        let submitted = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                editor.current_frame().revision(),
                [EditorAction::SourceTool(EditorSourceToolAction::Request {
                    activation: predecessor.clone(),
                    basis: predecessor_basis.clone(),
                    intent: EditorSourceToolIntent::Attach { entity: target() },
                    parameters: BTreeMap::new(),
                })],
            ))
            .unwrap();
        let [
            EditorActionOutcome::SourceTool(EditorSourceToolActionOutcome::Requested(Ok(
                predecessor_request,
            ))),
        ] = submitted.outcomes()
        else {
            panic!("the ordinary request action was not routed: {submitted:?}");
        };
        let predecessor_request = predecessor_request.clone();
        wait(editor, |editor| editor.source_tool_view().review.is_some());

        editor.enable_source_tool(project.tool()).unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let successor = editor.source_tool_view().active.unwrap().id().clone();
        assert_ne!(predecessor, successor);
        let successor_request = attach(editor, &successor);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let successor_patch = editor.source_tool_review().unwrap().patch().clone();
        let before = editor.source_tool_view();
        let sequence = editor.source_tools.sequence;

        assert!(matches!(
            editor.request_source_tool(
                &predecessor,
                &predecessor_basis,
                EditorSourceToolIntent::Attach { entity: target() },
                BTreeMap::new(),
            ),
            Err(EditorSourceToolError::StaleActivation),
        ));
        assert!(matches!(
            editor.disable_source_tool(&predecessor),
            Err(EditorSourceToolError::StaleActivation),
        ));
        assert_eq!(editor.source_tool_view(), before);
        assert_eq!(editor.source_tools.sequence, sequence);
        assert_eq!(
            editor.source_tool_view().request.as_ref(),
            Some(&successor_request)
        );
        assert_eq!(
            editor.source_tool_review().unwrap().patch(),
            &successor_patch
        );
        assert!(matches!(
            editor.accept_source_tool_review(&predecessor_request),
            Err(EditorSourceToolError::StaleRequest),
        ));
        assert_eq!(
            editor.source_tool_review().unwrap().patch(),
            &successor_patch
        );
    });
}

#[test]
fn a_nonmatching_integer_property_target_falls_back_without_starting_work() {
    let project = Project::new();
    project.run(|editor| {
        editor
            .enable_source_tool(project.tool_with(
                project.facet_description(),
                proposal(),
                Duration::from_secs(10),
            ))
            .unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let activation = editor.source_tool_view().active.unwrap().id().clone();
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let basis = editor.document_query_basis(document(editor)).unwrap();
        let before = editor.source_tool_view();
        let target = EditorPropertyTarget {
            basis,
            entity: target(),
            component: component(),
            property: ComponentPropertyPath::new(ComponentFieldId::new("armor")),
        };
        assert!(matches!(
            editor.request_source_tool_integer_property(&activation, target, 3),
            Err(EditorSourceToolError::PropertyFacet),
        ));
        let after = editor.source_tool_view();
        assert_eq!(after.active.as_ref().unwrap().id(), &activation);
        assert_eq!(after.request, before.request);
        assert_eq!(after.review, before.review);
        assert_eq!(after.request.as_ref(), Some(&request));
        assert!(!after.working);
        assert!(editor.source_tools.invocation.is_none());
    });
}

#[test]
fn invalid_integer_property_declarations_preserve_the_active_predecessor_and_review() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let before = editor.source_tool_view();
        let source = snapshot(editor);
        for integer_property in [
            json!({"field": "missing", "parameter": "bonus", "label": "Bonus", "initial": 0}),
            json!({"field": "amount", "parameter": "", "label": "Bonus", "initial": 0}),
            json!({"field": "amount", "parameter": "bonus", "label": "\n", "initial": 0}),
        ] {
            let description = Output::Result(json!({
                "kind": "description",
                "catalog": catalog_json(project.owner.current(), project.owner.predecessor()),
                "integer_property": integer_property,
            }));
            let attempt = editor
                .enable_source_tool(project.tool_with(
                    description,
                    proposal(),
                    Duration::from_secs(10),
                ))
                .unwrap();
            wait(editor, |editor| !editor.source_tool_view().probing);
            let view = editor.source_tool_view();
            assert_eq!(view.attempt.as_ref(), Some(&attempt));
            assert_eq!(view.attempt_error, Some(EditorSourceToolError::Declaration));
            assert_eq!(view.active, before.active);
            assert_eq!(view.request.as_ref(), Some(&request));
            assert_eq!(view.review, before.review);
            assert!(view.request_error.is_none());
            assert_eq!(snapshot(editor), source);
        }
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
fn a_successful_probe_blocked_by_a_dirty_close_prompt_retains_the_predecessor_review() {
    use nara_tooling::{
        EditorCloseDecision, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRequestResult,
    };

    let project = Project::new();
    project.run(|editor| {
        let document = document(editor);
        let basis = editor.document_query_basis(document).unwrap();
        let mut draft = editor.begin_authoring_construction(&basis).unwrap();
        draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let prepared = editor
            .prepare_authoring_construction(&basis, &draft)
            .unwrap();
        assert!(
            editor
                .commit_authoring_construction(prepared)
                .unwrap()
                .report()
                .applied
        );
        assert_eq!(editor.workspace.document_is_dirty(document), Some(true));

        let activation = activate(editor, &project);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let review = editor.source_tool_review().unwrap();
        let patch = review.patch().clone();
        let inverse = review.inverse().clone();
        let before = snapshot(editor);
        let persisted = project.persisted();
        let held = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        let attempt = editor
            .enable_source_tool(project.tool_with(
                project.description(),
                Output::Nonzero,
                Duration::from_secs(10),
            ))
            .unwrap();
        let returned = editor
            .source_tools
            .probe
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        held.wait_until_entered();
        assert_eq!(
            held.poll(|| editor
                .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document },)),
            EditorWorkspaceIntentRequestResult::Accepted,
        );
        assert_eq!(
            editor.workspace_intent_view().phase(),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision),
        );
        let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
        held.poll(|| editor.drive_editor_frame(Duration::ZERO));
        held.assert_pending(&returned);
        assert!(editor.source_tool_view().probing);
        assert_eq!(snapshot(editor), before);

        held.release();
        wait(editor, |editor| !editor.source_tool_view().probing);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        let view = editor.source_tool_view();
        assert_eq!(view.attempt.as_ref(), Some(&attempt));
        assert_eq!(view.attempt_error, Some(EditorSourceToolError::Closing));
        assert!(view.active.is_some() && view.review.is_some());
        assert!(!view.working && !view.retiring);
        assert_eq!(view.request.as_ref(), Some(&request));
        assert!(view.request_error.is_none());
        let review = editor.source_tool_review().unwrap();
        assert_eq!(review.request(), &request);
        assert_eq!(review.patch(), &patch);
        assert_eq!(review.inverse(), &inverse);
        assert_eq!(
            editor.workspace_intent_view().decision_prompt(),
            Some(prompt)
        );
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::Closing),
        ));
        assert_eq!(editor.source_tool_review().unwrap().patch(), &patch);
        assert_eq!(snapshot(editor), before);
        assert_eq!(project.persisted(), persisted);

        assert_eq!(
            editor.resolve_workspace_intent(prompt, EditorCloseDecision::Cancel),
            EditorWorkspaceIntentRequestResult::Accepted,
        );
        assert!(editor.workspace_intent_view().decision_prompt().is_none());
        let committed = editor.accept_source_tool_review(&request).unwrap();
        assert!(committed.report.applied);
        assert!(committed.created_entity.is_none());
        let accepted = snapshot(editor);
        assert_eq!(accepted.history.undo_depth, before.history.undo_depth + 1);
        let entity = accepted
            .source
            .entities
            .iter()
            .find(|entity| entity.id == target())
            .unwrap();
        assert_eq!(
            entity.components[&component()]
                .value
                .field_i64("amount")
                .unwrap(),
            7
        );
        assert_eq!(project.persisted(), persisted);
        assert_eq!(editor.source_tool_view().attempt.as_ref(), Some(&attempt));
        assert_eq!(
            editor.source_tool_view().attempt_error,
            Some(EditorSourceToolError::Closing)
        );
        assert!(editor.source_tool_review().is_none());
    });
}

#[test]
fn failed_replacement_keeps_the_old_activation_and_its_exact_review() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let patch = editor.source_tool_review().unwrap().patch().clone();
        let before = snapshot(editor);
        let cases = [
            (Output::Malformed, EditorSourceToolError::Protocol, Duration::from_secs(10)),
            (Output::StdoutLimit, EditorSourceToolError::Process("stdout-limit"), Duration::from_secs(10)),
            (Output::StderrLimit, EditorSourceToolError::Process("stderr-limit"), Duration::from_secs(10)),
            (Output::Nonzero, EditorSourceToolError::Process("nonzero-exit"), Duration::from_secs(10)),
            (
                Output::Result(json!({"kind": "description",
                    "catalog": catalog_json(&ComponentSchemaCatalog::default(), None)})),
                EditorSourceToolError::Declaration,
                Duration::from_secs(10),
            ),
            (
                Output::Wire(json!({"protocol_version": 1, "request": 0,
                    "result": {"kind": "description",
                        "catalog": catalog_json(project.owner.current(), project.owner.predecessor())}})),
                EditorSourceToolError::Protocol,
                Duration::from_secs(10),
            ),
            (
                Output::Wire(json!({"protocol_version": 2, "request": "__REQUEST__",
                    "result": {"kind": "description",
                        "catalog": catalog_json(project.owner.current(), project.owner.predecessor())}})),
                EditorSourceToolError::Protocol,
                Duration::from_secs(10),
            ),
            (Output::Timeout, EditorSourceToolError::Process("timed-out"), Duration::from_millis(100)),
        ];
        for (describe, expected, timeout) in cases {
            let attempt = editor.enable_source_tool(project.tool_with(
                describe, Output::Nonzero, timeout,
            )).unwrap();
            wait(editor, |editor| !editor.source_tool_view().probing);
            let view = editor.source_tool_view();
            assert_eq!(view.attempt.as_ref(), Some(&attempt));
            assert_eq!(view.attempt_error.as_ref(), Some(&expected));
            assert!(view.request_error.is_none());
            assert!(view.active.is_some() && view.review.is_some());
            assert_eq!(view.request.as_ref(), Some(&request));
            assert_eq!(editor.source_tool_review().unwrap().request(), &request);
            assert_eq!(editor.source_tool_review().unwrap().patch(), &patch);
            assert_eq!(snapshot(editor), before);
        }
        assert!(editor.accept_source_tool_review(&request).unwrap().report.applied);
        let basis = editor.document_query_basis(document(editor)).unwrap();
        let next_request = editor.request_source_tool(
            &activation,
            &basis,
            EditorSourceToolIntent::Spawn { name: "Old active route".into() },
            BTreeMap::new(),
        ).unwrap();
        wait(editor, |editor| !editor.source_tool_view().working);
        assert!(editor.source_tool_view().request_error.is_none(), "{:?}", editor.source_tool_view());
        assert_eq!(editor.source_tool_view().attempt_error, Some(EditorSourceToolError::Process("timed-out")));
        assert_eq!(editor.source_tool_review().unwrap().request(), &next_request);
        assert_eq!(editor.source_tool_review().unwrap().title(), "Set the finite payload");
    });
}

#[test]
fn child_process_and_protocol_failures_cannot_publish_partial_document_edits() {
    let project = Project::new();
    project.run(|editor| {
        let before = snapshot(editor);
        let persisted = project.persisted();
        let cases = [
            (Output::Malformed, EditorSourceToolError::Protocol),
            (Output::StdoutLimit, EditorSourceToolError::Process("stdout-limit")),
            (Output::StderrLimit, EditorSourceToolError::Process("stderr-limit")),
            (Output::Nonzero, EditorSourceToolError::Process("nonzero-exit")),
            (Output::ValueBudget, EditorSourceToolError::Limit),
            (
                project.description(),
                EditorSourceToolError::Protocol,
            ),
            (
                Output::Wire(json!({"protocol_version": 1, "request": 0,
                    "result": {"kind": "proposal", "title": "Wrong request", "writes": []}})),
                EditorSourceToolError::Protocol,
            ),
            (
                Output::Wire(json!({"protocol_version": 1, "request": "__REQUEST__",
                    "unexpected_authority": "target",
                    "result": {"kind": "proposal", "title": "Unknown envelope field", "writes": []}})),
                EditorSourceToolError::Protocol,
            ),
        ];
        for (invoke, expected) in cases {
            editor.enable_source_tool(project.tool_with(
                project.description(), invoke, Duration::from_secs(10),
            )).unwrap();
            wait(editor, |editor| !editor.source_tool_view().probing);
            let activation = editor.source_tool_view().active.unwrap().id().clone();
            let request = attach(editor, &activation);
            wait(editor, |editor| !editor.source_tool_view().working);
            let view = editor.source_tool_view();
            assert_eq!(view.request.as_ref(), Some(&request));
            assert_eq!(view.request_error.as_ref(), Some(&expected));
            assert!(view.attempt_error.is_none());
            assert!(view.active.is_some());
            assert!(view.review.is_none() && !view.retiring);
            assert!(editor.source_tool_review().is_none());
            assert_eq!(snapshot(editor), before);
            assert_eq!(project.persisted(), persisted);
        }
    });
}

#[test]
fn invalid_last_write_rejects_the_entire_proposal_before_publication() {
    let project = Project::new();
    project.run(|editor| {
        let invoke = Output::Result(json!({
            "kind": "proposal", "title": "Valid first, invalid last",
            "writes": [
                {"property": ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                 "value": ComponentValue::I64(7)},
                {"property": ComponentPropertyPath::new(ComponentFieldId::new("missing")),
                 "value": ComponentValue::I64(99)},
            ],
        }));
        editor
            .enable_source_tool(project.tool_with(
                project.description(),
                invoke,
                Duration::from_secs(10),
            ))
            .unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let activation = editor.source_tool_view().active.unwrap().id().clone();
        let before = snapshot(editor);
        let request = attach(editor, &activation);
        wait(editor, |editor| !editor.source_tool_view().working);
        assert!(
            matches!(
                editor.source_tool_view().request_error,
                Some(EditorSourceToolError::Preparation(_)),
            ),
            "{:?}",
            editor.source_tool_view()
        );
        assert!(editor.source_tool_view().review.is_none());
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::Busy),
        ));
        assert_eq!(snapshot(editor), before);
    });
}

#[test]
fn output_and_accept_both_reject_an_original_basis_after_document_revision_changes() {
    for after_review in [false, true] {
        let project = Project::new();
        project.run(|editor| {
            let activation = activate(editor, &project);
            let held = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
            let request = attach(editor, &activation);
            held.wait_until_entered();
            if after_review {
                held.release();
                wait(editor, |editor| editor.source_tool_view().review.is_some());
            }
            let document = document(editor);
            let basis = editor.document_query_basis(document).unwrap();
            let mut draft = editor.begin_authoring_construction(&basis).unwrap();
            draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .unwrap();
            let prepared = editor
                .prepare_authoring_construction(&basis, &draft)
                .unwrap();
            assert!(
                editor
                    .commit_authoring_construction(prepared)
                    .unwrap()
                    .report()
                    .applied
            );
            let changed = snapshot(editor);
            if after_review {
                assert!(matches!(
                    editor.accept_source_tool_review(&request),
                    Err(EditorSourceToolError::QueryBasis(
                        EditorDocumentQueryRejection::StaleRevision
                    )),
                ));
            } else {
                held.release();
                wait(editor, |editor| !editor.source_tool_view().working);
                assert!(matches!(
                    editor.source_tool_view().request_error,
                    Some(EditorSourceToolError::QueryBasis(
                        EditorDocumentQueryRejection::StaleRevision
                    )),
                ));
            }
            assert!(editor.source_tool_review().is_none());
            assert_eq!(snapshot(editor), changed);
            assert!(
                !changed
                    .source
                    .entities
                    .iter()
                    .any(|entity| entity.components.contains_key(&component()))
            );
        });
    }
}

#[test]
fn a_described_foreign_owner_cannot_execute_against_the_documents_selected_owner() {
    let project = Project::new();
    project.run(|editor| {
        let foreign = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("test.other-source-payload").unwrap(),
            project.owner.current().clone(),
            None,
        )
        .unwrap();
        let command = child::command(
            &project.root,
            Output::Result(json!({"kind": "description",
                "catalog": catalog_json(foreign.current(), foreign.predecessor())})),
            proposal(),
            Duration::from_secs(10),
        );
        let tool = EditorSourceToolDefinition::new(command, foreign, component()).unwrap();
        editor.enable_source_tool(tool).unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let activation = editor.source_tool_view().active.unwrap().id().clone();
        let before = snapshot(editor);
        let basis = editor.document_query_basis(document(editor)).unwrap();
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Attach { entity: target() },
                BTreeMap::new(),
            ),
            Err(EditorSourceToolError::Schema),
        ));
        assert!(!editor.source_tool_view().working);
        assert!(editor.source_tools.invocation.is_none());
        assert_eq!(snapshot(editor), before);
    });
}

#[test]
fn concurrent_probe_and_invocation_failures_keep_both_exact_observations() {
    let project = Project::new();
    project.run(|editor| {
        editor
            .enable_source_tool(project.tool_with(
                project.description(),
                Output::Malformed,
                Duration::from_secs(10),
            ))
            .unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let activation = editor.source_tool_view().active.unwrap().id().clone();
        let before = snapshot(editor);
        let invocation = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        let request = attach(editor, &activation);
        let request_returned = editor
            .source_tools
            .invocation
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        invocation.wait_until_entered();
        let probe = HeldProcess::arm(editor, GatePoint::BeforeIoReturn);
        let attempt = editor
            .enable_source_tool(project.tool_with(
                Output::Result(json!({"kind": "description",
                "catalog": catalog_json(&ComponentSchemaCatalog::default(), None)})),
                proposal(),
                Duration::from_secs(10),
            ))
            .unwrap();
        let attempt_returned = editor
            .source_tools
            .probe
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        probe.wait_until_entered();
        invocation.assert_pending(&request_returned);
        probe.assert_pending(&attempt_returned);
        let pending = editor.source_tool_view();
        assert!(pending.working && pending.probing && pending.active.is_some());
        assert_eq!(pending.request.as_ref(), Some(&request));
        assert_eq!(pending.attempt.as_ref(), Some(&attempt));
        assert!(pending.request_error.is_none() && pending.attempt_error.is_none());

        invocation.release();
        probe.release();
        // Do not advance the Session until both real Tasks closures have physically returned.
        let deadline = std::time::Instant::now() + WAIT;
        while request_returned.state() == TaskReturnState::Pending
            || attempt_returned.state() == TaskReturnState::Pending
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the two finite results did not return"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(request_returned.state(), TaskReturnState::Returned);
        assert_eq!(attempt_returned.state(), TaskReturnState::Returned);
        editor.drive_editor_frame(Duration::ZERO);
        let completed = editor.source_tool_view();
        assert!(completed.active.is_some());
        assert!(!completed.working && !completed.probing && completed.review.is_none());
        assert_eq!(completed.request.as_ref(), Some(&request));
        assert_eq!(completed.attempt.as_ref(), Some(&attempt));
        assert_eq!(
            completed.request_error,
            Some(EditorSourceToolError::Protocol)
        );
        assert_eq!(
            completed.attempt_error,
            Some(EditorSourceToolError::Declaration)
        );
        assert_eq!(snapshot(editor), before);
        for _ in 0..4 {
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.source_tool_view(), completed);
        }
        editor.disable_source_tool(&activation).unwrap();
        let disabled = editor.source_tool_view();
        assert!(disabled.request.is_none() && disabled.request_error.is_none());
        assert_eq!(disabled.attempt.as_ref(), Some(&attempt));
        assert_eq!(
            disabled.attempt_error,
            Some(EditorSourceToolError::Declaration)
        );
    });
}

#[test]
fn failed_read_and_request_budget_revoke_a_previous_review_without_publishing_it() {
    let project = Project::new();
    project.run(|editor| {
        let activation = activate(editor, &project);
        let before = snapshot(editor);
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        let basis = editor.document_query_basis(document(editor)).unwrap();
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit { entity: target() },
                BTreeMap::new(),
            ),
            Err(EditorSourceToolError::QueryBasis(
                EditorDocumentQueryRejection::Source(
                    nara_scene::AuthoringReadError::ComponentNotFound,
                )
            )),
        ));
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::StaleRequest),
        ));
        let request = attach(editor, &activation);
        wait(editor, |editor| editor.source_tool_view().review.is_some());
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Attach { entity: target() },
                BTreeMap::from([(
                    "large".into(),
                    ComponentValue::String("x".repeat(32 * 1024))
                )]),
            ),
            Err(EditorSourceToolError::Limit),
        ));
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&request),
            Err(EditorSourceToolError::StaleRequest),
        ));
        assert_eq!(snapshot(editor), before);
    });
}
