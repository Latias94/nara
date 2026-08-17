use std::collections::BTreeSet;

use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    PrefabDocument, SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePlacement, ScenePlacementOwner,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorAtomicFieldEdit, EditorCloseDecision, EditorDocumentId,
    EditorDocumentMutation, EditorExternalReloadState, EditorFieldFeedback,
    EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldWrite, EditorFrameClock,
    EditorFrameModel, EditorNavigationAction, EditorPersistenceAction, EditorPersistenceRejection,
    EditorPersistenceRequestKind, EditorPersistenceResult, EditorPersistenceView, EditorPlayAction,
    EditorPlayOperation, EditorPlayOperationResult, EditorPlayState, EditorPlayView,
    EditorProjectView, EditorReopenTarget, EditorSceneModel, EditorSceneTabModel,
    EditorSelectionSet, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentView, EditorWorkspaceModel,
    SceneCompositionCommand, SceneCompositionComponentKind, SceneCompositionEntityKind,
    SceneEditorModel, SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorEntityRow,
    SceneInspectorEntityView, SceneInspectorFieldState, SceneInspectorFieldView,
    SceneInspectorModel,
};

use crate::{
    EguiCentralViewportInput, EguiEditorOutput, EguiEditorPanel, EguiEditorViewport,
    EguiEditorViewportIntent, EguiUiAuthoringInput,
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
    let mut panel = EguiEditorPanel::new();
    let context = egui::Context::default();

    let output = click_viewport_control(
        &context,
        &mut panel,
        &running,
        EguiEditorViewport::Game,
        "Authoring tab",
    );
    let (actions, _, intent) = output.into_parts();

    assert!(actions.into_actions().is_empty());
    assert_eq!(intent, Some(EguiEditorViewportIntent::SelectAuthoring));
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
        let mut panel = EguiEditorPanel::new();
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
        let mut panel = EguiEditorPanel::new();
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
    let mut panel = EguiEditorPanel::new();
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
fn prefab_source_navigation_is_explicit_in_both_directions() {
    let mut scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        true,
    );
    scene.editor.inspector.entities[0].has_prefab = true;
    scene
        .editor
        .inspector
        .selected_entity_view
        .as_mut()
        .expect("the selected entity view must exist")
        .has_prefab = true;
    let frame = default_frame(Some(scene.clone()));
    assert_eq!(
        click_control(&mut EguiEditorPanel::new(), &frame, "Open Prefab Source"),
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: SceneEntityId::new("player").unwrap(),
                }
            )]
        )
    );

    let prefab = EditorDocumentId::from_raw(2);
    let prefab_frame = frame_with_active_prefab(scene.clone(), prefab, revision());
    assert_eq!(
        click_control(&mut EguiEditorPanel::new(), &prefab_frame, "Back to Scene"),
        EditorActionBatch::new(
            prefab_frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SetActiveDocument {
                    document: scene.document,
                }
            )]
        )
    );
}

#[test]
fn ui_hierarchy_selection_uses_the_exact_prefab_document_identity() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let prefab = EditorDocumentId::from_raw(2);
    let prefab_revision = revision();
    let frame = frame_with_active_prefab(scene, prefab, prefab_revision);
    let projection = test_ui_projection();
    let selection = EditorSelectionSet::default();
    let mut panel = EguiEditorPanel::new();
    let context = egui::Context::default();

    let batch = click_ui_authoring_control(
        &context,
        &mut panel,
        &frame,
        prefab,
        prefab_revision,
        &selection,
        &projection,
        "ui-node:button",
    );

    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: prefab,
                    entity: Some(SceneEntityId::new("button").unwrap()),
                }
            )]
        )
    );
}

