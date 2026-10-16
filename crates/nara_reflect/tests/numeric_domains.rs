use nara_reflect::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentNumericDomain, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, validate_component_value,
};

fn field(kind: ComponentValueKind, domain: Option<ComponentNumericDomain>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["config", "value"]),
        kind,
    )
    .with_numeric_domain(domain)
}

fn catalog(field: ComponentFieldSchema) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("nara.test.NumericConfig"),
                "Numeric config",
                ComponentSchemaVersion::ONE,
            )
            .with_fields([field]),
        ],
        ..ComponentSchemaCatalog::default()
    }
}

fn admit(catalog: ComponentSchemaCatalog) -> Result<ComponentRegistry, ComponentRegistryError> {
    ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("nara.test.numeric-owner"),
        catalog,
        None,
    )
}

#[test]
fn numeric_domains_are_part_of_exact_catalog_identity() {
    for (kind, domains) in [
        (
            ComponentValueKind::I64,
            [
                ComponentNumericDomain::I8,
                ComponentNumericDomain::I16,
                ComponentNumericDomain::I32,
            ]
            .as_slice(),
        ),
        (
            ComponentValueKind::U64,
            [
                ComponentNumericDomain::U8,
                ComponentNumericDomain::U16,
                ComponentNumericDomain::U32,
            ]
            .as_slice(),
        ),
        (
            ComponentValueKind::F64,
            [ComponentNumericDomain::F32].as_slice(),
        ),
    ] {
        let full_width = catalog(field(kind, None));
        let mut fingerprints = std::collections::BTreeSet::from([full_width.fingerprint()]);
        for domain in domains {
            let narrowed = catalog(field(kind, Some(*domain)));
            admit(narrowed.clone()).unwrap();
            assert!(fingerprints.insert(narrowed.fingerprint()));
        }
    }
}

#[test]
fn numeric_domains_must_match_the_declared_wire_kind() {
    for (kind, domain) in [
        (ComponentValueKind::F64, ComponentNumericDomain::U8),
        (ComponentValueKind::I64, ComponentNumericDomain::U16),
        (ComponentValueKind::U64, ComponentNumericDomain::I32),
        (ComponentValueKind::Bool, ComponentNumericDomain::F32),
        (ComponentValueKind::String, ComponentNumericDomain::I8),
    ] {
        assert!(matches!(
            admit(catalog(field(kind, Some(domain)))),
            Err(ComponentRegistryError::InvalidComponentFieldNumericDomain { .. })
        ));
    }
}

#[test]
fn numeric_defaults_are_checked_before_catalog_admission() {
    let mut bounded = field(ComponentValueKind::U64, Some(ComponentNumericDomain::U8));
    bounded.default_value = Some(ComponentValue::U64(256));
    assert!(matches!(
        admit(catalog(bounded.clone())),
        Err(ComponentRegistryError::ComponentFieldDefaultOutsideNumericDomain { .. })
    ));
    bounded.default_value = Some(ComponentValue::I64(1));
    assert!(matches!(
        admit(catalog(bounded.clone())),
        Err(ComponentRegistryError::InvalidComponentFieldDefault { .. })
    ));
    bounded.default_value = Some(ComponentValue::U64(255));
    admit(catalog(bounded.clone())).unwrap();
    bounded.required = false;
    bounded.default_value = Some(ComponentValue::Null);
    admit(catalog(bounded)).unwrap();

    let mut bounded_float = field(ComponentValueKind::F64, Some(ComponentNumericDomain::F32));
    bounded_float.default_value = Some(ComponentValue::f64(f64::MAX).unwrap());
    assert!(matches!(
        admit(catalog(bounded_float)),
        Err(ComponentRegistryError::ComponentFieldDefaultOutsideNumericDomain { .. })
    ));
}

