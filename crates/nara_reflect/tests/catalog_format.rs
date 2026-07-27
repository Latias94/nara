#![cfg(feature = "serde")]

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentCatalogFileBudgetKind, ComponentCatalogFileError,
    ComponentCatalogFileLimits, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind,
};
use serde::Serialize;

#[derive(Serialize)]
struct TestGenerator<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct TestEnvelope<'a, T> {
    kind: &'a str,
    format_version: u32,
    engine_min_version: &'a str,
    generator: TestGenerator<'a>,
    payload: T,
}

fn envelope<'a, T>(
    kind: &'a str,
    format_version: u32,
    minimum: &'a str,
    payload: T,
) -> TestEnvelope<'a, T> {
    TestEnvelope {
        kind,
        format_version,
        engine_min_version: minimum,
        generator: TestGenerator {
            name: "nara",
            version: "0.1.0",
        },
        payload,
    }
}

fn component_schema(id: &str, fields: usize) -> ComponentSchema {
    let fields = (0..fields).map(|index| {
        let token = format!("field_{index}");
        ComponentFieldSchema::required(
            ComponentFieldId::new(token.clone()),
            format!("Field {index}"),
            ComponentFieldPath::from_fields([token]),
            ComponentValueKind::F64,
        )
        .with_capabilities([ComponentCapability::Inspect])
    });
    ComponentSchema::new(
        ComponentTypeId::new(id),
        "Component",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Inspect])
    .with_fields(fields)
}

fn sample_catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        generation: 1,
        predecessor: None,
        components: vec![component_schema("nara.test.Position", 2)],
        type_tombstones: vec![ComponentTypeId::new("nara.test.Removed")],
    }
}

#[test]
fn catalog_json_and_ron_use_one_canonical_envelope_without_native_identity() {
    let mut expected = sample_catalog();
    expected.canonicalize();

    let json = expected.to_json_string().unwrap();
    let json_value = serde_json::from_str::<serde_json::Value>(&json).unwrap();
    assert_eq!(json_value["kind"], "component_schema_catalog");
    assert_eq!(json_value["format_version"], 1);
    assert_eq!(json_value["payload"]["generation"], 1);
    assert!(json_value.to_string().find("rust_type_path").is_none());
    assert_eq!(
        ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap(),
        expected
    );

    let ron = expected.to_ron_string().unwrap();
    assert_eq!(
        ComponentSchemaCatalog::from_ron_bytes(ron.as_bytes()).unwrap(),
        expected
    );
}

#[test]
fn catalog_header_contract_rejects_wrong_kind_version_and_engine_minimum() {
    let catalog = sample_catalog();
    let wrong_kind = serde_json::to_vec(&envelope("scene", 1, "0.1.0", &catalog)).unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes(&wrong_kind),
        Err(ComponentCatalogFileError::Contract(_))
    ));

    let wrong_version =
        ron::ser::to_string(&envelope("component_schema_catalog", 2, "0.1.0", &catalog)).unwrap();
    let error = ComponentSchemaCatalog::from_ron_bytes(wrong_version.as_bytes()).unwrap_err();
    assert!(
        matches!(error, ComponentCatalogFileError::Contract(_)),
        "unexpected RON error: {error:?}"
    );

    let future_engine = serde_json::to_vec(&envelope(
        "component_schema_catalog",
        1,
        "999.0.0",
        &catalog,
    ))
    .unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes(&future_engine),
        Err(ComponentCatalogFileError::Contract(_))
    ));
}

#[test]
fn catalog_byte_shape_and_domain_budgets_reject_before_candidate_publication() {
    let catalog = sample_catalog();
    let encoded = catalog.to_json_string().unwrap();

    let byte_limits = ComponentCatalogFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(encoded.len() - 1).unwrap());
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(encoded.as_bytes(), byte_limits),
        Err(ComponentCatalogFileError::EncodedBytesExceeded { .. })
    ));

    let defaults = ComponentCatalogFileLimits::default();
    let shape_limits = defaults.with_shape(SerdeShapeLimits::new(
        DepthLimit::ONE,
        defaults.shape().nodes(),
        defaults.shape().container_items(),
        defaults.shape().string_bytes(),
        defaults.shape().total_string_bytes(),
    ));
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(encoded.as_bytes(), shape_limits),
        Err(ComponentCatalogFileError::Shape { .. })
    ));

    let component_limits = defaults.with_components(ItemLimit::ONE);
    let two_components = ComponentSchemaCatalog {
        components: vec![
            component_schema("nara.test.Position", 1),
            component_schema("nara.test.Velocity", 1),
        ],
        ..ComponentSchemaCatalog::default()
    };
    let encoded = serde_json::to_vec(&envelope(
        "component_schema_catalog",
        1,
        "0.1.0",
        &two_components,
    ))
    .unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(&encoded, component_limits),
        Err(ComponentCatalogFileError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::Components
    ));

    let field_limits = defaults.with_fields(ItemLimit::ONE);
    let encoded =
        serde_json::to_vec(&envelope("component_schema_catalog", 1, "0.1.0", &catalog)).unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(&encoded, field_limits),
        Err(ComponentCatalogFileError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::Fields
    ));
}

