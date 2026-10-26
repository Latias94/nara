#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringRecordReadiness, PrefabDocument, PrefabDocumentCandidate,
    PrefabInstance, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFileLimits, ScenePatchDocument, ScenePatchOperation,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.Assets")
}
fn asset(path: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String(path.into())),
    ])
}

fn bundle() -> AuthoringSchemaBundle {
    let field = |name, shape| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            shape,
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef])
    };
    let schema = ComponentSchema::new(component(), "Assets", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([
            field(
                "images",
                ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::AssetRef)),
            ),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueShape::AssetRef,
                asset("default.png"),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
            field(
                "optional",
                ComponentValueShape::optional(ComponentValueShape::AssetRef),
            ),
        ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.assets").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn field(
    name: &str,
    value: ComponentValue,
    version: ComponentSchemaVersion,
) -> ScenePatchOperation {
    ScenePatchOperation::SetField {
        entity: id("base"),
        component: component(),
        component_version: version,
        field: ComponentFieldId::new(name),
        value,
    }
}

fn document(operations: impl IntoIterator<Item = ScenePatchOperation>) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("base.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new(operations),
    });
    SceneDocument::from_ordered_roots([anchor])
}

#[test]
fn field_dependencies_include_nested_snapshots_and_exposed_defaults_without_fake_records() {
    let snapshots = |name| {
        Some(PrefabInstance {
            source: AssetRef::path("nested.prefab.json").unwrap(),
            overrides: ScenePatchDocument::new([field(
                "image",
                asset(name),
                ComponentSchemaVersion::ONE,
            )]),
        })
    };
    let document = document([
        field(
            "images",
            ComponentValue::List(vec![ComponentValue::map([
                ("left", asset("left.png")),
                ("right", asset("right.png")),
            ])]),
            ComponentSchemaVersion::ONE,
        ),
        ScenePatchOperation::SetAssetRefField {
            entity: id("base"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("image"),
            asset_ref: AssetRef::path("explicit.png").unwrap(),
        },
        ScenePatchOperation::RemoveField {
            entity: id("base"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("image"),
        },
        field(
            "optional",
            ComponentValue::Null,
            ComponentSchemaVersion::ONE,
        ),
        ScenePatchOperation::SetPrefabInstance {
            entity: id("base"),
            expected: snapshots("prior.png"),
            value: snapshots("next.png"),
        },
    ]);
    let bundle = bundle();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    assert!(published.record_facts().is_empty());
    assert!(
        published
            .unavailable_manifest()
            .entries()
            .iter()
            .all(|entry| matches!(
                entry, nara_scene::UnavailableRecordManifestEntry::PatchTarget(target)
                    if target.readiness() == AuthoringRecordReadiness::KnownUnbound
            ))
    );
    let expected = [
        "left.png",
        "right.png",
        "explicit.png",
        "default.png",
        "prior.png",
        "next.png",
    ]
    .map(|path| AssetRef::path(path).unwrap());
    assert_eq!(
        published
            .schema_declared_asset_references()
            .map(|reference| reference.asset_ref().clone())
            .collect::<Vec<_>>(),
        expected
    );
    let prefab = PrefabDocument::new(document.roots, document.entities);
    let published =
        PrefabDocumentCandidate::decode_ron_bytes(prefab.to_ron_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefab.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    assert!(published.record_facts().is_empty());
    assert_eq!(
        published
            .schema_declared_asset_references()
            .map(|reference| reference.asset_ref().clone())
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn unknown_or_version_incompatible_field_payloads_are_preserved_without_dependency_guessing() {
    let bundle = bundle();
    for version in [ComponentSchemaVersion::ONE, ComponentSchemaVersion(2)] {
        let document = document([field(
            "image",
            ComponentValue::String("invalid as an asset".into()),
            version,
        )]);
        let candidate = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap();
        let result = candidate.admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
            &bundle,
            &[],
        );
        if version == ComponentSchemaVersion::ONE {
            assert!(result.is_err());
        } else {
            let published = result.unwrap().publish();
            assert_eq!(published.schema_declared_asset_references().count(), 0);
            assert_eq!(published.document(), &document);
        }
    }
    let document = document([field(
        "image",
        asset("unknown.png"),
        ComponentSchemaVersion::ONE,
    )]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
                &AuthoringSchemaBundle::new([]).unwrap(),
                &[],
            )
            .unwrap()
            .publish();
    assert_eq!(published.schema_declared_asset_references().count(), 0);
    assert_eq!(published.document(), &document);
}

#[test]
fn repeated_default_reference_facts_share_the_remaining_traversal_allowance() {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.assets").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component(), "Assets", ComponentSchemaVersion::ONE)
                    .with_capabilities([ComponentCapability::Scene])
                    .with_fields([ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("image"),
                        "Image",
                        ComponentFieldPath::from_fields(["image"]),
                        ComponentValueShape::AssetRef,
                        asset("p"),
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::AssetRef,
                    ])]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let document = document((0..2).map(|_| ScenePatchOperation::RemoveField {
        entity: id("base"),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("image"),
    }));
    let encoded = document.to_json_string().unwrap();
    // One default's traversal sees 3 value nodes + 1 reference and 14 payload + 11 result
    // bytes. Its retained projection reserves 1 node/11 bytes before the second call.
    for (nodes, bytes, accepted) in [(5, 36, true), (4, 36, false), (5, 35, false)] {
        let limits = SceneFileLimits::default()
            .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
            .with_component_value_bytes(ByteLimit::new(bytes).unwrap());
        let result =
            SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits)
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
                    &bundle,
                    &[],
                );
        if accepted {
            assert_eq!(
                result
                    .unwrap()
                    .publish()
                    .schema_declared_asset_references()
                    .count(),
                2
            );
        } else {
            assert!(result.is_err(), "nodes={nodes}, bytes={bytes}");
        }
    }
}
