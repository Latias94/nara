use nara::{
    reflect::{
        ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind,
    },
    scene::{PrefabAuthoringSession, PrefabDocument, SceneEntityId},
    tooling::{
        EditorAction, EditorAtomicFieldEdit, EditorDocumentId, EditorFieldTarget, EditorFieldWrite,
        EditorGestureAction, EditorGestureCancelReason, EditorGestureController,
    },
};

use super::egui_workspace::{EguiPanelPresentation, RegisteredEguiLayout};
use crate::workspace::{EditorPanelTarget, EditorPanelWorkspace};

#[test]
fn hiding_the_gesture_owner_cancels_it_but_closing_another_inspector_does_not() {
    let mut workspace = EditorPanelWorkspace::default();
    let owner = workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    let other = workspace
        .open("nara.inspector", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let revision = PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([])).revision();
    let baseline = EditorAtomicFieldEdit::new(
        EditorFieldTarget::new(
            EditorDocumentId::from_raw(1),
            revision,
            SceneEntityId::new("ui-field").unwrap(),
            ComponentTypeId::new("nara.ui.UiNode"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("opacity"),
        ),
        [EditorFieldWrite::new(
            ComponentFieldId::new("opacity"),
            ComponentValueKind::F64,
            ComponentValue::f64(1.0).unwrap(),
        )
        .unwrap()],
    )
    .unwrap();
    let mut gestures = EditorGestureController::default();
    let _ = gestures.begin(baseline.clone());
    let gesture = gestures.active().unwrap();
    let mut presentation = EguiPanelPresentation::default();
    let begun = [EditorAction::Gesture(EditorGestureAction::Begin {
        baseline,
    })];
    presentation.observe_panel_actions(owner, &EditorPanelTarget::ActiveDocument, 0, &begun);
    let frame = empty_frame();
    presentation.observe_report(&nara::tooling::EditorActionBatchReport::new(
        frame.revision(),
        [nara::tooling::EditorActionOutcome::Gesture(
            nara::tooling::EditorGestureOutcome::Began(gesture.clone()),
        )],
    ));
    workspace.layout_mut().close(other);
    let mut actions = Vec::new();
    RegisteredEguiLayout {
        workspace: &mut workspace,
        presentation: &mut presentation,
        active_gesture: Some(gesture),
    }
    .reconcile_gesture(Some(gesture), &mut actions);
    assert!(actions.is_empty());
    workspace.layout_mut().set_visible(owner, false);
    RegisteredEguiLayout {
        workspace: &mut workspace,
        presentation: &mut presentation,
        active_gesture: Some(gesture),
    }
    .reconcile_gesture(Some(gesture), &mut actions);
    assert!(
        matches!(actions.as_slice(), [EditorAction::Gesture(EditorGestureAction::Cancel { gesture: id, reason: EditorGestureCancelReason::TargetRetired })] if *id == gesture.stamp().id())
    );
}

#[test]
fn a_rejected_second_begin_preserves_the_accepted_instance_owner() {
    use nara::tooling::{EditorActionBatchReport, EditorActionOutcome, EditorGestureOutcome};
    for different_field in [false, true] {
        let mut workspace = EditorPanelWorkspace::default();
        let first = workspace
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.inspector")
            .unwrap()
            .id();
        let second = workspace
            .open("nara.inspector", EditorPanelTarget::ActiveDocument)
            .unwrap();
        let revision =
            PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([])).revision();
        let baseline = field_baseline(revision, "opacity");
        let second_baseline =
            field_baseline(revision, if different_field { "scale" } else { "opacity" });
        let mut gestures = EditorGestureController::default();
        let accepted = gestures.begin(baseline.clone());
        let frame = empty_frame();
        let mut presentation = EguiPanelPresentation::default();
        presentation.observe_panel_actions(
            first,
            &EditorPanelTarget::ActiveDocument,
            0,
            &[EditorAction::Gesture(EditorGestureAction::Begin {
                baseline,
            })],
        );
        presentation.observe_report(&EditorActionBatchReport::new(
            frame.revision(),
            [EditorActionOutcome::Gesture(accepted)],
        ));
        presentation.observe_panel_actions(
            second,
            &EditorPanelTarget::ActiveDocument,
            0,
            &[EditorAction::Gesture(EditorGestureAction::Begin {
                baseline: second_baseline.clone(),
            })],
        );
        let rejected = gestures.begin(second_baseline);
        assert!(matches!(
            rejected,
            EditorGestureOutcome::Rejected(nara::tooling::EditorGestureRejection::AlreadyActive)
        ));
        presentation.observe_report(&EditorActionBatchReport::new(
            frame.revision(),
            [EditorActionOutcome::Gesture(rejected)],
        ));
        workspace.layout_mut().close(second);
        let mut actions = Vec::new();
        let gesture = gestures.active().unwrap();
        RegisteredEguiLayout {
            workspace: &mut workspace,
            presentation: &mut presentation,
            active_gesture: Some(gesture),
        }
        .reconcile_gesture(Some(gesture), &mut actions);
        assert!(
            actions.is_empty(),
            "closing a rejected Begin contributor must not cancel the first gesture"
        );
        workspace.layout_mut().close(first);
        RegisteredEguiLayout {
            workspace: &mut workspace,
            presentation: &mut presentation,
            active_gesture: Some(gesture),
        }
        .reconcile_gesture(Some(gesture), &mut actions);
        assert!(
            matches!(actions.as_slice(), [EditorAction::Gesture(EditorGestureAction::Cancel { gesture: id, .. })] if *id == gesture.stamp().id())
        );
    }
}

