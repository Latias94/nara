use std::sync::Arc;

use dear_imgui_rs::{Condition, Context, Key, MouseButton};
use nara::{
    asset::AssetRef,
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
        ComponentTypeId, ComponentValueKind, PersistentComponentSchemaOwnerId,
    },
    scene::{
        __private::AuthoringAdapterReceipt, AuthoringDocumentSource, SceneAuthoringSession,
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityRecord,
    },
    tooling::{
        EditorAction, EditorAuthoringReadiness, EditorDocumentMutation, EditorFrameClock,
        EditorPersistenceView, EditorPlayState, EditorPlayView, EditorProjectContentView,
        EditorProjectReadinessView, EditorProjectView, EditorReopenTarget,
        EditorRuntimeAvailability, EditorSourceToolIntegerProperty, EditorSourceToolView,
        EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceIntentView, SceneInspectorCommand,
    },
};
use nara_text::{TextEdit, bundled_font_face};
use nara_ui_kernel::{
    UiEnvironment, UiFrame, UiKey, UiRootKey, UiSemanticRole, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::UiTextInputToken;

use super::super::{
    submission::{self, PanelOrigin, Recipient},
    ui_interaction_tests::{clear_controls, control_rect},
};
use super::*;
use crate::frontend::controls::{
    field_drafts::FieldEditors,
    owned_inspector::{OwnedInspector, OwnedInspectorError},
};

const COMPONENT: &str = "test.source-tool.Amount";
const FIELD: &str = "amount";
const PARAMETER_LABEL: &str = "Increment###child-metadata";

struct Fixture {
    context: Context,
    workspace: EditorWorkspace,
    clock: EditorFrameClock,
    frame: EditorFrameModel,
    owner: ComponentSchemaOwnerSnapshot,
    fields: FieldEditors,
    tools: SourceToolWidgets,
    painted_actions: Vec<EditorSourceToolAction>,
    generic_actions: usize,
}

impl Fixture {
    fn new() -> Self {
        let owner_id = PersistentComponentSchemaOwnerId::try_new("test.source-tool.owner").unwrap();
        let catalog = ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Amount",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new(FIELD),
                    "Amount",
                    ComponentFieldPath::from_fields([FIELD]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                .with_authoring_initial_value(ComponentValue::map([(
                    FIELD,
                    ComponentValue::I64(10),
                )])),
            ],
            ..Default::default()
        };
        let owner =
            ComponentSchemaOwnerSnapshot::admit(owner_id.clone(), catalog.clone(), None).unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner_id.clone(),
            catalog,
            None,
        )])
        .unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("item").unwrap(),
        )
        .with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(FIELD, ComponentValue::I64(10))]),
            )
            .with_owner(owner_id),
        )]);
        let published =
            SceneDocumentCandidate::decode_json_bytes(source.to_json_string().unwrap().as_bytes())
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(AssetRef::path("source.scene.json").unwrap()),
                    &bundle,
                    &[adapter],
                )
                .unwrap()
                .publish();
        let mut workspace = EditorWorkspace::new();
        let document = workspace
            .open_scene_session(
                "Source",
                SceneAuthoringSession::from_published_authoring_document(published),
            )
            .unwrap()
            .opened_document
            .unwrap();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("item").unwrap()),
                })
                .applied
        );
        let mut clock = EditorFrameClock::new();
        let frame = publish(&mut clock, &workspace, EditorSourceToolView::default());
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([1000.0, 800.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut fixture = Self {
            context,
            workspace,
            clock,
            frame,
            owner,
            fields: FieldEditors::default(),
            tools: SourceToolWidgets::default(),
            painted_actions: Vec::new(),
            generic_actions: 0,
        };
        for _ in 0..4 {
            fixture.paint(true);
        }
        fixture
    }

    fn activate(&mut self, facet: bool) {
        let property = facet.then(|| {
            EditorSourceToolIntegerProperty::new(
                ComponentFieldId::new(FIELD),
                "increment".to_owned(),
                // Metadata containing ImGui's identity syntax must remain plain display text.
                PARAMETER_LABEL.to_owned(),
                7,
            )
            .unwrap()
        });
        let active = EditorSourceToolActive::admit(
            EditorSourceToolActivationId::issue(),
            self.owner.clone(),
            ComponentTypeId::new(COMPONENT),
            property,
        )
        .unwrap();
        self.frame = publish(
            &mut self.clock,
            &self.workspace,
            EditorSourceToolView {
                active: Some(active),
                ..Default::default()
            },
        );
    }

    fn paint(&mut self, visible: bool) {
        clear_controls();
        let scene = self.frame.workspace().active_scene.as_ref().unwrap();
        let entity = scene
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .unwrap();
        let component = &entity.components[0];
        let field = &component.fields[0];
        let target = EditorPropertyTarget {
            basis: document_basis(&self.frame, scene.document).unwrap(),
            entity: entity.id.clone(),
            component: component.component.clone(),
            property: ComponentPropertyPath::new(field.id.clone()),
        };
        self.fields.begin_paint(&self.frame);
        self.tools.begin_paint(&self.frame, Some(scene.document));
        let ui = self.context.frame();
        ui.window("Finite source-tool control")
            .position([20.0, 20.0], Condition::Always)
            .size([900.0, 700.0], Condition::Always)
            .build(|| {
                if !visible {
                    return;
                }
                let (customized, action) = self.tools.show_integer_property(
                    ui,
                    &self.frame,
                    &target,
                    field.value.as_ref().and_then(ComponentValue::as_i64),
                    true,
                    None,
                );
                self.painted_actions.extend(action);
                if !customized
                    && self
                        .fields
                        .show(
                            ui,
                            &self.frame,
                            scene,
                            &entity.id,
                            component,
                            field,
                            true,
                            &mut None,
                        )
                        .is_some()
                {
                    self.generic_actions += 1;
                }
            });
        self.tools.end_paint();
        let _ = self.context.render_legacy();
    }

    fn click(&mut self, label: &str) {
        let rect = control_rect(label).unwrap_or_else(|| panic!("missing control: {label}"));
        self.context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        self.paint(true);
        for down in [true, false] {
            self.context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, down);
            self.paint(true);
        }
        for _ in 0..3 {
            self.paint(true);
        }
    }

    fn type_text(&mut self, label: &str, text: &str) {
        self.click(label);
        self.context.io_mut().add_key_event(Key::ModCtrl, true);
        self.context.io_mut().add_key_event(Key::A, true);
        self.paint(true);
        self.context.io_mut().add_key_event(Key::A, false);
        self.context.io_mut().add_key_event(Key::ModCtrl, false);
        self.paint(true);
        self.context.io_mut().add_input_characters_utf8(text);
        self.paint(true);
    }

    fn confirm_parameter(&mut self) {
        for down in [true, false] {
            self.context.io_mut().add_key_event(Key::Enter, down);
            self.paint(true);
        }
    }
}

