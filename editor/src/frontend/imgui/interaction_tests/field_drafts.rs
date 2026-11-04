use super::owned::InspectorOperation;
use super::*;
use nara::{
    reflect::{
        ComponentFloat, ComponentNumericBound, ComponentNumericDomain, ComponentNumericInterval,
        ComponentValueShape,
    },
    tooling::{EditorDocumentMutation, SceneInspectorCommand},
};

pub(super) fn configure(project: &InteractionProject) {
    let fields = [
        (
            "value",
            ComponentValueShape::scalar(ComponentValueKind::U64)
                .with_numeric_domain(ComponentNumericDomain::U8),
        ),
        (
            "label",
            ComponentValueShape::scalar(ComponentValueKind::String),
        ),
        (
            "other",
            ComponentValueShape::scalar(ComponentValueKind::I64),
        ),
        (
            "flag",
            ComponentValueShape::scalar(ComponentValueKind::Bool),
        ),
        (
            "seconds",
            ComponentValueShape::scalar(ComponentValueKind::F64)
                .with_numeric_domain(ComponentNumericDomain::F32)
                .with_numeric_interval(ComponentNumericInterval::F64 {
                    min: Some(ComponentNumericBound::Included(
                        ComponentFloat::new(0.0).unwrap(),
                    )),
                    max: Some(ComponentNumericBound::Included(
                        ComponentFloat::new(30.0).unwrap(),
                    )),
                }),
        ),
    ];
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Field drafts",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(fields.into_iter().map(|(name, shape)| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    }))
    .with_authoring_initial_value(ComponentValue::map([
        ("value", ComponentValue::U64(10)),
        ("label", ComponentValue::String("before".into())),
        ("other", ComponentValue::I64(0)),
        ("flag", ComponentValue::Bool(false)),
        ("seconds", ComponentValue::f64(0.5).unwrap()),
    ]));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
}

fn setup(project: &InteractionProject) -> Interaction {
    let mut interaction = Interaction::new(project);
    interaction.click_hierarchy("+ Root");
    interaction.click_owned(None, "Add Component");
    interaction.click_owned(None, &format!("Add {CUSTOM_COMPONENT}"));
    interaction
}

fn draft(interaction: &Interaction, field: &str) -> Option<(String, bool, bool, bool)> {
    interaction
        .frontend
        .retained_field_draft(CUSTOM_COMPONENT, field)
}

fn change_other(interaction: &mut Interaction) {
    let frame = interaction.frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let entity = scene.selection.top_entity().unwrap().clone();
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: scene.document,
                    base_revision: scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity,
                        component: ComponentTypeId::new(CUSTOM_COMPONENT),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("other"),
                        value: ComponentValue::I64(7),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
}

#[test]
fn actual_scalar_controls_retain_invalid_u8_and_refuse_unbounded_text_until_correction() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    let history = interaction
        .frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .history
        .undo_depth;
    interaction.replace_field("value", "256");
    assert_eq!(draft(&interaction, "value").unwrap().0, "256");
    assert!(
        interaction
            .frontend
            .retained_field_error(CUSTOM_COMPONENT, "value")
            .is_some(),
        "the shared numeric-domain validator explains u8 overflow before submission"
    );
    assert!(
        interaction
            .field_operation("value", InspectorOperation::Apply)
            .is_err()
    );
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
    assert_eq!(draft(&interaction, "value").unwrap().0, "256");
    let long = "9".repeat(1024);
    interaction.replace_field("value", &long);
    assert_eq!(draft(&interaction, "value").unwrap().0, long);
    assert!(
        interaction
            .field_operation("value", InspectorOperation::Apply)
            .is_err()
    );
    assert!(
        interaction
            .try_replace_owned_text("value", "value", &"9".repeat(8256))
            .is_err()
    );
    assert_eq!(draft(&interaction, "value").unwrap().0, long);
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .history
            .undo_depth,
        history
    );
    interaction.replace_field("value", "25");
    interaction
        .field_operation("value", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(25)));
    assert!(
        draft(&interaction, "value").is_none(),
        "only this edit's owner acceptance clears its draft"
    );
}

#[test]
fn actual_interval_controls_keep_out_of_range_input_until_corrected() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    let before = interaction
        .frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .revision;
    interaction.replace_field("seconds", "31");
    assert!(
        interaction
            .field_operation("seconds", InspectorOperation::Apply)
            .is_err()
    );
    assert_eq!(draft(&interaction, "seconds").unwrap().0, "31");
    assert!(
        interaction
            .frontend
            .retained_field_error(CUSTOM_COMPONENT, "seconds")
            .is_some()
    );
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        before
    );
    interaction.replace_field("seconds", "30");
    interaction
        .field_operation("seconds", InspectorOperation::Apply)
        .unwrap();
    assert!(draft(&interaction, "seconds").is_none());
    let frame = interaction.frame();
    let field = frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "seconds")
        .unwrap();
    assert_eq!(field.value, Some(ComponentValue::f64(30.0).unwrap()));
}

#[test]
fn collapsed_and_temporarily_unselected_drafts_require_explicit_conflict_reapply() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    let entity = interaction
        .frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .top_entity()
        .unwrap()
        .clone();
    interaction.replace_field("value", "25");
    interaction.click_owned(None, &format!("Collapse {CUSTOM_COMPONENT}"));
    change_other(&mut interaction);
    interaction.render();
    assert_eq!(
        draft(&interaction, "value"),
        Some(("25".into(), true, false, false))
    );
    interaction.click_owned(None, &format!("Expand {CUSTOM_COMPONENT}"));
    interaction.click_hierarchy("Select preserved");
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    interaction.click_hierarchy(&format!("Select {}", entity.as_str()));
    assert!(
        interaction
            .field_operation("value", InspectorOperation::Apply)
            .is_err()
    );
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    interaction
        .field_operation("value", InspectorOperation::Reapply)
        .unwrap();
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(25)));
    assert!(draft(&interaction, "value").is_none());
}

