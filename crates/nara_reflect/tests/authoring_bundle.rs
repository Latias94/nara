#![cfg(feature = "serde")]

use nara_core::ItemLimit;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, AuthoringSchemaBundleLimits,
    AuthoringSchemaBundleOwner, ComponentCatalogFileBudgetKind, ComponentCatalogFileLimits,
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion,
    ComponentShapeErrorKind, ComponentTypeId, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use serde_json::{Value, json};

fn component_catalog(id: &str, alias: &str, version: u32) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![ComponentSchema::new(
            ComponentTypeId::new(id),
            alias,
            ComponentSchemaVersion::new(version).unwrap(),
        )],
        ..ComponentSchemaCatalog::default()
    }
}

#[test]
fn bundle_admission_rejects_deep_caller_shape_before_canonical_snapshot() {
    let mut catalog = component_catalog("nara.test.DeepBundle", "Deep bundle", 1);
    let mut shape = ComponentValueShape::scalar(ComponentValueKind::Bool);
    for _depth in 0..512 {
        shape = ComponentValueShape::list(shape);
    }
    catalog.components[0].fields = vec![ComponentFieldSchema::required(
        ComponentFieldId::new("data"),
        "Data",
        ComponentFieldPath::from_fields(["data"]),
        shape,
    )];
    let error = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("nara.test.deep-bundle").unwrap(),
        catalog,
        None,
    )])
    .unwrap_err();
    assert!(matches!(
        error,
        AuthoringSchemaBundleError::Catalog { source, .. }
            if matches!(source.downcast_ref::<ComponentRegistryError>(), Some(
                ComponentRegistryError::InvalidComponentFieldShape { source, .. }
            ) if source.kind == ComponentShapeErrorKind::DepthLimit)
    ));
}

fn successor_of(predecessor: &ComponentSchemaCatalog) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        generation: 2,
        predecessor: Some(predecessor.fingerprint()),
        components: vec![ComponentSchema::new(
            ComponentTypeId::new("nara.test.Beta"),
            "Beta",
            ComponentSchemaVersion::new(2).unwrap(),
        )],
        type_tombstones: Vec::new(),
    }
}

fn envelope(owners: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "kind": "authoring_schema_bundle",
        "format_version": 1,
        "engine_min_version": "0.1.0",
        "generator": {
            "name": "nara",
            "version": "0.1.0"
        },
        "payload": {
            "owners": owners
        }
    }))
    .unwrap()
}

fn owner_value(
    owner: &str,
    current: &ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
) -> Value {
    let mut value = json!({
        "owner": owner,
        "current": current,
    });
    if let Some(predecessor) = predecessor {
        value["predecessor"] = serde_json::to_value(predecessor).unwrap();
    }
    value
}

#[test]
fn canonical_multi_owner_json_roundtrip_derives_composition_fingerprint() {
    let alpha = component_catalog("nara.test.Alpha", "Alpha", 1);
    let beta_predecessor = component_catalog("nara.test.Beta", "Beta", 1);
    let beta = successor_of(&beta_predecessor);
    let bundle = AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("nara.test.beta").unwrap(),
            beta,
            Some(beta_predecessor),
        ),
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("nara.test.alpha").unwrap(),
            alpha,
            None,
        ),
    ])
    .unwrap();

    assert_eq!(bundle.owners()[0].owner_id().as_str(), "nara.test.alpha");
    assert_eq!(bundle.owners()[1].current().generation(), 2);
    assert_eq!(bundle.owners()[1].predecessor().unwrap().generation(), 1);
    assert_eq!(
        bundle.fingerprint().to_hex(),
        "17077fc2a6937fc1bc3f11c2a8d9f74e7b35aeabe9e48323dd6eae5918df4405"
    );
    assert_eq!(
        bundle.owners()[0]
            .owner_id()
            .matches_executable(ComponentSchemaOwnerId::new("nara.test.alpha")),
        Ok(true)
    );

    let encoded = bundle.to_json_string().unwrap();
    let encoded_value = serde_json::from_str::<Value>(&encoded).unwrap();
    assert_eq!(encoded_value["kind"], "authoring_schema_bundle");
    assert_eq!(encoded_value["format_version"], 1);
    assert_eq!(
        encoded_value["payload"]["owners"][0]["owner"],
        "nara.test.alpha"
    );
    assert!(
        encoded_value["payload"]
            .get("composition_fingerprint")
            .is_none()
    );
    assert_eq!(
        AuthoringSchemaBundle::from_json_bytes(encoded.as_bytes()).unwrap(),
        bundle
    );
}

