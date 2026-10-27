#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{EditorUiAuthoringEditError, TrustedCompiledEditorProjectIntent},
    project_workflow::{
        NaraDependencySource, ProjectCreationRequest, ProjectDirectory, ProjectMember, ProjectSlug,
        prepare_desktop_2d_project,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind},
    render::Extent2d,
    scene::{
        SceneAuthoringRevision, SceneComponentRecord, SceneDocument, SceneEntityId,
        SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
        EditorCloseDecision, EditorDocumentId, EditorDocumentMutation, EditorFieldFeedbackReason,
        EditorFieldTarget, EditorFieldValidationFailure, EditorNavigationAction, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorPlayState,
        EditorRuntimeUnavailableReason, EditorSelectionSet, EditorWorkspaceIntent,
        EditorWorkspaceIntentPhase, EditorWorkspaceIntentResult, SceneInspectorCommand,
        SceneInspectorReadOnlyReason,
    },
    ui::__private::{UiAuthoringCanvasProjection, UiAuthoringFieldValue},
};
use nara_editor::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationActionNotDispatched,
    EditorApplicationState, EditorLaunchOptions, EditorProjectAction, ImguiEditorFrontend,
};

#[path = "support/imgui.rs"]
mod imgui;

const NARA_GIT_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
fn direct_submission_retains_each_action_result_across_later_drives() {
    let project = StarterProject::new("direct-submission-results");
    let mut application = open_ordinary(project.root());
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let create = |name| {
        EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document: scene.document,
            base_revision: scene.revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(SceneEntityId::new(name).unwrap()),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]),
        })
    };
    let submission = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [create("accepted"), create("stale")]),
        Duration::ZERO,
    );
    assert!(submission.update().actions_observed());
    assert!(matches!(
        submission.actions(),
        [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(_)),
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Rejected(
                EditorActionRejection::StaleDocumentFact { .. }
            ))
        ]
    ));
    let retained = submission.actions().to_vec();
    let advanced = application.advance(Duration::ZERO);
    assert!(!advanced.actions_observed());
    assert_eq!(submission.actions(), retained);
    assert!(active_scene_has_entity(
        &current_frame(&application),
        "accepted"
    ));
    assert!(!active_scene_has_entity(
        &current_frame(&application),
        "stale"
    ));
    close_direct_application(&mut application);
    assert_eq!(submission.actions(), retained);
}

#[test]
fn direct_submission_reports_whole_batch_rejection_without_dispatch() {
    let project = StarterProject::new("direct-stale-batch");
    let mut application = open_ordinary(project.root());
    let frame = current_frame(&application);
    let document = frame.workspace().active_document.unwrap();
    let received = frame.revision();
    application.advance(Duration::ZERO);
    let expected = current_frame(&application).revision();
    let submission = application.submit_and_drive(
        EditorActionBatch::new(
            received,
            [
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
            ],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        submission.actions(),
        &vec![
            EditorApplicationActionDispatch::NotDispatched(
                EditorApplicationActionNotDispatched::BatchRejected(
                    EditorActionRejection::StaleFrame { expected, received }
                )
            );
            2
        ]
    );
    close_direct_application(&mut application);
}

#[test]
fn direct_submission_keeps_close_filtered_slots_in_original_order() {
    let project = StarterProject::new("direct-closing-results");
    let mut application = open_ordinary(project.root());
    dirty_active_scene(&mut application);
    application.submit_project_action(EditorProjectAction::CloseProject);
    let frame = current_frame(&application);
    let document = frame.workspace().active_document.unwrap();
    let navigate =
        || EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document });
    let filtered = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [navigate()]),
        Duration::ZERO,
    );
    assert_eq!(
        filtered.actions(),
        &[EditorApplicationActionDispatch::NotDispatched(
            EditorApplicationActionNotDispatched::Closing
        )]
    );
    assert!(!filtered.update().actions_observed());
    let frame = current_frame(&application);
    let cancel = EditorAction::WorkspaceIntent(
        frame
            .workspace_intent()
            .decision_prompt()
            .unwrap()
            .resolve(EditorCloseDecision::Cancel),
    );
    let submission = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [navigate(), cancel, navigate()]),
        Duration::ZERO,
    );
    assert!(matches!(
        submission.actions(),
        [
            EditorApplicationActionDispatch::NotDispatched(
                EditorApplicationActionNotDispatched::Closing
            ),
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_)),
            EditorApplicationActionDispatch::NotDispatched(
                EditorApplicationActionNotDispatched::Closing
            )
        ]
    ));
    assert!(
        current_frame(&application)
            .workspace_intent()
            .intent()
            .is_none()
    );
    close_direct_application(&mut application);
}

#[test]
fn direct_submission_without_workspace_returns_observations_without_replaying_actions() {
    let project = StarterProject::new("direct-no-workspace");
    let mut application = open_ordinary(project.root());
    let frame = current_frame(&application);
    let action = EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
        document: frame.workspace().active_document.unwrap(),
    });
    close_direct_application(&mut application);
    let submission = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [action]),
        Duration::ZERO,
    );
    assert_eq!(
        submission.actions(),
        &[EditorApplicationActionDispatch::NotDispatched(
            EditorApplicationActionNotDispatched::WorkspaceUnavailable
        )]
    );
    assert!(!submission.update().actions_observed());
    assert!(application.current_frame().is_none());
}

