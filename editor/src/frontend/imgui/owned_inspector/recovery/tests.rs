use std::time::{Duration, Instant};

use nara::{
    reflect::{ComponentFieldId, ComponentFieldPath, ComponentTypeId},
    scene::{ScenePlacement, ScenePlacementParent},
    tooling::{
        EditorActionBatch, EditorFieldTarget, SceneCompositionCommand, SceneCompositionEntityKind,
    },
};
use nara_text::bundled_font_face;
use nara_ui_kernel::UiSurfaceId;
use nara_ui_widgets::UiTextInputToken;

use super::*;
use crate::{
    EditorApplication, EditorLaunchOptions,
    frontend::imgui::{
        interaction_tests::{CUSTOM_COMPONENT, InteractionProject},
        submission::{self, PanelOrigin, Recipient},
    },
};

fn application(project: InteractionProject) -> (InteractionProject, EditorApplication) {
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() && Instant::now() < deadline {
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let model = application
        .current_frame()
        .expect("the ordinary fixture opens");
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementParent::Root),
                },
            })],
        ),
        Duration::ZERO,
    );
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: scene.selection.top_entity().unwrap().clone(),
                    component: ComponentTypeId::new(CUSTOM_COMPONENT),
                },
            })],
        ),
        Duration::ZERO,
    );
    (project, application)
}

