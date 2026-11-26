#![cfg(feature = "serde")]

use nara_core::ByteLimit;
use nara_reflect::{
    ComponentCatalogFileError, ComponentCatalogFileLimits, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeErrorKind, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};

fn schema(id: &str) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(id),
        "Example",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )])
}

#[test]
fn catalog_is_plain_canonical_json() {
    let catalog =
        ComponentSchemaCatalog::from_components([schema("game.Z"), schema("game.A")]).unwrap();
    let json = catalog.to_json_string().unwrap();
    let wire: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.as_object().unwrap().len(), 1);
    assert_eq!(wire["components"][0]["id"], "game.A");
    assert_eq!(wire["components"][1]["id"], "game.Z");
    let decoded = ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap();
    assert_eq!(decoded, catalog);
    assert_eq!(decoded.fingerprint(), catalog.fingerprint());
    assert_eq!(decoded.to_json_string().unwrap(), json);
}

#[test]
fn encoded_byte_limit_is_inclusive_and_checked_before_json_parsing() {
    let json = ComponentSchemaCatalog::default().to_json_string().unwrap();
    let limits = |max| {
        ComponentCatalogFileLimits::default().with_encoded_bytes(ByteLimit::new(max).unwrap())
    };
    assert!(
        ComponentSchemaCatalog::from_json_bytes_with_limits(json.as_bytes(), limits(json.len()))
            .is_ok()
    );
    let invalid = vec![b'!'; json.len()];
    assert!(
        matches!(ComponentSchemaCatalog::from_json_bytes_with_limits(&invalid, limits(json.len() - 1)), Err(ComponentCatalogFileError::EncodedBytesExceeded { maximum }) if maximum == json.len() - 1)
    );
}

#[test]
fn decoder_rejects_duplicate_ids_and_invalid_or_unrecognized_metadata() {
    let component = serde_json::to_value(schema("game.A")).unwrap();
    let duplicate = serde_json::json!({ "components": [component.clone(), component.clone()] });
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes(&serde_json::to_vec(&duplicate).unwrap()),
        Err(ComponentCatalogFileError::Catalog(
            ComponentRegistryError::DuplicateComponentId(_)
        ))
    ));
    let mut bad_default = component;
    bad_default["fields"][0]["default_value"] =
        serde_json::json!({ "type": "bool", "value": true });
    let invalid = serde_json::json!({ "components": [bad_default] });
    assert!(matches!(
        ComponentSchemaCatalog::from_json_bytes(&serde_json::to_vec(&invalid).unwrap()),
        Err(ComponentCatalogFileError::Catalog(
            ComponentRegistryError::InvalidComponentFieldDefault { .. }
        ))
    ));
    for unknown in [
        br#"{"components": [], "extra": true}"#.as_slice(),
        br#"{"payload": {"components": []}}"#,
    ] {
        assert!(matches!(
            ComponentSchemaCatalog::from_json_bytes(unknown),
            Err(ComponentCatalogFileError::Json(_))
        ));
    }
}

#[test]
fn encoder_rejects_output_above_the_shared_byte_limit() {
    let mut components = [schema("game.A"), schema("game.B"), schema("game.C")];
    for component in &mut components {
        component.fields[0].shape = ComponentValueKind::String.into();
        component.fields[0].default_value =
            Some(ComponentValue::String("x".repeat(3 * 1024 * 1024)));
    }
    let catalog = ComponentSchemaCatalog::from_components(components).unwrap();
    assert!(
        matches!(catalog.to_json_string(), Err(ComponentCatalogFileError::EncodedBytesExceeded { maximum }) if maximum == ComponentCatalogFileLimits::default().encoded_bytes().get())
    );
}

#[test]
fn maximum_record_nesting_round_trips_with_the_default_json_reader() {
    let mut component = schema("game.Deep");
    let field = &mut component.fields[0];
    field.path = ComponentFieldPath::empty();
    field.shape = (0..40).fold(ComponentValueKind::Bool.into(), |shape, _| {
        ComponentValueShape::record([("child".to_owned(), shape)])
    });
    let catalog = ComponentSchemaCatalog::from_components([component.clone()]).unwrap();
    let json = catalog.to_json_string().unwrap();
    assert_eq!(
        ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap(),
        catalog
    );

    let field = &mut component.fields[0];
    field.shape = ComponentValueShape::record([("child".to_owned(), field.shape.clone())]);
    assert!(matches!(
        ComponentSchemaCatalog::from_components([component]),
        Err(ComponentRegistryError::InvalidComponentFieldShape { source, .. })
            if source.kind == ComponentShapeErrorKind::DepthLimit
    ));
}