fn publish(
    clock: &mut EditorFrameClock,
    workspace: &EditorWorkspace,
    view: EditorSourceToolView,
) -> EditorFrameModel {
    clock
        .publish(
            workspace.authoring_model(),
            EditorProjectView::new(
                EditorProjectReadinessView::new(
                    EditorAuthoringReadiness::Complete,
                    EditorRuntimeAvailability::TrustedInProcess,
                ),
                Default::default(),
                EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                EditorPersistenceView::Idle,
                EditorProjectContentView::NotInstalled,
                EditorWorkspaceIntentView::new(None, None, None, None, None),
            ),
            Default::default(),
            EditorReopenTarget::Unavailable,
            None,
        )
        .with_source_tool(view)
}

#[test]
fn generic_dirty_input_survives_a_facet_while_tool_parameters_retire_without_publishing() {
    let mut fixture = Fixture::new();
    fixture.type_text(&format!("field:{COMPONENT}:{FIELD}"), "123");
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap().0,
        "123"
    );
    fixture.activate(true);
    fixture.paint(true);
    assert_eq!(fixture.tools.integer.as_ref().unwrap().value, 7);
    fixture.type_text("source-tool:integer:parameter", "99");
    assert!(fixture.tools.text_owner().is_some());
    fixture.confirm_parameter();
    assert_eq!(fixture.tools.integer.as_ref().unwrap().value, 99);
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap().0,
        "123"
    );
    let serial = fixture.tools.serial;
    fixture.paint(false);
    assert!(fixture.tools.integer.is_none());
    assert!(fixture.tools.serial > serial);
    fixture.paint(true);
    assert_eq!(fixture.tools.integer.as_ref().unwrap().value, 7);

    // Admitted source metadata remains installed while the executable facet is absent.
    fixture.activate(false);
    fixture.paint(true);
    assert!(fixture.tools.integer.is_none());
    assert!(control_rect(&format!("field:{COMPONENT}:{FIELD}")).is_some());
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap().0,
        "123"
    );
    assert_eq!(fixture.generic_actions, 0);
    assert!(fixture.painted_actions.is_empty());
    let scene = fixture.frame.workspace().active_scene.as_ref().unwrap();
    assert_eq!(scene.editor.inspector.history.undo_depth, 0);
    assert!(!scene.dirty);
    assert_eq!(
        scene
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components[0]
            .fields[0]
            .value,
        Some(ComponentValue::I64(10))
    );
}