#[test]
fn duplicate_owner_and_divergent_component_ownership_are_rejected() {
    let alpha = component_catalog("nara.test.Shared", "Shared", 1);
    let duplicate_owner = owner_value("nara.test.alpha", &alpha, None);
    let cases = [
        (
            "duplicate owner",
            envelope(vec![duplicate_owner.clone(), duplicate_owner]),
            true,
        ),
        (
            "divergent component ownership",
            envelope(vec![
                owner_value("nara.test.alpha", &alpha, None),
                owner_value("nara.test.beta", &alpha, None),
            ]),
            false,
        ),
    ];

    for (name, encoded, duplicate) in cases {
        let error = AuthoringSchemaBundle::from_json_bytes(&encoded).unwrap_err();
        if duplicate {
            assert!(
                matches!(error, AuthoringSchemaBundleError::DuplicateOwner { .. }),
                "{name}: {error:?}"
            );
        } else {
            assert!(
                matches!(
                    error,
                    AuthoringSchemaBundleError::DivergentComponentOwnership { .. }
                ),
                "{name}: {error:?}"
            );
        }
    }
}

#[test]
fn aggregate_owner_limit_rejects_before_bundle_publication() {
    let alpha = component_catalog("nara.test.Alpha", "Alpha", 1);
    let beta = component_catalog("nara.test.Beta", "Beta", 1);
    let encoded = envelope(vec![
        owner_value("nara.test.alpha", &alpha, None),
        owner_value("nara.test.beta", &beta, None),
    ]);
    let limits = AuthoringSchemaBundleLimits::default().with_owners(ItemLimit::ONE);

    assert!(matches!(
        AuthoringSchemaBundle::from_json_bytes_with_limits(&encoded, limits),
        Err(AuthoringSchemaBundleError::OwnerLimitExceeded {
            observed: 2,
            maximum: 1
        })
    ));
}

#[test]
fn aggregate_catalog_budget_spans_current_and_predecessor() {
    let predecessor = component_catalog("nara.test.Beta", "Beta", 1);
    let current = successor_of(&predecessor);
    let encoded = envelope(vec![owner_value(
        "nara.test.beta",
        &current,
        Some(&predecessor),
    )]);
    let limits = AuthoringSchemaBundleLimits::default()
        .with_catalogs(ComponentCatalogFileLimits::default().with_components(ItemLimit::ONE));

    assert!(matches!(
        AuthoringSchemaBundle::from_json_bytes_with_limits(&encoded, limits),
        Err(AuthoringSchemaBundleError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::Components
                && error.observed == 2
                && error.maximum == 1
    ));
}

#[test]
fn unsupported_bundle_envelope_is_rejected_before_payload_publication() {
    let alpha = component_catalog("nara.test.Alpha", "Alpha", 1);
    let mut encoded = serde_json::from_slice::<Value>(&envelope(vec![owner_value(
        "nara.test.alpha",
        &alpha,
        None,
    )]))
    .unwrap();
    for version in [2, 3, u32::MAX] {
        encoded["format_version"] = Value::from(version);
        assert!(matches!(
            AuthoringSchemaBundle::from_json_bytes(&serde_json::to_vec(&encoded).unwrap()),
            Err(AuthoringSchemaBundleError::Contract(_))
        ));
    }
    encoded["format_version"] = Value::from(1);
    encoded["kind"] = Value::String("future_authoring_bundle".to_owned());
    let encoded = serde_json::to_vec(&encoded).unwrap();

    assert!(matches!(
        AuthoringSchemaBundle::from_json_bytes(&encoded),
        Err(AuthoringSchemaBundleError::Contract(_))
    ));
}
