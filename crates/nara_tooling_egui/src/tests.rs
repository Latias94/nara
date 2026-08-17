use std::collections::BTreeSet;

use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneDocument, SceneEntityId,
    ScenePlacement, ScenePlacementOwner,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentId, EditorDocumentMutation,
    EditorExternalReloadState, EditorFieldFeedback, EditorFieldFeedbackReason, EditorFieldTarget,
    EditorFrameClock, EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
    EditorPersistenceRejection, EditorPersistenceRequestKind, EditorPersistenceResult,
    EditorPersistenceView, EditorPlayAction, EditorPlayOperation, EditorPlayOperationResult,
    EditorPlayState, EditorPlayView, EditorProjectView, EditorReopenTarget, EditorSceneModel,
    EditorSceneTabModel, EditorSelectionSet, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentView, EditorWorkspaceModel,
    SceneCompositionCommand, SceneCompositionComponentKind, SceneCompositionEntityKind,
    SceneEditorModel, SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorEntityRow,
    SceneInspectorEntityView, SceneInspectorFieldState, SceneInspectorFieldView,
    SceneInspectorModel,
};

use crate::{
    EguiCentralViewportInput, EguiEditorViewport, EguiEditorViewportIntent, EguiSceneEditorOutput,
    EguiSceneEditorPanel,
    editor::{test_control_rect, test_diagnostic_codes, test_inline_field_feedback},
    fields::parse_editable_value,
};

#[test]
fn scene_tab_emits_one_typed_shell_intent() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let running = frame_with_play_state(scene.clone(), EditorPlayState::Running);
    let mut panel = EguiSceneEditorPanel::new();
    let context = egui::Context::default();

    let output = click_viewport_control(
        &context,
        &mut panel,
        &running,
        EguiEditorViewport::Game,
        "Scene tab",
    );
    let (actions, _, intent) = output.into_parts();

    assert!(actions.into_actions().is_empty());
    assert_eq!(intent, Some(EguiEditorViewportIntent::SelectScene));
}

#[test]
fn parses_only_the_scalar_values_supported_by_the_v1_inspector() {
    assert_eq!(
        parse_editable_value(ComponentValueKind::String, "Hero").unwrap(),
        ComponentValue::String("Hero".to_owned())
    );
    assert_eq!(
        parse_editable_value(ComponentValueKind::Bool, "true").unwrap(),
        ComponentValue::Bool(true)
    );
    assert_eq!(
        parse_editable_value(ComponentValueKind::I64, "-7").unwrap(),
        ComponentValue::I64(-7)
    );
    assert!(parse_editable_value(ComponentValueKind::F64, "NaN").is_err());
    assert!(parse_editable_value(ComponentValueKind::Map, "{}").is_err());
}

#[test]
fn fixed_toolbar_emits_exact_frame_bound_authoring_actions() {
    let scene = scene_model(
        revision(),
        true,
        SceneAuthoringHistoryStatus {
            undo_depth: 1,
            redo_depth: 1,
        },
        true,
    );
    let frame = frame(
        Some(scene.clone()),
        EditorProjectView::new(
            play_view(Some(&scene)),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
    );

    let cases = [
        (
            "Undo",
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: scene.document,
                base_revision: scene.revision,
            }),
        ),
        (
            "Redo",
            EditorAction::Mutate(EditorDocumentMutation::Redo {
                document: scene.document,
                base_revision: scene.revision,
            }),
        ),
        (
            "Save",
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document: scene.document,
                base_revision: scene.revision,
            }),
        ),
        (
            "Reopen",
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document: scene.document,
                    base_revision: scene.revision,
                },
            }),
        ),
    ];

    for (label, expected) in cases {
        let mut panel = EguiSceneEditorPanel::new();
        let batch = click_control(&mut panel, &frame, label);
        assert_eq!(batch, EditorActionBatch::new(frame.revision(), [expected]));
    }
}

