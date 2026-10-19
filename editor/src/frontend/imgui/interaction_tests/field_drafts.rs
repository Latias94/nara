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
    interaction.click("+ Root");
    interaction.click("Add Component");
    interaction.click(CUSTOM_COMPONENT);
    interaction
}

fn control(field: &str, action: &str) -> String {
    format!("field-action:{CUSTOM_COMPONENT}:{field}:{action}")
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
fn actual_scalar_controls_retain_invalid_u8_and_long_input_until_explicit_correction() {
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
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), "256");
    assert_eq!(draft(&interaction, "value").unwrap().0, "256");
    assert!(
        interaction
            .frontend
            .retained_field_error(CUSTOM_COMPONENT, "value")
            .is_some(),
        "the shared numeric-domain validator explains u8 overflow before submission"
    );
    interaction.click(&control("value", "Apply"));
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
    assert_eq!(draft(&interaction, "value").unwrap().0, "256");
    let long = "9".repeat(8256);
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), &long);
    assert_eq!(draft(&interaction, "value").unwrap().0, long);
    interaction.click(&control("value", "Apply"));
    assert_eq!(draft(&interaction, "value").unwrap().0.len(), 8256);
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
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), "25");
    interaction.click(&control("value", "Apply"));
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
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:seconds"), "31");
    interaction.click(&control("seconds", "Apply"));
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
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:seconds"), "30");
    interaction.click(&control("seconds", "Apply"));
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
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), "25");
    interaction.click(&format!("component:{CUSTOM_COMPONENT}"));
    change_other(&mut interaction);
    interaction.render();
    assert_eq!(
        draft(&interaction, "value"),
        Some(("25".into(), true, false, false))
    );
    interaction.click("entity:preserved");
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    interaction.click(&format!("entity:{}", entity.as_str()));
    interaction.click(&format!("component:{CUSTOM_COMPONENT}"));
    interaction.click(&control("value", "Apply"));
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(10)));
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    interaction.click(&control("value", "Reapply draft"));
    assert_eq!(interaction.selected_value(), Some(ComponentValue::U64(25)));
    assert!(draft(&interaction, "value").is_none());
}

#[test]
fn actual_queued_field_rejection_preserves_the_exact_input_and_releases_pending() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    interaction.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), "25");
    interaction.context.io_mut().add_key_event(Key::Enter, true);
    let ui = interaction.context.frame();
    let submission = interaction
        .frontend
        .show(ui, &mut interaction.application)
        .unwrap()
        .unwrap();
    let _ = interaction.context.render_legacy();
    assert!(
        draft(&interaction, "value").unwrap().3,
        "the real Enter control queued its exact candidate"
    );
    change_other(&mut interaction);
    interaction
        .frontend
        .submit_and_drive(&mut interaction.application, submission, Duration::ZERO);
    assert_eq!(draft(&interaction, "value").unwrap().0, "25");
    assert!(!draft(&interaction, "value").unwrap().3);
    interaction
        .context
        .io_mut()
        .add_key_event(Key::Enter, false);
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
    interaction.replace_text(
        &format!("field:{CUSTOM_COMPONENT}:value"),
        "invalid retained input",
    );
    interaction.click("Remove Component");
    assert!(draft(&interaction, "value").unwrap().2);
    interaction.click(&control("value", "Discard incompatible draft"));
    assert!(draft(&interaction, "value").is_none());
}

#[test]
fn superseded_reset_reapplies_override_removal_after_an_unrelated_edit() {
    let project = InteractionProject::new_field_overrides();
    let mut interaction = setup(&project);
    let reset = control("required_default", "Reset to default");
    let rect = CONTROLS.with(|controls| controls.borrow()[&reset]);
    interaction.context.io_mut().add_mouse_pos_event([
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]);
    interaction.render();
    interaction
        .context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    interaction.render();
    interaction
        .context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    let ui = interaction.context.frame();
    let dropped = interaction
        .frontend
        .show(ui, &mut interaction.application)
        .unwrap()
        .unwrap();
    let _ = interaction.context.render_legacy();
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
    interaction.click(&control("required_default", "Reapply draft"));
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
    let first = format!("field:{CUSTOM_COMPONENT}:field00");
    let last = format!("field:{CUSTOM_COMPONENT}:field64");
    let rect = super::super::ui_interaction_tests::control_rect(&first).unwrap();
    interaction.context.io_mut().add_mouse_pos_event([
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]);
    interaction.render();
    for _ in 0..20 {
        if super::super::ui_interaction_tests::control_visible(&last) {
            break;
        }
        interaction
            .context
            .io_mut()
            .add_mouse_wheel_event([0.0, -12.0]);
        interaction.render();
    }
    assert!(super::super::ui_interaction_tests::control_visible(&last));
    interaction.replace_text(&last, "65");
    assert_eq!(draft(&interaction, "field64").unwrap().0, "65");
    interaction.click(&control("field64", "Apply"));
    assert!(draft(&interaction, "field64").is_none());
}

#[test]
fn superseded_bool_edit_retries_its_retained_value_without_toggling_it_back() {
    let project = InteractionProject::new_field_drafts();
    let mut interaction = setup(&project);
    let label = format!("field:{CUSTOM_COMPONENT}:flag");
    let rect = CONTROLS.with(|controls| controls.borrow()[&label]);
    interaction.context.io_mut().add_mouse_pos_event([
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]);
    interaction.render();
    interaction
        .context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    interaction.render();
    interaction
        .context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    let ui = interaction.context.frame();
    let dropped = interaction
        .frontend
        .show(ui, &mut interaction.application)
        .unwrap()
        .unwrap();
    let _ = interaction.context.render_legacy();
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
    interaction.click(&control("flag", "Apply"));
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
