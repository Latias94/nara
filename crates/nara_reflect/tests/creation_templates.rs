#![cfg(feature = "serde")]

use nara_core::{ByteLimit, SerdeShapeLimits};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability,
    ComponentCatalogFileError, ComponentCatalogFileLimits, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentNumericDomain, ComponentRegistry, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId, validate_component_value,
};

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.Creatable"),
        "Creatable",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["health"]),
            ComponentValueKind::I64,
        )
        .with_numeric_domain(Some(ComponentNumericDomain::I8)),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["label"]),
            ComponentValueKind::String,
            ComponentValue::String("decode fallback".into()),
        ),
    ])
}

fn initial(health: i64) -> ComponentValue {
    ComponentValue::map([
        ("health", ComponentValue::I64(health)),
        ("label", ComponentValue::String("New entity".into())),
    ])
}

fn catalog(schema: ComponentSchema) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![schema],
        ..ComponentSchemaCatalog::default()
    }
}

#[test]
fn complete_template_roundtrips_in_exact_data_only_authoring_basis_and_copies_by_value() {
    let schema = schema().with_authoring_initial_value(initial(10));
    let schema_catalog = catalog(schema.clone());
    assert_eq!(
        ComponentSchemaCatalog::from_ron_bytes(schema_catalog.to_ron_string().unwrap().as_bytes())
            .unwrap(),
        schema_catalog
    );
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("nara.test.creation").unwrap(),
        catalog(schema),
        None,
    )])
    .unwrap();
    let encoded = bundle.to_json_string().unwrap();
    let loaded = AuthoringSchemaBundle::from_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(loaded, bundle);
    let admitted = &loaded.owners()[0].current().components()[0];
    let mut copied = admitted.authoring_initial_value().unwrap().clone();
    copied
        .set_path(
            &ComponentFieldPath::from_fields(["health"]),
            ComponentValue::I64(20),
        )
        .unwrap();
    assert_eq!(copied.field("health").unwrap(), &ComponentValue::I64(20));
    assert_eq!(admitted.authoring_initial_value(), Some(&initial(10)));
}

#[test]
fn catalog_admission_requires_complete_templates_but_record_decode_keeps_optional_semantics() {
    let required_only = ComponentValue::map([("health", ComponentValue::I64(10))]);
    validate_component_value(&schema(), &required_only).unwrap();
    for invalid in [
        required_only,
        ComponentValue::map([
            ("health", ComponentValue::Bool(true)),
            ("label", ComponentValue::String("New".into())),
        ]),
        initial(128),
        ComponentValue::map([
            ("health", ComponentValue::I64(10)),
            ("label", ComponentValue::String("New".into())),
            ("undeclared", ComponentValue::Bool(true)),
        ]),
    ] {
        let candidate = catalog(schema().with_authoring_initial_value(invalid));
        assert!(matches!(
            candidate.to_json_string(),
            Err(ComponentCatalogFileError::Catalog(
                ComponentRegistryError::InvalidComponentAuthoringInitialValue { .. }
            ))
        ));
        let mut registry = ComponentRegistry::new();
        registry
            .register_component_schema(candidate.components[0].clone())
            .unwrap();
        assert!(matches!(
            registry.freeze(),
            Err(ComponentRegistryError::InvalidComponentAuthoringInitialValue { .. })
        ));
    }
    let mut duplicate = schema().with_authoring_initial_value(initial(10));
    duplicate.fields.push(duplicate.fields[0].clone());
    assert!(matches!(
        catalog(duplicate).to_json_string(),
        Err(ComponentCatalogFileError::Catalog(
            ComponentRegistryError::DuplicateComponentFieldId { .. }
        ))
    ));
}

#[test]
fn creation_template_does_not_supply_missing_required_stored_fields() {
    let schema = schema().with_authoring_initial_value(initial(10));
    let missing_health = ComponentValue::map([("label", ComponentValue::String("Stored".into()))]);
    assert!(validate_component_value(&schema, &missing_health).is_err());
}

#[test]
fn template_only_successor_changes_catalog_identity_without_changing_wire_version() {
    let previous = catalog(schema().with_authoring_initial_value(initial(10)));
    let mut current = ComponentSchemaCatalog::successor_of(&previous).unwrap();
    current
        .components
        .push(schema().with_authoring_initial_value(initial(20)));
    assert_ne!(previous.fingerprint(), current.fingerprint());
    assert_eq!(current.components[0].version(), ComponentSchemaVersion::ONE);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("nara.test.creation").unwrap(),
        current,
        Some(previous),
    )])
    .unwrap();
    let stored = ComponentValue::map([("health", ComponentValue::I64(7))]);
    validate_component_value(&bundle.owners()[0].current().components[0], &stored).unwrap();
}

#[test]
fn absent_template_is_legal_and_serializes_without_new_metadata() {
    let catalog = catalog(schema());
    assert!(catalog.components[0].authoring_initial_value().is_none());
    let encoded = catalog.to_json_string().unwrap();
    assert!(!encoded.contains("authoring_initial_value"));
    assert_eq!(
        ComponentSchemaCatalog::from_json_bytes(encoded.as_bytes())
            .unwrap()
            .fingerprint(),
        catalog.fingerprint()
    );
}

#[test]
fn creation_metadata_uses_existing_whole_file_shape_limits() {
    let template = ComponentValue::map([
        ("health", ComponentValue::I64(10)),
        ("label", ComponentValue::String("x".repeat(65))),
    ]);
    let encoded = catalog(schema().with_authoring_initial_value(template))
        .to_json_string()
        .unwrap();
    let defaults = ComponentCatalogFileLimits::default();
    let shape = defaults.shape();
    let limits = defaults.with_shape(SerdeShapeLimits::new(
        shape.depth(),
        shape.nodes(),
        shape.container_items(),
        ByteLimit::new(64).unwrap(),
        shape.total_string_bytes(),
    ));
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(encoded.as_bytes(), limits),
        Err(ComponentCatalogFileError::Shape { .. })
    ));
}

#[test]
fn non_finite_creation_values_are_rejected_by_the_existing_value_decoder() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("nara.test.FloatCreation"),
        "Float creation",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("speed"),
        "Speed",
        ComponentFieldPath::from_fields(["speed"]),
        ComponentValueKind::F64,
    )])
    .with_authoring_initial_value(ComponentValue::map([(
        "speed",
        ComponentValue::f64(1.25).unwrap(),
    )]));
    let encoded = catalog(schema).to_ron_string().unwrap();
    assert!(encoded.contains("1.25"));
    let non_finite = encoded.replace("1.25", "NaN");
    assert!(ComponentSchemaCatalog::from_ron_bytes(non_finite.as_bytes()).is_err());
}