#[test]
fn play_toolbar_emits_ui_neutral_pause_and_step_actions() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let cases = [
        (
            EditorPlayState::Running,
            None,
            "Pause",
            EditorPlayAction::Pause,
        ),
        (
            EditorPlayState::Paused,
            None,
            "Step",
            EditorPlayAction::StepFixedTick,
        ),
        (
            EditorPlayState::Starting,
            Some(EditorPlayOperationResult::Pending {
                operation: EditorPlayOperation::Play,
            }),
            "Stop",
            EditorPlayAction::Cancel,
        ),
        (
            EditorPlayState::Stepping,
            Some(EditorPlayOperationResult::Pending {
                operation: EditorPlayOperation::StepFixedTick,
            }),
            "Stop",
            EditorPlayAction::Stop,
        ),
    ];

    for (state, result, label, expected) in cases {
        let frame = frame_with_play_state_and_result(scene.clone(), state, result);
        let mut panel = EguiSceneEditorPanel::new();
        assert_eq!(
            click_control(&mut panel, &frame, label),
            EditorActionBatch::new(frame.revision(), [EditorAction::Play(expected)])
        );
    }
}

#[test]
fn canonical_tree_selection_emits_one_revision_free_navigation_action() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let frame = default_frame(Some(scene.clone()));
    let mut panel = EguiSceneEditorPanel::new();
    let batch = click_control(&mut panel, &frame, "entity:player");
    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(SceneEntityId::new("player").unwrap()),
                }
            )]
        )
    );
}

#[test]
fn composition_controls_emit_exact_revision_bound_actions() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let frame = default_frame(Some(scene.clone()));

    let add_root = click_control(&mut EguiSceneEditorPanel::new(), &frame, "Add Root");
    assert_eq!(
        add_root,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::ColoredSprite,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            })]
        )
    );

    let add_sprite = click_control(&mut EguiSceneEditorPanel::new(), &frame, "+ Sprite");
    assert_eq!(
        add_sprite,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: SceneEntityId::new("player").unwrap(),
                    component: SceneCompositionComponentKind::Sprite,
                },
            })]
        )
    );
}

#[test]
fn faulted_play_keeps_scene_authoring_available_after_handoff() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let frame = frame_with_play_state(scene.clone(), EditorPlayState::Faulted);

    let batch = click_control(&mut EguiSceneEditorPanel::new(), &frame, "Add Root");

    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::ColoredSprite,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            })]
        )
    );
}

#[test]
fn fixed_panels_remain_inside_the_native_viewport() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let frame = default_frame(Some(scene));
    let mut panel = EguiSceneEditorPanel::new();
    let viewport = egui::vec2(800.0, 540.0);
    let _ = render_editor_frame_at(
        &egui::Context::default(),
        &mut panel,
        &frame,
        egui::RawInput::default(),
        viewport,
    );

    let tree = test_control_rect("Scene Tree heading").unwrap();
    let scene_view = test_control_rect("Scene View heading").unwrap();
    let inspector = test_control_rect("Inspector heading").unwrap();
    let diagnostics = test_control_rect("Diagnostics heading").unwrap();
    assert!(tree.top() < diagnostics.top());
    assert!(tree.right() < scene_view.right());
    assert!(scene_view.right() < inspector.right());
    assert!(inspector.right() <= viewport.x, "{inspector:?}");
    assert!(inspector.top() < diagnostics.top());
    assert!(
        diagnostics.bottom() <= viewport.y,
        "tree={tree:?}, scene={scene_view:?}, inspector={inspector:?}, diagnostics={diagnostics:?}"
    );
}

#[test]
fn inspector_edit_uses_the_exact_document_and_revision_from_its_frame() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let frame = default_frame(Some(scene.clone()));
    let mut panel = EguiSceneEditorPanel::new();
    let batch = click_control(
        &mut panel,
        &frame,
        "field-apply:player:nara.test.Transform:x",
    );
    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: scene.document,
                    base_revision: scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity: SceneEntityId::new("player").unwrap(),
                        component: ComponentTypeId::new("nara.test.Transform"),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("x"),
                        value: ComponentValue::I64(1),
                    },
                },
            )]
        )
    );
}

