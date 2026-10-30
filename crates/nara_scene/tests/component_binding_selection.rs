//! Public Scene selection identity, source admission and strict backend-free validation.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_app::PluginSchemaProviderId;
use nara_asset::{AssetRecord, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Component;
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryError, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaOwnerSnapshot,
    ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSourceError, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindingError, SceneComponentBindingLimits,
    SceneComponentBindings, SceneComponentBindingsWitness, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord,
};

const SCORE_OWNER: &str = "game.selection";
const SCORE_COMPONENT: &str = "game.Score";
const SCORE_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new(SCORE_OWNER),
    PluginSchemaProviderId::new("game.selection.native"),
    ComponentSchemaProviderBindingId::new("game.selection.binding", 1),
    score_catalog,
    register_score,
);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn owner_id(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn empty_value() -> ComponentValue {
    ComponentValue::map([] as [(&str, ComponentValue); 0])
}

fn score_schema(default: u64) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(SCORE_COMPONENT),
        "Score",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
        ComponentValue::U64(default),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn score_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![score_schema(7)],
        ..Default::default()
    })
}

#[derive(Component)]
struct Score(u64);

fn register_score(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_with_codec::<Score, _, _>(
        score_schema(7),
        |value| Ok(Score(value.field_u64("value")?)),
        |score| {
            Ok(ComponentValue::map([(
                "value",
                ComponentValue::U64(score.0),
            )]))
        },
    )?;
    Ok(())
}

fn score_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(owner_id(SCORE_OWNER), score_catalog().unwrap(), None)
        .unwrap()
}

fn retired_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        owner_id("game.retired"),
        ComponentSchemaCatalog {
            type_tombstones: vec![ComponentTypeId::new("game.Retired")],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn native() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistry::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    registry.snapshot().unwrap()
}

fn empty_native() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    registry.snapshot().unwrap()
}

fn score_entity(name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        ComponentTypeId::new(SCORE_COMPONENT),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty_value())
            .with_owner(owner_id(SCORE_OWNER)),
    )
}

fn assert_code(report: &DiagnosticReport, code: &str) {
    assert!(
        report
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "expected {code}: {report:?}"
    );
}

#[test]
fn exact_selection_and_witness_ignore_container_and_input_order_but_not_readmission() {
    let native = native();
    let score = score_owner();
    let retired = retired_owner();
    let selected =
        SceneComponentBindings::new(native.clone(), [score.clone(), retired.clone()]).unwrap();
    let reordered =
        SceneComponentBindings::new(native.clone(), [retired.clone(), score.clone()]).unwrap();
    let witness: SceneComponentBindingsWitness = selected.witness();
    assert!(selected.same_definitions(&selected.clone()));
    assert!(selected.same_definitions(&reordered));
    assert!(witness.clone().matches(&reordered));
    drop(selected);
    assert!(witness.matches(&reordered));

    for different in [
        SceneComponentBindings::new(native.clone(), [score.clone()]).unwrap(),
        SceneComponentBindings::new(native.clone(), [score_owner(), retired.clone()]).unwrap(),
        SceneComponentBindings::new(native.clone(), [score.clone(), retired_owner()]).unwrap(),
        SceneComponentBindings::new(self::native(), [score, retired]).unwrap(),
    ] {
        assert!(!reordered.same_definitions(&different));
        assert!(!different.same_definitions(&reordered));
        assert!(!witness.matches(&different));
    }
}