#[test]
fn prepared_application_commits_are_not_dispatched_during_close_or_after_retirement() {
    let project = StarterProject::new("prepared-close-admission");
    let mut application = open_ordinary(project.root());
    dirty_active_scene(&mut application);
    let before = current_frame(&application);
    let scene = before.workspace().active_scene.as_ref().unwrap();
    let basis = application.document_query_basis(scene.document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("never-published").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let closing = application.prepare_authoring_edit(&basis, &patch).unwrap();
    let retired = application.prepare_authoring_edit(&basis, &patch).unwrap();
    application.request_exit();
    let blocked = application.commit_authoring_edit(closing, Duration::ZERO);
    let after_blocked = current_frame(&application);
    close_direct_application(&mut application);
    let unavailable = application.commit_authoring_edit(retired, Duration::ZERO);
    assert_eq!(
        blocked.actions(),
        &[EditorApplicationActionDispatch::NotDispatched(
            EditorApplicationActionNotDispatched::Closing
        )]
    );
    assert_eq!(
        after_blocked.workspace().documents,
        before.workspace().documents
    );
    assert_eq!(
        unavailable.actions(),
        &[EditorApplicationActionDispatch::NotDispatched(
            EditorApplicationActionNotDispatched::WorkspaceUnavailable
        )]
    );
    assert!(!unavailable.update().actions_observed());
    assert!(application.current_frame().is_none());
}

#[test]
fn direct_submission_keeps_request_admission_distinct_from_failed_save_completion() {
    use nara::tooling::{
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    };
    let project = StarterProject::new("direct-save-admission");
    let mut application = open_ordinary(project.root());
    dirty_active_scene(&mut application);
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    fs::write(
        project.root().join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("external-save-conflict").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let submission = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document: scene.document,
                base_revision: scene.revision,
            })],
        ),
        Duration::ZERO,
    );
    assert!(matches!(
        submission.actions(),
        &[EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        )]
    ));
    settle_persistence(&mut application);
    assert!(matches!(
        current_frame(&application).persistence().result(),
        Some(EditorPersistenceResult::Rejected { .. } | EditorPersistenceResult::Failed { .. })
    ));
    assert!(matches!(
        submission.actions(),
        &[EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        )]
    ));
    assert!(
        current_frame(&application)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .dirty
    );
    close_direct_application(&mut application);
}

#[test]
fn starter_opens_one_coherent_frame_and_empty_repaint_preserves_document_state() {
    let project = StarterProject::new("open-and-repaint");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the canonical starter should open in the Editor");
    let before = current_frame(&application);
    let before_scene = before
        .workspace()
        .active_scene
        .as_ref()
        .expect("the starter opens its configured startup Scene");
    let document = before_scene.document;
    let revision = before_scene.revision;
    let dirty = before_scene.dirty;
    let selection = before_scene.selection.clone();

    let update = render_once(&mut application);
    let after = current_frame(&application);
    let after_scene = after
        .workspace()
        .scene(document)
        .expect("an empty redraw must retain the startup Scene");

    assert!(!update.actions_observed());
    assert!(!update.exit_ready());
    assert_eq!(after_scene.revision, revision);
    assert_eq!(after_scene.dirty, dirty);
    assert_eq!(
        after.workspace().active_scene.as_ref().unwrap().selection,
        selection
    );
    assert_ne!(after.revision(), before.revision());
}

#[test]
fn dirty_exit_cancel_preserves_the_session_and_discard_closes_it() {
    let project = StarterProject::new("dirty-close");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the canonical starter should open in the Editor");
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("dirty-probe").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let batch = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyScenePatch {
                document: scene.document,
                base_revision: scene.revision,
                patch,
            },
        )],
    );
    application.submit_and_drive(batch, Duration::ZERO);
    assert!(
        current_frame(&application)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .dirty
    );

    let close_request = application.request_exit();
    let awaiting_cancel = current_frame(&application);
    assert!(!close_request.exit_ready());
    assert_eq!(
        awaiting_cancel.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );

    let cancel = EditorActionBatch::new(
        awaiting_cancel.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_cancel
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Cancel),
        )],
    );
    let cancelled = application.submit_and_drive(cancel, Duration::ZERO);
    let cancelled_frame = current_frame(&application);
    assert!(!cancelled.update().exit_ready());
    assert!(cancelled_frame.workspace().active_scene.is_some());
    assert!(matches!(
        cancelled_frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::Exit,
        })
    ));
    for _ in 0..3 {
        assert!(!application.advance(Duration::ZERO).exit_ready());
        assert_eq!(
            active_scene_state(&current_frame(&application)),
            active_scene_state(&cancelled_frame)
        );
    }

    let second_request = application.request_exit();
    let awaiting_discard = current_frame(&application);
    assert!(!second_request.exit_ready());
    assert_eq!(
        awaiting_discard.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    let discard = EditorActionBatch::new(
        awaiting_discard.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_discard
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Discard),
        )],
    );
    let discarded = application.submit_and_drive(discard, Duration::ZERO);

    assert!(!discarded.update().exit_ready());
    assert_eq!(
        current_frame(&application).workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::Prepared)
    );
    assert!(drive_process_exit_to_terminal(&mut application));
    assert!(application.current_frame().is_none());
}

#[test]
fn clean_ordinary_exit_reaches_the_process_terminal() {
    let project = StarterProject::new("clean-ordinary-close");
    let mut application = open_ordinary(project.root());

    let requested = application.request_exit();

    assert!(!requested.exit_ready());
    assert!(drive_process_exit_to_terminal(&mut application));
    assert!(application.current_frame().is_none());
}