#[test]
fn accepted_begin_uses_its_batch_index_and_survives_a_later_batch_rejection() {
    use nara::tooling::{
        EditorActionAcknowledgement, EditorActionBatchReport, EditorActionOutcome,
        EditorActionRejection, EditorWorkspaceIntentAction,
    };
    let mut workspace = EditorPanelWorkspace::default();
    let first = workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    let second = workspace
        .open("nara.inspector", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let revision = PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([])).revision();
    let baseline = field_baseline(revision, "opacity");
    let mut gestures = EditorGestureController::default();
    let accepted = gestures.begin(baseline.clone());
    let frame = empty_frame();
    let mut presentation = EguiPanelPresentation::default();
    let actions = [
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
        EditorAction::Gesture(EditorGestureAction::Begin {
            baseline: baseline.clone(),
        }),
    ];
    presentation.observe_panel_actions(first, &EditorPanelTarget::ActiveDocument, 1, &actions);
    presentation.observe_report(&EditorActionBatchReport::new(
        frame.revision(),
        [
            EditorActionOutcome::Acknowledged(EditorActionAcknowledgement::WorkspaceIntent(false)),
            EditorActionOutcome::Gesture(accepted),
        ],
    ));
    presentation.observe_panel_actions(
        second,
        &EditorPanelTarget::ActiveDocument,
        0,
        &[EditorAction::Gesture(EditorGestureAction::Begin {
            baseline,
        })],
    );
    presentation.observe_rejection(EditorActionRejection::NoPublishedFrame);
    workspace.layout_mut().close(second);
    let gesture = gestures.active().unwrap();
    let mut actions = Vec::new();
    RegisteredEguiLayout {
        workspace: &mut workspace,
        presentation: &mut presentation,
        active_gesture: Some(gesture),
    }
    .reconcile_gesture(Some(gesture), &mut actions);
    assert!(
        actions.is_empty(),
        "a rejected batch must not replace the previously accepted owner"
    );
    workspace.layout_mut().close(first);
    RegisteredEguiLayout {
        workspace: &mut workspace,
        presentation: &mut presentation,
        active_gesture: Some(gesture),
    }
    .reconcile_gesture(Some(gesture), &mut actions);
    assert!(
        matches!(actions.as_slice(), [EditorAction::Gesture(EditorGestureAction::Cancel { gesture: id, .. })] if *id == gesture.stamp().id())
    );
}