#[test]
fn retained_witness_does_not_keep_native_codec_captures_alive() {
    let (witness, code_capture) = {
        let calls = Arc::new(AtomicUsize::new(0));
        let weak = Arc::downgrade(&calls);
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_with_codec::<Score, _, _>(
                score_schema(7),
                move |value| {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Ok(Score(value.field_u64("value")?))
                },
                |score| {
                    Ok(ComponentValue::map([(
                        "value",
                        ComponentValue::U64(score.0),
                    )]))
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        let bindings = SceneComponentBindings::new(registry.snapshot().unwrap(), []).unwrap();
        let witness = bindings.witness();
        assert!(witness.matches(&bindings));
        assert!(weak.upgrade().is_some());
        (witness, weak)
    };
    assert!(code_capture.upgrade().is_none());
    let successor = SceneComponentBindings::new(empty_native(), []).unwrap();
    assert!(!witness.matches(&successor));
}

#[test]
fn mixed_fingerprint_reuses_the_native_owner_composition_domain() {
    let native = native();
    let score = score_owner();
    let mixed = SceneComponentBindings::new(native.clone(), [score.clone()]).unwrap();
    let native_only = SceneComponentBindings::new(native.clone(), []).unwrap();
    assert_eq!(
        native_only.composition_fingerprint(),
        native.schema_composition_fingerprint().unwrap()
    );
    assert_ne!(
        mixed.composition_fingerprint(),
        native_only.composition_fingerprint()
    );

    let mut all_native = ComponentRegistry::new();
    SCORE_PROVIDER
        .register_or_validate_into(&mut all_native)
        .unwrap();
    all_native.freeze().unwrap();
    let data_only = SceneComponentBindings::new(empty_native(), [score.clone()]).unwrap();
    assert_eq!(
        data_only.composition_fingerprint(),
        all_native
            .snapshot()
            .unwrap()
            .schema_composition_fingerprint()
            .unwrap()
    );
    let mut all_native = ComponentRegistry::new();
    SCORE_PROVIDER
        .register_or_validate_into(&mut all_native)
        .unwrap();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut all_native)
        .unwrap();
    all_native.freeze().unwrap();
    assert_eq!(
        mixed.composition_fingerprint(),
        all_native
            .snapshot()
            .unwrap()
            .schema_composition_fingerprint()
            .unwrap()
    );
    let readmitted = SceneComponentBindings::new(native.clone(), [score_owner()]).unwrap();
    assert_eq!(
        mixed.composition_fingerprint(),
        readmitted.composition_fingerprint()
    );
    assert!(!mixed.same_definitions(&readmitted));

    let retired = retired_owner();
    let first =
        SceneComponentBindings::new(native.clone(), [score.clone(), retired.clone()]).unwrap();
    let second = SceneComponentBindings::new(native.clone(), [retired, score.clone()]).unwrap();
    assert_eq!(
        first.composition_fingerprint(),
        second.composition_fingerprint()
    );
    assert_ne!(
        first.composition_fingerprint(),
        mixed.composition_fingerprint()
    );
    for (owner, schema) in [
        (SCORE_OWNER, score_schema(8)),
        ("game.renamed-selection", score_schema(7)),
    ] {
        let changed = ComponentSchemaOwnerSnapshot::admit(
            owner_id(owner),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let changed = SceneComponentBindings::new(native.clone(), [changed]).unwrap();
        assert_ne!(
            mixed.composition_fingerprint(),
            changed.composition_fingerprint()
        );
    }
}

#[test]
fn binding_errors_remain_cloneable_typed_owner_component_and_limit_failures() {
    let native = empty_native();
    let score = score_owner();
    let duplicate =
        SceneComponentBindings::new(native.clone(), [score.clone(), score.clone()]).unwrap_err();
    assert_eq!(duplicate.clone(), duplicate);
    assert_eq!(
        duplicate,
        SceneComponentBindingError::DuplicateOwner {
            owner: owner_id(SCORE_OWNER),
        }
    );
    let foreign = ComponentSchemaOwnerSnapshot::admit(
        owner_id("game.foreign"),
        score_catalog().unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        SceneComponentBindings::new(native.clone(), [score.clone(), foreign]).unwrap_err(),
        SceneComponentBindingError::DivergentComponentOwnership {
            component: ComponentTypeId::new(SCORE_COMPONENT),
        }
    );
    for (limits, expected) in [
        (
            SceneComponentBindingLimits::default().with_owners(ItemLimit::new(1).unwrap()),
            SceneComponentBindingError::OwnerLimitExceeded {
                observed: 2,
                maximum: 1,
            },
        ),
        (
            SceneComponentBindingLimits::default().with_claims(ItemLimit::new(1).unwrap()),
            SceneComponentBindingError::ClaimLimitExceeded {
                observed: 2,
                maximum: 1,
            },
        ),
    ] {
        assert_eq!(
            SceneComponentBindings::new_with_limits(
                native.clone(),
                [score.clone(), retired_owner()],
                limits,
            )
            .unwrap_err(),
            expected
        );
    }
}

#[test]
fn validation_keeps_real_native_codecs_strict_and_defaults_out_of_the_source() {
    let bindings = SceneComponentBindings::new(native(), [score_owner()]).unwrap();
    let visibility = ComponentTypeId::new("nara.scene.Visibility");
    let mut document = SceneDocument::from_ordered_roots([score_entity("root").with_component(
        visibility.clone(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("visible".into()),
        ),
    )]);
    let before = document.clone();
    let report = bindings.validate_scene(&document, None, Default::default());
    assert!(!report.has_errors(), "{report:?}");
    assert_eq!(document, before);
    document.entities[0]
        .components
        .get_mut(&visibility)
        .unwrap()
        .value = ComponentValue::String("not-a-visibility".into());
    assert_code(
        &bindings.validate_scene(&document, None, Default::default()),
        "scene.invalid-component-payload",
    );
}

#[test]
fn validation_requires_explicit_data_owner_current_version_and_one_aggregate_allowance() {
    let bindings = SceneComponentBindings::new(native(), [score_owner()]).unwrap();
    let document = SceneDocument::from_ordered_roots([score_entity("root")]);
    let component = ComponentTypeId::new(SCORE_COMPONENT);
    for owner in [None, Some(owner_id("game.foreign"))] {
        let mut changed = document.clone();
        changed.entities[0]
            .components
            .get_mut(&component)
            .unwrap()
            .owner = owner;
        assert_code(
            &bindings.validate_scene(&changed, None, Default::default()),
            "scene.component-owner-mismatch",
        );
    }
    let mut changed = document.clone();
    changed.entities[0]
        .components
        .get_mut(&component)
        .unwrap()
        .version = ComponentSchemaVersion::new(2).unwrap();
    assert!(
        bindings
            .validate_scene(&changed, None, Default::default())
            .has_errors()
    );

    let limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(2).unwrap(),
        ByteLimit::new(1_024).unwrap(),
        DepthLimit::new(8).unwrap(),
    );
    assert!(
        !bindings
            .validate_scene(&document, None, limits)
            .has_errors()
    );
    let mixed = SceneDocument::from_ordered_roots([score_entity("root").with_component(
        ComponentTypeId::new("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("Root".into()),
        ),
    )]);
    assert_code(
        &bindings.validate_scene(&mixed, None, limits),
        "scene.invalid-component-payload",
    );
    let across_entities =
        SceneDocument::from_ordered_roots([score_entity("one"), score_entity("two")]);
    assert_code(
        &bindings.validate_scene(&across_entities, None, limits),
        "scene.invalid-component-payload",
    );
}

#[test]
fn validation_closes_default_references_over_scene_entities_and_optional_asset_metadata() {
    let asset_path = AssetSourcePath::new("textures/selection.png").unwrap();
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner_id("game.references"),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("game.References"),
                    "References",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("target"),
                        "Target",
                        ComponentFieldPath::from_fields(["target"]),
                        ComponentValueKind::EntityRef,
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("target"),
                        }),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_capability(ComponentCapability::EntityRef),
                    ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("image"),
                        "Image",
                        ComponentFieldPath::from_fields(["image"]),
                        ComponentValueKind::AssetRef,
                        ComponentValue::map([
                            ("kind", ComponentValue::String("path".into())),
                            ("value", ComponentValue::String(asset_path.to_string())),
                        ]),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_capability(ComponentCapability::AssetRef),
                ]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bindings = SceneComponentBindings::new(empty_native(), [owner.clone()]).unwrap();
    let root = SceneEntityRecord::new(id("root")).with_component(
        ComponentTypeId::new("game.References"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty_value())
            .with_owner(owner.owner_id().clone()),
    );
    let document =
        SceneDocument::from_ordered_roots([root.clone(), SceneEntityRecord::new(id("target"))]);
    let before = document.clone();
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            "11111111-1111-4111-8111-111111111111".parse().unwrap(),
            asset_path,
            AssetSourceKind::Image,
        ))
        .unwrap();
    let report = bindings.validate_scene(
        &document,
        Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        Default::default(),
    );
    assert!(!report.has_errors(), "{report:?}");
    let report = bindings.validate_scene(&document, None, Default::default());
    assert!(!report.has_errors(), "{report:?}");
    assert_code(
        &bindings.validate_scene(
            &document,
            Some(ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT)),
            Default::default(),
        ),
        "scene.invalid-component-payload",
    );
    assert_code(
        &bindings.validate_scene(
            &SceneDocument::from_ordered_roots([root]),
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
            Default::default(),
        ),
        "scene.local-reference-invalid",
    );
    assert_eq!(document, before);
}

