//! Real saved dependencies preserve invalid old targets when a final pin repairs the reference.

use nara_identity::EntityReference;
use nara_reflect::{ComponentPropertyPath, ComponentValueShape};
use nara_scene::{PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceBinding};

use super::*;

#[test]
fn runtime_content_resolves_repaired_reference_after_property_composition() {
    let component = ComponentTypeId::new("test.content.EntityLink");
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("test.content.entity-links").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    component.clone(),
                    "Entity link",
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
                .with_capability(ComponentCapability::EntityRef)]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let id = |name| SceneEntityId::new(name).unwrap();
    let reference =
        |name| ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) });
    for repair in [true, false] {
        let project = LazyContentProject::new();
        let original = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("host")).with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("target", reference("missing"))]),
                )
                .with_owner(owner.owner_id().clone()),
            ),
            SceneEntityRecord::new(id("socket")),
        ]);
        let encoded = original.to_json_string().unwrap();
        fs::write(project.root.join("prefabs/p.prefab.json"), &encoded).unwrap();
        let mut overlay = PrefabOverlay::new(SourceDocumentKey::project_location(
            "39f9d09d-63e3-5fc4-8e3f-71e84893cb8d".parse().unwrap(),
            AssetSourcePath::new("prefabs/p.prefab.json").unwrap(),
            SourceDocumentKind::Prefab,
        ));
        if repair {
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("host")),
                component: component.clone(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(reference("socket")),
                    bindings: vec![PrefabReferenceBinding {
                        slot: ComponentFieldPath::empty(),
                        expected: id("socket"),
                        target: PrefabEntityTarget::source(id("socket")),
                    }],
                }],
            });
        }
        let mut anchor = SceneEntityRecord::new(id("instance"));
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("p.prefab.json").unwrap(),
            overrides: repair.then_some(overlay),
        });
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([anchor])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs_with_recipe(data_recipe(&owner));
        let result = loader.load(&candidate, &plan);
        if repair {
            let snapshot = result.unwrap();
            assert_eq!(snapshot.prefabs().len(), 1);
            assert_eq!(snapshot.prefabs()[0].document(), &original);
            let socket = snapshot
                .expanded_scene_provenance()
                .iter()
                .find_map(|(entity, origin)| {
                    (origin.local_entity_id() == &id("socket")).then_some(entity)
                })
                .unwrap();
            let host = snapshot
                .expanded_scene()
                .entities
                .iter()
                .find(|entity| entity.components.contains_key(&component))
                .unwrap();
            assert_eq!(
                host.components[&component].value.get("target"),
                Some(&ComponentValue::EntityReference(
                    EntityReference::SceneLocal {
                        entity: socket.clone(),
                    }
                )),
            );
            drop(snapshot);
        } else {
            assert!(result.is_err(), "an uncorrected reference cannot activate");
        }
        assert_eq!(
            fs::read_to_string(project.root.join("prefabs/p.prefab.json")).unwrap(),
            encoded,
            "effective composition must not rewrite the saved dependency",
        );
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }
}