#[derive(Serialize)]
struct UnknownEnvelope<'a, T> {
    kind: &'a str,
    format_version: u32,
    engine_min_version: &'a str,
    generator: TestGenerator<'a>,
    payload: T,
    unexpected: bool,
}

#[test]
fn catalog_rejects_unknown_fields_and_invalid_identity_before_registry_mutation() {
    let catalog = sample_catalog();
    let unknown = UnknownEnvelope {
        kind: "component_schema_catalog",
        format_version: 1,
        engine_min_version: "0.1.0",
        generator: TestGenerator {
            name: "nara",
            version: "0.1.0",
        },
        payload: &catalog,
        unexpected: true,
    };
    let ron = ron::ser::to_string(&unknown).unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_ron_bytes(ron.as_bytes()),
        Err(ComponentCatalogFileError::Header { .. })
            | Err(ComponentCatalogFileError::Payload { .. })
    ));

    let duplicate_id = ComponentFieldId::new("duplicate");
    let invalid = ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("nara.test.Invalid"),
                "Invalid",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities([ComponentCapability::Inspect])
            .with_fields([
                ComponentFieldSchema::required(
                    duplicate_id.clone(),
                    "First",
                    ComponentFieldPath::from_fields(["first"]),
                    ComponentValueKind::F64,
                )
                .with_capabilities([ComponentCapability::Inspect]),
                ComponentFieldSchema::required(
                    duplicate_id,
                    "Second",
                    ComponentFieldPath::from_fields(["second"]),
                    ComponentValueKind::F64,
                )
                .with_capabilities([ComponentCapability::Inspect]),
            ]),
        ],
        ..ComponentSchemaCatalog::default()
    };
    let encoded =
        serde_json::to_vec(&envelope("component_schema_catalog", 1, "0.1.0", invalid)).unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes(&encoded),
        Err(ComponentCatalogFileError::Catalog(
            ComponentRegistryError::DuplicateComponentFieldId { .. }
        ))
    ));
}

#[derive(Component)]
struct Position(f32);

#[test]
fn loaded_catalog_remains_separate_from_native_binding_until_atomic_freeze() {
    let encoded = sample_catalog().to_json_string().unwrap();
    let catalog = ComponentSchemaCatalog::from_json_bytes(encoded.as_bytes()).unwrap();
    let id = ComponentTypeId::new("nara.test.Position");
    let mut registry = ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("nara.test.loaded-catalog"),
        catalog,
        None,
    )
    .unwrap();
    let before = registry.catalog_candidate().clone();

    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::MissingNativeBinding { component_id }) if component_id == id
    ));
    assert_eq!(registry.catalog_candidate(), &before);
    assert!(!registry.is_frozen());

    registry
        .register_native_component_with_codec::<Position, _, _>(
            &id,
            |value| Ok(Position(value.field_f64("field_0")? as f32)),
            |position| {
                Ok(ComponentValue::map([
                    ("field_0", ComponentValue::f64(f64::from(position.0))?),
                    ("field_1", ComponentValue::f64(0.0)?),
                ]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    assert!(registry.is_frozen());
}

#[test]
fn canonical_component_catalog_v1_fixtures_reserialize_exactly() {
    let json = include_str!("../../../tests/fixtures/formats/v1/component_schema_catalog.json");
    let ron = include_str!("../../../tests/fixtures/formats/v1/component_schema_catalog.ron");

    let from_json = ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap();
    assert_golden(&from_json.to_json_string().unwrap(), json);
    let from_ron = ComponentSchemaCatalog::from_ron_bytes(ron.as_bytes()).unwrap();
    assert_golden(&from_ron.to_ron_string().unwrap(), ron);
}

fn assert_golden(encoded: &str, fixture: &str) {
    assert!(!fixture.contains('\r'), "canonical fixtures must use LF");
    assert_eq!(format!("{encoded}\n"), fixture);
}

fn _codec_error_is_concrete(error: ComponentCodecError) -> ComponentCodecError {
    error
}
