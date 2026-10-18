use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain,
    ComponentRegistry, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, validate_component_value,
};

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.PayloadBudget"),
        "Payload budget",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("data"),
        "Data",
        ComponentFieldPath::from_fields(["data"]),
        shape,
    )])
}

fn payload_values() -> Vec<ComponentValue> {
    (0..17_000).map(ComponentValue::U64).collect()
}

#[test]
fn numeric_domain_builder_stops_at_the_shape_depth_boundary() {
    let mut shape = ComponentValueShape::scalar(ComponentValueKind::U64);
    for _depth in 0..4_096 {
        shape = ComponentValueShape::Optional {
            value: Box::new(shape),
        };
    }
    let mut shape = shape.with_numeric_domain(ComponentNumericDomain::U8);
    assert!(shape.validate().is_err());
    while let ComponentValueShape::Optional { value } = shape {
        shape = *value;
    }
    assert_eq!(shape.numeric_domain(), None);
}

#[test]
fn opaque_payload_size_is_admitted_by_its_caller_instead_of_the_schema_budget() {
    let schema = schema(ComponentValueShape::opaque(ComponentValueKind::List));
    let value = ComponentValue::map([("data", ComponentValue::List(payload_values()))]);
    assert!(validate_component_value(&schema, &value).is_ok());
    assert!(
        schema.fields[0]
            .shape
            .validate_value(value.field("data").unwrap())
            .is_ok()
    );
}

#[test]
fn typed_lists_validate_large_payloads_and_reject_the_exact_wrong_item() {
    let schema = schema(ComponentValueShape::list(ComponentValueShape::scalar(
        ComponentValueKind::U64,
    )));
    let value = ComponentValue::map([("data", ComponentValue::List(payload_values()))]);
    assert!(validate_component_value(&schema, &value).is_ok());
    let mut values = payload_values();
    values[16_999] = ComponentValue::Bool(false);
    let invalid = ComponentValue::map([("data", ComponentValue::List(values))]);
    let error = validate_component_value(&schema, &invalid).unwrap_err();
    assert!(error.to_string().contains("data[16999]"));
}

#[test]
fn catalog_defaults_and_creation_values_keep_their_complete_value_budget() {
    let mut with_default = schema(ComponentValueShape::opaque(ComponentValueKind::List));
    with_default.fields[0].default_value = Some(ComponentValue::List(payload_values()));
    let catalog = ComponentSchemaCatalog {
        components: vec![with_default],
        ..ComponentSchemaCatalog::default()
    };
    assert!(matches!(
        ComponentRegistry::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new("nara.test.payload-budget-owner"),
            catalog,
            None,
        ),
        Err(ComponentRegistryError::InvalidComponentFieldDefaultShape { .. })
    ));
    let with_initial =
        schema(ComponentValueShape::opaque(ComponentValueKind::List)).with_authoring_initial_value(
            ComponentValue::map([("data", ComponentValue::List(payload_values()))]),
        );
    let catalog = ComponentSchemaCatalog {
        components: vec![with_initial],
        ..ComponentSchemaCatalog::default()
    };
    assert!(matches!(
        ComponentRegistry::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new("nara.test.payload-budget-owner"),
            catalog,
            None,
        ),
        Err(ComponentRegistryError::InvalidComponentAuthoringInitialValue { .. })
    ));
}

#[test]
fn coverage_rejects_an_undeclared_deep_value_without_recursing_through_opaque_data() {
    let scalar_schema = schema(ComponentValueShape::scalar(ComponentValueKind::U64));
    let mut deep = ComponentValue::U64(1);
    for _depth in 0..4_096 {
        deep = ComponentValue::List(vec![deep]);
    }
    let mut value = ComponentValue::map([("data", ComponentValue::U64(1)), ("extra", deep)]);
    assert!(validate_component_value(&scalar_schema, &value).is_err());
    let ComponentValue::Map(fields) = &mut value else {
        unreachable!()
    };
    let mut deep = fields.remove("extra").unwrap();
    let opaque_schema = schema(ComponentValueShape::opaque(ComponentValueKind::List));
    assert!(opaque_schema.fields[0].shape.validate_value(&deep).is_ok());
    let opaque_value = ComponentValue::map([("data", deep)]);
    assert!(validate_component_value(&opaque_schema, &opaque_value).is_ok());
    let ComponentValue::Map(mut fields) = opaque_value else {
        unreachable!()
    };
    deep = fields.remove("data").unwrap();
    while let ComponentValue::List(mut values) = deep {
        deep = values.pop().unwrap();
    }
}
