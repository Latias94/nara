use super::owned::InspectorOperation;
use super::*;
use nara::reflect::{
    ComponentFieldId, ComponentNumericDomain, ComponentShapeMember, ComponentValueShape,
};

pub(super) fn configure(project: &InteractionProject) {
    let number = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32);
    let fields = [
        ("optional", ComponentValueShape::optional(number.clone())),
        (
            "vector",
            ComponentValueShape::record_members([
                ComponentShapeMember::new("x", number.clone()).with_id(ComponentFieldId::new("x")),
                ComponentShapeMember::new("y", number).with_id(ComponentFieldId::new("y")),
            ]),
        ),
        (
            "list",
            ComponentValueShape::list(
                ComponentValueShape::scalar(ComponentValueKind::U64)
                    .with_numeric_domain(ComponentNumericDomain::U8),
            ),
        ),
        (
            "map",
            ComponentValueShape::map(ComponentValueShape::scalar(ComponentValueKind::String)),
        ),
    ];
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Recursive values",
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
        ("optional", ComponentValue::Null),
        (
            "vector",
            ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(2.0).unwrap()),
            ]),
        ),
        ("list", ComponentValue::List(vec![ComponentValue::U64(3)])),
        (
            "map",
            ComponentValue::map([("first", ComponentValue::String("before".into()))]),
        ),
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

fn selected(interaction: &Interaction, name: &str) -> ComponentValue {
    interaction
        .frame()
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
        .find(|field| field.id.as_str() == name)
        .unwrap()
        .value
        .clone()
        .unwrap()
}

fn history(interaction: &Interaction) -> usize {
    interaction
        .frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .history
        .undo_depth
}

fn click(interaction: &mut Interaction, field: &str, path: &str, name: &str) {
    interaction.click_owned(Some(field), &format!("{path}: {name}"));
}

fn replace(interaction: &mut Interaction, field: &str, path: &str, value: &str) {
    interaction
        .try_replace_owned_text(field, path, value)
        .unwrap();
}