#[test]
fn registered_authoring_view_tessellates_the_exact_presented_texture() {
    use nara::tooling_egui::{
        EguiCentralViewportInput, EguiEditorPanel, EguiEditorViewport, EguiGameViewInput,
        EguiSceneViewInput,
    };
    let texture = egui::TextureId::User(77);
    for size in [
        egui::vec2(720.0, 520.0),
        egui::vec2(800.0, 600.0),
        egui::vec2(1200.0, 800.0),
    ] {
        let context = egui::Context::default();
        let mut workspace = EditorPanelWorkspace::default();
        let mut presentation = EguiPanelPresentation::default();
        let frame = empty_frame();
        let mut panel = EguiEditorPanel::new();
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                let mut layout = RegisteredEguiLayout {
                    workspace: &mut workspace,
                    presentation: &mut presentation,
                    active_gesture: None,
                };
                let _ = panel.show_with_layout(
                    ui,
                    &frame,
                    None,
                    EguiCentralViewportInput::new(
                        EguiSceneViewInput::presented(texture, [640, 360]),
                        EguiGameViewInput::unavailable(),
                    ),
                    EguiEditorViewport::Authoring,
                    false,
                    &mut layout,
                );
            },
        );
        let primitives = context.tessellate(output.shapes.clone(), output.pixels_per_point);
        let samples_texture = primitives.iter().any(|primitive| matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture));
        output.drop_without_applying_deltas();
        assert!(
            samples_texture,
            "the registered view must sample its exact texture at logical window size {size:?}"
        );
    }
}

