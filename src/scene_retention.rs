//! Shared Scene allocation observations and the retained material record charge.

pub(crate) use nara_scene::retention::direct_startup_scene_retained_bytes;
#[cfg(feature = "serde")]
pub(crate) use nara_scene::retention::{
    ARC_CONTROL_BLOCK_BYTES, direct_runtime_scene_retained_bytes,
    runtime_startup_scene_retained_bytes,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) use nara_scene::retention::{BTREE_ENTRY_OVERHEAD, STRING_ALLOCATION_OVERHEAD};
#[cfg(feature = "project-source")]
pub(crate) use nara_scene::retention::{
    VALUE_NODE_ALLOCATION_BYTES, compiled_scene_retained_bytes, prefab_retained_bytes,
    scene_retained_bytes, startup_scene_retained_bytes, string_retained_bytes,
};

#[cfg(all(feature = "project-runtime", feature = "runtime-2d", feature = "serde"))]
use std::mem::size_of;

#[cfg(all(feature = "project-runtime", feature = "runtime-2d", feature = "serde"))]
pub(crate) fn material_record_work_bytes(cost: nara_reflect::ComponentValueCost) -> Option<usize> {
    cost.nodes()
        .checked_mul(size_of::<nara_reflect::ComponentValue>().checked_add(BTREE_ENTRY_OVERHEAD)?)?
        .checked_add(cost.logical_bytes())?
        .checked_add(size_of::<nara_render::material::Material2dRecord>())
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;
    use nara_reflect::ComponentRegistryBuilder;
    use nara_scene::{
        SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
    };

    fn scene_id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    #[cfg(feature = "serde")]
    #[test]
    fn runtime_retention_charges_unused_definitions_and_enforces_the_exact_direct_limit() {
        use nara_reflect::{
            ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
            ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
            ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
            PersistentComponentSchemaOwnerId,
        };
        use nara_scene::{RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentBindings};

        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let document =
            std::sync::Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                scene_id("local"),
            )]));
        let key = SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "retention.runtime-definitions",
            SourceDocumentKind::Scene,
        )
        .unwrap();
        let cook = |alias: &str, initial: &str| {
            let owner = ComponentSchemaOwnerSnapshot::admit(
                PersistentComponentSchemaOwnerId::try_new("test.retention.runtime-data").unwrap(),
                ComponentSchemaCatalog {
                    components: vec![
                        ComponentSchema::new(
                            ComponentTypeId::new("test.retention.Unused"),
                            alias,
                            ComponentSchemaVersion::ONE,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                        .with_fields([ComponentFieldSchema::required(
                            ComponentFieldId::new("value"),
                            "Value",
                            ComponentFieldPath::empty(),
                            ComponentValueKind::String,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                        .with_authoring_initial_value(ComponentValue::String(initial.to_owned())),
                    ],
                    ..Default::default()
                },
                None,
            )
            .unwrap();
            let bindings = SceneComponentBindings::new(registry.clone(), [owner]).unwrap();
            let compiled =
                crate::startup_scene::compile_test_scene(&document, &bindings, key.clone());
            RuntimeSceneCandidate::cook(
                &compiled,
                &bindings,
                nara_asset::ProjectAssetDatabase::default()
                    .resolver(&nara_asset::AssetPathScope::ROOT),
                RuntimeSceneLimits::default(),
            )
            .unwrap()
        };
        let short = cook("Short", "x");
        let long_alias = "A longer unused definition alias";
        let long = cook(long_alias, "x");
        let initial = "A retained creation template independent of the document";
        let template = cook("Short", initial);
        assert_eq!(short.document(), long.document());
        assert_eq!(short.canonical_provenance(), long.canonical_provenance());
        assert_eq!(short.canonical_provenance().len(), 1);
        assert_eq!(
            runtime_startup_scene_retained_bytes(&long).unwrap()
                - runtime_startup_scene_retained_bytes(&short).unwrap(),
            long_alias.len() - "Short".len()
        );
        assert_eq!(
            runtime_startup_scene_retained_bytes(&template).unwrap()
                - runtime_startup_scene_retained_bytes(&short).unwrap(),
            initial.len() - "x".len()
        );
        let required = direct_runtime_scene_retained_bytes(&template).unwrap();
        let source = crate::startup_scene::SceneSource::runtime(
            template,
            nara_core::ByteLimit::new(required).unwrap(),
        )
        .unwrap();
        assert_eq!(source.retained_bytes(), required);
        #[cfg(feature = "project-source")]
        assert!(!source.is_project_backed());
        assert!(!source.is_content_managed());
        assert!(source.asset_resolution().is_none());
        assert_eq!(
            crate::startup_scene::SceneSource::runtime(
                cook("Short", initial),
                nara_core::ByteLimit::new(required - 1).unwrap(),
            )
            .unwrap_err(),
            crate::startup_scene::SceneSourceError::RetainedBytesExceeded {
                required,
                limit: required - 1,
            }
        );
    }
}