#[test]
fn numeric_domain_addition_change_and_removal_require_a_component_version_bump() {
    for (before, after) in [
        (None, Some(ComponentNumericDomain::U8)),
        (
            Some(ComponentNumericDomain::U8),
            Some(ComponentNumericDomain::U16),
        ),
        (Some(ComponentNumericDomain::U8), None),
    ] {
        let predecessor = catalog(field(ComponentValueKind::U64, before));
        let mut successor = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
        successor.components = catalog(field(ComponentValueKind::U64, after)).components;
        assert!(matches!(
            ComponentRegistry::from_owner_catalog_candidate(
                ComponentSchemaOwnerId::new("nara.test.numeric-owner"),
                successor.clone(),
                Some(predecessor.clone()),
            ),
            Err(ComponentRegistryError::ComponentSchemaChangedWithoutVersionBump { .. })
        ));
        successor.components[0].version = ComponentSchemaVersion::new(2).unwrap();
        ComponentRegistry::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new("nara.test.numeric-owner"),
            successor,
            Some(predecessor),
        )
        .unwrap();
    }
}

#[test]
fn data_validation_shares_required_paths_kinds_domains_and_unknown_leaf_coverage() {
    let schema = catalog(field(
        ComponentValueKind::U64,
        Some(ComponentNumericDomain::U8),
    ))
    .components
    .remove(0);
    let value = |value| ComponentValue::map([("config", ComponentValue::map([("value", value)]))]);
    validate_component_value(&schema, &value(ComponentValue::U64(255))).unwrap();
    for invalid in [ComponentValue::U64(256), ComponentValue::I64(1)] {
        assert!(matches!(
            validate_component_value(&schema, &value(invalid)),
            Err(ComponentCodecError::InvalidField { field, .. }) if field == "config.value"
        ));
    }
    assert!(matches!(
        validate_component_value(&schema, &ComponentValue::map([("config", ComponentValue::Map(Default::default()))])),
        Err(ComponentCodecError::MissingField { field }) if field == "config.value"
    ));
    let unknown = ComponentValue::map([
        (
            "config",
            ComponentValue::map([("value", ComponentValue::U64(1))]),
        ),
        ("extra", ComponentValue::Bool(true)),
    ]);
    assert!(
        validate_component_value(&schema, &unknown)
            .unwrap_err()
            .to_string()
            .contains("extra")
    );
    assert!(validate_component_value(&schema, &ComponentValue::U64(1)).is_err());

    let mut optional = schema;
    optional.fields[0].required = false;
    validate_component_value(&optional, &value(ComponentValue::Null)).unwrap();
    validate_component_value(&optional, &ComponentValue::Map(Default::default())).unwrap();
    assert!(
        validate_component_value(
            &optional,
            &ComponentValue::map([("config", ComponentValue::Bool(true))])
        )
        .is_err()
    );
}

#[cfg(feature = "serde")]
#[test]
fn numeric_domain_metadata_round_trips_and_absence_keeps_legacy_wire() {
    let legacy = catalog(field(ComponentValueKind::F64, None));
    let legacy_json = legacy.to_json_string().unwrap();
    assert!(!legacy_json.contains("numeric_domain"));
    assert_eq!(
        ComponentSchemaCatalog::from_json_bytes(legacy_json.as_bytes()).unwrap(),
        legacy
    );

    let narrowed = catalog(field(
        ComponentValueKind::F64,
        Some(ComponentNumericDomain::F32),
    ));
    let encoded = narrowed.to_json_string().unwrap();
    assert!(encoded.contains("\"numeric_domain\": \"f32\""));
    let decoded = ComponentSchemaCatalog::from_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(decoded, narrowed);
    assert_eq!(decoded.fingerprint(), narrowed.fingerprint());
    assert_ne!(decoded.fingerprint(), legacy.fingerprint());
    let ron = narrowed.to_ron_string().unwrap();
    assert_eq!(
        ComponentSchemaCatalog::from_ron_bytes(ron.as_bytes()).unwrap(),
        narrowed
    );

    let unknown = encoded.replace("\"numeric_domain\": \"f32\"", "\"numeric_domain\": \"f16\"");
    assert!(ComponentSchemaCatalog::from_json_bytes(unknown.as_bytes()).is_err());
}