#[test]
fn revision_change_retires_tool_input_but_preserves_generic_conflict_and_original_queued_intent() {
    let mut fixture = Fixture::new();
    fixture.type_text(&format!("field:{COMPONENT}:{FIELD}"), "123");
    fixture.activate(true);
    fixture.paint(true);
    fixture.type_text("source-tool:integer:parameter", "25");
    fixture.confirm_parameter();
    let original = fixture.tools.integer.as_ref().unwrap().target.clone();
    let activation = fixture.tools.integer.as_ref().unwrap().activation.clone();
    fixture.click("source-tool:integer:prepare");
    assert_eq!(fixture.painted_actions.len(), 1);
    let queued = fixture.painted_actions.pop().unwrap();
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::ApplyInspectorCommand {
                document: original.basis.document,
                base_revision: original.basis.revision,
                command: SceneInspectorCommand::SetField {
                    entity: original.entity.clone(),
                    component: original.component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new(FIELD),
                    value: ComponentValue::I64(15),
                },
            })
            .applied
    );
    fixture.frame = publish(
        &mut fixture.clock,
        &fixture.workspace,
        fixture.frame.source_tool().clone(),
    );
    fixture.paint(true);
    assert_eq!(fixture.tools.integer.as_ref().unwrap().value, 7);
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap().0,
        "123"
    );
    assert!(fixture.fields.retained_text(COMPONENT, FIELD).unwrap().1);
    assert!(
        matches!(queued, EditorSourceToolAction::IntegerProperty { activation: retained, target, value: 25 }
        if retained == activation && target == original)
    );
}

#[test]
fn predecessor_retirement_or_a_replacement_probe_does_not_disable_the_active_property() {
    for retiring in [true, false] {
        let mut fixture = Fixture::new();
        fixture.activate(true);
        let mut view = fixture.frame.source_tool().clone();
        view.retiring = retiring;
        view.probing = !retiring;
        let activation = view.active.as_ref().unwrap().id().clone();
        fixture.frame = publish(&mut fixture.clock, &fixture.workspace, view);
        fixture.paint(true);
        fixture.click("source-tool:integer:prepare");
        assert_eq!(fixture.painted_actions.len(), 1);
        assert!(matches!(&fixture.painted_actions[0],
            EditorSourceToolAction::IntegerProperty { activation: original, value: 7, .. }
                if original == &activation));
        assert!(fixture.frame.source_tool().has_pending_work());
    }
}