#[test]
fn typed_ui_boolean_control_emits_one_exact_prefab_edit() {
    let scene = scene_model(
        revision(),
        false,
        SceneAuthoringHistoryStatus::default(),
        false,
    );
    let prefab = EditorDocumentId::from_raw(2);
    let prefab_revision = revision();
    let frame = frame_with_active_prefab(scene, prefab, prefab_revision);
    let projection = test_ui_projection();
    let mut selection = EditorSelectionSet::default();
    selection.select_entity(Some(SceneEntityId::new("button").unwrap()));
    let enabled = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity().as_str() == "button"
                && field.key().component().as_str() == "nara.ui.UiButton"
                && field.key().primary_field().as_str() == "enabled"
        })
        .expect("the representative button must expose enabled");
    let expected = test_lower_ui_field_edit(
        prefab,
        prefab_revision,
        &projection,
        enabled.key(),
        nara_ui::__private::UiAuthoringFieldValue::Bool(false),
    )
    .unwrap();

    let batch = click_ui_authoring_control(
        &egui::Context::default(),
        &mut EguiEditorPanel::new(),
        &frame,
        prefab,
        prefab_revision,
        &selection,
        &projection,
        "ui-field:button:nara.ui.UiButton:enabled",
    );

    assert_eq!(
        batch,
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyPrefabFieldEdit { edit: expected }
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

    let add_root = click_control(&mut EguiEditorPanel::new(), &frame, "Add Root");
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

    let add_sprite = click_control(&mut EguiEditorPanel::new(), &frame, "+ Sprite");
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

    let batch = click_control(&mut EguiEditorPanel::new(), &frame, "Add Root");

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
    let mut panel = EguiEditorPanel::new();
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
    let mut panel = EguiEditorPanel::new();
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
    let mut panel = EguiEditorPanel::new();
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
    let mut panel = EguiEditorPanel::new();

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
    let mut panel = EguiEditorPanel::new();
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

    let retry = click_control(&mut EguiEditorPanel::new(), &frame, "Save");
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
    let mut panel = EguiEditorPanel::new();
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

fn frame_with_active_prefab(
    scene: EditorSceneModel,
    prefab: EditorDocumentId,
    prefab_revision: nara_scene::SceneAuthoringRevision,
) -> EditorFrameModel {
    let scene_tab = EditorSceneTabModel {
        document: scene.document,
        title: scene.title.clone(),
        dirty: scene.dirty,
        revision: scene.revision,
        external_reload: scene.external_reload,
        active: false,
    };
    EditorFrameClock::new().publish(
        EditorWorkspaceModel {
            active_document: Some(prefab),
            documents: vec![
                nara_tooling::EditorDocumentTabModel {
                    document: scene.document,
                    kind: nara_tooling::EditorDocumentKind::Scene,
                    title: scene.title,
                    dirty: scene.dirty,
                    revision: scene.revision,
                    saved_revision: scene.saved_revision,
                    history: scene.editor.inspector.history,
                    source_binding: None,
                    external_reload: scene.external_reload,
                    active: false,
                },
                nara_tooling::EditorDocumentTabModel {
                    document: prefab,
                    kind: nara_tooling::EditorDocumentKind::Prefab,
                    title: "Interface.prefab".to_owned(),
                    dirty: false,
                    revision: prefab_revision,
                    saved_revision: prefab_revision,
                    history: SceneAuthoringHistoryStatus::default(),
                    source_binding: None,
                    external_reload: EditorExternalReloadState::Clean,
                    active: true,
                },
            ],
            scenes: vec![scene_tab],
            active_scene: None,
            diagnostics: DiagnosticReport::default(),
        },
        EditorProjectView::new(
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
        EditorReopenTarget::OpenDocument {
            document: prefab,
            base_revision: prefab_revision,
        },
        None,
    )
}

fn test_ui_projection() -> nara_ui::__private::UiAuthoringProjection {
    use nara_ui::__private::{UiAuthoringComponentKind, project_prefab_authoring};

    let root = SceneEntityId::new("root").unwrap();
    let button = SceneEntityId::new("button").unwrap();
    let component = |value| SceneComponentRecord::new(ComponentSchemaVersion::ONE, value);
    let prefab = PrefabDocument::new(
        [root.clone()],
        [
            SceneEntityRecord::new(root.clone())
                .with_children([button.clone()])
                .with_component(
                    UiAuthoringComponentKind::Root.component_type(),
                    component(ComponentValue::map([(
                        "layer",
                        ComponentValue::String("base".to_owned()),
                    )])),
                )
                .with_component(
                    UiAuthoringComponentKind::Node.component_type(),
                    component(test_ui_node_value(
                        "flow",
                        ("percent", 1.0),
                        ("percent", 1.0),
                    )),
                ),
            SceneEntityRecord::new(button)
                .with_component(
                    UiAuthoringComponentKind::Node.component_type(),
                    component(test_ui_node_value("absolute", ("px", 180.0), ("px", 48.0))),
                )
                .with_component(
                    UiAuthoringComponentKind::Button.component_type(),
                    component(ComponentValue::map([
                        ("label", ComponentValue::String("Retry".to_owned())),
                        ("action", ComponentValue::String("test.retry".to_owned())),
                        ("enabled", ComponentValue::Bool(true)),
                        ("logical_text_size", ComponentValue::f64(20.0).unwrap()),
                    ])),
                ),
        ],
    );
    project_prefab_authoring(&prefab).expect("the representative UI Prefab must project")
}

fn test_ui_node_value(position: &str, width: (&str, f64), height: (&str, f64)) -> ComponentValue {
    ComponentValue::map([(
        "style",
        ComponentValue::map([
            ("position_mode", ComponentValue::String(position.to_owned())),
            (
                "flex_direction",
                ComponentValue::String("column".to_owned()),
            ),
            ("align_items", ComponentValue::String("stretch".to_owned())),
            (
                "justify_content",
                ComponentValue::String("start".to_owned()),
            ),
            ("gap", ComponentValue::f64(0.0).unwrap()),
            (
                "padding",
                ComponentValue::map([
                    ("left", ComponentValue::f64(0.0).unwrap()),
                    ("right", ComponentValue::f64(0.0).unwrap()),
                    ("top", ComponentValue::f64(0.0).unwrap()),
                    ("bottom", ComponentValue::f64(0.0).unwrap()),
                ]),
            ),
            ("flex_grow", ComponentValue::f64(0.0).unwrap()),
            ("flex_shrink", ComponentValue::f64(1.0).unwrap()),
            ("width", test_ui_dimension(width.0, width.1)),
            ("height", test_ui_dimension(height.0, height.1)),
            ("left", test_ui_dimension("px", 10.0)),
            ("top", test_ui_dimension("px", 20.0)),
            ("z_index", ComponentValue::I64(0)),
            ("visible", ComponentValue::Bool(true)),
            ("overflow", ComponentValue::String("visible".to_owned())),
            ("stacking_context", ComponentValue::Bool(false)),
        ]),
    )])
}

fn test_ui_dimension(kind: &str, value: f64) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_owned())),
        ("value", ComponentValue::f64(value).unwrap()),
    ])
}