#[cfg(feature = "serde")]
mod files {
    use nara_asset::AssetRef;
    use nara_scene::{
        PrefabComponentIntent, PrefabEntityTarget, PrefabInstance, PrefabOverlay,
        SceneDocumentCandidate, SceneFileLimits, SceneFormatError, SourceDocumentKey,
        SourceDocumentKind,
    };

    use super::*;

    #[test]
    fn composition_source_preserves_opaque_records_and_unactivated_overlay_intent() {
        let base = SourceDocumentKey::named(
            "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
            "selection.base",
            SourceDocumentKind::Prefab,
        )
        .unwrap();
        let mut overlay = PrefabOverlay::new(base);
        overlay.components.push(PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id("unresolved-target")),
            component: ComponentTypeId::new("missing.Component"),
        });
        let mut root = SceneEntityRecord::new(id("root")).with_component(
            ComponentTypeId::new("missing.Component"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::new(99).unwrap(),
                ComponentValue::Null,
            )
            .with_owner(owner_id("missing.owner")),
        );
        root.prefab = Some(PrefabInstance {
            source: AssetRef::path("prefabs/selection.ron").unwrap(),
            overrides: Some(overlay),
        });
        let document = SceneDocument::from_ordered_roots([root]);
        for candidate in [
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
            SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
        ] {
            assert_eq!(candidate.into_composition_source().unwrap(), document);
        }
    }

    #[test]
    fn composition_source_rejects_invalid_local_structure_after_bounded_decode() {
        let document = SceneDocument::new([id("missing")], [SceneEntityRecord::new(id("present"))]);
        for candidate in [
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
            SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
        ] {
            let error = candidate.into_composition_source().unwrap_err();
            assert!(error.diagnostics().has_errors());
        }
        let document =
            SceneDocument::from_ordered_roots([score_entity("one"), score_entity("two")]);
        let encoded = document.to_json_string().unwrap();
        let limits = SceneFileLimits::default().with_entities(ItemLimit::new(1).unwrap());
        assert!(matches!(
            SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits),
            Err(SceneFormatError::Budget(_))
        ));
    }
}