#[test]
fn unknown_submission_results_do_not_settle_the_controllers_own_pending_action() {
    let mut next = 0;
    let mut entries = Vec::new();
    let mut issue = || {
        submission::enqueue(
            &mut next,
            &mut entries,
            PanelOrigin {
                instance: crate::workspace::EditorPanelInstanceId(1),
                incarnation: Default::default(),
            },
            0,
            Recipient::Authoring,
        )
        .unwrap()
    };
    let own = issue();
    let other = issue();
    let mut widgets = SourceToolWidgets::default();
    assert!(
        widgets
            .offer(
                EditorSourceToolAction::Disable {
                    activation: EditorSourceToolActivationId::issue(),
                },
                SubmissionKind::Lifecycle
            )
            .is_some()
    );
    widgets.admitted(Ok(own));
    widgets.action_finished(
        other,
        &PanelActionDispatch::Dispatched(EditorActionOutcome::SourceTool(
            EditorSourceToolActionOutcome::Disabled(Ok(())),
        )),
    );
    assert!(widgets.pending.is_some_and(|(id, _)| id == own));
    assert!(widgets.feedback.is_none());
    widgets.action_finished(
        own,
        &PanelActionDispatch::Dispatched(EditorActionOutcome::SourceTool(
            EditorSourceToolActionOutcome::Disabled(Err(EditorSourceToolError::StaleActivation)),
        )),
    );
    assert!(widgets.pending.is_none());
    assert!(
        widgets
            .feedback
            .as_ref()
            .unwrap()
            .contains("no longer active")
    );
}