fn test_lower_ui_field_edit(
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    projection: &nara_ui::__private::UiAuthoringProjection,
    key: &nara_ui::__private::UiAuthoringFieldKey,
    value: nara_ui::__private::UiAuthoringFieldValue,
) -> Result<EditorAtomicFieldEdit, String> {
    let descriptor = projection
        .field(key)
        .ok_or_else(|| "field is absent from the projection".to_owned())?;
    let patch = descriptor
        .lower_edit(value)
        .map_err(|error| error.to_string())?;
    let writes = patch
        .operations
        .into_iter()
        .map(|operation| {
            let nara_scene::ScenePatchOperation::SetField { field, value, .. } = operation else {
                return Err("descriptor emitted a non-field operation".to_owned());
            };
            EditorFieldWrite::new(field, value.kind(), value).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    EditorAtomicFieldEdit::new(
        EditorFieldTarget::new(
            document,
            revision,
            key.entity().clone(),
            key.component().clone(),
            descriptor.component_version(),
            key.primary_field().clone(),
        ),
        writes,
    )
    .map_err(|error| error.to_string())
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
    let history = scene
        .as_ref()
        .map(|scene| scene.editor.inspector.history)
        .unwrap_or_default();
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
                    history,
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
    panel: &mut EguiEditorPanel,
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

#[allow(clippy::too_many_arguments)]
fn click_ui_authoring_control(
    context: &egui::Context,
    panel: &mut EguiEditorPanel,
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    selection: &EditorSelectionSet,
    projection: &nara_ui::__private::UiAuthoringProjection,
    label: &str,
) -> EditorActionBatch {
    let render = |panel: &mut EguiEditorPanel, input| {
        render_ui_authoring_frame(
            context, panel, frame, document, revision, selection, projection, input,
        )
    };
    let _ = render(panel, egui::RawInput::default());
    let _ = render(panel, egui::RawInput::default());
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
    let _ = render(panel, pressed);

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
    render(panel, released)
}

fn click_viewport_control(
    context: &egui::Context,
    panel: &mut EguiEditorPanel,
    frame: &EditorFrameModel,
    selected: EguiEditorViewport,
    label: &str,
) -> EguiEditorOutput {
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
    panel: &mut EguiEditorPanel,
    frame: &EditorFrameModel,
    input: egui::RawInput,
) -> EditorActionBatch {
    render_editor_frame_at(context, panel, frame, input, egui::vec2(1280.0, 720.0))
}

fn render_editor_frame_at(
    context: &egui::Context,
    panel: &mut EguiEditorPanel,
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

#[allow(clippy::too_many_arguments)]
fn render_ui_authoring_frame(
    context: &egui::Context,
    panel: &mut EguiEditorPanel,
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    selection: &EditorSelectionSet,
    projection: &nara_ui::__private::UiAuthoringProjection,
    mut input: egui::RawInput,
) -> EditorActionBatch {
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1280.0, 1600.0),
    ));
    let mut lower_field =
        |key: &nara_ui::__private::UiAuthoringFieldKey,
         value: nara_ui::__private::UiAuthoringFieldValue| {
            test_lower_ui_field_edit(document, revision, projection, key, value)
        };
    let mut lower_direct = |_edit: &nara_ui::__private::UiAuthoringDirectEdit|
     -> Result<EditorAtomicFieldEdit, String> {
        panic!("hierarchy selection must not lower a direct edit")
    };
    let authoring = EguiUiAuthoringInput::new(
        document,
        revision,
        selection,
        None,
        Ok(projection),
        &mut lower_field,
        &mut lower_direct,
    );
    let mut authoring = Some(authoring);
    let mut response = None;
    let output = context.run_ui(input, |ui| {
        response = Some(
            panel
                .show_with_authoring(
                    ui,
                    frame,
                    Some(
                        authoring
                            .take()
                            .expect("egui renders the authoring frame exactly once"),
                    ),
                    EguiCentralViewportInput::unavailable(),
                    EguiEditorViewport::Authoring,
                )
                .into_parts()
                .0,
        );
    });
    output.drop_without_applying_deltas();
    response.expect("the UI authoring frame should render")
}

fn render_editor_viewport_frame(
    context: &egui::Context,
    panel: &mut EguiEditorPanel,
    frame: &EditorFrameModel,
    mut input: egui::RawInput,
    selected: EguiEditorViewport,
) -> EguiEditorOutput {
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