#[test]
fn document_or_revision_change_invalidates_drafts_and_inline_errors() {
    let first = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let replacement = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let mut panel = EguiSceneEditorPanel::new();
    panel.inspector_mut().seed_test_draft(
        &first,
        SceneEntityId::new("player").unwrap(),
        ComponentTypeId::new("nara.test.Transform"),
        ComponentFieldId::new("x"),
    );
    assert_eq!(panel.inspector_mut().test_transient_entry_count(), 1);

    panel.inspector_mut().synchronize_scene(Some(&replacement));
    assert_eq!(panel.inspector_mut().test_transient_entry_count(), 0);
}

#[test]
fn host_field_feedback_is_rendered_only_for_the_matching_field() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let feedback = EditorFieldFeedback::new(
        EditorFieldTarget::new(
            scene.document,
            scene.revision,
            SceneEntityId::new("player").unwrap(),
            ComponentTypeId::new("nara.test.Transform"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("x"),
        ),
        EditorFieldFeedbackReason::StaleAction,
    );
    let frame = frame_with_feedback(
        Some(scene.clone()),
        EditorProjectView::new(
            play_view(Some(&scene)),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
        Some(feedback),
    );
    let mut panel = EguiSceneEditorPanel::new();

    let _ = render_editor_frame(
        &egui::Context::default(),
        &mut panel,
        &frame,
        egui::RawInput::default(),
    );

    assert_eq!(
        test_inline_field_feedback(),
        ["The field changed before this edit was applied"]
    );
}

#[test]
fn diagnostics_and_persistence_failure_remain_visible_and_dismissible() {
    let scene = scene_model(
        revision(),
        true,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(
        DiagnosticCode::new("editor.test.stale-action").unwrap(),
        SafeSummary::new("stale action rejected").unwrap(),
    ));
    let document = scene.document;
    let base_revision = scene.revision;
    let project = EditorProjectView::new(
        play_view(Some(&scene)),
        EditorPersistenceView::new(
            nara_tooling::EditorPersistenceOperation::Idle,
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(scene.document),
                reason: EditorPersistenceRejection::TargetChanged,
            }),
        ),
        EditorWorkspaceIntentView::default(),
    );
    let frame = frame(Some(scene), project, diagnostics);
    let mut panel = EguiSceneEditorPanel::new();
    let _ = render_editor_frame(
        &egui::Context::default(),
        &mut panel,
        &frame,
        egui::RawInput::default(),
    );
    assert_eq!(test_diagnostic_codes(), ["editor.test.stale-action"]);

    let batch = click_control(&mut panel, &frame, "Dismiss result");
    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult,
            )]
        )
    );

    let retry = click_control(&mut EguiSceneEditorPanel::new(), &frame, "Save");
    assert_eq!(
        retry,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            })],
        )
    );
}

#[test]
fn dirty_close_dialog_returns_only_the_typed_decision_action() {
    let scene = scene_model(
        revision(),
        true,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    let project = EditorProjectView::new(
        play_view(Some(&scene)),
        EditorPersistenceView::default(),
        EditorWorkspaceIntentView::new(
            Some(EditorWorkspaceIntent::CloseDocument {
                document: scene.document,
            }),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision),
            None,
        ),
    );
    let frame = frame(Some(scene), project, DiagnosticReport::default());
    let mut panel = EguiSceneEditorPanel::new();
    let batch = click_control(&mut panel, &frame, "Close Cancel");
    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Cancel),
            )]
        )
    );
}

fn revision() -> nara_scene::SceneAuthoringRevision {
    SceneAuthoringSession::new(SceneDocument::default()).revision()
}