#[test]
fn final_exit_maintenance_outlives_the_workspace_frame() {
    use nara_tasks::{TaskPoolKind, TaskPools};
    use std::sync::mpsc;

    let project = StarterProject::new("exit-after-workspace-retirement");
    let (release, receiver) = mpsc::channel();
    let (mut application, probe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, usize::MAX, || {
            TaskPools::hold_partial_worker_exit_for_tests(receiver);
            open_ordinary(project.root())
        });
    assert_eq!(probe.started(), 1);
    assert!(!application.request_exit().exit_ready());
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_some() {
        assert!(Instant::now() < deadline, "the workspace did not retire");
        assert!(!application.advance(Duration::ZERO).exit_ready());
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..3 {
        assert!(!application.advance(Duration::ZERO).exit_ready());
        assert!(application.current_frame().is_none());
    }
    assert_eq!(probe.exited(), 0);
    release.send(()).unwrap();
    assert!(drive_process_exit_to_terminal(&mut application));
    assert_eq!(probe.exited(), 1);
    assert!(application.request_exit().exit_ready());
    let completed = application.advance(Duration::ZERO);
    assert!(completed.exit_ready());
    assert!(!completed.actions_observed());
}

#[test]
fn ordinary_workspace_exit_action_joins_process_retirement_only_after_admission() {
    let project = StarterProject::new("workspace-action-close");
    let mut application = open_ordinary(project.root());
    let frame = current_frame(&application);
    let stale = EditorActionBatch::new(
        frame.revision(),
        [
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                document: frame.workspace().active_document.unwrap(),
            }),
            EditorAction::WorkspaceIntent(nara::tooling::EditorWorkspaceIntentAction::Request(
                EditorWorkspaceIntent::Exit,
            )),
        ],
    );
    application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
    let rejected = application.submit_and_drive(stale, Duration::ZERO);
    assert!(!rejected.update().exit_ready());
    assert_eq!(
        current_frame(&application).workspace_intent().intent(),
        None
    );
    let frame = current_frame(&application);
    let accepted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                nara::tooling::EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
            )],
        ),
        Duration::ZERO,
    );
    assert!(!accepted.update().exit_ready());
    assert!(drive_process_exit_to_terminal(&mut application));
    assert!(application.current_frame().is_none());
}

#[test]
fn public_workspace_exit_action_preserves_dirty_cancel_and_later_discard() {
    let project = StarterProject::new("workspace-action-dirty-close");
    let mut application = open_ordinary(project.root());
    dirty_active_scene(&mut application);
    let before = current_frame(&application)
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .clone();
    let request = |application: &mut EditorApplication| {
        let frame = current_frame(application);
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::WorkspaceIntent(
                    nara::tooling::EditorWorkspaceIntentAction::Request(
                        EditorWorkspaceIntent::Exit,
                    ),
                )],
            ),
            Duration::ZERO,
        )
    };
    assert!(!request(&mut application).update().exit_ready());
    let frame = current_frame(&application);
    let prompt = frame.workspace_intent().decision_prompt().unwrap();
    assert!(
        !application
            .submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        prompt.resolve(EditorCloseDecision::Cancel)
                    ),]
                ),
                Duration::ZERO
            )
            .update()
            .exit_ready()
    );
    assert_eq!(
        current_frame(&application)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap(),
        &before
    );
    let frame = current_frame(&application);
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                nara::tooling::EditorWorkspaceIntentAction::AcknowledgeResult,
            )],
        ),
        Duration::ZERO,
    );
    assert!(!request(&mut application).update().exit_ready());
    let frame = current_frame(&application);
    assert!(
        !application
            .submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        frame
                            .workspace_intent()
                            .decision_prompt()
                            .unwrap()
                            .resolve(EditorCloseDecision::Discard,)
                    ),]
                ),
                Duration::ZERO
            )
            .update()
            .exit_ready()
    );
    assert!(drive_process_exit_to_terminal(&mut application));
    assert!(application.current_frame().is_none());
}

#[test]
fn field_failures_publish_inline_feedback_and_durable_diagnostics() {
    let project = StarterProject::new("field-feedback");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the canonical starter should open in the Editor");
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let entity = scene.editor.inspector.entities[0].id.clone();
    let select = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: scene.document,
                entity: Some(entity.clone()),
            },
        )],
    );
    application.submit_and_drive(select, Duration::ZERO);

    let selected_frame = current_frame(&application);
    let selected_scene = selected_frame.workspace().active_scene.as_ref().unwrap();
    let selected = selected_scene
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let (component, field) = selected
        .components
        .iter()
        .find_map(|component| {
            component
                .fields
                .iter()
                .find(|field| field.value_kind == ComponentValueKind::F64)
                .map(|field| (component, field))
        })
        .expect("the starter camera exposes one editable floating-point field");
    let target = EditorFieldTarget::new(
        selected_scene.document,
        selected_scene.revision,
        entity.clone(),
        component.component.clone(),
        component.schema_version,
        field.id.clone(),
    );
    let failure = EditorFieldValidationFailure::new(target.clone(), ComponentValueKind::F64);
    let invalid = EditorActionBatch::new(
        selected_frame.revision(),
        [EditorAction::ReportFieldValidation(failure.clone())],
    );
    application.submit_and_drive(invalid, Duration::ZERO);

    let invalid_frame = current_frame(&application);
    let feedback = invalid_frame.field_feedback().unwrap();
    assert!(feedback.target().same_field(&target));
    assert_eq!(
        feedback.reason(),
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::F64)
    );
    assert!(has_diagnostic(&invalid_frame, "editor.field.invalid-value"));

    let current_scene = invalid_frame.workspace().active_scene.as_ref().unwrap();
    let rejected_mutation = EditorActionBatch::new(
        invalid_frame.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document: current_scene.document,
                base_revision: current_scene.revision,
                command: SceneInspectorCommand::SetField {
                    entity,
                    component: component.component.clone(),
                    component_version: component.schema_version,
                    field: field.id.clone(),
                    value: ComponentValue::String("not-a-number".to_owned()),
                },
            },
        )],
    );
    application.submit_and_drive(rejected_mutation, Duration::ZERO);
    let rejected_frame = current_frame(&application);
    assert_eq!(
        rejected_frame.field_feedback().unwrap().reason(),
        EditorFieldFeedbackReason::MutationRejected
    );
    assert!(rejected_frame.diagnostics().stats().observed_errors() >= 2);

    let stale_source = current_frame(&application);
    let stale_scene = stale_source.workspace().active_scene.as_ref().unwrap();
    let original_value = field.value.as_ref().unwrap().as_f64().unwrap();
    let accepted_value = ComponentValue::f64(original_value * 0.5 + 0.25).unwrap();
    assert_ne!(Some(&accepted_value), field.value.as_ref());
    application.submit_and_drive(
        EditorActionBatch::new(
            stale_source.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: stale_scene.document,
                    base_revision: stale_scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity: target.entity().clone(),
                        component: component.component.clone(),
                        component_version: component.schema_version,
                        field: field.id.clone(),
                        value: accepted_value.clone(),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    let accepted_frame = current_frame(&application);
    let accepted_scene = accepted_frame.workspace().active_scene.as_ref().unwrap();
    assert_ne!(accepted_frame.revision(), stale_source.revision());
    assert_ne!(accepted_scene.revision, stale_scene.revision);
    assert!(accepted_scene.dirty);
    let accepted_field = accepted_scene
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|candidate| candidate.component == component.component)
        .unwrap()
        .fields
        .iter()
        .find(|candidate| candidate.id == field.id)
        .unwrap();
    assert_eq!(accepted_field.value.as_ref(), Some(&accepted_value));
    assert!(accepted_frame.field_feedback().is_none());
    let accepted_history = accepted_frame
        .workspace()
        .document(accepted_scene.document)
        .unwrap()
        .history;
    application.submit_and_drive(
        EditorActionBatch::new(
            accepted_frame.revision(),
            [EditorAction::ReportFieldValidation(failure)],
        ),
        Duration::ZERO,
    );
    let stale_frame = current_frame(&application);
    let feedback = stale_frame.field_feedback().unwrap();
    assert!(feedback.target().same_field(&target));
    assert_eq!(feedback.reason(), EditorFieldFeedbackReason::StaleAction);
    assert!(has_diagnostic(&stale_frame, "editor.action.stale-document"));
    assert_eq!(
        stale_frame
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        accepted_scene.revision
    );
    assert_eq!(
        stale_frame
            .workspace()
            .document(accepted_scene.document)
            .unwrap()
            .history,
        accepted_history
    );
}

