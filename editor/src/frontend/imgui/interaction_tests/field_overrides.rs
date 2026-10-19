use super::*;
use nara::tooling::{EditorDocumentMutation, SceneInspectorCommand, SceneInspectorFieldState};

pub(super) fn configure(project: &InteractionProject) {
    let field = |name: &str| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let mut required = field("required_default");
    required.default_value = Some(ComponentValue::I64(10));
    let optional = |name: &str, default| {
        let mut field = ComponentFieldSchema::optional(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING);
        field.default_value = default;
        field
    };
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Field overrides",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        required,
        optional("optional_default", Some(ComponentValue::I64(20))),
        optional("optional", None),
        optional("nullable", Some(ComponentValue::I64(40))),
    ])
    .with_authoring_initial_value(ComponentValue::map([
        ("required_default", ComponentValue::I64(1)),
        ("optional_default", ComponentValue::I64(2)),
        ("optional", ComponentValue::I64(3)),
        ("nullable", ComponentValue::Null),
    ]));
    write_schema(project, schema);
}

fn write_schema(project: &InteractionProject, schema: ComponentSchema) {
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

fn control(field: &str, label: &str) -> String {
    format!("field-action:{CUSTOM_COMPONENT}:{field}:{label}")
}

fn expect_field(
    interaction: &Interaction,
    name: &str,
    state: SceneInspectorFieldState,
    value: Option<ComponentValue>,
) {
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
        .find(|field| field.id.as_str() == name)
        .unwrap();
    assert_eq!(field.state, state, "{name}");
    assert_eq!(field.value, value, "{name}");
}

fn saved_value(
    project: &InteractionProject,
    kind: EditorDocumentKind,
    entity: &SceneEntityId,
) -> ComponentValue {
    let source: serde_json::Value =
        serde_json::from_slice(&fs::read(project.source(kind)).unwrap()).unwrap();
    let record = source["payload"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"].as_str() == Some(entity.as_str()))
        .unwrap();
    serde_json::from_value(record["components"][CUSTOM_COMPONENT]["value"].clone()).unwrap()
}

#[test]
fn field_reset_and_remove_controls_preserve_authored_presence_through_history_and_persistence() {
    let project = InteractionProject::new_field_overrides();
    let mut interaction = Interaction::new(&project);
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            interaction.click("entity:prefab-anchor");
            interaction.click("Open Prefab Source");
        }
        interaction.click("+ Root");
        let frame = interaction.frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let entity = scene.selection.top_entity().unwrap().clone();
        let document = scene.document;
        interaction.click("Add Component");
        interaction.click(CUSTOM_COMPONENT);
        interaction.click("Save");
        interaction.settle_persistence();
        assert_eq!(
            saved_value(&project, kind, &entity),
            ComponentValue::map([
                ("required_default", ComponentValue::I64(1)),
                ("optional_default", ComponentValue::I64(2)),
                ("optional", ComponentValue::I64(3)),
                ("nullable", ComponentValue::Null),
            ])
        );
        interaction.click("Dismiss persistence result");
        let mut depth = interaction
            .frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth;
        for (name, authored, effective) in [
            (
                "required_default",
                ComponentValue::I64(1),
                Some(ComponentValue::I64(10)),
            ),
            (
                "optional_default",
                ComponentValue::I64(2),
                Some(ComponentValue::I64(20)),
            ),
            ("optional", ComponentValue::I64(3), None),
            (
                "nullable",
                ComponentValue::Null,
                Some(ComponentValue::I64(40)),
            ),
        ] {
            let label = if effective.is_some() {
                "Reset to default"
            } else {
                "Remove field"
            };
            let state = if effective.is_some() {
                SceneInspectorFieldState::Defaulted
            } else {
                SceneInspectorFieldState::Missing
            };
            expect_field(
                &interaction,
                name,
                SceneInspectorFieldState::Present,
                Some(authored.clone()),
            );
            let revision = interaction
                .frame()
                .workspace()
                .document(document)
                .unwrap()
                .revision;
            interaction.click(&control(name, label));
            // The command must remove the authored override, never write the current default.
            let [
                EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                    document: actual_document,
                    base_revision,
                    command:
                        SceneInspectorCommand::RemoveField {
                            entity: actual_entity,
                            component,
                            component_version,
                            field,
                        },
                }),
            ] = interaction.actions.as_slice()
            else {
                panic!("the real {label} control did not emit one RemoveField action");
            };
            assert_eq!(*actual_document, document);
            assert_eq!(*base_revision, revision);
            assert_eq!(actual_entity, &entity);
            assert_eq!(component.as_str(), CUSTOM_COMPONENT);
            assert_eq!(*component_version, ComponentSchemaVersion::ONE);
            assert_eq!(field.as_str(), name);
            depth += 1;
            assert_eq!(
                interaction
                    .frame()
                    .workspace()
                    .document(document)
                    .unwrap()
                    .history
                    .undo_depth,
                depth,
                "{kind:?}/{name}: remove was not accepted; feedback: {:?}; diagnostics: {:?}",
                interaction.frame().field_feedback(),
                interaction.frame().diagnostics(),
            );
            expect_field(&interaction, name, state.clone(), effective.clone());
            interaction.click("Undo");
            expect_field(
                &interaction,
                name,
                SceneInspectorFieldState::Present,
                Some(authored),
            );
            assert_eq!(
                interaction
                    .frame()
                    .workspace()
                    .document(document)
                    .unwrap()
                    .history
                    .undo_depth,
                depth - 1
            );
            interaction.click("Redo");
            expect_field(&interaction, name, state.clone(), effective.clone());
            interaction.click(&control(name, label));
            assert!(
                interaction.actions.is_empty(),
                "an omitted override has no removal intent"
            );
            assert_eq!(
                interaction
                    .frame()
                    .workspace()
                    .document(document)
                    .unwrap()
                    .history
                    .undo_depth,
                depth
            );
        }
        interaction.click("Save");
        interaction.settle_persistence();
        assert_eq!(
            saved_value(&project, kind, &entity),
            ComponentValue::map([] as [(&str, ComponentValue); 0])
        );
        assert!(
            !interaction
                .frame()
                .workspace()
                .document(document)
                .unwrap()
                .dirty
        );
        interaction.click("Dismiss persistence result");
        interaction.click("Reopen");
        interaction.settle_persistence();
        interaction.click(&format!("entity:{}", entity.as_str()));
        for (name, value) in [
            ("required_default", 10),
            ("optional_default", 20),
            ("nullable", 40),
        ] {
            expect_field(
                &interaction,
                name,
                SceneInspectorFieldState::Defaulted,
                Some(ComponentValue::I64(value)),
            );
            interaction.click(&control(name, "Reset to default"));
            assert!(interaction.actions.is_empty());
        }
        expect_field(
            &interaction,
            "optional",
            SceneInspectorFieldState::Missing,
            None,
        );
        interaction.click(&control("optional", "Remove field"));
        assert!(interaction.actions.is_empty());
        assert_eq!(
            interaction
                .frame()
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            0
        );
        interaction.click("Dismiss persistence result");
    }
}