fn scene_model(
    revision: nara_scene::SceneAuthoringRevision,
    dirty: bool,
    history: SceneAuthoringHistoryStatus,
    selected: bool,
) -> EditorSceneModel {
    let document = EditorDocumentId::from_raw(1);
    let entity = SceneEntityId::new("player").unwrap();
    let component = ComponentTypeId::new("nara.test.Transform");
    let field = SceneInspectorFieldView {
        id: ComponentFieldId::new("x"),
        aliases: Vec::new(),
        path: ComponentFieldPath::from_fields(["x"]),
        value_kind: ComponentValueKind::I64,
        required: true,
        capabilities: BTreeSet::from([ComponentCapability::Edit]),
        default_value: None,
        value: Some(ComponentValue::I64(1)),
        state: SceneInspectorFieldState::Present,
    };
    let component_view = SceneInspectorComponentView {
        component: component.clone(),
        document_version: ComponentSchemaVersion::ONE,
        schema_version: ComponentSchemaVersion::ONE,
        capabilities: BTreeSet::from([
            ComponentCapability::Inspect,
            ComponentCapability::Edit,
            ComponentCapability::Scene,
        ]),
        fields: vec![field],
    };
    let selected_entity_view = selected.then(|| SceneInspectorEntityView {
        id: entity.clone(),
        parent: None,
        has_prefab: false,
        components: vec![component_view],
    });
    EditorSceneModel {
        document,
        title: "Scene".to_owned(),
        dirty,
        revision,
        saved_revision: revision,
        external_reload: EditorExternalReloadState::Clean,
        selection: EditorSelectionSet::default(),
        editor: SceneEditorModel {
            inspector: SceneInspectorModel {
                selected_entity: selected.then_some(entity.clone()),
                entities: vec![SceneInspectorEntityRow {
                    id: entity,
                    parent: None,
                    depth: 0,
                    inspectable_component_count: 1,
                    has_prefab: false,
                    selected,
                    live_locator: None,
                }],
                selected_entity_view,
                world_snapshot: None,
                history,
                live_dirty: false,
                diagnostics: DiagnosticReport::default(),
            },
            diagnostics: DiagnosticReport::default(),
        },
    }
}

