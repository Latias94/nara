use super::*;
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion,
};

fn owner(kind: ComponentValueKind, editable: bool) -> ComponentSchemaOwnerSnapshot {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("health"),
        "Health",
        ComponentFieldPath::from_fields(["health"]),
        kind,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if !editable {
        field.capabilities.remove(&ComponentCapability::Edit);
    }
    let schema = ComponentSchema::new(
        ComponentTypeId::new("tool.Stats"),
        "Stats",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("tool.stats").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn property(field: &str) -> EditorSourceToolIntegerProperty {
    EditorSourceToolIntegerProperty::new(
        ComponentFieldId::new(field),
        "bonus".into(),
        "Health bonus".into(),
        -10,
    )
    .unwrap()
}

#[test]
fn integer_proposal_facet_requires_the_actual_editable_i64_root() {
    let admitted = EditorSourceToolActive::admit(
        EditorSourceToolActivationId::issue(),
        owner(ComponentValueKind::I64, true),
        ComponentTypeId::new("tool.Stats"),
        Some(property("health")),
    )
    .unwrap();
    assert_eq!(admitted.integer_property().unwrap().initial(), -10);
    assert_eq!(
        admitted.integer_property().unwrap().field().as_str(),
        "health"
    );
    for (owner, field) in [
        (owner(ComponentValueKind::I64, true), "removed-health"),
        (owner(ComponentValueKind::I64, false), "health"),
        (owner(ComponentValueKind::String, true), "health"),
    ] {
        assert_eq!(
            EditorSourceToolActive::admit(
                EditorSourceToolActivationId::issue(),
                owner,
                ComponentTypeId::new("tool.Stats"),
                Some(property(field)),
            )
            .unwrap_err(),
            EditorSourceToolError::Declaration
        );
    }
    // A semantic tool without a property contribution still has a valid active declaration.
    assert!(
        EditorSourceToolActive::admit(
            EditorSourceToolActivationId::issue(),
            owner(ComponentValueKind::String, false),
            ComponentTypeId::new("tool.Stats"),
            None,
        )
        .unwrap()
        .integer_property()
        .is_none()
    );
}

#[test]
fn property_metadata_rejects_oversized_or_ambiguous_input_before_ui_use() {
    for (field, parameter, label) in [
        ("health".into(), "".into(), "Health bonus".into()),
        ("health".into(), "bonus".into(), " ".into()),
        ("health".into(), "bonus".into(), "Health\0bonus".into()),
        ("health".into(), "bonus".into(), "Health\nbonus".into()),
        (
            "health".into(),
            "bonus with spaces".into(),
            "Health bonus".into(),
        ),
        (
            "x".repeat(ComponentFieldId::MAX_BYTES + 1),
            "bonus".into(),
            "Health bonus".into(),
        ),
        ("health".into(), "x".repeat(65), "Health bonus".into()),
        ("health".into(), "bonus".into(), "x".repeat(257)),
    ] {
        assert_eq!(EditorSourceToolIntegerProperty::new(
            ComponentFieldId::new(field), parameter, label, 0,
        ).unwrap_err(), EditorSourceToolError::Declaration);
    }
    let property = EditorSourceToolIntegerProperty::new(
        ComponentFieldId::new("health"),
        "health.bonus-1".into(),
        "Health bonus##label".into(),
        i64::MAX,
    )
    .unwrap();
    assert_eq!(property.label(), "Health bonus##label");
    assert_eq!(property.initial(), i64::MAX);
}