#[test]
fn shell_first_open_routes_rejections_and_preservable_unknown_schema() {
    let mut manager = EditorApplication::new(EditorLaunchOptions::project_manager());
    assert_eq!(manager.state(), EditorApplicationState::ProjectManager);
    assert!(manager.current_frame().is_none());
    let _ = manager.native_shell_ready();
    assert_eq!(manager.state(), EditorApplicationState::ProjectManager);

    let malformed_manifest = StarterProject::new("malformed-manifest");
    fs::write(malformed_manifest.root().join("nara.toml"), "not = [toml").unwrap();
    assert_recovery(malformed_manifest.root());

    let missing_bundle = StarterProject::new("missing-bundle");
    fs::remove_file(missing_bundle.authoring_bundle()).unwrap();
    assert_recovery(missing_bundle.root());

    let rejected_startup = StarterProject::new("rejected-startup");
    fs::write(
        rejected_startup.root().join("scenes/startup.scene.json"),
        "{\"not\":\"a scene envelope\"}",
    )
    .unwrap();
    assert_recovery(rejected_startup.root());

    let unavailable_schema = StarterProject::new("unavailable-schema");
    let entity = SceneEntityRecord::new(SceneEntityId::new("unknown-schema").unwrap())
        .with_component(
            ComponentTypeId::new("example.Missing"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        );
    fs::write(
        unavailable_schema.root().join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([entity])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let application = open_ordinary(unavailable_schema.root());
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    assert!(application.current_frame().is_some());
}

#[test]
fn clean_room_custom_owner_opens_through_the_ordinary_editor() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/generic-editor/known-unbound")
        .canonicalize()
        .unwrap();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project_root).with_profile("desktop"));
    let _ = application.native_shell_ready();
    drive_open_to_terminal(&mut application);

    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let frame = current_frame(&application);
    assert!(active_scene_has_entity(&frame, "signal-node"));
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let signal_node = SceneEntityId::new("signal-node").unwrap();
    let select = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: scene.document,
                entity: Some(signal_node.clone()),
            },
        )],
    );
    application.submit_and_drive(select, Duration::ZERO);

    let selected_frame = current_frame(&application);
    let selected_entity = selected_frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let name_component = selected_entity
        .components
        .iter()
        .find(|component| component.component == ComponentTypeId::new("nara.scene.Name"))
        .unwrap();
    assert!(name_component.access.is_editable());
    let pulse = selected_entity
        .components
        .iter()
        .find(|component| component.component == ComponentTypeId::new("studio.signal.Pulse"))
        .unwrap();
    assert_eq!(
        pulse.access.read_only_reason(),
        Some(SceneInspectorReadOnlyReason::KnownUnbound)
    );
    assert!(pulse.fields.is_empty());
    assert!(has_diagnostic(
        &selected_frame,
        "scene.authoring-known-unbound"
    ));
    assert!(!has_diagnostic(
        &selected_frame,
        "scene.authoring-unknown-schema"
    ));
}