fn field_baseline(
    revision: nara::scene::SceneAuthoringRevision,
    field: &str,
) -> EditorAtomicFieldEdit {
    EditorAtomicFieldEdit::new(
        EditorFieldTarget::new(
            EditorDocumentId::from_raw(1),
            revision,
            SceneEntityId::new("ui-field").unwrap(),
            ComponentTypeId::new("nara.ui.UiNode"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new(field),
        ),
        [EditorFieldWrite::new(
            ComponentFieldId::new(field),
            ComponentValueKind::F64,
            ComponentValue::f64(1.0).unwrap(),
        )
        .unwrap()],
    )
    .unwrap()
}

fn empty_frame() -> nara::tooling::EditorFrameModel {
    use nara::{diagnostic::DiagnosticReport, tooling::*};
    EditorFrameClock::new().publish(
        EditorWorkspaceModel {
            active_document: None,
            documents: Vec::new(),
            scenes: Vec::new(),
            active_scene: None,
            diagnostics: DiagnosticReport::default(),
        },
        EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
            EditorProjectBrowserView::default(),
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::default(),
            EditorProjectContentView::Current {
                generation: EditorProjectContentGeneration::new(1),
            },
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
        EditorReopenTarget::Unavailable,
        None,
    )
}

#[test]
fn hiding_and_reopening_an_inspector_discards_its_cancelled_continuous_draft() {
    use nara::tooling::{EditorActionBatchReport, EditorActionOutcome};
    let (prefab, projection) = ui_projection();
    let revision = PrefabAuthoringSession::new(prefab).revision();
    let frame = ui_frame(revision);
    let mut workspace = EditorPanelWorkspace::default();
    let owner = workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    let mut presentation = EguiPanelPresentation::default();
    let mut gestures = EditorGestureController::default();
    let context = egui::Context::default();
    let _ = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        None,
        vec![],
    );
    let (_, _, position) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        None,
        vec![],
    );
    let position = position.expect("the actual UI Gap scalar control must be painted");
    let _ = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        None,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let (begun, _, _) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        None,
        vec![egui::Event::PointerMoved(position + egui::vec2(40.0, 0.0))],
    );
    let outcomes = begun
        .into_iter()
        .map(|action| match action {
            EditorAction::Gesture(EditorGestureAction::Begin { baseline }) => {
                EditorActionOutcome::Gesture(gestures.begin(baseline))
            }
            other => panic!("the drag must only begin its ordinary gesture, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes.len(),
        1,
        "a real DragValue change must begin a gesture"
    );
    presentation.observe_report(&EditorActionBatchReport::new(frame.revision(), outcomes));
    let (previewed, _, _) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        gestures.active(),
        vec![egui::Event::PointerMoved(position + egui::vec2(80.0, 0.0))],
    );
    let outcomes = previewed
        .into_iter()
        .map(|action| match action {
            EditorAction::Gesture(EditorGestureAction::Preview { gesture, edit }) => {
                EditorActionOutcome::Gesture(gestures.preview(gesture, edit))
            }
            other => panic!("the held drag must preview its ordinary gesture, got {other:?}"),
        })
        .collect::<Vec<_>>();
    presentation.observe_report(&EditorActionBatchReport::new(frame.revision(), outcomes));
    let (_, dragged_value, _) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        gestures.active(),
        vec![],
    );
    assert!(dragged_value.unwrap() > 0.0);
    workspace.layout_mut().set_visible(owner, false);
    let (cancelled, _, _) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        gestures.active(),
        vec![],
    );
    let outcomes = cancelled
        .into_iter()
        .map(|action| match action {
            EditorAction::Gesture(EditorGestureAction::Cancel { gesture, reason }) => {
                EditorActionOutcome::Gesture(gestures.cancel(gesture, reason))
            }
            other => panic!("hiding the owner must only cancel its gesture, got {other:?}"),
        })
        .collect::<Vec<_>>();
    presentation.observe_report(&EditorActionBatchReport::new(frame.revision(), outcomes));
    assert!(gestures.active().is_none());
    workspace.layout_mut().set_visible(owner, true);
    let (_, reopened_value, _) = draw_ui(
        &context,
        &mut workspace,
        &mut presentation,
        &frame,
        revision,
        &projection,
        None,
        vec![egui::Event::PointerButton {
            pos: position + egui::vec2(80.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(
        reopened_value,
        Some(0.0),
        "the cancelled draft must not replace the unchanged source value after reopen"
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_ui(
    context: &egui::Context,
    workspace: &mut EditorPanelWorkspace,
    presentation: &mut EguiPanelPresentation,
    frame: &nara::tooling::EditorFrameModel,
    revision: nara::scene::SceneAuthoringRevision,
    projection: &nara::ui::__private::UiAuthoringProjection,
    active: Option<&nara::tooling::EditorGestureView>,
    events: Vec<egui::Event>,
) -> (Vec<EditorAction>, Option<f64>, Option<egui::Pos2>) {
    use nara::{
        tooling::{EditorDocumentMutation, EditorSelectionSet},
        tooling_egui::*,
        ui::__private::*,
    };
    let mut selection = EditorSelectionSet::default();
    selection.select_entity(Some(SceneEntityId::new("root").unwrap()));
    let editable = |_: &UiAuthoringFieldKey| true;
    let mut lower = |key: &UiAuthoringFieldKey, value: UiAuthoringFieldValue| {
        let descriptor = projection.field(key).unwrap();
        let patch = descriptor
            .lower_edit(value)
            .map_err(|error| error.to_string())?;
        let writes = patch
            .operations
            .into_iter()
            .map(|operation| {
                let nara::scene::ScenePatchOperation::SetField { field, value, .. } = operation
                else {
                    panic!("UI scalar lowering emits only field writes")
                };
                EditorFieldWrite::new(field, value.kind(), value).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        EditorAtomicFieldEdit::new(
            EditorFieldTarget::new(
                EditorDocumentId::from_raw(1),
                revision,
                key.entity().clone(),
                key.component().clone(),
                descriptor.component_version(),
                key.primary_field().clone(),
            ),
            writes,
        )
        .map_err(|error| error.to_string())
    };
    let mut discrete = |_: &UiAuthoringFieldKey,
                        _: UiAuthoringFieldValue|
     -> Result<EditorDocumentMutation, String> {
        panic!("this proof only drags a continuous scalar")
    };
    let mut direct = |_: &UiAuthoringDirectEdit| -> Result<EditorAtomicFieldEdit, String> {
        panic!("the unavailable Canvas must not lower a direct edit")
    };
    let mut structure = |_: &UiStructureIntent| -> Result<EditorDocumentMutation, String> {
        panic!("this proof does not compose UI")
    };
    let images = UiImageChoiceSet::new(UiImageChoiceGeneration::new(1), []).unwrap();
    let mut authoring = EguiUiAuthoringInput::new(
        EditorDocumentId::from_raw(1),
        revision,
        &selection,
        active,
        Ok(projection),
        EguiUiAuthoringImages::new(images.generation(), Ok(&images)),
        EguiUiAuthoringMutations::new(
            &editable,
            true,
            true,
            &mut lower,
            &mut discrete,
            &mut direct,
            &mut structure,
        ),
    );
    let mut actions = Vec::new();
    let output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 1600.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let mut layout = RegisteredEguiLayout {
                workspace,
                presentation,
                active_gesture: active,
            };
            let _ = layout.show(
                ui,
                &mut EguiEditorPanel::new(),
                EguiEditorLayoutInput {
                    frame,
                    ui_authoring: Some(&mut authoring),
                    viewport: EguiCentralViewportInput::unavailable(),
                    selected_viewport: EguiEditorViewport::Authoring,
                },
                &mut actions,
                &mut Vec::new(),
            );
        },
    );
    let texts = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => {
                Some((text.pos, text.galley.rect, text.galley.job.text.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let control = texts
        .iter()
        .find(|(_, _, text)| *text == "Gap")
        .and_then(|(label, _, _)| {
            texts
                .iter()
                .filter(|(position, _, text)| {
                    position.x > label.x
                        && (position.y - label.y).abs() < 3.0
                        && text.parse::<f64>().is_ok()
                })
                .min_by(|left, right| left.0.x.total_cmp(&right.0.x))
        })
        .map(|(position, rect, text)| {
            (
                text.parse::<f64>().unwrap(),
                *position + rect.center().to_vec2(),
            )
        });
    let result = (
        actions,
        control.map(|(value, _)| value),
        control.map(|(_, position)| position),
    );
    output.drop_without_applying_deltas();
    result
}

fn ui_projection() -> (PrefabDocument, nara::ui::__private::UiAuthoringProjection) {
    use nara::{
        scene::{SceneComponentRecord, SceneEntityRecord},
        ui::__private::{UiAuthoringComponentKind, project_prefab_authoring},
    };
    let scalar = |value| ComponentValue::f64(value).unwrap();
    let dimension = |kind: &str, value| {
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("value", scalar(value)),
        ])
    };
    let style = ComponentValue::map([
        ("position_mode", ComponentValue::String("flow".to_owned())),
        (
            "flex_direction",
            ComponentValue::String("column".to_owned()),
        ),
        ("align_items", ComponentValue::String("stretch".to_owned())),
        (
            "justify_content",
            ComponentValue::String("start".to_owned()),
        ),
        ("gap", scalar(0.0)),
        (
            "padding",
            ComponentValue::map([
                ("left", scalar(0.0)),
                ("right", scalar(0.0)),
                ("top", scalar(0.0)),
                ("bottom", scalar(0.0)),
            ]),
        ),
        ("flex_grow", scalar(0.0)),
        ("flex_shrink", scalar(1.0)),
        ("width", dimension("percent", 1.0)),
        ("height", dimension("percent", 1.0)),
        ("left", dimension("px", 0.0)),
        ("top", dimension("px", 0.0)),
        ("z_index", ComponentValue::I64(0)),
        ("visible", ComponentValue::Bool(true)),
        ("overflow", ComponentValue::String("visible".to_owned())),
        ("stacking_context", ComponentValue::Bool(false)),
    ]);
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        UiAuthoringComponentKind::Root.component_type(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("layer", ComponentValue::String("base".to_owned()))]),
        ),
    )
    .with_component(
        UiAuthoringComponentKind::Node.component_type(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("style", style)]),
        ),
    )]);
    let projection = project_prefab_authoring(&prefab).unwrap();
    (prefab, projection)
}

fn ui_frame(revision: nara::scene::SceneAuthoringRevision) -> nara::tooling::EditorFrameModel {
    use nara::{diagnostic::DiagnosticReport, tooling::*};
    EditorFrameClock::new().publish(
        EditorWorkspaceModel {
            active_document: Some(EditorDocumentId::from_raw(1)),
            documents: Vec::new(),
            scenes: Vec::new(),
            active_scene: None,
            diagnostics: DiagnosticReport::default(),
        },
        EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
            EditorProjectBrowserView::default(),
            EditorPlayView::new(
                EditorPlayState::Empty,
                None,
                None,
                Some(revision),
                false,
                None,
            ),
            EditorPersistenceView::default(),
            EditorProjectContentView::Current {
                generation: EditorProjectContentGeneration::new(1),
            },
            EditorWorkspaceIntentView::default(),
        ),
        DiagnosticReport::default(),
        EditorReopenTarget::Unavailable,
        None,
    )
}