#[test]
fn actual_queued_field_rejection_preserves_the_exact_input_and_releases_pending() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    interaction.replace_field("value", "25");
    interaction
        .queue_owned_operation("value", InspectorOperation::Apply)
        .unwrap();
    let submission = interaction.draw().unwrap();
    assert!(
        draft(&interaction, "value").unwrap().3,
        "the owned Apply operation queued its exact candidate"
    );
    change_other(&mut interaction);
    interaction.submit(submission);
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    assert!(!draft(&interaction, "value").unwrap().3);
    interaction.render();
    assert_eq!(
        draft(&interaction, "value"),
        Some(("25".into(), true, false, false))
    );
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
}

#[test]
fn removed_component_draft_has_an_explicit_recovery_control() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    interaction.replace_field("value", "invalid retained input");
    interaction.click_owned(None, &format!("Remove {CUSTOM_COMPONENT}"));
    assert!(draft(&interaction, "value").unwrap().2);
    interaction
        .field_operation("value", InspectorOperation::Discard)
        .unwrap();
    assert!(draft(&interaction, "value").is_none());
}

#[test]
fn superseded_reset_reapplies_override_removal_after_an_unrelated_edit() {
    let project = InteractionProject::new_field_overrides();
    let mut interaction = setup(&project);
    interaction
        .queue_owned_operation("required_default", InspectorOperation::Reset)
        .unwrap();
    let dropped = interaction.draw().unwrap();
    assert!(draft(&interaction, "required_default").unwrap().3);
    drop(dropped);
    interaction.render();
    assert!(!draft(&interaction, "required_default").unwrap().3);
    let frame = interaction.frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: scene.document,
                    base_revision: scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity: scene.selection.top_entity().unwrap().clone(),
                        component: ComponentTypeId::new(CUSTOM_COMPONENT),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("optional"),
                        value: ComponentValue::I64(7),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    interaction.render();
    interaction
        .field_operation("required_default", InspectorOperation::Reapply)
        .unwrap();
    let frame = interaction.frame();
    let field = &frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "required_default")
        .unwrap();
    assert_eq!(
        field.state,
        nara::tooling::SceneInspectorFieldState::Defaulted
    );
    assert_eq!(
        field.value,
        Some(ComponentValue::I64(10)),
        "the read projection contains the effective default after override removal"
    );
    assert!(
        matches!(
            interaction.actions.as_slice(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    command: SceneInspectorCommand::RemoveField { .. },
                    ..
                }
            )]
        ),
        "reapplying reset preserves RemoveField rather than storing the previous candidate"
    );
}

#[test]
fn the_sixty_fifth_clean_field_can_be_edited_after_scrolling_to_its_real_control() {
    let project = InteractionProject::new();
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Many fields",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields((0..65).map(|index| {
        let name = format!("field{index:02}");
        ComponentFieldSchema::required(
            ComponentFieldId::new(&name),
            &name,
            ComponentFieldPath::from_fields([name.clone()]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    }))
    .with_authoring_initial_value(ComponentValue::map(
        (0..65).map(|index| (format!("field{index:02}"), ComponentValue::I64(1))),
    ));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let mut interaction = setup(&project);
    assert!(
        interaction
            .find_owned_control(
                Some("field00"),
                nara_ui_kernel::UiSemanticRole::TextInput,
                "field00"
            )
            .is_some()
    );
    assert!(
        interaction
            .find_owned_control(
                Some("field64"),
                nara_ui_kernel::UiSemanticRole::TextInput,
                "field64"
            )
            .is_none()
    );
    assert!(interaction.scroll_owned(f32::MAX));
    assert!(
        interaction
            .find_owned_control(
                Some("field64"),
                nara_ui_kernel::UiSemanticRole::TextInput,
                "field64"
            )
            .is_some()
    );
    interaction.replace_field("field64", "65");
    assert_eq!(draft(&interaction, "field64").unwrap().0, "65");
    interaction
        .field_operation("field64", InspectorOperation::Apply)
        .unwrap();
    assert!(draft(&interaction, "field64").is_none());
    let frame = interaction.frame();
    let field = frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "field64")
        .unwrap();
    assert_eq!(field.value, Some(ComponentValue::I64(65)));
}

#[test]
fn superseded_bool_edit_retries_its_retained_value_without_toggling_it_back() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    interaction.queue_owned_click(Some("flag"), "Set true");
    let dropped = interaction.draw().unwrap();
    assert_eq!(
        draft(&interaction, "flag"),
        Some(("true".into(), false, false, true))
    );
    drop(dropped);
    interaction.render();
    assert_eq!(
        draft(&interaction, "flag"),
        Some(("true".into(), false, false, false))
    );
    interaction
        .field_operation("flag", InspectorOperation::Apply)
        .unwrap();
    assert!(draft(&interaction, "flag").is_none());
    let frame = interaction.frame();
    let actual = frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "flag")
        .unwrap()
        .value
        .clone();
    assert_eq!(actual, Some(ComponentValue::Bool(true)));
}