#[test]
fn reference_game_opens_through_the_same_ordinary_editor_api() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reference-game")
        .canonicalize()
        .unwrap();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project_root).with_profile("desktop"));
    let _ = application.native_shell_ready();
    drive_open_to_terminal(&mut application);

    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let frame = current_frame(&application);
    assert!(has_diagnostic(&frame, "scene.authoring-known-unbound"));
    assert!(!has_diagnostic(&frame, "scene.authoring-unknown-schema"));
    application.set_scene_preview_extent(Some(Extent2d::new(640, 360).unwrap()));
    let deadline = Instant::now() + Duration::from_secs(10);
    let scene_candidate = loop {
        let revision = current_frame(&application).revision();
        application.submit_and_drive(EditorActionBatch::new(revision, []), Duration::ZERO);
        if let Some(candidate) = application.take_scene_preview_candidate() {
            break candidate;
        }
        if Instant::now() >= deadline {
            let diagnostics = current_frame(&application).diagnostics().clone();
            close_direct_application(&mut application);
            panic!(
                "ordinary Generic Open did not finish Scene preview preparation: {diagnostics:?}"
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let active_scene = frame.workspace().active_scene.as_ref().unwrap();
    assert_eq!(scene_candidate.document(), active_scene.document);
    assert_eq!(scene_candidate.revision(), active_scene.revision);
    assert!(
        scene_candidate
            .hits()
            .iter()
            .any(|hit| hit.entity().as_str() == "player-weapon"),
        "ordinary Generic Open must preserve one known renderable Sprite"
    );
    use nara::scene::__private::{
        AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason,
    };
    for (expected_component, expected_reason) in [
        (
            "reference_game.EnemyRole",
            AuthoringPreviewOmissionReason::RecordUnavailable(
                nara::scene::AuthoringRecordReadiness::KnownUnbound,
            ),
        ),
        (
            "reference_game.WaveSpawn",
            AuthoringPreviewOmissionReason::NativeBindingUnavailable,
        ),
    ] {
        let mut anchors = scene_candidate
            .preview_omissions()
            .iter()
            .filter_map(|omission| {
                let AuthoringPreviewOmissionLocation::ProjectedEntityComponent {
                    provenance,
                    component,
                    ..
                } = omission.location()
                else {
                    return None;
                };
                (component.as_str() == expected_component && omission.reason() == expected_reason)
                    .then(|| provenance.projection_path().unwrap().root_anchor().as_str())
            })
            .collect::<Vec<_>>();
        anchors.sort_unstable();
        assert_eq!(
            anchors,
            ["enemy-anchor", "enemy-anchor-2", "enemy-anchor-3"],
            "every actual Prefab occurrence must retain its typed {expected_component} omission"
        );
    }

    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let reference_ui = SceneEntityId::new("reference-ui").unwrap();
    let opened = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(reference_ui.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: reference_ui,
                }),
            ],
        ),
        Duration::ZERO,
    );
    assert!(opened.update().actions_observed());
    let ui_frame = application
        .current_ui_authoring_frame()
        .expect("ordinary Generic Open must publish the active UI Prefab authoring frame");
    let projection = ui_frame
        .projection()
        .expect("the built-in UI Prefab must retain its typed authoring projection");
    assert!(!projection.hierarchy().is_empty());
    assert!(matches!(
        projection.canvas(),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let candidate = application
        .take_editor_ui_preview_candidate()
        .expect("ordinary Generic Open must publish one UI Canvas candidate");
    assert_eq!(candidate.document(), ui_frame.document());
    assert_eq!(candidate.revision(), ui_frame.revision());
    let discrete = projection
        .fields()
        .iter()
        .find(|field| {
            ui_frame.field_editing_enabled(field.key())
                && matches!(field.value(), UiAuthoringFieldValue::Bool(_))
        })
        .expect("the built-in UI Prefab must expose one preservation-safe Boolean field");
    let proposed = match discrete.value() {
        UiAuthoringFieldValue::Bool(value) => UiAuthoringFieldValue::Bool(!value),
        _ => unreachable!("the selected field is Boolean"),
    };
    assert!(!ui_frame.direct_editing_enabled());
    assert!(!ui_frame.structure_editing_enabled());
    assert!(matches!(
        ui_frame.lower_field_edit(discrete.key(), proposed.clone()),
        Err(EditorUiAuthoringEditError::MutationUnavailable)
    ));
    let mutation = ui_frame
        .lower_discrete_field_edit(discrete.key(), proposed)
        .expect("ordinary UI field lowering must use the preservation-safe command");
    assert!(matches!(
        mutation,
        EditorDocumentMutation::ApplyAuthoringFieldEdit { .. }
    ));
    let action_frame = current_frame(&application).revision();
    let edited = application.submit_and_drive(
        EditorActionBatch::new(action_frame, [EditorAction::Mutate(mutation)]),
        Duration::ZERO,
    );
    assert!(edited.update().actions_observed());
    assert_ne!(
        current_frame(&application)
            .workspace()
            .document(ui_frame.document())
            .expect("the edited UI Prefab must remain open")
            .revision,
        ui_frame.revision()
    );
    close_direct_application(&mut application);
}

#[test]
fn retry_and_replacement_close_are_failure_atomic() {
    let repairable = StarterProject::new("retry-after-repair");
    let bundle = fs::read(repairable.authoring_bundle()).unwrap();
    fs::remove_file(repairable.authoring_bundle()).unwrap();
    let mut retrying = EditorApplication::new(EditorLaunchOptions::new(repairable.root()));
    let _ = retrying.native_shell_ready();
    drive_open_to_terminal(&mut retrying);
    assert_eq!(retrying.state(), EditorApplicationState::Recovery);
    fs::write(repairable.authoring_bundle(), bundle).unwrap();
    assert!(retrying.retry_project_open());
    drive_open_to_terminal(&mut retrying);
    assert_eq!(retrying.state(), EditorApplicationState::AuthoringReady);

    let retained = StarterProject::new("retained-workspace");
    let replacement = StarterProject::new("replacement-workspace");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(retained.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    dirty_active_scene(&mut application);
    let before_failure = current_frame(&application);

    assert!(application.request_project_replacement(unique_test_path("missing-replacement"), None));
    drive_open_to_terminal(&mut application);
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    let after_failure = current_frame(&application);
    assert!(Arc::ptr_eq(&before_failure, &after_failure));

    assert!(application.request_project_replacement(replacement.root(), None));
    drive_replacement_to_close_decision(&mut application);
    assert!(!render_once(&mut application).actions_observed());
    let awaiting_cancel = current_frame(&application);
    let cancel = EditorActionBatch::new(
        awaiting_cancel.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_cancel
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Cancel),
        )],
    );
    let _ = application.submit_and_drive(cancel, Duration::ZERO);
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    let after_cancel = current_frame(&application);
    assert_eq!(
        active_scene_state(&after_cancel),
        active_scene_state(&before_failure)
    );

    assert!(application.request_project_replacement(replacement.root(), None));
    drive_replacement_to_close_decision(&mut application);
    fs::write(
        retained.root().join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("external-write").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let awaiting_failed_save = current_frame(&application);
    let save = EditorActionBatch::new(
        awaiting_failed_save.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_failed_save
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Save),
        )],
    );
    let failed_save_submission = application.submit_and_drive(save, Duration::ZERO);
    let deadline = Instant::now() + Duration::from_secs(10);
    while current_frame(&application)
        .persistence()
        .operation()
        .is_some()
    {
        assert!(Instant::now() < deadline, "replacement Save did not settle");
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let failure_frame = current_frame(&application);
    let failure_still_replacing = application.state() == EditorApplicationState::OpeningReplacement;
    let failure_phase = failure_frame.workspace_intent().phase();
    application.advance(Duration::ZERO);
    let cancel_frame = current_frame(&application);
    let cancelled_on_next_pass = application.state() == EditorApplicationState::AuthoringReady;
    assert!(
        failure_still_replacing,
        "Save failure must not recursively publish Cancel"
    );
    assert_eq!(
        failure_phase,
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert!(
        cancelled_on_next_pass,
        "the following pass publishes Cancel and restores the incumbent"
    );
    assert_eq!(failure_frame.persistence(), cancel_frame.persistence());
    assert!(matches!(
        cancel_frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::Exit
        })
    ));
    assert!(matches!(
        failed_save_submission.actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::WorkspaceIntent(_)
        )]
    ));
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert_eq!(
        active_scene_state(&current_frame(&application)),
        active_scene_state(&after_cancel)
    );
    let failed_save = current_frame(&application);
    assert_eq!(failed_save.workspace_intent().phase(), None);
    assert!(matches!(
        failed_save.persistence().result(),
        Some(
            nara::tooling::EditorPersistenceResult::Rejected { .. }
                | nara::tooling::EditorPersistenceResult::Failed { .. }
                | nara::tooling::EditorPersistenceResult::PersistenceUncertain { .. }
        )
    ));
    let failed_scene = failed_save.workspace().active_scene.as_ref().unwrap();
    let edit_after_failed_save = EditorActionBatch::new(
        failed_save.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyScenePatch {
                document: failed_scene.document,
                base_revision: failed_scene.revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(
                        SceneEntityId::new("edit-after-failed-save").unwrap(),
                    ),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            },
        )],
    );
    let resumed = application.submit_and_drive(edit_after_failed_save, Duration::ZERO);
    assert!(resumed.update().actions_observed());
    assert_eq!(current_frame(&application).workspace_intent().phase(), None);
    assert!(active_scene_has_entity(
        &current_frame(&application),
        "edit-after-failed-save"
    ));

    assert!(application.request_project_replacement(replacement.root(), None));
    drive_replacement_to_close_decision(&mut application);
    let awaiting_discard = current_frame(&application);
    let stale_save_prompt = awaiting_failed_save
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    assert_ne!(
        awaiting_discard.workspace_intent().decision_prompt(),
        Some(stale_save_prompt)
    );
    let disk_before_replay = fs::read(retained.root().join("scenes/startup.scene.json")).unwrap();
    let replayed_save = application.submit_and_drive(
        EditorActionBatch::new(
            awaiting_discard.revision(),
            [EditorAction::WorkspaceIntent(
                stale_save_prompt.resolve(EditorCloseDecision::Save),
            )],
        ),
        Duration::ZERO,
    );
    assert!(replayed_save.update().actions_observed());
    assert_eq!(
        application.state(),
        EditorApplicationState::OpeningReplacement
    );
    let after_stale_save = current_frame(&application);
    assert_eq!(
        after_stale_save.workspace_intent().decision_prompt(),
        awaiting_discard.workspace_intent().decision_prompt()
    );
    assert_eq!(
        active_scene_state(&after_stale_save),
        active_scene_state(&awaiting_discard)
    );
    assert_eq!(
        fs::read(retained.root().join("scenes/startup.scene.json")).unwrap(),
        disk_before_replay
    );
    let discard = EditorActionBatch::new(
        awaiting_discard.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_discard
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Discard),
        )],
    );
    let _ = application.submit_and_drive(discard, Duration::ZERO);
    drive_replacement_to_terminal(&mut application);
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(!active_scene_state(&current_frame(&application)).2);
}

