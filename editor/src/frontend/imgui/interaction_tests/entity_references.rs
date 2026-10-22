use super::*;

use nara::identity::EntityReference;
use nara::reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldSchema,
    ComponentSchemaCatalog, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

const OWNER: &str = "studio.entity-references";

pub(super) fn configure(project: &InteractionProject) {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Entity references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)])
    .with_authoring_initial_value(ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: SceneEntityId::new("target").unwrap(),
        }),
    )]));
    let owner = PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
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

    let source = SceneEntityRecord::new(SceneEntityId::new("source").unwrap()).with_component(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("target").unwrap(),
                }),
            )]),
        )
        .with_owner(owner),
    );
    let target = SceneEntityRecord::new(SceneEntityId::new("target").unwrap());
    let other = SceneEntityRecord::new(SceneEntityId::new("other").unwrap());
    fs::write(
        project.root.join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots([source, target, other])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

fn selected_target(interaction: &Interaction) -> Option<ComponentValue> {
    interaction
        .frame()
        .workspace()
        .active_scene
        .as_ref()?
        .editor
        .inspector
        .selected_entity_view
        .as_ref()?
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)?
        .fields
        .iter()
        .find(|field| field.id.as_str() == "target")?
        .value
        .clone()
}

#[test]
fn source_local_entity_reference_uses_document_choices_and_history() {
    let project = InteractionProject::new_entity_references();
    let mut interaction = Interaction::new(&project);
    interaction.click("entity:source");
    let apply = format!("field-action:{CUSTOM_COMPONENT}:target:Apply");
    interaction.click("entity-reference:picker");
    interaction.click("entity-reference:choice:other");
    interaction.click(&apply);
    assert_eq!(
        selected_target(&interaction),
        Some(ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: SceneEntityId::new("other").unwrap(),
            }
        ))
    );
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
        1
    );
    interaction.click("Undo");
    assert_eq!(
        selected_target(&interaction),
        Some(ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: SceneEntityId::new("target").unwrap(),
            }
        ))
    );
    interaction.click("Redo");
    assert_eq!(
        selected_target(&interaction),
        Some(ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: SceneEntityId::new("other").unwrap(),
            }
        ))
    );
}