fn new_controller() -> OwnedInspector {
    OwnedInspector::new(
        UiRootKey::runtime("editor.inspector", "recovery-tests").unwrap(),
        vec![bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn paint(
    controller: &mut OwnedInspector,
    fields: &mut FieldEditors,
    model: &EditorFrameModel,
    read_only: bool,
) -> Arc<UiFrame> {
    fields.begin_paint(model);
    controller
        .compile(
            model,
            fields,
            UiEnvironment::new(
                UiSurfaceId::new(91).unwrap(),
                600.0,
                3200.0,
                1.0,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
            read_only,
            None,
        )
        .unwrap()
}

fn control(frame: &Arc<UiFrame>, field: &str, role: UiSemanticRole, name: &str) -> UiKey {
    let group = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Group && node.name() == field)
        .unwrap_or_else(|| panic!("missing field group: {field}"));
    frame
        .semantics()
        .iter()
        .find(|node| {
            node.role() == role && node.name() == name && node.parent() == Some(group.key())
        })
        .unwrap_or_else(|| panic!("missing control: {field}/{name}"))
        .key()
        .clone()
}

fn replace(
    controller: &mut OwnedInspector,
    fields: &mut FieldEditors,
    frame: &Arc<UiFrame>,
    input: &UiKey,
    value: &str,
) -> UiTextInputToken {
    let token = controller.focus(frame, input).unwrap();
    controller
        .edit(frame, input, token, &TextEdit::SelectAll, fields)
        .unwrap();
    controller
        .edit(frame, input, token, &TextEdit::Insert(value.into()), fields)
        .unwrap();
    token
}

fn snapshots(fields: &FieldEditors) -> HashMap<FieldKey, (DisplayedDraft, Option<String>)> {
    fields
        .drafts
        .iter()
        .map(|(key, draft)| {
            (
                key.clone(),
                (DisplayedDraft::observe(draft), draft.error.clone()),
            )
        })
        .collect()
}

#[test]
fn scalar_copy_preserves_failed_clipboard_state_excludes_preedit_and_rejects_stale_controls() {
    let (_project, application) = application(InteractionProject::new_field_drafts());
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model, false);
    let input = control(&frame, "value", UiSemanticRole::TextInput, "value");
    let key = controller.live_field(&frame, &input).unwrap().key.clone();
    let token = replace(&mut controller, &mut fields, &frame, &input, "-");
    let frame = paint(&mut controller, &mut fields, &model, false);
    controller
        .edit(
            &frame,
            &input,
            token,
            &TextEdit::Preedit {
                text: "preedit".into(),
                cursor: Some((0, 7)),
            },
            &mut fields,
        )
        .unwrap();
    let frame = paint(&mut controller, &mut fields, &model, false);
    let copy = control(
        &frame,
        "value",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    let before = snapshots(&fields);
    assert!(fields.drafts[&key].error.is_some());
    let editor = controller.controls[&key].text().unwrap().editor();
    let display = editor.display_text().to_owned();
    let selection = editor.selection_bytes();
    assert!(editor.can_undo());
    assert!(!editor.can_redo());
    assert!(editor.is_composing());
    assert!(display.contains("preedit"));
    assert_eq!(controller.copy_draft(&frame, &copy, &fields).unwrap(), "-");
    controller
        .copy_finished(&frame, &copy, Err("clipboard unavailable".into()))
        .unwrap();
    assert!(
        snapshots(&fields) == before,
        "failed copying must preserve every draft fact"
    );
    assert_eq!(
        controller.controls[&key].copy_feedback.as_deref(),
        Some("Clipboard write failed: clipboard unavailable"),
    );
    let editor = controller.controls[&key].text().unwrap().editor();
    assert_eq!(editor.text(), "-");
    assert_eq!(editor.display_text(), display);
    assert_eq!(editor.selection_bytes(), selection);
    assert!(editor.can_undo());
    assert!(!editor.can_redo());
    assert!(editor.is_composing());
    controller
        .edit(
            &frame,
            &input,
            token,
            &TextEdit::CancelComposition,
            &mut fields,
        )
        .unwrap();
    let cancelled = paint(&mut controller, &mut fields, &model, false);
    assert!(
        !controller
            .invalidated_controls(&frame, &cancelled)
            .unwrap()
            .contains(&copy),
        "cancelling preedit must preserve Copy for the same committed candidate"
    );
    for (edit, expected) in [(TextEdit::Undo, "10"), (TextEdit::Redo, "-")] {
        controller
            .edit(&frame, &input, token, &edit, &mut fields)
            .unwrap();
        assert_eq!(fields.drafts[&key].candidate.copy_text(), expected);
    }
    controller.blur(&frame, &input).unwrap();
    let read_only = paint(&mut controller, &mut fields, &model, true);
    let copy = control(
        &read_only,
        "value",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    assert_eq!(controller.input_token(&read_only, &input).unwrap(), None);
    let before = snapshots(&fields);
    assert_eq!(
        controller.copy_draft(&read_only, &copy, &fields).unwrap(),
        "-"
    );
    controller.copy_finished(&read_only, &copy, Ok(())).unwrap();
    assert!(snapshots(&fields) == before);
    assert_eq!(
        controller.controls[&key].copy_feedback.as_deref(),
        Some("Retained input copied"),
    );
    let editable = paint(&mut controller, &mut fields, &model, false);
    replace(&mut controller, &mut fields, &editable, &input, "256");
    assert!(matches!(
        controller.copy_draft(&read_only, &copy, &fields),
        Err(OwnedInspectorError::ChangedDraft)
    ));
    let changed = paint(&mut controller, &mut fields, &model, false);
    let copy = control(
        &changed,
        "value",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    let discard = control(&changed, "value", UiSemanticRole::Button, "Discard");
    assert!(
        controller
            .activate(&changed, &discard, &model, &mut fields)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        controller.copy_draft(&changed, &copy, &fields),
        Err(OwnedInspectorError::RetiredField)
    ));
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        model.workspace().active_scene,
    );
}

#[test]
fn text_limit_refused_candidate_remains_exactly_copyable_without_a_text_control() {
    let (_project, application) = application(InteractionProject::new_field_drafts());
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model, false);
    let input = control(&frame, "label", UiSemanticRole::TextInput, "label");
    let key = controller.live_field(&frame, &input).unwrap().key.clone();
    let retained = "retained ".repeat(2048);
    let draft = fields.drafts.get_mut(&key).unwrap();
    draft.candidate = Candidate::Text(retained.clone());
    draft.dirty = true;
    draft.incompatible = true;
    let frame = paint(&mut controller, &mut fields, &model, false);
    assert!(controller.controls[&key].text().is_none());
    let copy = control(
        &frame,
        "label",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    let before = snapshots(&fields);
    assert_eq!(
        controller.copy_draft(&frame, &copy, &fields).unwrap(),
        retained
    );
    controller.copy_finished(&frame, &copy, Ok(())).unwrap();
    assert!(snapshots(&fields) == before);
    assert!(frame.diagnostics().is_empty());
    assert!(
        frame
            .semantics()
            .iter()
            .any(|node| { node.name().starts_with("Retained: ") && node.name().ends_with('…') })
    );
}

#[test]
fn structured_copy_keeps_invalid_and_unconfirmed_optional_list_and_map_input() {
    let (_project, application) = application(InteractionProject::new_recursive());
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    for (field, begin, text, expected) in [
        (
            "optional",
            "root: Some...",
            "1e100",
            "null\nUnconfirmed Some: 1e100",
        ),
        ("list", "root: Add...", "256", "3\nUnconfirmed item: 256"),
        (
            "map",
            "root: Insert...",
            "unfinished value",
            "first: before\nUnconfirmed entry pending key: unfinished value",
        ),
    ] {
        for name in ["root: Edit value", begin] {
            let frame = paint(&mut controller, &mut fields, &model, false);
            let button = control(&frame, field, UiSemanticRole::Button, name);
            assert!(
                controller
                    .activate(&frame, &button, &model, &mut fields)
                    .unwrap()
                    .is_none()
            );
        }
        if field == "map" {
            let frame = paint(&mut controller, &mut fields, &model, false);
            let input = control(&frame, field, UiSemanticRole::TextInput, "root.key");
            replace(&mut controller, &mut fields, &frame, &input, "pending key");
        }
        let frame = paint(&mut controller, &mut fields, &model, false);
        let input = control(&frame, field, UiSemanticRole::TextInput, "root.new");
        replace(&mut controller, &mut fields, &frame, &input, text);
        controller.blur(&frame, &input).unwrap();
        let frame = paint(&mut controller, &mut fields, &model, false);
        let key = controller.live_field(&frame, &input).unwrap().key.clone();
        let draft = &fields.drafts[&key];
        assert!(draft.candidate.value(&draft.basis.field.shape).is_err());
        assert_eq!(draft.candidate.copy_text(), expected);
        assert_eq!(controller.input_token(&frame, &input).unwrap(), None);
        let copy = control(&frame, field, UiSemanticRole::Button, "Copy retained input");
        let before = snapshots(&fields);
        assert_eq!(
            controller.copy_draft(&frame, &copy, &fields).unwrap(),
            expected
        );
        controller.copy_finished(&frame, &copy, Ok(())).unwrap();
        assert!(
            snapshots(&fields) == before,
            "copying must preserve pending child identities"
        );
    }
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        model.workspace().active_scene,
    );
}

#[test]
fn removed_component_keeps_copy_and_discard_without_edit_or_publication_authority() {
    let (_project, mut application) = application(InteractionProject::new_field_drafts());
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model, false);
    let input = control(&frame, "label", UiSemanticRole::TextInput, "label");
    let key = controller.live_field(&frame, &input).unwrap().key.clone();
    let token = replace(
        &mut controller,
        &mut fields,
        &frame,
        &input,
        "retained removed field",
    );
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::RemoveComponent {
                    entity: key.entity.clone(),
                    component: key.component.clone(),
                },
            })],
        ),
        Duration::ZERO,
    );
    let removed = application.current_frame().unwrap();
    assert!(current_basis(&removed, &key).is_none());
    let frame = paint(&mut controller, &mut fields, &removed, false);
    assert!(fields.drafts[&key].incompatible);
    assert!(
        !frame
            .semantics()
            .iter()
            .find(|node| node.key() == &input)
            .unwrap()
            .enabled()
    );
    assert!(controller.focus(&frame, &input).is_err());
    for name in ["Apply", "Reapply", "Reset"] {
        let button = control(&frame, "label", UiSemanticRole::Button, name);
        assert!(
            !frame
                .semantics()
                .iter()
                .find(|node| node.key() == &button)
                .unwrap()
                .enabled()
        );
        assert!(
            controller
                .activate(&frame, &button, &removed, &mut fields)
                .is_err()
        );
    }
    let copy = control(
        &frame,
        "label",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    let before = snapshots(&fields);
    assert_eq!(
        controller.copy_draft(&frame, &copy, &fields).unwrap(),
        "retained removed field"
    );
    assert!(snapshots(&fields) == before);
    let discard = control(&frame, "label", UiSemanticRole::Button, "Discard");
    assert!(
        controller
            .activate(&frame, &discard, &removed, &mut fields)
            .unwrap()
            .is_none()
    );
    assert!(!fields.drafts.contains_key(&key));
    assert!(controller.copy_draft(&frame, &copy, &fields).is_err());
    assert!(
        controller
            .edit(
                &frame,
                &input,
                token,
                &TextEdit::Insert("late".into()),
                &mut fields,
            )
            .is_err()
    );
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        removed.workspace().active_scene,
    );
}