fn default_frame(scene: Option<EditorSceneModel>) -> EditorFrameModel {
    let play = play_view(scene.as_ref());
    frame(
        scene,
        EditorProjectView::new(
            play,
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
    )
}

fn frame_with_play_state(scene: EditorSceneModel, state: EditorPlayState) -> EditorFrameModel {
    frame_with_play_state_and_result(scene, state, None)
}

fn frame_with_play_state_and_result(
    scene: EditorSceneModel,
    state: EditorPlayState,
    result: Option<EditorPlayOperationResult>,
) -> EditorFrameModel {
    let revision = scene.revision;
    frame(
        Some(scene),
        EditorProjectView::new(
            EditorPlayView::new(
                state,
                Some(7),
                Some(revision),
                Some(revision),
                false,
                result,
            ),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
    )
}

fn frame(
    scene: Option<EditorSceneModel>,
    project: EditorProjectView,
    diagnostics: DiagnosticReport,
) -> EditorFrameModel {
    frame_with_feedback(scene, project, diagnostics, None)
}

fn frame_with_feedback(
    scene: Option<EditorSceneModel>,
    project: EditorProjectView,
    diagnostics: DiagnosticReport,
    field_feedback: Option<EditorFieldFeedback>,
) -> EditorFrameModel {
    let active_document = scene.as_ref().map(|scene| scene.document);
    let reopen_target = scene.as_ref().map_or(
        EditorReopenTarget::ClosedSource {
            document: EditorDocumentId::from_raw(1),
        },
        |scene| EditorReopenTarget::OpenDocument {
            document: scene.document,
            base_revision: scene.revision,
        },
    );
    let scenes = scene
        .as_ref()
        .map(|scene| {
            vec![EditorSceneTabModel {
                document: scene.document,
                title: scene.title.clone(),
                dirty: scene.dirty,
                revision: scene.revision,
                external_reload: scene.external_reload,
                active: true,
            }]
        })
        .unwrap_or_default();
    EditorFrameClock::new().publish(
        EditorWorkspaceModel {
            active_document,
            documents: scenes
                .iter()
                .map(|scene| nara_tooling::EditorDocumentTabModel {
                    document: scene.document,
                    kind: nara_tooling::EditorDocumentKind::Scene,
                    title: scene.title.clone(),
                    dirty: scene.dirty,
                    revision: scene.revision,
                    saved_revision: scene.revision,
                    source_binding: None,
                    external_reload: scene.external_reload,
                    active: scene.active,
                })
                .collect(),
            scenes,
            active_scene: scene,
            diagnostics: DiagnosticReport::default(),
        },
        project,
        diagnostics,
        reopen_target,
        field_feedback,
    )
}

fn play_view(scene: Option<&EditorSceneModel>) -> EditorPlayView {
    EditorPlayView::new(
        EditorPlayState::Empty,
        None,
        None,
        scene.map(|scene| scene.revision),
        false,
        None,
    )
}

fn click_control(
    panel: &mut EguiSceneEditorPanel,
    frame: &EditorFrameModel,
    label: &str,
) -> EditorActionBatch {
    let context = egui::Context::default();
    let _ = render_editor_frame(&context, panel, frame, egui::RawInput::default());
    let _ = render_editor_frame(&context, panel, frame, egui::RawInput::default());
    let position = test_control_rect(label)
        .unwrap_or_else(|| panic!("control {label} was not rendered"))
        .center();

    let mut pressed = egui::RawInput::default();
    pressed.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let _ = render_editor_frame(&context, panel, frame, pressed);

    let mut released = egui::RawInput::default();
    released.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    render_editor_frame(&context, panel, frame, released)
}

fn click_viewport_control(
    context: &egui::Context,
    panel: &mut EguiSceneEditorPanel,
    frame: &EditorFrameModel,
    selected: EguiEditorViewport,
    label: &str,
) -> EguiSceneEditorOutput {
    let _ =
        render_editor_viewport_frame(context, panel, frame, egui::RawInput::default(), selected);
    let _ =
        render_editor_viewport_frame(context, panel, frame, egui::RawInput::default(), selected);
    let position = test_control_rect(label)
        .unwrap_or_else(|| panic!("control {label} was not rendered"))
        .center();

    let mut pressed = egui::RawInput::default();
    pressed.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let _ = render_editor_viewport_frame(context, panel, frame, pressed, selected);

    let mut released = egui::RawInput::default();
    released.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    render_editor_viewport_frame(context, panel, frame, released, selected)
}

fn render_editor_frame(
    context: &egui::Context,
    panel: &mut EguiSceneEditorPanel,
    frame: &EditorFrameModel,
    input: egui::RawInput,
) -> EditorActionBatch {
    render_editor_frame_at(context, panel, frame, input, egui::vec2(1280.0, 720.0))
}

fn render_editor_frame_at(
    context: &egui::Context,
    panel: &mut EguiSceneEditorPanel,
    frame: &EditorFrameModel,
    mut input: egui::RawInput,
    viewport: egui::Vec2,
) -> EditorActionBatch {
    input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport));
    let mut response = None;
    let output = context.run_ui(input, |ui| {
        response = Some(panel.show(ui, frame));
    });
    output.drop_without_applying_deltas();
    response.expect("the Editor frame should render the fixed panel")
}

fn render_editor_viewport_frame(
    context: &egui::Context,
    panel: &mut EguiSceneEditorPanel,
    frame: &EditorFrameModel,
    mut input: egui::RawInput,
    selected: EguiEditorViewport,
) -> EguiSceneEditorOutput {
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1280.0, 720.0),
    ));
    let mut response = None;
    let output = context.run_ui(input, |ui| {
        response = Some(panel.show_with_viewport(
            ui,
            frame,
            EguiCentralViewportInput::unavailable(),
            selected,
        ));
    });
    output.drop_without_applying_deltas();
    response.expect("the Editor frame should render the fixed viewport panel")
}