#[test]
fn recursive_container_remove_buttons_target_only_the_second_item_or_entry() {
    let project = InteractionProject::new_recursive();
    let mut interaction = Interaction::new(&project);
    interaction.click_hierarchy("+ Root");
    interaction.click_owned(None, "Add Component");
    interaction.click_owned(None, &format!("Add {CUSTOM_COMPONENT}"));
    let baseline = history(&interaction);

    click(&mut interaction, "list", "root", "Edit value");
    click(&mut interaction, "list", "root", "Add...");
    replace(&mut interaction, "list", "root.new", "8");
    click(&mut interaction, "list", "root", "Add value");
    interaction
        .field_operation("list", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(
        selected(&interaction, "list"),
        ComponentValue::List(vec![ComponentValue::U64(3), ComponentValue::U64(8),])
    );
    click(&mut interaction, "list", "root", "Edit value");
    click(&mut interaction, "list", "root[1]", "Remove item");
    assert_eq!(
        history(&interaction),
        baseline + 1,
        "removal remains local until Apply"
    );
    interaction
        .field_operation("list", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(
        selected(&interaction, "list"),
        ComponentValue::List(vec![ComponentValue::U64(3)])
    );

    click(&mut interaction, "map", "root", "Edit value");
    click(&mut interaction, "map", "root", "Insert...");
    replace(&mut interaction, "map", "root.key", "second");
    replace(&mut interaction, "map", "root.new", "after");
    click(&mut interaction, "map", "root", "Insert value");
    interaction
        .field_operation("map", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(
        selected(&interaction, "map"),
        ComponentValue::map([
            ("first", ComponentValue::String("before".into())),
            ("second", ComponentValue::String("after".into())),
        ])
    );
    click(&mut interaction, "map", "root", "Edit value");
    click(&mut interaction, "map", "root[\"second\"]", "Remove entry");
    assert_eq!(
        history(&interaction),
        baseline + 3,
        "removal remains local until Apply"
    );
    interaction
        .field_operation("map", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(
        selected(&interaction, "map"),
        ComponentValue::map([("first", ComponentValue::String("before".into())),])
    );
    assert_eq!(history(&interaction), baseline + 4);
}

#[test]
fn recursive_widgets_draft_optional_record_and_scalar_containers_in_scene_and_prefab() {
    let project = InteractionProject::new_recursive();
    let mut interaction = Interaction::new(&project);
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            interaction.click_hierarchy("Select prefab-anchor");
            interaction.click_owned(None, "Open Prefab Source");
        }
        let manifest = interaction
            .application
            .active_authoring_unavailable_record_manifest()
            .unwrap();
        interaction.click_hierarchy("+ Root");
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
        interaction.click_owned(None, "Add Component");
        interaction.click_owned(None, &format!("Add {CUSTOM_COMPONENT}"));
        let initial_history = history(&interaction);

        click(&mut interaction, "optional", "root", "Edit value");
        click(&mut interaction, "optional", "root", "Some...");
        click(&mut interaction, "optional", "root", "Use Some");
        assert_eq!(
            history(&interaction),
            initial_history,
            "blank Some cannot fabricate zero"
        );
        replace(&mut interaction, "optional", "root.new", "1e100");
        click(&mut interaction, "optional", "root", "Use Some");
        assert_eq!(selected(&interaction, "optional"), ComponentValue::Null);
        click(&mut interaction, "optional", "root", "Cancel Some");
        interaction
            .field_operation("optional", InspectorOperation::Discard)
            .unwrap();
        assert_eq!(
            history(&interaction),
            initial_history,
            "Cancel has no transaction"
        );

        click(&mut interaction, "optional", "root", "Edit value");
        click(&mut interaction, "optional", "root", "Some...");
        replace(&mut interaction, "optional", "root.new", "7");
        click(&mut interaction, "optional", "root", "Use Some");
        assert_eq!(
            selected(&interaction, "optional"),
            ComponentValue::Null,
            "child confirmation stays local"
        );
        interaction
            .field_operation("optional", InspectorOperation::Apply)
            .unwrap();
        assert_eq!(
            selected(&interaction, "optional"),
            ComponentValue::f64(7.0).unwrap()
        );
        click(&mut interaction, "optional", "root", "Edit value");
        click(&mut interaction, "optional", "root", "Set Null");
        interaction
            .field_operation("optional", InspectorOperation::Apply)
            .unwrap();
        assert_eq!(selected(&interaction, "optional"), ComponentValue::Null);
        interaction.click("Undo");
        assert_eq!(
            selected(&interaction, "optional"),
            ComponentValue::f64(7.0).unwrap()
        );
        interaction.click("Redo");

        click(&mut interaction, "vector", "root", "Edit value");
        replace(&mut interaction, "vector", "root.x", "9");
        assert_eq!(history(&interaction), initial_history + 2);
        interaction
            .field_operation("vector", InspectorOperation::Apply)
            .unwrap();
        let vector = ComponentValue::map([
            ("x", ComponentValue::f64(9.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
        ]);
        assert_eq!(selected(&interaction, "vector"), vector);

        click(&mut interaction, "list", "root", "Edit value");
        click(&mut interaction, "list", "root", "Add...");
        replace(&mut interaction, "list", "root.new", "256");
        click(&mut interaction, "list", "root", "Add value");
        assert_eq!(
            history(&interaction),
            initial_history + 3,
            "U8 domain rejects an invalid child"
        );
        replace(&mut interaction, "list", "root.new", "8");
        click(&mut interaction, "list", "root", "Add value");
        click(&mut interaction, "list", "root[0]", "Remove item");
        interaction
            .field_operation("list", InspectorOperation::Apply)
            .unwrap();
        assert_eq!(
            selected(&interaction, "list"),
            ComponentValue::List(vec![ComponentValue::U64(8)])
        );

        click(&mut interaction, "map", "root", "Edit value");
        click(&mut interaction, "map", "root", "Insert...");
        replace(&mut interaction, "map", "root.key", "second");
        replace(&mut interaction, "map", "root.new", "after");
        click(&mut interaction, "map", "root", "Insert value");
        click(&mut interaction, "map", "root[\"first\"]", "Remove entry");
        interaction
            .field_operation("map", InspectorOperation::Apply)
            .unwrap();
        assert_eq!(
            selected(&interaction, "map"),
            ComponentValue::map([("second", ComponentValue::String("after".into()))])
        );
        assert_eq!(history(&interaction), initial_history + 5);
        let edited_manifest = interaction
            .application
            .active_authoring_unavailable_record_manifest()
            .unwrap();
        assert_eq!(edited_manifest.entries(), manifest.entries());
        assert_eq!(edited_manifest.record_count(), manifest.record_count());
        assert_eq!(
            edited_manifest.overlay_target_count(),
            manifest.overlay_target_count()
        );
        assert_eq!(
            edited_manifest.overlay_declarations().collect::<Vec<_>>(),
            manifest.overlay_declarations().collect::<Vec<_>>()
        );

        interaction.click("Save");
        interaction.settle_persistence();
        assert!(matches!(
            interaction.frame().persistence().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ));
        let bytes = fs::read(project.source(kind)).unwrap();
        let saved_placements = match kind {
            EditorDocumentKind::Scene => {
                SceneDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap()
                    .placements
            }
            EditorDocumentKind::Prefab => {
                PrefabDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap()
                    .placements
            }
            EditorDocumentKind::Extension(_) => unreachable!("the fixture edits Scene and Prefab"),
        };
        assert_eq!(
            edited_manifest.source_placements(),
            saved_placements.as_slice()
        );
        interaction.click("Dismiss persistence result");
        interaction.click("Reopen");
        interaction.settle_persistence();
        interaction.click_hierarchy(&format!("Select {}", entity.as_str()));
        assert_eq!(selected(&interaction, "optional"), ComponentValue::Null);
        assert_eq!(selected(&interaction, "vector"), vector);
        assert_eq!(
            selected(&interaction, "list"),
            ComponentValue::List(vec![ComponentValue::U64(8)])
        );
        assert_eq!(
            selected(&interaction, "map"),
            ComponentValue::map([("second", ComponentValue::String("after".into()))])
        );
        assert_eq!(
            interaction
                .application
                .active_authoring_unavailable_record_manifest(),
            Some(edited_manifest)
        );
        interaction.click("Dismiss persistence result");
    }
}

#[test]
fn unfinished_recursive_child_survives_collapse_and_unrelated_revision_conflict() {
    use nara::tooling::{EditorDocumentMutation, SceneInspectorCommand};

    let project = InteractionProject::new_recursive();
    let mut interaction = Interaction::new(&project);
    interaction.click_hierarchy("+ Root");
    interaction.click_owned(None, "Add Component");
    interaction.click_owned(None, &format!("Add {CUSTOM_COMPONENT}"));
    click(&mut interaction, "list", "root", "Edit value");
    click(&mut interaction, "list", "root", "Add...");
    replace(&mut interaction, "list", "root.new", "256");
    interaction.click_owned(None, &format!("Collapse {CUSTOM_COMPONENT}"));
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
                        value: ComponentValue::f64(7.0).unwrap(),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    interaction.render();
    let retained = interaction
        .frontend
        .retained_field_draft(CUSTOM_COMPONENT, "list")
        .unwrap();
    assert!(retained.0.contains("256"));
    assert!(retained.1);
    interaction.click_owned(None, &format!("Expand {CUSTOM_COMPONENT}"));
    assert!(
        interaction
            .field_operation("list", InspectorOperation::Reapply)
            .is_err()
    );
    assert_eq!(
        selected(&interaction, "list"),
        ComponentValue::List(vec![ComponentValue::U64(3)])
    );
    assert!(
        interaction
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "list")
            .unwrap()
            .0
            .contains("256")
    );
    replace(&mut interaction, "list", "root.new", "8");
    click(&mut interaction, "list", "root", "Add value");
    interaction
        .field_operation("list", InspectorOperation::Apply)
        .unwrap();
    assert_eq!(
        selected(&interaction, "list"),
        ComponentValue::List(vec![ComponentValue::U64(3), ComponentValue::U64(8)])
    );
}
