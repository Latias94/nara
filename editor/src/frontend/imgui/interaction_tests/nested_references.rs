use super::owned::InspectorOperation;
use super::*;

use nara::{
    asset::AssetSourcePath,
    identity::{AuthoredEntityReference, EntityReference},
    reflect::{ComponentShapeMember, ComponentValueShape},
    scene::{PrefabEntityOrigin, PrefabInstanceHop, SourceDocumentKey, SourceDocumentKind},
};

pub(super) fn configure(project: &InteractionProject) {
    let reference = || {
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: SceneEntityId::new("target").unwrap(),
        })
    };
    let references = ComponentValue::map([
        ("direct", reference()),
        ("optional", ComponentValue::Null),
        ("list", ComponentValue::List(vec![reference()])),
        ("map", ComponentValue::map([("first", reference())])),
        ("other", ComponentValue::String("before".into())),
    ]);
    let value = ComponentValue::map([("references", references)]);
    let shape = ComponentValueShape::record_members([
        ComponentShapeMember::new("direct", ComponentValueShape::EntityRef),
        ComponentShapeMember::new(
            "optional",
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
        ),
        ComponentShapeMember::new(
            "list",
            ComponentValueShape::list(ComponentValueShape::EntityRef),
        ),
        ComponentShapeMember::new(
            "map",
            ComponentValueShape::map(ComponentValueShape::EntityRef),
        ),
        ComponentShapeMember::new("other", ComponentValueKind::String),
    ]);
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Nested entity references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("references"),
        "References",
        ComponentFieldPath::from_fields(["references"]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)])
    .with_authoring_initial_value(value.clone());
    let owner =
        PersistentComponentSchemaOwnerId::try_new("studio.nested-entity-references").unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let hierarchy = AuthoringSchemaBundle::from_trusted_provider_definitions([
        nara::scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
    ])
    .unwrap();
    let name_owner = hierarchy
        .owner_for_component(&ComponentTypeId::new("nara.scene.Name"))
        .unwrap()
        .owner_id()
        .clone();
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(
        bundle.owners().iter().chain(hierarchy.owners()).cloned(),
    )
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();

    let named = |entity, name: &str| {
        SceneEntityRecord::new(SceneEntityId::new(entity).unwrap()).with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(name.into()),
            )
            .with_owner(name_owner.clone()),
        )
    };
    let additions = SceneDocument::from_ordered_roots([
        named("source", "Source").with_component(
            ComponentTypeId::new(CUSTOM_COMPONENT),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner),
        ),
        named("target", "Door"),
        named("other", "Door"),
    ]);
    let scene_path = project.source(EditorDocumentKind::Scene);
    let mut scene = SceneDocumentCandidate::decode_json_bytes(&fs::read(&scene_path).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    scene.placements.extend(additions.placements.clone());
    scene.entities.extend(additions.entities.clone());
    fs::write(scene_path, scene.to_json_string().unwrap()).unwrap();

    let prefab_path = project.source(EditorDocumentKind::Prefab);
    let mut prefab = PrefabDocumentCandidate::decode_json_bytes(&fs::read(&prefab_path).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    prefab.placements.extend(additions.placements);
    prefab.entities.extend(additions.entities);
    fs::write(prefab_path, prefab.to_json_string().unwrap()).unwrap();
}

fn selected_references(interaction: &Interaction) -> ComponentValue {
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
        .find(|field| field.id.as_str() == "references")
        .unwrap()
        .value
        .clone()
        .unwrap()
}

#[test]
fn inspector_sibling_apply_and_undo_preserve_authored_reference_scope() {
    let project = InteractionProject::new_nested_entity_references();
    let path = project.source(EditorDocumentKind::Scene);
    let mut scene = SceneDocumentCandidate::decode_json_bytes(&fs::read(&path).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    let references = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "source")
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(CUSTOM_COMPONENT))
        .unwrap()
        .value_mut()
        .get_path_mut(&ComponentFieldPath::from_fields(["references"]))
        .unwrap();
    let direct = AuthoredEntityReference::source(SceneEntityId::new("target").unwrap());
    let nested = AuthoredEntityReference {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("prefab-anchor").unwrap()),
            expected_base: SourceDocumentKey::project_location(
                "c3504a11-271b-59f3-b385-4810c9a5c5a2".parse().unwrap(),
                AssetSourcePath::new("blueprints/ordinary.prefab.json").unwrap(),
                SourceDocumentKind::Prefab,
            ),
        }],
        ..direct.clone()
    };
    references
        .set_path(
            &ComponentFieldPath::from_fields(["direct"]),
            ComponentValue::EntityReference(EntityReference::Authored { reference: direct }),
        )
        .unwrap();
    references
        .set_path(
            &ComponentFieldPath::from_fields(["list"]),
            ComponentValue::List(vec![ComponentValue::EntityReference(
                EntityReference::Authored { reference: nested },
            )]),
        )
        .unwrap();
    let original = references.clone();
    fs::write(path, scene.to_json_string().unwrap()).unwrap();

    let mut interaction = Interaction::new(&project);
    interaction.click_hierarchy("Select source");
    interaction.click_owned(Some("references"), "root: Edit value");
    interaction
        .try_replace_owned_text("references", "root.other", "after")
        .unwrap();
    assert_eq!(selected_references(&interaction), original);
    interaction
        .field_operation("references", InspectorOperation::Apply)
        .unwrap();

    let mut expected = original.clone();
    expected
        .set_path(
            &ComponentFieldPath::from_fields(["other"]),
            ComponentValue::String("after".into()),
        )
        .unwrap();
    assert_eq!(selected_references(&interaction), expected);
    interaction.click("Undo");
    assert_eq!(selected_references(&interaction), original);
}