#[test]
fn required_field_without_a_default_has_no_remove_intent() {
    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    interaction.click("+ Root");
    interaction.click("Add Component");
    interaction.click(CUSTOM_COMPONENT);
    let before = interaction.frame();
    interaction.click(&control("value", "Remove field"));
    assert!(interaction.actions.is_empty());
    assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(10)));
    assert_eq!(
        interaction.frame().workspace().active_scene,
        before.workspace().active_scene
    );
}

#[test]
fn root_and_list_index_fields_have_no_remove_intent() {
    use nara::reflect::ComponentFieldPathSegment;

    for (path, initial) in [
        (ComponentFieldPath::empty(), ComponentValue::I64(1)),
        (
            ComponentFieldPath::new([ComponentFieldPathSegment::index(0)]),
            ComponentValue::List(vec![ComponentValue::I64(1)]),
        ),
    ] {
        let project = InteractionProject::new();
        let field = ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("value"),
            "Value",
            path.clone(),
            ComponentValueKind::I64,
            ComponentValue::I64(10),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING);
        write_schema(
            &project,
            ComponentSchema::new(
                ComponentTypeId::new(CUSTOM_COMPONENT),
                "Field removal boundaries",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field])
            .with_authoring_initial_value(initial),
        );
        let mut interaction = Interaction::new(&project);
        interaction.click("+ Root");
        interaction.click("Add Component");
        interaction.click(CUSTOM_COMPONENT);
        expect_field(
            &interaction,
            "value",
            SceneInspectorFieldState::Present,
            Some(ComponentValue::I64(1)),
        );
        let before = interaction.frame();
        interaction.click(&control("value", "Reset to default"));
        assert!(
            interaction.actions.is_empty(),
            "{path}: removal must be disabled"
        );
        assert_eq!(
            interaction.frame().workspace().active_scene,
            before.workspace().active_scene
        );
    }
}
