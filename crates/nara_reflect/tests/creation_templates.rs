use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, validate_component_value,
};

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("game.Health"),
        "Health",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["health"]),
            ComponentValueKind::U64,
        ),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["label"]),
            ComponentValueKind::String,
        ),
    ])
}

#[test]
fn creation_metadata_requires_a_complete_template() {
    let initial = ComponentValue::map([("health", ComponentValue::U64(5))]);
    validate_component_value(&schema(), &initial).unwrap();
    let incomplete = schema().with_authoring_initial_value(initial);
    assert!(matches!(
        ComponentSchemaCatalog::from_components([incomplete]),
        Err(ComponentRegistryError::InvalidComponentAuthoringInitialValue { .. })
    ));
}

#[test]
fn complete_templates_round_trip_without_becoming_field_defaults() {
    let initial = ComponentValue::map([
        ("health", ComponentValue::U64(5)),
        ("label", ComponentValue::String("Full".to_owned())),
    ]);
    let catalog = ComponentSchemaCatalog::from_components([
        schema().with_authoring_initial_value(initial.clone())
    ])
    .unwrap();
    #[cfg(feature = "serde")]
    let catalog =
        ComponentSchemaCatalog::from_json_bytes(catalog.to_json_string().unwrap().as_bytes())
            .unwrap();
    let schema = &catalog.components()[0];
    assert_eq!(schema.authoring_initial_value(), Some(&initial));
    assert!(
        validate_component_value(
            schema,
            &ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>())
        )
        .is_err()
    );
}
