#![cfg(feature = "serde")]

use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    ComponentCapability, ComponentCatalogFileBudgetKind, ComponentCatalogFileError,
    ComponentCatalogFileLimits, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.catalog-pair").unwrap()
}

fn initial() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("test.CatalogPair"),
                "Pair",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("health"),
                "Health",
                ComponentFieldPath::from_fields(["health"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
            .with_authoring_initial_value(ComponentValue::map([(
                "health",
                ComponentValue::I64(100),
            )])),
        ],
        ..Default::default()
    }
}

fn successor(previous: &ComponentSchemaCatalog, value: i64) -> ComponentSchemaCatalog {
    let mut catalog = ComponentSchemaCatalog::successor_of(previous).unwrap();
    catalog.components = previous.components().to_vec();
    catalog.components[0].authoring_initial_value = Some(ComponentValue::map([(
        "health",
        ComponentValue::I64(value),
    )]));
    catalog
}

#[test]
fn owner_pair_decodes_immediate_later_generation_without_recursive_catalog_history() {
    let first = initial();
    let second = successor(&first, 200);
    let third = successor(&second, 300);
    let predecessor = second
        .to_json_string_with_predecessor(Some(&first))
        .unwrap();
    let current = third
        .to_json_string_with_predecessor(Some(&second))
        .unwrap();
    assert!(ComponentSchemaCatalog::from_json_bytes(predecessor.as_bytes()).is_err());
    let admitted = ComponentSchemaOwnerSnapshot::from_json_catalogs(
        owner(),
        current.as_bytes(),
        Some(predecessor.as_bytes()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(admitted.current(), &third);
    assert_eq!(admitted.predecessor(), Some(&second));
    let previous = ComponentSchemaOwnerSnapshot::admit(owner(), second, Some(first)).unwrap();
    assert!(admitted.is_creation_metadata_successor_of(&previous));
}

#[test]
fn owner_pair_requires_predecessor_for_evolved_current_and_validates_the_complete_pair() {
    let first = initial();
    let second = successor(&first, 200);
    let current = second
        .to_json_string_with_predecessor(Some(&first))
        .unwrap();
    assert!(
        ComponentSchemaOwnerSnapshot::from_json_catalogs(
            owner(),
            current.as_bytes(),
            None,
            Default::default(),
        )
        .is_err()
    );
    let mut unrelated = first.clone();
    unrelated.components[0].authoring_initial_value = None;
    let unrelated = unrelated.to_json_string().unwrap();
    assert!(
        ComponentSchemaOwnerSnapshot::from_json_catalogs(
            owner(),
            current.as_bytes(),
            Some(unrelated.as_bytes()),
            Default::default(),
        )
        .is_err()
    );
}

#[test]
fn owner_pair_refuses_an_invalid_predecessor_boundary() {
    let first = initial();
    let second = successor(&first, 200);
    let third = successor(&second, 300);
    let current = third
        .to_json_string_with_predecessor(Some(&second))
        .unwrap();
    let mut predecessor: serde_json::Value = serde_json::from_str(
        &second
            .to_json_string_with_predecessor(Some(&first))
            .unwrap(),
    )
    .unwrap();
    predecessor["payload"]["predecessor"] = serde_json::Value::Null;
    let predecessor = serde_json::to_vec(&predecessor).unwrap();
    assert!(
        ComponentSchemaOwnerSnapshot::from_json_catalogs(
            owner(),
            current.as_bytes(),
            Some(&predecessor),
            Default::default(),
        )
        .is_err()
    );
}

#[test]
fn owner_pair_charges_encoded_bytes_and_declarations_across_both_files() {
    let first = initial();
    let second = successor(&first, 200);
    let predecessor = first.to_json_string().unwrap();
    let current = second
        .to_json_string_with_predecessor(Some(&first))
        .unwrap();
    let length = predecessor.len() + current.len();
    let limits =
        ComponentCatalogFileLimits::default().with_encoded_bytes(ByteLimit::new(length).unwrap());
    assert!(
        ComponentSchemaOwnerSnapshot::from_json_catalogs(
            owner(),
            current.as_bytes(),
            Some(predecessor.as_bytes()),
            limits,
        )
        .is_ok()
    );
    assert!(matches!(ComponentSchemaOwnerSnapshot::from_json_catalogs(
        owner(), current.as_bytes(), Some(predecessor.as_bytes()),
        limits.with_encoded_bytes(ByteLimit::new(length - 1).unwrap()),
    ), Err(ComponentCatalogFileError::EncodedBytesExceeded { observed, maximum })
        if observed == length && maximum == length - 1));
    assert!(matches!(ComponentSchemaOwnerSnapshot::from_json_catalogs(
        owner(), current.as_bytes(), Some(predecessor.as_bytes()), limits.with_components(ItemLimit::ONE),
    ), Err(ComponentCatalogFileError::Budget(error)) if error.kind == ComponentCatalogFileBudgetKind::Components));
}