fn owned_inspector() -> OwnedInspector {
    OwnedInspector::new(
        UiRootKey::runtime("editor.inspector", "source-tool-tests").unwrap(),
        vec![bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn paint_owned(fixture: &mut Fixture, inspector: &mut OwnedInspector) -> Arc<UiFrame> {
    fixture.fields.begin_paint(&fixture.frame);
    fixture
        .tools
        .begin_paint(&fixture.frame, fixture.frame.workspace().active_document);
    let presentation = fixture.tools.owned_presentation(&fixture.frame, None, true);
    let frame = inspector
        .compile(
            &fixture.frame,
            &mut fixture.fields,
            UiEnvironment::new(
                UiSurfaceId::new(91).unwrap(),
                600.0,
                1600.0,
                1.0,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
            false,
            Some(&presentation),
        )
        .unwrap();
    fixture.tools.end_paint();
    frame
}

fn owned_control(frame: &UiFrame, role: UiSemanticRole, name: &str) -> UiKey {
    let group = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Group && node.name() == FIELD)
        .expect("the amount field has an owned group");
    frame
        .semantics()
        .iter()
        .find(|node| {
            node.role() == role && node.name() == name && node.parent() == Some(group.key())
        })
        .unwrap_or_else(|| panic!("missing owned control: {FIELD}/{name}"))
        .key()
        .clone()
}

fn replace_owned(
    inspector: &mut OwnedInspector,
    fixture: &mut Fixture,
    frame: &Arc<UiFrame>,
    input: &UiKey,
    value: &str,
) -> UiTextInputToken {
    let token = inspector.focus(frame, input).unwrap();
    inspector
        .edit(
            frame,
            input,
            token,
            &TextEdit::SelectAll,
            &mut fixture.fields,
        )
        .unwrap();
    inspector
        .edit(
            frame,
            input,
            token,
            &TextEdit::Insert(value.into()),
            &mut fixture.fields,
        )
        .unwrap();
    token
}

#[test]
fn owned_parameter_composition_and_control_retirement_preserve_the_ordinary_draft() {
    let mut fixture = Fixture::new();
    fixture.type_text(&format!("field:{COMPONENT}:{FIELD}"), "123");
    let generic = fixture.fields.retained_text(COMPONENT, FIELD).unwrap();
    let source = fixture.workspace.authoring_model();
    fixture.activate(true);
    let mut inspector = owned_inspector();
    let frame = paint_owned(&mut fixture, &mut inspector);
    let input = owned_control(&frame, UiSemanticRole::TextInput, PARAMETER_LABEL);
    let token = replace_owned(&mut inspector, &mut fixture, &frame, &input, "25");
    inspector
        .edit(
            &frame,
            &input,
            token,
            &TextEdit::Preedit {
                text: "preedit".into(),
                cursor: Some((0, 7)),
            },
            &mut fixture.fields,
        )
        .unwrap();
    let composing = paint_owned(&mut fixture, &mut inspector);
    assert!(
        inspector
            .prepare_tool_input(&composing, &input, &fixture.frame)
            .is_err()
    );
    inspector.input_error(
        &composing,
        &input,
        token,
        &mut fixture.fields,
        "clipboard refused".into(),
    );
    assert!(
        fixture
            .fields
            .drafts
            .values()
            .all(|draft| draft.error.is_none())
    );
    inspector.retire_tool_controls();
    assert!(
        inspector
            .edit(
                &composing,
                &input,
                token,
                &TextEdit::Commit("late".into()),
                &mut fixture.fields
            )
            .is_err()
    );
    let replacement = paint_owned(&mut fixture, &mut inspector);
    let current = owned_control(&replacement, UiSemanticRole::TextInput, PARAMETER_LABEL);
    assert_ne!(current, input);
    assert_eq!(
        replacement
            .semantics()
            .iter()
            .find(|node| node.key() == &current)
            .unwrap()
            .value(),
        Some("7")
    );
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap(),
        generic
    );
    assert_eq!(fixture.workspace.authoring_model(), source);
}

#[test]
fn owned_integer_parameter_is_local_validated_and_protected_after_queueing() {
    let mut fixture = Fixture::new();
    let source = fixture.workspace.authoring_model();
    let generic_key = fixture.fields.drafts.keys().next().unwrap().clone();
    let generic = fixture.fields.drafts[&generic_key].candidate.clone();
    fixture.activate(true);
    let activation = fixture
        .frame
        .source_tool()
        .active
        .as_ref()
        .unwrap()
        .id()
        .clone();
    let target = EditorPropertyTarget {
        basis: document_basis(&fixture.frame, generic_key.document).unwrap(),
        entity: generic_key.entity.clone(),
        component: generic_key.component.clone(),
        property: ComponentPropertyPath::new(generic_key.field.clone()),
    };
    let mut inspector = owned_inspector();
    let frame = paint_owned(&mut fixture, &mut inspector);
    let input = owned_control(&frame, UiSemanticRole::TextInput, PARAMETER_LABEL);
    replace_owned(&mut inspector, &mut fixture, &frame, &input, "-");
    let invalid = paint_owned(&mut fixture, &mut inspector);
    let prepare = owned_control(
        &invalid,
        UiSemanticRole::Button,
        "Prepare property proposal",
    );
    assert!(
        inspector
            .source_tool_action(&invalid, &prepare, &fixture.frame)
            .is_err()
    );
    assert!(fixture.tools.offered.is_none());
    assert_eq!(fixture.fields.drafts[&generic_key].candidate, generic);
    assert_eq!(fixture.workspace.authoring_model(), source);

    let token = replace_owned(&mut inspector, &mut fixture, &invalid, &input, "00025");
    let valid = paint_owned(&mut fixture, &mut inspector);
    let prepare = owned_control(&valid, UiSemanticRole::Button, "Prepare property proposal");
    let action = inspector
        .source_tool_action(&valid, &prepare, &fixture.frame)
        .unwrap();
    assert!(matches!(
        &action,
        EditorSourceToolAction::IntegerProperty {
            activation: original, target: original_target, value: 25,
        } if original == &activation && original_target == &target
    ));
    let action = fixture.tools.offer_owned(action).unwrap();
    assert!(matches!(
        fixture.tools.offered,
        Some(SubmissionKind::Integer)
    ));
    inspector.tool_queued(&valid, &input).unwrap();
    assert!(matches!(
        inspector.edit(
            &valid,
            &input,
            token,
            &TextEdit::Insert("9".into()),
            &mut fixture.fields,
        ),
        Err(OwnedInspectorError::Pending)
    ));
    assert!(
        inspector
            .source_tool_action(&valid, &prepare, &fixture.frame)
            .is_err()
    );
    assert!(fixture.tools.offer_owned(action).is_none());
    assert!(fixture.tools.integer.is_none());
    assert_eq!(fixture.fields.drafts.len(), 1);
    assert_eq!(fixture.fields.drafts[&generic_key].candidate, generic);
    assert!(fixture.fields.retained_text(COMPONENT, FIELD).is_none());
    assert_eq!(fixture.workspace.authoring_model(), source);
}

#[test]
fn owned_activation_replacement_retires_tokens_and_preserves_generic_dirty_input() {
    let mut fixture = Fixture::new();
    fixture.type_text(&format!("field:{COMPONENT}:{FIELD}"), "123");
    let generic = fixture.fields.retained_text(COMPONENT, FIELD).unwrap();
    let source = fixture.workspace.authoring_model();
    fixture.activate(true);
    let mut inspector = owned_inspector();
    let frame = paint_owned(&mut fixture, &mut inspector);
    let input = owned_control(&frame, UiSemanticRole::TextInput, PARAMETER_LABEL);
    let token = replace_owned(&mut inspector, &mut fixture, &frame, &input, "99");
    let original = paint_owned(&mut fixture, &mut inspector);
    let prepare = owned_control(
        &original,
        UiSemanticRole::Button,
        "Prepare property proposal",
    );

    fixture.activate(true);
    assert!(
        inspector
            .source_tool_action(&original, &prepare, &fixture.frame)
            .is_err()
    );
    let replacement = paint_owned(&mut fixture, &mut inspector);
    let replacement_input = owned_control(&replacement, UiSemanticRole::TextInput, PARAMETER_LABEL);
    assert_ne!(replacement_input, input);
    assert!(
        inspector
            .edit(
                &original,
                &input,
                token,
                &TextEdit::Insert("late".into()),
                &mut fixture.fields,
            )
            .is_err()
    );
    inspector.focus(&replacement, &replacement_input).unwrap();
    assert!(
        inspector
            .edit(
                &replacement,
                &replacement_input,
                token,
                &TextEdit::Insert("late".into()),
                &mut fixture.fields,
            )
            .is_err()
    );
    let prepare = owned_control(
        &replacement,
        UiSemanticRole::Button,
        "Prepare property proposal",
    );
    assert!(matches!(
        inspector
            .source_tool_action(&replacement, &prepare, &fixture.frame)
            .unwrap(),
        EditorSourceToolAction::IntegerProperty { activation, value: 7, .. }
            if &activation == fixture.frame.source_tool().active.as_ref().unwrap().id()
    ));

    fixture.frame = publish(
        &mut fixture.clock,
        &fixture.workspace,
        EditorSourceToolView::default(),
    );
    let restored = paint_owned(&mut fixture, &mut inspector);
    let generic_input = owned_control(&restored, UiSemanticRole::TextInput, FIELD);
    assert_eq!(
        restored
            .semantics()
            .iter()
            .find(|node| node.key() == &generic_input)
            .unwrap()
            .value(),
        Some("123")
    );
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap(),
        generic
    );
    assert!(fixture.tools.integer.is_none());
    assert_eq!(fixture.workspace.authoring_model(), source);
}

#[test]
fn owned_source_change_retires_tool_input_and_restores_generic_conflict_recovery() {
    let mut fixture = Fixture::new();
    let mut inspector = owned_inspector();
    let generic = paint_owned(&mut fixture, &mut inspector);
    let generic_input = owned_control(&generic, UiSemanticRole::TextInput, FIELD);
    replace_owned(
        &mut inspector,
        &mut fixture,
        &generic,
        &generic_input,
        "123",
    );
    fixture.activate(true);
    let frame = paint_owned(&mut fixture, &mut inspector);
    let input = owned_control(&frame, UiSemanticRole::TextInput, PARAMETER_LABEL);
    let token = replace_owned(&mut inspector, &mut fixture, &frame, &input, "25");
    let original = paint_owned(&mut fixture, &mut inspector);
    let prepare = owned_control(
        &original,
        UiSemanticRole::Button,
        "Prepare property proposal",
    );
    let action = inspector
        .source_tool_action(&original, &prepare, &fixture.frame)
        .unwrap();
    let EditorSourceToolAction::IntegerProperty {
        activation,
        target,
        value: 25,
    } = &action
    else {
        panic!("the owned parameter must produce an exact integer property action")
    };
    let activation = activation.clone();
    let target = target.clone();
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::ApplyInspectorCommand {
                document: target.basis.document,
                base_revision: target.basis.revision,
                command: SceneInspectorCommand::SetField {
                    entity: target.entity.clone(),
                    component: target.component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new(FIELD),
                    value: ComponentValue::I64(15),
                },
            })
            .applied
    );
    fixture.frame = publish(
        &mut fixture.clock,
        &fixture.workspace,
        fixture.frame.source_tool().clone(),
    );
    let changed_source = fixture.workspace.authoring_model();
    assert!(
        inspector
            .source_tool_action(&original, &prepare, &fixture.frame)
            .is_err()
    );
    let replacement = paint_owned(&mut fixture, &mut inspector);
    let replacement_input = owned_control(&replacement, UiSemanticRole::TextInput, PARAMETER_LABEL);
    assert_ne!(replacement_input, input);
    assert!(
        inspector
            .edit(
                &original,
                &input,
                token,
                &TextEdit::Insert("late".into()),
                &mut fixture.fields,
            )
            .is_err()
    );
    assert_eq!(
        fixture.fields.retained_text(COMPONENT, FIELD).unwrap(),
        ("123".into(), true, false, false)
    );
    assert!(matches!(
        action,
        EditorSourceToolAction::IntegerProperty {
            activation: original_activation, target: original_target, value: 25,
        } if original_activation == activation && original_target == target
    ));

    fixture.frame = publish(
        &mut fixture.clock,
        &fixture.workspace,
        EditorSourceToolView::default(),
    );
    let restored = paint_owned(&mut fixture, &mut inspector);
    let generic_input = owned_control(&restored, UiSemanticRole::TextInput, FIELD);
    assert_eq!(
        restored
            .semantics()
            .iter()
            .find(|node| node.key() == &generic_input)
            .unwrap()
            .value(),
        Some("123")
    );
    let apply = owned_control(&restored, UiSemanticRole::Button, "Apply");
    assert!(
        inspector
            .activate(&restored, &apply, &fixture.frame, &mut fixture.fields)
            .is_err()
    );
    let reapply = owned_control(&restored, UiSemanticRole::Button, "Reapply");
    let prepared = inspector
        .activate(&restored, &reapply, &fixture.frame, &mut fixture.fields)
        .unwrap()
        .expect("explicit Reapply retains the ordinary field recovery path");
    assert!(matches!(
        prepared.action,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            document, base_revision,
            command: SceneInspectorCommand::SetField { value: ComponentValue::I64(123), .. },
        }) if document == target.basis.document
            && base_revision == fixture.frame.workspace().active_scene.as_ref().unwrap().revision
    ));
    assert_eq!(fixture.workspace.authoring_model(), changed_source);
}