#[test]
fn workspace_open_project_action_enters_the_retained_replacement_flow() {
    let retained = StarterProject::new("reachable-open-retained");
    let replacement = StarterProject::new("reachable-open-replacement");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(retained.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let before = current_frame(&application);

    let selected = application.submit_project_action(EditorProjectAction::OpenProject {
        project_root: replacement.root().to_owned(),
        profile: None,
    });

    assert!(selected.actions_observed());
    assert!(!selected.exit_ready());
    assert_eq!(
        application.state(),
        EditorApplicationState::OpeningReplacement
    );
    assert!(Arc::ptr_eq(&before, &current_frame(&application)));
    assert!(!render_once(&mut application).actions_observed());
    let preparing_frame = current_frame(&application);
    let preparing_scene = preparing_frame.workspace().active_scene.as_ref().unwrap();
    let ignored = application.submit_and_drive(
        EditorActionBatch::new(
            preparing_frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: preparing_scene.document,
                    entity: None,
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(!ignored.update().actions_observed());
    drive_replacement_to_terminal(&mut application);

    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(!Arc::ptr_eq(&before, &current_frame(&application)));
}

#[test]
fn replacement_cancel_restores_the_exact_retained_workspace() {
    let retained = StarterProject::new("cancel-opening-retained");
    let replacement = StarterProject::new("cancel-opening-replacement");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(retained.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let before = current_frame(&application);

    assert!(application.request_project_replacement(replacement.root(), None));
    let mut context = imgui::context();
    let mut frontend = ImguiEditorFrontend::new();
    for _ in 0..3 {
        let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    }
    context.binding().with_bound_context(|| {
        // SAFETY: this Context owns the Project window and its live button identity. The
        // navigation focus is local to this test; Enter below activates the real Cancel widget.
        unsafe {
            let window = dear_imgui_rs::sys::igFindWindowByName(c"Project".as_ptr());
            assert!(!window.is_null());
            let id = dear_imgui_rs::sys::ImGuiWindow_GetID_Str(
                window,
                c"Cancel".as_ptr(),
                std::ptr::null(),
            );
            dear_imgui_rs::sys::igFocusWindow(window, 0);
            dear_imgui_rs::sys::igSetFocusID(id, window);
            dear_imgui_rs::sys::igSetNavCursorVisible(true);
        }
    });
    context
        .io_mut()
        .add_key_event(dear_imgui_rs::Key::Enter, true);
    let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    context
        .io_mut()
        .add_key_event(dear_imgui_rs::Key::Enter, false);
    let _ = imgui::draw(&mut context, &mut frontend, &mut application);

    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(Arc::ptr_eq(&before, &current_frame(&application)));
    assert_eq!(current_frame(&application).workspace_intent().phase(), None);
}

#[test]
fn close_project_reuses_the_exit_decision_without_exiting_the_process() {
    let project = StarterProject::new("reachable-close");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    dirty_active_scene(&mut application);
    let before = current_frame(&application);
    let before_scene = active_scene_state(&before);

    let requested = application.submit_project_action(EditorProjectAction::CloseProject);
    assert!(requested.actions_observed());
    assert!(!requested.exit_ready());
    assert_eq!(
        current_frame(&application).workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );

    let locked = current_frame(&application);
    let scene = locked.workspace().active_scene.as_ref().unwrap();
    let ignored = application.submit_and_drive(
        EditorActionBatch::new(
            locked.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: scene.document,
                    base_revision: scene.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(SceneEntityId::new("locked-edit").unwrap()),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(!ignored.update().actions_observed());
    assert_eq!(
        active_scene_state(&current_frame(&application)),
        before_scene
    );

    let awaiting_cancel = current_frame(&application);
    let cancelled = application.submit_and_drive(
        EditorActionBatch::new(
            awaiting_cancel.revision(),
            [EditorAction::WorkspaceIntent(
                awaiting_cancel
                    .workspace_intent()
                    .decision_prompt()
                    .unwrap()
                    .resolve(EditorCloseDecision::Cancel),
            )],
        ),
        Duration::ZERO,
    );
    assert!(!cancelled.update().exit_ready());
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert_eq!(
        active_scene_state(&current_frame(&application)),
        before_scene
    );

    let _ = application.submit_project_action(EditorProjectAction::CloseProject);
    let awaiting_discard = current_frame(&application);
    let closed = application.submit_and_drive(
        EditorActionBatch::new(
            awaiting_discard.revision(),
            [EditorAction::WorkspaceIntent(
                awaiting_discard
                    .workspace_intent()
                    .decision_prompt()
                    .unwrap()
                    .resolve(EditorCloseDecision::Discard),
            )],
        ),
        Duration::ZERO,
    );

    assert!(!closed.update().exit_ready());
    drive_session_retirement_to_manager(&mut application);
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert!(application.current_frame().is_none());
    assert!(application.request_exit().exit_ready());
}

#[test]
fn replacement_same_root_alias_save_publishes_a_fresh_candidate() {
    let project = StarterProject::new("same-root-alias-save");
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project.root()),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    dirty_active_scene(&mut application);

    let alias = project.root().join(".");
    assert!(application.request_project_replacement(alias, None));
    drive_replacement_to_close_decision(&mut application);
    let awaiting_save = current_frame(&application);
    let save = EditorActionBatch::new(
        awaiting_save.revision(),
        [EditorAction::WorkspaceIntent(
            awaiting_save
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Save),
        )],
    );
    let _ = application.submit_and_drive(save, Duration::ZERO);
    settle_persistence(&mut application);
    let saved_frame = current_frame(&application);
    let _ = application.submit_and_drive(
        EditorActionBatch::new(saved_frame.revision(), []),
        Duration::ZERO,
    );
    assert_eq!(
        application.state(),
        EditorApplicationState::OpeningReplacement
    );
    assert_eq!(
        current_frame(&application).workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::Prepared)
    );
    assert!(!render_once(&mut application).actions_observed());
    drive_replacement_to_terminal(&mut application);

    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    let fresh = current_frame(&application);
    assert!(active_scene_has_entity(&fresh, "dirty-probe"));
    assert!(!active_scene_state(&fresh).2);
}

#[test]
fn ordinary_play_reports_typed_supervised_play_contract_missing() {
    let project = StarterProject::new("runtime-unavailable");
    let mut application = open_ordinary(project.root());
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let batch = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Play(EditorPlayAction::Play {
            document: scene.document,
            base_revision: scene.revision,
        })],
    );
    let _ = application.submit_and_drive(batch, Duration::ZERO);
    let frame = current_frame(&application);

    assert_eq!(frame.play().state(), EditorPlayState::Empty);
    assert_eq!(frame.play().generation(), None);
    assert_eq!(
        frame.play().result(),
        Some(EditorPlayOperationResult::Rejected {
            operation: EditorPlayOperation::Play,
            reason: EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
            ),
        })
    );
    assert!(frame.workspace().active_scene.is_some());
}

