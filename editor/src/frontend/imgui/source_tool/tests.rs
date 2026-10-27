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
        EditorAuthoringReadiness, EditorFrameClock, EditorPersistenceView, EditorPlayState,
        EditorPlayView, EditorProjectContentView, EditorProjectReadinessView, EditorProjectView,
        EditorReopenTarget, EditorRuntimeAvailability, EditorSourceToolIntegerProperty,
        EditorSourceToolView, EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceIntentView,
        SceneInspectorCommand,
    },
};

use super::super::{
    field_editor::FieldEditors,
    submission::{self, PanelOrigin, Recipient},
    ui_interaction_tests::{clear_controls, control_rect},
};
use super::*;

const COMPONENT: &str = "test.source-tool.Amount";
const FIELD: &str = "amount";

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
                "Increment###child-metadata".to_owned(),
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
