use nara_reflect::{ComponentFieldId, ComponentShapeMember, ComponentTypeId, ComponentValueKind};

use super::*;

fn id(name: &str) -> ComponentFieldId {
    ComponentFieldId::new(name)
}

#[test]
fn a_late_key_collision_drops_all_preceding_owned_replacements() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.OverlayOwnedProperty"),
        "Overlay owned property",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            id("blob"),
            "Blob",
            ComponentFieldPath::from_fields(["blob"]),
            ComponentValueKind::String,
        ),
        ComponentFieldSchema::required(
            id("rows"),
            "Rows",
            ComponentFieldPath::from_fields(["rows"]),
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([ComponentShapeMember::new(
                    "wire_id",
                    ComponentValueKind::String,
                )
                .with_id(id("id"))]),
                id("id"),
            ),
        ),
    ]);
    let original = ComponentValue::map([
        ("blob", ComponentValue::String("x".repeat(64 * 1024))),
        (
            "rows",
            ComponentValue::List(vec![
                ComponentValue::map([("wire_id", ComponentValue::String("a".into()))]),
                ComponentValue::map([("wire_id", ComponentValue::String("b".into()))]),
            ]),
        ),
    ]);
    let claims = [
        PrefabPropertyClaim {
            path: ComponentPropertyPath::new(id("blob")),
            intent: PrefabPropertyIntent::Set(ComponentValue::String("changed".into())),
            bindings: Vec::new(),
        },
        PrefabPropertyClaim {
            path: ComponentPropertyPath::new(id("rows")).with_steps([
                ComponentPropertyStep::ListItem {
                    by: id("id"),
                    key: nara_reflect::ComponentPropertyKey::String("a".into()),
                },
                ComponentPropertyStep::Member(id("id")),
            ]),
            intent: PrefabPropertyIntent::Set(ComponentValue::String("b".into())),
            bindings: Vec::new(),
        },
    ];
    let original_before = original.clone();
    let claims_before = claims.clone();

    assert_eq!(
        apply_claims(
            &schema,
            schema.version(),
            &original,
            &claims,
            &BTreeMap::new(),
            ComponentValueMaterializationLimits::default(),
        ),
        Err(PrefabExpansionFailureKind::Component),
    );
    assert_eq!(original, original_before);
    assert_eq!(claims, claims_before);
}