fn render_once(application: &mut EditorApplication) -> nara_editor::EditorApplicationUpdate {
    let mut frontend = ImguiEditorFrontend::new();
    let submission = imgui::draw(&mut imgui::context(), &mut frontend, application)
        .expect("the fixed Editor panel returns one workspace result");
    frontend.submit_and_drive(application, submission, Duration::ZERO)
}

fn current_frame(application: &EditorApplication) -> Arc<nara::tooling::EditorFrameModel> {
    application
        .current_frame()
        .expect("the trusted compiled application owns one Editor frame")
}

fn settle_persistence(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while current_frame(application)
        .persistence()
        .operation()
        .is_some()
    {
        assert!(Instant::now() < deadline, "persistence did not settle");
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_ordinary(project_root: &Path) -> EditorApplication {
    let mut application = EditorApplication::new(EditorLaunchOptions::new(project_root));
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert!(application.current_frame().is_none());
    let _ = application.native_shell_ready();
    assert_eq!(application.state(), EditorApplicationState::OpeningInitial);
    drive_open_to_terminal(&mut application);
    application
}

fn close_direct_application(application: &mut EditorApplication) {
    if application.request_exit().exit_ready() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let decision = application.current_frame().and_then(|frame| {
            frame.workspace_intent().decision_prompt().map(|prompt| {
                (
                    frame.revision(),
                    prompt.resolve(EditorCloseDecision::Discard),
                )
            })
        });
        let update = if let Some((revision, decision)) = decision {
            application
                .submit_and_drive(
                    EditorActionBatch::new(revision, [EditorAction::WorkspaceIntent(decision)]),
                    Duration::ZERO,
                )
                .update()
        } else {
            application.advance(Duration::ZERO)
        };
        if update.exit_ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "direct application did not retire"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_recovery(project_root: &Path) {
    let application = open_ordinary(project_root);
    assert_eq!(application.state(), EditorApplicationState::Recovery);
    assert!(application.current_frame().is_none());
}

fn drive_open_to_terminal(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady
            | EditorApplicationState::AuthoringDegraded
            | EditorApplicationState::Recovery => return,
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance(Duration::ZERO);
            }
            EditorApplicationState::ProjectManager => {
                panic!("the native shell must arm the project-open request")
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the bounded project-open state machine did not reach a terminal state")
}

fn drive_replacement_to_close_decision(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let _ = application.advance(Duration::ZERO);
        if application.state() == EditorApplicationState::OpeningReplacement
            && current_frame(application).workspace_intent().phase()
                == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the replacement did not reach the retained session close decision")
}

fn drive_replacement_to_terminal(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded => {
                return;
            }
            EditorApplicationState::OpeningReplacement => {
                let _ = application.advance(Duration::ZERO);
            }
            EditorApplicationState::Recovery => return,
            EditorApplicationState::ProjectManager | EditorApplicationState::OpeningInitial => {
                panic!("the replacement should retain a published workspace")
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the replacement did not reach a published terminal state")
}

fn drive_session_retirement_to_manager(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if application.state() == EditorApplicationState::ProjectManager {
            return;
        }
        let _ = application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the session retirement did not return to Project Manager")
}

fn drive_process_exit_to_terminal(application: &mut EditorApplication) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let update = application.advance(Duration::ZERO);
        if update.exit_ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

fn dirty_active_scene(application: &mut EditorApplication) {
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("dirty-probe").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let batch = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyScenePatch {
                document: scene.document,
                base_revision: scene.revision,
                patch,
            },
        )],
    );
    let _ = application.submit_and_drive(batch, Duration::ZERO);
    assert!(active_scene_state(&current_frame(application)).2);
}

fn active_scene_state(
    frame: &nara::tooling::EditorFrameModel,
) -> (
    EditorDocumentId,
    SceneAuthoringRevision,
    bool,
    EditorSelectionSet,
) {
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    (
        scene.document,
        scene.revision,
        scene.dirty,
        scene.selection.clone(),
    )
}

fn active_scene_has_entity(frame: &nara::tooling::EditorFrameModel, id: &str) -> bool {
    let expected = SceneEntityId::new(id).unwrap();
    frame
        .workspace()
        .active_scene
        .as_ref()
        .is_some_and(|scene| {
            scene
                .editor
                .inspector
                .entities
                .iter()
                .any(|entity| entity.id == expected)
        })
}

fn has_diagnostic(frame: &nara::tooling::EditorFrameModel, code: &str) -> bool {
    frame
        .diagnostics()
        .iter()
        .any(|entry| entry.code().as_str() == code)
}

struct StarterProject {
    parent: PathBuf,
    root: PathBuf,
}

impl StarterProject {
    fn new(label: &str) -> Self {
        let parent = unique_test_path(label);
        fs::create_dir(&parent).unwrap();
        let slug = ProjectSlug::new("editor-app").unwrap();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            directory_capability(&parent),
            slug,
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                NARA_GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let root = parent.join("editor-app");
        fs::create_dir(&root).unwrap();
        for directory in prepared.plan().directories() {
            fs::create_dir_all(root.join(directory_path(*directory))).unwrap();
        }
        for member in prepared.plan().members() {
            fs::write(root.join(member_path(member.member())), member.bytes()).unwrap();
        }
        Self { parent, root }
    }

    fn authoring_bundle(&self) -> PathBuf {
        self.root.join("schema/authoring.bundle.json")
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for StarterProject {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let parent = self.parent.canonicalize().unwrap();
        assert!(parent.starts_with(&temporary_root));
        assert!(
            parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara-editor-application-"))
        );
        fs::remove_dir_all(parent).unwrap();
    }
}

fn unique_test_path(label: &str) -> PathBuf {
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nara-editor-application-{label}-{}-{sequence}",
        std::process::id()
    ))
}

