use std::convert::Infallible;

use nara_asset::{AssetPath, AssetRecord, AssetSourceKind, ProjectAssetDatabase, StableAssetId};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
};

#[test]
fn data_defaults_close_nested_asset_references_over_the_selected_metadata_without_a_codec() {
    let owner = PersistentComponentSchemaOwnerId::try_new("game.images").unwrap();
    let component = ComponentTypeId::new("game.Images");
    let asset = StableAssetId::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let references: Vec<_> = [
        ("path", "textures/icon.png".to_owned()),
        ("stable_id", asset.to_string()),
    ]
    .into_iter()
    .map(|(kind, value)| {
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.into())),
            ("value", ComponentValue::String(value)),
        ])
    })
    .collect();
    for values in [
        vec![references[0].clone()],
        vec![references[1].clone()],
        references,
    ] {
        let references = ComponentValue::List(values);
        let schema = ComponentSchema::new(component.clone(), "Images", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("icons"),
                "Icons",
                ComponentFieldPath::from_fields(["icons"]),
                ComponentValueShape::list(ComponentValueShape::AssetRef),
                references.clone(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef)]);
        let snapshot = ComponentSchemaOwnerSnapshot::admit(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let mut native = ComponentRegistry::new();
        native.freeze().unwrap();
        let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [snapshot]).unwrap();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("image").unwrap(),
        )
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            )
            .with_owner(owner.clone()),
        )]);
        let before = document.clone();
        let mut complete = ProjectAssetDatabase::default();
        complete
            .insert(AssetRecord::new(
                asset,
                AssetPath::new("textures/icon.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        let missing = ProjectAssetDatabase::default();
        for (database, expected_success) in [(&complete, true), (&missing, false)] {
            let report = bindings
                .try_expand_prefabs_with_loader(
                    &document,
                    Default::default(),
                    Some(database),
                    |_, _| -> Result<_, Infallible> {
                        panic!("a source-free document must not load a Prefab");
                    },
                )
                .unwrap();
            assert_eq!(
                !report.diagnostics.has_errors(),
                expected_success,
                "{report:?}"
            );
            if expected_success {
                let result = report.document().unwrap();
                assert_eq!(
                    result.entities[0].components[&component].value.get("icons"),
                    Some(&references)
                );
            } else {
                assert!(
                    report.candidate.is_none(),
                    "missing default assets must refuse the entire candidate"
                );
            }
        }
        assert_eq!(document, before);
    }
}