#[test]
fn capacity_recovery_is_atomic_preserves_submissions_and_recreates_fresh_controls() {
    let (_project, application) = application(InteractionProject::new_field_drafts());
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let mut next_submission = 0;
    let mut submissions = Vec::new();
    let mut pending = Vec::new();
    let mut queued = None;
    for (slot, (field, value)) in [("value", "25"), ("other", "-3"), ("seconds", "2.5")]
        .into_iter()
        .enumerate()
    {
        let frame = paint(&mut controller, &mut fields, &model, false);
        let input = control(&frame, field, UiSemanticRole::TextInput, field);
        replace(&mut controller, &mut fields, &frame, &input, value);
        let frame = paint(&mut controller, &mut fields, &model, false);
        let apply = control(&frame, field, UiSemanticRole::Button, "Apply");
        let edit = controller
            .activate(&frame, &apply, &model, &mut fields)
            .unwrap()
            .unwrap();
        assert!(fields.drafts[&edit.key].queued);
        if slot < 2 {
            let id = submission::enqueue(
                &mut next_submission,
                &mut submissions,
                PanelOrigin {
                    instance: crate::workspace::EditorPanelInstanceId(1),
                    incarnation: Default::default(),
                },
                slot,
                Recipient::Authoring,
            )
            .unwrap();
            fields.admitted(&edit.key, Ok(id));
            pending.push((edit.key, id));
        } else {
            queued = Some(edit.key);
        }
    }
    let queued = queued.unwrap();
    let frame = paint(&mut controller, &mut fields, &model, false);
    let input = control(&frame, "label", UiSemanticRole::TextInput, "label");
    let key = controller.live_field(&frame, &input).unwrap().key.clone();
    let token = replace(
        &mut controller,
        &mut fields,
        &frame,
        &input,
        "recoverable label",
    );
    let basis = fields.drafts[&key].basis.clone();
    let mut retained = Vec::new();
    for index in 0..60 {
        let name = format!("retained{index}");
        let mut basis = basis.clone();
        let id = ComponentFieldId::new(&name);
        basis.field.id = id.clone();
        basis.field.path = ComponentFieldPath::from_fields([name.as_str()]);
        basis.target = EditorFieldTarget::new(
            key.document,
            basis.target.base_revision(),
            key.entity.clone(),
            key.component.clone(),
            basis.target.component_version(),
            id,
        );
        let retained_key = FieldKey::from_target(&basis.target);
        let mut draft = FieldDraft::new(basis);
        draft.candidate = Candidate::Text(format!("retained input {index}"));
        draft.dirty = true;
        draft.error = Some(format!("retained diagnostic {index}"));
        fields.drafts.insert(retained_key.clone(), draft);
        retained.push(retained_key);
    }
    assert_eq!(fields.retained_count(), 64);
    assert!(fields.at_capacity());
    let full = paint(&mut controller, &mut fields, &model, false);
    let recovery = control(
        &full,
        "Retained drafts",
        UiSemanticRole::Button,
        "Discard retained field drafts",
    );
    let copy = control(
        &full,
        "label",
        UiSemanticRole::Button,
        "Copy retained input",
    );
    assert!(
        full.semantics()
            .iter()
            .find(|node| node.key() == &recovery)
            .unwrap()
            .enabled()
    );
    fields.drafts.get_mut(&retained[0]).unwrap().candidate =
        Candidate::Text("changed after paint".into());
    let changed = snapshots(&fields);
    assert!(matches!(
        controller.activate(&full, &recovery, &model, &mut fields),
        Err(OwnedInspectorError::ChangedDraft)
    ));
    assert!(
        snapshots(&fields) == changed,
        "a stale snapshot must not discard any draft"
    );
    assert_eq!(fields.retained_count(), 64);
    let fresh = paint(&mut controller, &mut fields, &model, true);
    let current = control(
        &fresh,
        "Retained drafts",
        UiSemanticRole::Button,
        "Discard retained field drafts",
    );
    assert_eq!(current, recovery);
    assert!(
        controller
            .invalidated_controls(&full, &fresh)
            .unwrap()
            .contains(&recovery)
    );
    assert!(
        snapshots(&fields) == changed,
        "read-only painting must preserve in-flight drafts"
    );
    let eligible = fields
        .drafts
        .iter()
        .filter(|(_, draft)| draft.dirty && !draft.in_flight())
        .map(|(key, _)| key.clone())
        .collect::<HashSet<_>>();
    assert_eq!(eligible.len(), 61);
    let survivors = changed
        .into_iter()
        .filter(|(key, _)| !eligible.contains(key))
        .collect::<HashMap<_, _>>();
    assert!(
        controller
            .activate(&fresh, &current, &model, &mut fields)
            .unwrap()
            .is_none()
    );
    assert!(
        snapshots(&fields) == survivors,
        "all noncaptured draft facts must survive exactly"
    );
    assert_eq!(fields.retained_count(), 3);
    assert!(!fields.at_capacity());
    assert!(fields.drafts[&queued].queued);
    for (key, id) in &pending {
        assert_eq!(fields.drafts[key].pending, Some(*id));
        assert!(!fields.drafts[key].queued);
    }
    assert!(matches!(
        controller.copy_draft(&full, &copy, &fields),
        Err(OwnedInspectorError::RetiredField)
    ));
    assert!(matches!(
        controller.edit(
            &full,
            &input,
            token,
            &TextEdit::Insert("late".into()),
            &mut fields,
        ),
        Err(OwnedInspectorError::RetiredField)
    ));
    let reopened = paint(&mut controller, &mut fields, &model, false);
    let replacement = control(&reopened, "label", UiSemanticRole::TextInput, "label");
    assert_ne!(replacement, input);
    assert!(
        reopened
            .semantics()
            .iter()
            .find(|node| node.key() == &replacement)
            .unwrap()
            .enabled()
    );
    let fresh_token = controller.focus(&reopened, &replacement).unwrap();
    assert_ne!(fresh_token, token);
    assert!(
        controller
            .edit(
                &reopened,
                &replacement,
                token,
                &TextEdit::Insert("late".into()),
                &mut fields,
            )
            .is_err()
    );
    replace(
        &mut controller,
        &mut fields,
        &reopened,
        &replacement,
        "fresh label",
    );
    assert_eq!(fields.drafts[&key].candidate.copy_text(), "fresh label");
    assert!(!fields.drafts[&key].incompatible);
    assert_eq!(fields.retained_count(), 4);
    for (key, id) in pending {
        assert_eq!(fields.drafts[&key].pending, Some(id));
    }
    assert!(fields.drafts[&queued].queued);
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        model.workspace().active_scene,
    );
}