fn directory_path(directory: ProjectDirectory) -> &'static str {
    match directory {
        ProjectDirectory::Assets => "assets",
        ProjectDirectory::Prefabs => "prefabs",
        ProjectDirectory::Scenes => "scenes",
        ProjectDirectory::Schema => "schema",
        ProjectDirectory::Source => "src",
        ProjectDirectory::SourceBinaries => "src/bin",
    }
}

fn member_path(member: ProjectMember) -> &'static str {
    match member {
        ProjectMember::CargoManifest => "Cargo.toml",
        ProjectMember::ProjectManifest => "nara.toml",
        ProjectMember::AuthorGuide => "README.md",
        ProjectMember::LibrarySource => "src/lib.rs",
        ProjectMember::HeadlessSource => "src/bin/headless.rs",
        ProjectMember::DesktopSource => "src/bin/desktop.rs",
        ProjectMember::StartupScene => "scenes/startup.scene.json",
        ProjectMember::StartupSceneMetadata => "scenes/startup.scene.json.meta",
        ProjectMember::AuthoringSchemaBundle => "schema/authoring.bundle.json",
    }
}

fn directory_capability(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        open_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

#[cfg(windows)]
fn open_directory(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
fn open_directory(path: &Path) -> File {
    File::open(path).unwrap()
}
