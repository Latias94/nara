use std::collections::BTreeMap;

use super::*;
use nara_asset::{
    AssetId, AssetPath, AssetRecord, AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::ItemLimit;
use nara_diagnostic::{Diagnostic, DiagnosticFieldClass, DiagnosticValueRef};
use nara_ecs::{Component, Entity, Mut, World};
use nara_identity::{
    EntityLookup, EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, SpawnedSceneInstance, TombstoneCause, WorldEntityLocator,
    WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
};
use nara_reflect::bevy_reflect;
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentRegistry, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PreparedComponent, Reflect,
};
#[derive(Clone, Debug, PartialEq, Component, Reflect)]
struct TestPosition {
    x: i32,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestAssetLink {
    handle: Handle<TestAsset>,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestBrokenExport;

#[derive(Clone, Debug, PartialEq, Component)]
struct TestApplyFails;

#[derive(Clone, Debug, PartialEq, Component)]
struct TestEntityLink {
    target: EntityReference,
}

#[derive(Debug)]
struct TestAsset;

fn spawned_instance(report: &SceneSpawnReport) -> &SpawnedSceneInstance {
    report
        .instance
        .as_ref()
        .expect("successful scene spawn must publish an identity instance")
}

fn spawned_entity(
    world: &World,
    report: &SceneSpawnReport,
    id: &SceneEntityId,
) -> nara_ecs::Entity {
    match spawned_instance(report).resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("spawned scene entity did not resolve: {lookup:?}"),
    }
}

fn persistent_reference(value: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("save").unwrap(),
        PersistentRuntimeId::parse_str(value).unwrap(),
    )
}

fn register_persistent_axis(
    world: &mut World,
    entity: Entity,
    persistent: PersistentRuntimeReference,
) -> WorldEntityLocator {
    world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
        let token = domain.adopt_entity(world, entity).unwrap();
        domain
            .register_persistent(world, token, persistent)
            .unwrap()
    })
}

fn diagnostic_has_text_field(diagnostic: &Diagnostic, key: &str, expected: &str) -> bool {
    diagnostic.fields().iter().any(|field| {
        field.key().as_str() == key
            && matches!(
                field.value(),
                DiagnosticValueRef::Identifier(value)
                    | DiagnosticValueRef::Display(value)
                    | DiagnosticValueRef::ProjectRelative(value)
                    if value == expected
            )
    })
}

fn diagnostic_has_u64_field(diagnostic: &Diagnostic, key: &str, expected: u64) -> bool {
    diagnostic.fields().iter().any(|field| {
        field.key().as_str() == key
            && matches!(field.value(), DiagnosticValueRef::Unsigned(value) if value == expected)
    })
}

fn diagnostic_has_redacted_field(diagnostic: &Diagnostic, key: &str) -> bool {
    diagnostic.fields().iter().any(|field| {
        field.key().as_str() == key && matches!(field.value(), DiagnosticValueRef::Redacted)
    })
}

fn diagnostic_has_field_class(
    diagnostic: &Diagnostic,
    key: &str,
    expected: DiagnosticFieldClass,
) -> bool {
    diagnostic
        .fields()
        .iter()
        .any(|field| field.key().as_str() == key && field.class() == expected)
}

#[test]
fn scene_diagnostics_redact_legacy_codec_text_and_sensitive_locators() {
    let canary = "password=scene-private-value C:\\private\\scene.ron";
    let diagnostic = crate::diagnostics::with_codec_error(
        crate::diagnostics::with_public_locator(
            crate::diagnostics::error("scene.test-private-error", "Scene test operation failed"),
            "component-id",
            "password",
        ),
        &ComponentCodecError::Message(canary.to_string()),
    );

    assert!(diagnostic_has_redacted_field(&diagnostic, "component-id"));
    assert!(diagnostic_has_redacted_field(&diagnostic, "codec-detail"));
    assert!(diagnostic_has_field_class(
        &diagnostic,
        "component-id",
        DiagnosticFieldClass::Sensitive,
    ));
    assert!(diagnostic_has_field_class(
        &diagnostic,
        "codec-detail",
        DiagnosticFieldClass::Secret,
    ));
    assert!(!format!("{diagnostic:?}").contains(canary));
}

#[test]
fn component_field_paths_lower_from_structure_with_explicit_bounds() {
    let root = ComponentFieldPath::empty();
    let root_diagnostic = crate::diagnostics::with_component_field_path(
        crate::diagnostics::error(
            "scene.test-root-field-path",
            "Scene test root field path was observed",
        ),
        "field-path",
        "field-path-depth",
        &root,
    );
    assert!(diagnostic_has_text_field(
        &root_diagnostic,
        "field-path",
        "root",
    ));
    assert!(diagnostic_has_field_class(
        &root_diagnostic,
        "field-path",
        DiagnosticFieldClass::Public,
    ));
    assert!(diagnostic_has_u64_field(
        &root_diagnostic,
        "field-path-depth",
        0,
    ));

    let indexed = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("items"),
        ComponentFieldPathSegment::index(7),
    ]);
    let indexed_diagnostic = crate::diagnostics::with_component_field_path(
        crate::diagnostics::error(
            "scene.test-indexed-field-path",
            "Scene test indexed field path was observed",
        ),
        "field-path",
        "field-path-depth",
        &indexed,
    );
    assert!(diagnostic_has_text_field(
        &indexed_diagnostic,
        "field-path",
        "f_items/i_7",
    ));
    assert!(diagnostic_has_field_class(
        &indexed_diagnostic,
        "field-path",
        DiagnosticFieldClass::Public,
    ));
    assert!(diagnostic_has_u64_field(
        &indexed_diagnostic,
        "field-path-depth",
        2,
    ));

    let oversized_segment = "x".repeat(512);
    let oversized = ComponentFieldPath::from_fields([oversized_segment.clone()]);
    let oversized_diagnostic = crate::diagnostics::with_component_field_path(
        crate::diagnostics::error(
            "scene.test-oversized-field-path",
            "Scene test field path exceeded its public bound",
        ),
        "field-path",
        "field-path-depth",
        &oversized,
    );
    assert!(diagnostic_has_field_class(
        &oversized_diagnostic,
        "field-path",
        DiagnosticFieldClass::Sensitive,
    ));
    assert!(diagnostic_has_u64_field(
        &oversized_diagnostic,
        "field-path-depth",
        1,
    ));
    assert!(!format!("{oversized_diagnostic:?}").contains(&oversized_segment));

    let sensitive = ComponentFieldPath::from_fields(["password"]);
    let sensitive_diagnostic = crate::diagnostics::with_component_field_path(
        crate::diagnostics::error(
            "scene.test-sensitive-field-path",
            "Scene test field path required redaction",
        ),
        "field-path",
        "field-path-depth",
        &sensitive,
    );
    assert!(diagnostic_has_field_class(
        &sensitive_diagnostic,
        "field-path",
        DiagnosticFieldClass::Sensitive,
    ));
    assert!(!format!("{sensitive_diagnostic:?}").contains("password"));
}

#[test]
fn component_field_path_errors_publish_stable_reason_and_numeric_bounds() {
    let path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("items"),
        ComponentFieldPathSegment::index(7),
    ]);
    let diagnostic = crate::diagnostics::with_component_field_path_error(
        crate::diagnostics::error(
            "scene.test-field-path-error",
            "Scene test field path operation failed",
        ),
        &ComponentFieldPathError::IndexOutOfBounds {
            path,
            index: 7,
            len: 2,
        },
    );

    assert!(diagnostic_has_text_field(
        &diagnostic,
        "path-error-kind",
        "index-out-of-bounds",
    ));
    assert!(diagnostic_has_text_field(
        &diagnostic,
        "error-field-path",
        "f_items/i_7",
    ));
    assert!(diagnostic_has_u64_field(
        &diagnostic,
        "error-field-path-depth",
        2,
    ));
    assert!(diagnostic_has_u64_field(&diagnostic, "path-error-index", 7,));
    assert!(diagnostic_has_u64_field(
        &diagnostic,
        "path-error-length",
        2,
    ));
}

#[test]
fn scene_diagnostic_asset_refs_keep_semantic_classification() {
    let path_diagnostic = crate::diagnostics::with_asset_ref(
        crate::diagnostics::error(
            "scene.test-asset-path",
            "Scene test asset path was observed",
        ),
        "asset-ref",
        &AssetRef::path("textures/player.png").unwrap(),
    );
    assert!(diagnostic_has_field_class(
        &path_diagnostic,
        "asset-ref",
        DiagnosticFieldClass::ProjectRelative,
    ));

    let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let stable_diagnostic = crate::diagnostics::with_asset_ref(
        crate::diagnostics::error(
            "scene.test-asset-stable-id",
            "Scene test stable asset ID was observed",
        ),
        "asset-ref",
        &AssetRef::StableId(stable_id),
    );
    assert!(diagnostic_has_field_class(
        &stable_diagnostic,
        "asset-ref",
        DiagnosticFieldClass::Public,
    ));
    assert!(diagnostic_has_text_field(
        &stable_diagnostic,
        "asset-ref",
        &stable_id.to_string(),
    ));
}

#[test]
fn syncs_parent_child_links() {
    let mut world = World::new();
    let parent = world.spawn((Name::new("parent"),)).id();
    let child = spawn_child(&mut world, parent, (Name::new("child"),));

    sync_children(&mut world);

    let parent_ref = world.get_entity(parent).unwrap();
    let children = parent_ref.get::<Children>().unwrap();
    assert_eq!(children.as_slice(), &[child]);
}

#[test]
fn validates_scene_entity_id_shape() {
    assert_eq!(SceneEntityId::new(""), Err(SceneEntityIdError::Empty));
    assert_eq!(
        SceneEntityId::new("/player"),
        Err(SceneEntityIdError::LeadingSlash)
    );
    assert_eq!(
        SceneEntityId::new("root//player"),
        Err(SceneEntityIdError::EmptySegment)
    );
    assert_eq!(
        SceneEntityId::new("root/../player"),
        Err(SceneEntityIdError::ParentDirectorySegment)
    );
    assert!(SceneEntityId::new("root/player-1").is_ok());
}

#[test]
fn validation_reports_duplicate_missing_parent_cycle_and_unknown_component() {
    let registry = test_registry();
    let id = scene_id("player");
    let missing_parent = scene_id("missing");
    let cycle_a = scene_id("cycle_a");
    let cycle_b = scene_id("cycle_b");
    let unknown_component = ComponentTypeId::new("nara.test.Unknown");
    let document = SceneDocument {
        format_version: SceneDocument::CURRENT_FORMAT_VERSION,
        entities: vec![
            SceneEntityRecord::new(id.clone()),
            SceneEntityRecord::new(id),
            SceneEntityRecord::new(scene_id("orphan")).with_parent(missing_parent),
            SceneEntityRecord::new(cycle_a.clone()).with_parent(cycle_b.clone()),
            SceneEntityRecord::new(cycle_b).with_parent(cycle_a),
            SceneEntityRecord::new(scene_id("unknown")).with_component(
                unknown_component,
                SceneComponentRecord::new(ComponentSchemaVersion(1), ComponentValue::Null),
            ),
        ],
    };

    let report = document.validate(&registry);
    let codes = report
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect::<Vec<_>>();

    assert!(codes.contains(&"scene.duplicate-entity-id"));
    assert!(codes.contains(&"scene.missing-parent"));
    assert!(codes.contains(&"scene.parent-cycle"));
    assert!(codes.contains(&"scene.unknown-component"));
    assert!(report.has_errors());
}

#[test]
fn validates_document_format_version() {
    let registry = test_registry();
    let document = SceneDocument {
        format_version: SceneDocument::CURRENT_FORMAT_VERSION + 1,
        entities: vec![SceneEntityRecord::new(scene_id("player"))],
    };

    let report = document.validate(&registry);

    assert!(report.has_errors());
    assert!(
        report
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.unsupported-format-version")
    );
}

#[test]
fn prefab_default_uses_current_format_version() {
    assert_eq!(
        PrefabDocument::default().format_version,
        PrefabDocument::CURRENT_FORMAT_VERSION
    );
}

#[test]
fn unsupported_prefab_instance_prevents_world_mutation() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord {
        id: scene_id("enemy"),
        parent: None,
        components: BTreeMap::new(),
        prefab: Some(PrefabInstance {
            source: AssetRef::path("prefabs/enemy.ron").unwrap(),
            overrides: ScenePatchDocument::default(),
        }),
    }]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-instance-unsupported"
            && diagnostic_has_field_class(diagnostic, "entity-id", DiagnosticFieldClass::Public)
            && diagnostic_has_text_field(diagnostic, "field-path", "prefab.source")
            && diagnostic_has_field_class(
                diagnostic,
                "asset-ref",
                DiagnosticFieldClass::ProjectRelative,
            )
            && diagnostic_has_text_field(diagnostic, "asset-ref", "prefabs/enemy.ron")
    }));
}

#[test]
fn scene_prefab_resolver_expands_one_level_instance_before_spawn() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("visual"))
            .with_component(position_type_id(), position_record(3))]),
    );
    let document = SceneDocument::new([prefab_anchor("enemy", source)]);
    let mut world = World::new();

    let expansion = document.expand_prefabs(&registry, &resolver);
    let report = spawn_scene_with_prefab_resolver(&mut world, &registry, &document, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    assert_eq!(
        expanded
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec!["enemy", "enemy/visual"]
    );
    assert_eq!(
        expanded.entities[1]
            .parent
            .as_ref()
            .map(SceneEntityId::as_str),
        Some("enemy")
    );
    assert!(!report.diagnostics.has_errors());
    assert_eq!(spawned_instance(&report).len(), 2);
    let visual = spawned_entity(&world, &report, &scene_id("enemy/visual"));
    assert_eq!(world.get::<TestPosition>(visual).unwrap().x, 3);
}

#[test]
fn prefab_instance_override_patch_applies_before_namespacing() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("visual"))
            .with_component(position_type_id(), position_record(3))]),
    );
    let overrides = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("visual"),
        component: position_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["x"]),
        value: ComponentValue::I64(11),
    }]);
    let document = SceneDocument::new([prefab_anchor_with_overrides("enemy", source, overrides)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    let visual = expanded
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "enemy/visual")
        .unwrap();
    assert_eq!(
        visual
            .components
            .get(&position_type_id())
            .unwrap()
            .value
            .field_i64("x")
            .unwrap(),
        11
    );
}

#[test]
fn scene_prefab_resolver_expands_nested_instances_with_deterministic_ids() {
    let registry = test_registry();
    let enemy_source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let weapon_source = AssetRef::path("prefabs/weapon.ron").unwrap();
    let muzzle_source = AssetRef::path("prefabs/muzzle.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            enemy_source.clone(),
            PrefabDocument::new([prefab_anchor("weapon", weapon_source.clone())]),
        )
        .with_prefab(
            weapon_source,
            PrefabDocument::new([prefab_anchor("muzzle", muzzle_source.clone())]),
        )
        .with_prefab(
            muzzle_source,
            PrefabDocument::new([SceneEntityRecord::new(scene_id("flash"))
                .with_component(position_type_id(), position_record(8))]),
        );
    let document = SceneDocument::new([prefab_anchor("enemy", enemy_source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    assert_eq!(
        expanded
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "enemy",
            "enemy/weapon",
            "enemy/weapon/muzzle",
            "enemy/weapon/muzzle/flash"
        ]
    );
}

#[test]
fn repeated_prefab_instances_expand_without_id_collisions() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("visual"))]),
    );
    let document = SceneDocument::new([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    assert_eq!(
        expanded
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec!["left", "left/visual", "right", "right/visual"]
    );
}

#[test]
fn missing_prefab_source_reports_asset_ref_without_expanded_document() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/missing.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new();
    let document = SceneDocument::new([prefab_anchor("enemy", source)]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let expansion = document.expand_prefabs(&registry, &resolver);
    let report = spawn_scene_with_prefab_resolver(&mut world, &registry, &document, &resolver);

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-source-missing"
            && diagnostic_has_text_field(diagnostic, "entity-id", "enemy")
            && diagnostic_has_text_field(diagnostic, "asset-ref", "prefabs/missing.ron")
    }));
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.instance.is_none());
}

#[test]
fn prefab_source_cycle_reports_typed_edge_without_concatenated_chain() {
    let registry = test_registry();
    let a = AssetRef::path("prefabs/a.ron").unwrap();
    let b = AssetRef::path("prefabs/b.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(a.clone(), PrefabDocument::new([prefab_anchor("b", b)]))
        .with_prefab(
            AssetRef::path("prefabs/b.ron").unwrap(),
            PrefabDocument::new([prefab_anchor("a", a.clone())]),
        );
    let document = SceneDocument::new([prefab_anchor("root", a)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-cycle"
            && diagnostic_has_text_field(diagnostic, "cycle-from", "prefabs/b.ron")
            && diagnostic_has_text_field(diagnostic, "cycle-to", "prefabs/a.ron")
            && diagnostic_has_field_class(
                diagnostic,
                "cycle-from",
                DiagnosticFieldClass::ProjectRelative,
            )
            && diagnostic_has_field_class(
                diagnostic,
                "cycle-to",
                DiagnosticFieldClass::ProjectRelative,
            )
            && diagnostic_has_u64_field(diagnostic, "cycle-start-index", 0)
            && diagnostic_has_u64_field(diagnostic, "cycle-depth", 3)
            && diagnostic.summary().as_str() == "Prefab source cycle was detected"
            && !diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == "asset-ref")
    }));
}

#[test]
fn prefab_source_cycle_reports_middle_cycle_start_for_multi_edge_chain() {
    let registry = test_registry();
    let a = AssetRef::path("prefabs/a.ron").unwrap();
    let b = AssetRef::path("prefabs/b.ron").unwrap();
    let c = AssetRef::path("prefabs/c.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            a.clone(),
            PrefabDocument::new([prefab_anchor("b", b.clone())]),
        )
        .with_prefab(
            b.clone(),
            PrefabDocument::new([prefab_anchor("c", c.clone())]),
        )
        .with_prefab(c, PrefabDocument::new([prefab_anchor("b", b)]));
    let document = SceneDocument::new([prefab_anchor("root", a)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-cycle"
            && diagnostic_has_text_field(diagnostic, "cycle-from", "prefabs/c.ron")
            && diagnostic_has_text_field(diagnostic, "cycle-to", "prefabs/b.ron")
            && diagnostic_has_u64_field(diagnostic, "cycle-start-index", 1)
            && diagnostic_has_u64_field(diagnostic, "cycle-depth", 3)
    }));
}

#[test]
fn prefab_expansion_depth_limit_reports_before_spawn_mutation() {
    let registry = test_registry();
    let enemy_source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let weapon_source = AssetRef::path("prefabs/weapon.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        enemy_source.clone(),
        PrefabDocument::new([prefab_anchor("weapon", weapon_source)]),
    );
    let document = SceneDocument::new([prefab_anchor("enemy", enemy_source)]);

    let expansion = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions { max_depth: 1 },
    );

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-depth-exceeded"
            && diagnostic_has_text_field(diagnostic, "entity-id", "weapon")
            && diagnostic_has_u64_field(diagnostic, "maximum-depth", 1)
    }));
}

#[test]
fn invalid_component_payload_does_not_mutate_world() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("bad")).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::String("not a number".to_string()))]),
        ),
    )]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.instance.is_none());
}

#[test]
fn component_migration_runs_before_scene_preflight_without_mutating_document() {
    let registry = migrated_position_registry();
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::I64(8))]),
        ),
    )]);
    let mut world = World::new();

    let validation = document.validate(&registry);
    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!validation.has_errors());
    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&world, &report, &id);
    assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 8);
    let source_component = document.entities[0]
        .components
        .get(&position_type_id())
        .unwrap();
    assert_eq!(source_component.version, ComponentSchemaVersion(1));
    assert!(source_component.value.get("x").is_some());
    assert!(source_component.value.get("x2").is_none());
}

#[test]
fn missing_component_migration_reports_unsupported_version() {
    let mut registry = ComponentRegistry::new();
    registry
        .register_scene_component_with_fields::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(2),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["x2"]),
                ComponentValueKind::I64,
            )],
            |value| {
                let x = value.field_i64("x2")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("bad")).with_component(
        position_type_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([("x", ComponentValue::I64(8))]),
        ),
    )]);

    let report = document.validate(&registry);

    assert!(report.has_errors());
    assert!(report.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.unsupported-component-version"
            && diagnostic_has_text_field(diagnostic, "component-id", position_type_id().as_str())
    }));
}

#[test]
fn path_asset_ref_resolves_before_scene_spawn_without_database() {
    let registry = test_asset_registry();
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::path("textures/player.png").unwrap()),
    )]);
    let mut world = World::new();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&world, &report, &id);
    let link = world.get::<TestAssetLink>(entity).unwrap();
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(link.handle.id()),
        Some("textures/player.png")
    );
}

#[test]
#[allow(clippy::default_constructed_unit_structs)]
fn scene_instance_allocation_is_world_global_across_every_spawner_entrypoint() {
    let registry = test_registry();
    let id = scene_id("entity");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone())]);
    let mut world = World::new();

    let first = SceneSpawner::new().spawn(&mut world, &registry, &document);
    let second = SceneSpawner::default().spawn(&mut world, &registry, &document);
    let third = spawn_scene(&mut world, &registry, &document);

    assert!(
        [&first, &second, &third]
            .into_iter()
            .all(|report| !report.diagnostics.has_errors())
    );
    assert_eq!(spawned_instance(&first).instance_id().get(), 1);
    assert_eq!(spawned_instance(&second).instance_id().get(), 2);
    assert_eq!(spawned_instance(&third).instance_id().get(), 3);
    let entities = [
        spawned_entity(&world, &first, &id),
        spawned_entity(&world, &second, &id),
        spawned_entity(&world, &third, &id),
    ];
    assert!(entities[0] != entities[1] && entities[1] != entities[2]);
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        3
    );
}

#[test]
fn empty_scene_spawn_publishes_a_non_reusable_instance_claim() {
    let registry = ComponentRegistry::new();
    let document = SceneDocument::new([]);
    let mut world = World::new();

    let first = spawn_scene(&mut world, &registry, &document);
    let second = spawn_scene(&mut world, &registry, &document);

    assert!(!first.diagnostics.has_errors());
    assert!(!second.diagnostics.has_errors());
    assert!(spawned_instance(&first).is_empty());
    assert!(spawned_instance(&second).is_empty());
    assert_eq!(spawned_instance(&first).instance_id().get(), 1);
    assert_eq!(spawned_instance(&second).instance_id().get(), 2);
    let stats = world.resource::<WorldIdentityDomain>().stats();
    assert_eq!(stats.lifetime_claims, 2);
    assert_eq!(stats.claimed_scene_instances, 2);
    assert_eq!(stats.active_scene_instances, 2);
    assert_eq!(stats.active_scene_entities, 0);
}

#[test]
fn identity_claim_failure_preserves_world_assets_ticks_and_allocator() {
    let registry = test_asset_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::path("textures/generated.png").unwrap()),
    )]);
    let mut world = World::new();
    let settings =
        WorldIdentityDomainSettings::new(ItemLimit::new(1).unwrap(), ItemLimit::new(1).unwrap())
            .unwrap();
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    world.insert_resource(domain);
    let mut asset_server = AssetServer::new();
    let existing = asset_server
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    world.insert_resource(asset_server);
    world.increment_change_tick();

    let before_entities = world.iter_entities().count();
    let before_stats = world.resource::<WorldIdentityDomain>().stats();
    let before_ticks = world
        .get_resource_change_ticks::<AssetServer>()
        .expect("asset server must be installed");

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        before_stats
    );
    let after_ticks = world
        .get_resource_change_ticks::<AssetServer>()
        .expect("asset server must remain installed");
    assert_eq!(after_ticks.added, before_ticks.added);
    assert_eq!(after_ticks.changed, before_ticks.changed);
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(existing.id()),
        Some("textures/existing.png")
    );
    assert_eq!(asset_server.path(AssetId::from_raw(2)), None);

    let empty = spawn_scene(&mut world, &registry, &SceneDocument::new([]));
    assert!(!empty.diagnostics.has_errors());
    assert_eq!(spawned_instance(&empty).instance_id().get(), 1);
}

#[test]
fn authoring_replacement_and_clear_publish_typed_tombstones() {
    let registry = test_registry();
    let id = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]));
    let mut world = World::new();

    let first_sync = session.sync_world(&mut world, &registry);
    assert!(first_sync.synced);
    let first = first_sync.live_instance.unwrap();
    let first_entity = match first.resolve(&world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("first authoring instance did not resolve: {lookup:?}"),
    };

    session
        .replace_document(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(2))]));
    let second_sync = session.sync_world(&mut world, &registry);
    assert!(second_sync.synced);
    assert_eq!(second_sync.removed_entities, 1);
    let second = second_sync.live_instance.unwrap();
    assert_ne!(first.instance_id(), second.instance_id());
    assert!(world.get_entity(first_entity).is_err());
    let EntityLookup::Tombstoned(Some(replaced)) = first.resolve(&world, &id) else {
        panic!("replaced instance must resolve to a retained tombstone");
    };
    assert_eq!(replaced.cause(), TombstoneCause::Replaced);
    let second_entity = match second.resolve(&world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("replacement authoring instance did not resolve: {lookup:?}"),
    };
    assert_eq!(world.get::<TestPosition>(second_entity).unwrap().x, 2);

    let revision = session.revision();
    let clear = session.clear_live_world(&mut world);
    assert!(clear.cleared);
    assert_eq!(clear.removed_entities, 1);
    assert!(clear.live_instance.is_none());
    assert!(session.live_instance().is_none());
    assert_eq!(session.revision(), revision);
    assert!(session.is_live_dirty());
    assert!(world.get_entity(second_entity).is_err());
    let EntityLookup::Tombstoned(Some(unloaded)) = second.resolve(&world, &id) else {
        panic!("cleared instance must resolve to a retained tombstone");
    };
    assert_eq!(unloaded.cause(), TombstoneCause::Unloaded);
}

#[test]
fn authoring_replacement_failure_keeps_the_previous_projection() {
    let registry = test_registry();
    let id = scene_id("player");
    let mut world = World::new();
    let settings =
        WorldIdentityDomainSettings::new(ItemLimit::new(3).unwrap(), ItemLimit::new(3).unwrap())
            .unwrap();
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    world.insert_resource(domain);
    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]));

    let first_sync = session.sync_world(&mut world, &registry);
    assert!(first_sync.synced);
    let first = first_sync.live_instance.unwrap();
    let first_entity = match first.resolve(&world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("first authoring instance did not resolve: {lookup:?}"),
    };
    let before_entities = world.iter_entities().count();
    let before_stats = world.resource::<WorldIdentityDomain>().stats();

    session
        .replace_document(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(2))]));
    let failed = session.sync_world(&mut world, &registry);

    assert!(!failed.synced);
    assert!(failed.diagnostics.has_errors());
    assert_eq!(failed.removed_entities, 0);
    assert_eq!(failed.live_instance.as_ref(), Some(&first));
    assert_eq!(session.live_instance(), Some(&first));
    assert!(session.is_live_dirty());
    assert_eq!(world.iter_entities().count(), before_entities);
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        before_stats
    );
    assert_eq!(
        first.resolve(&world, &id),
        EntityLookup::Resolved(first_entity)
    );
    assert_eq!(world.get::<TestPosition>(first_entity).unwrap().x, 1);
}

#[test]
fn stable_asset_ref_resolves_with_database_before_scene_spawn() {
    let registry = test_asset_registry();
    let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let database = test_database(stable_id, "textures/player.png");
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::StableId(stable_id)),
    )]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let validation = document.validate_with_asset_database(&registry, &database);
    let report = spawner.spawn_with_asset_database(&mut world, &registry, &document, &database);

    assert!(!validation.has_errors());
    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&world, &report, &id);
    let link = world.get::<TestAssetLink>(entity).unwrap();
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(link.handle.id()),
        Some("textures/player.png")
    );
    assert_eq!(asset_server.stable_id(link.handle.id()), Some(stable_id));
}

#[test]
fn unknown_stable_asset_ref_is_redacted_in_diagnostics_without_world_mutation() {
    let registry = test_asset_registry();
    let known_stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let unknown_stable_id = stable_id("b73f0f16-09e8-4265-b090-b689b41c197e");
    let database = test_database(known_stable_id, "textures/player.png");
    let id = scene_id("player");
    let document = SceneDocument::new([SceneEntityRecord::new(id).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::StableId(unknown_stable_id)),
    )]);
    let expected_component_id = asset_link_type_id();
    let mut existing_asset_server = AssetServer::new();
    let existing_handle = existing_asset_server
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    let mut world = World::new();
    world.insert_resource(existing_asset_server);
    let before_entities = world.iter_entities().count();

    let report = spawn_scene_with_asset_database(&mut world, &registry, &document, &database);
    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert!(world.get_resource::<WorldIdentityDomain>().is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(existing_handle.id()),
        Some("textures/existing.png")
    );
    assert_eq!(asset_server.path(AssetId::from_raw(2)), None);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.invalid-component-payload"
            && diagnostic_has_text_field(diagnostic, "entity-id", "player")
            && diagnostic_has_text_field(diagnostic, "component-id", expected_component_id.as_str())
            && diagnostic_has_text_field(diagnostic, "field-path", "asset.value")
            && diagnostic_has_redacted_field(diagnostic, "asset-ref")
            && diagnostic_has_field_class(diagnostic, "asset-ref", DiagnosticFieldClass::Sensitive)
    }));
    assert!(!format!("{:?}", report.diagnostics).contains(&unknown_stable_id.to_string()));
}

#[test]
fn component_apply_failure_in_multi_component_entity_reports_component_and_rolls_back() {
    let registry = failing_apply_registry();
    let document = SceneDocument::new([
        SceneEntityRecord::new(scene_id("ok"))
            .with_component(position_type_id(), position_record(1)),
        SceneEntityRecord::new(scene_id("fails"))
            .with_component(position_type_id(), position_record(2))
            .with_component(apply_fails_type_id(), apply_fails_record(None)),
    ]);
    let mut world = World::new();
    let before_entities = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.component-apply-failed"
            && diagnostic_has_text_field(diagnostic, "entity-id", "fails")
            && diagnostic_has_text_field(diagnostic, "component-id", apply_fails_type_id().as_str())
            && diagnostic_has_redacted_field(diagnostic, "codec-detail")
    }));
}

#[test]
fn component_apply_failure_removes_scratch_asset_server() {
    let registry = failing_apply_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("fails")).with_component(
        apply_fails_type_id(),
        apply_fails_record(Some(AssetRef::path("textures/generated.png").unwrap())),
    )]);
    let mut world = World::new();
    let before_entities = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert!(world.get_resource::<AssetServer>().is_none());
    assert!(world.get_resource::<WorldIdentityDomain>().is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
}

#[test]
fn component_apply_failure_restores_original_asset_server() {
    let registry = failing_apply_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("fails")).with_component(
        apply_fails_type_id(),
        apply_fails_record(Some(AssetRef::path("textures/generated.png").unwrap())),
    )]);
    let mut existing_asset_server = AssetServer::new();
    let existing_handle = existing_asset_server
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    let mut world = World::new();
    world.insert_resource(existing_asset_server);
    let before_entities = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert!(world.get_resource::<WorldIdentityDomain>().is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(existing_handle.id()),
        Some("textures/existing.png")
    );
    assert_eq!(asset_server.path(AssetId::from_raw(2)), None);
}

#[test]
fn later_component_apply_failure_does_not_publish_prior_asset_resolution() {
    let registry = failing_apply_registry();
    let document = SceneDocument::new([
        SceneEntityRecord::new(scene_id("asset")).with_component(
            apply_resolves_asset_type_id(),
            asset_link_record(AssetRef::path("textures/generated.png").unwrap()),
        ),
        SceneEntityRecord::new(scene_id("fails"))
            .with_component(apply_fails_type_id(), apply_fails_record(None)),
    ]);
    let mut asset_server = AssetServer::new();
    let existing = asset_server
        .reserve::<TestAsset>("textures/existing.png")
        .unwrap();
    let mut world = World::new();
    world.insert_resource(asset_server);
    world.increment_change_tick();
    let before_entities = world.iter_entities().count();
    let before_ticks = world
        .get_resource_change_ticks::<AssetServer>()
        .expect("asset server must be installed");

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before_entities);
    assert!(world.get_resource::<WorldIdentityDomain>().is_none());
    let after_ticks = world
        .get_resource_change_ticks::<AssetServer>()
        .expect("asset server must remain installed");
    assert_eq!(after_ticks.added, before_ticks.added);
    assert_eq!(after_ticks.changed, before_ticks.changed);
    let asset_server = world.resource::<AssetServer>();
    assert_eq!(
        asset_server.path(existing.id()),
        Some("textures/existing.png")
    );
    assert_eq!(asset_server.path(AssetId::from_raw(2)), None);
}

#[test]
fn spawns_hierarchy_records_source_and_exports_stable_document() {
    let registry = test_registry();
    let parent_id = scene_id("parent");
    let child_id = scene_id("parent/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(parent_id.clone())
            .with_component(position_type_id(), position_record(1)),
        SceneEntityRecord::new(child_id.clone())
            .with_parent(parent_id.clone())
            .with_component(position_type_id(), position_record(2)),
    ]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let report = spawner.spawn(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    assert_eq!(spawned_instance(&report).len(), 2);
    let parent = spawned_entity(&world, &report, &parent_id);
    let child = spawned_entity(&world, &report, &child_id);
    assert_eq!(
        world
            .get::<Children>(parent)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![child]
    );
    assert_eq!(
        world.get::<SceneEntitySource>(child).unwrap().entity_id,
        child_id
    );

    let export = export_scene(&world, &registry);

    assert!(!export.diagnostics.has_errors());
    let output = export.output().unwrap();
    assert_eq!(output.document.entities.len(), 2);
    assert_eq!(
        export
            .output()
            .unwrap()
            .document
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec!["parent", "parent/child"]
    );
    assert_eq!(
        output.document.entities[1]
            .parent
            .as_ref()
            .unwrap()
            .as_str(),
        "parent"
    );
}

#[test]
fn repeated_scene_spawns_export_with_injective_generated_ids() {
    let registry = test_registry();
    let id = scene_id("enemy");
    let document =
        SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(7))]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    assert!(
        !spawner
            .spawn(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );
    assert!(
        !spawner
            .spawn(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );

    let export = export_scene(&world, &registry);
    let output = export.output().unwrap();
    let ids = output
        .document
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["entity_1", "entity_2"]);
    assert_eq!(output.remap.len(), 2);
    assert!(
        output
            .remap
            .iter()
            .all(|(_, id)| !id.as_str().starts_with("instance_"))
    );
}

#[test]
fn generated_export_ids_skip_every_authored_claim() {
    let registry = test_registry();
    let authored = SceneDocument::new([SceneEntityRecord::new(scene_id("entity_1"))]);
    let duplicate = SceneDocument::new([SceneEntityRecord::new(scene_id("enemy"))]);
    let mut world = World::new();

    assert!(
        !spawn_scene(&mut world, &registry, &authored)
            .diagnostics
            .has_errors()
    );
    assert!(
        !spawn_scene(&mut world, &registry, &duplicate)
            .diagnostics
            .has_errors()
    );
    assert!(
        !spawn_scene(&mut world, &registry, &duplicate)
            .diagnostics
            .has_errors()
    );

    let export = export_scene(&world, &registry);
    let output = export.output().unwrap();
    let ids = output
        .document
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["entity_1", "entity_2", "entity_3"]);
    let assigned = output.remap.iter().map(|(_, id)| id).collect::<Vec<_>>();
    assert_eq!(assigned.len(), output.document.entities.len());
    assert!(assigned.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn export_rewrites_scene_local_references_with_owner_instance_context() {
    let registry = entity_link_registry();
    let source = scene_id("source");
    let target = scene_id("target");
    let document = SceneDocument::new([
        SceneEntityRecord::new(source.clone()).with_component(
            entity_link_type_id(),
            entity_link_record(EntityReference::SceneLocal {
                entity: target.clone(),
            }),
        ),
        SceneEntityRecord::new(target),
    ]);
    let mut world = World::new();

    assert!(
        !spawn_scene(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );
    assert!(
        !spawn_scene(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );

    let export = export_scene(&world, &registry);
    assert!(!export.diagnostics.has_errors());
    let output = export.output().unwrap();
    let ids = output
        .document
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["entity_1", "entity_2", "entity_3", "entity_4"]);
    assert_eq!(
        exported_entity_link(&output.document.entities[0]),
        &EntityReference::SceneLocal {
            entity: scene_id("entity_2"),
        }
    );
    assert_eq!(
        exported_entity_link(&output.document.entities[2]),
        &EntityReference::SceneLocal {
            entity: scene_id("entity_4"),
        }
    );
}

#[test]
fn dangling_scene_local_reference_prevents_export_publication() {
    let registry = entity_link_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
        entity_link_type_id(),
        entity_link_record(EntityReference::SceneLocal {
            entity: scene_id("missing"),
        }),
    )]);
    let mut world = World::new();
    let spawn = spawn_scene(&mut world, &registry, &document);
    assert!(!spawn.diagnostics.has_errors());

    let export = export_scene(&world, &registry);

    assert!(export.diagnostics.has_errors());
    assert!(export.output().is_none());
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-entity-reference-rewrite-failed"
            && diagnostic_has_text_field(
                diagnostic,
                "rewrite-error-kind",
                "scene-local-target-missing",
            )
    }));
}

#[test]
fn persistent_reference_to_exported_entity_rewrites_to_scene_local() {
    let registry = entity_link_registry();
    let source = scene_id("source");
    let target = scene_id("target");
    let persistent = persistent_reference("11111111-1111-4111-8111-111111111111");
    let document = SceneDocument::new([
        SceneEntityRecord::new(source.clone()).with_component(
            entity_link_type_id(),
            entity_link_record(EntityReference::Persistent {
                entity: persistent.clone(),
            }),
        ),
        SceneEntityRecord::new(target.clone()),
    ]);
    let mut world = World::new();
    let spawn = spawn_scene(&mut world, &registry, &document);
    assert!(!spawn.diagnostics.has_errors());
    let target_entity = spawned_entity(&world, &spawn, &target);
    let persistent_locator = register_persistent_axis(&mut world, target_entity, persistent);

    let export = export_scene(&world, &registry);

    assert!(!export.diagnostics.has_errors());
    let output = export.output().unwrap();
    let source_record = output
        .document
        .entities
        .iter()
        .find(|record| record.id == source)
        .unwrap();
    assert_eq!(
        exported_entity_link(source_record),
        &EntityReference::SceneLocal {
            entity: target.clone(),
        }
    );
    assert_eq!(output.remap.get(&persistent_locator), Some(&target));
}

#[test]
fn resolved_external_persistent_reference_is_preserved() {
    let registry = entity_link_registry();
    let source = scene_id("source");
    let persistent = persistent_reference("22222222-2222-4222-8222-222222222222");
    let document = SceneDocument::new([SceneEntityRecord::new(source.clone()).with_component(
        entity_link_type_id(),
        entity_link_record(EntityReference::Persistent {
            entity: persistent.clone(),
        }),
    )]);
    let mut world = World::new();
    let spawn = spawn_scene(&mut world, &registry, &document);
    assert!(!spawn.diagnostics.has_errors());
    let external = spawn_identity_entity(&mut world).unwrap().entity();
    register_persistent_axis(&mut world, external, persistent.clone());

    let export = export_scene(&world, &registry);

    assert!(!export.diagnostics.has_errors());
    let source_record = &export.output().unwrap().document.entities[0];
    assert_eq!(
        exported_entity_link(source_record),
        &EntityReference::Persistent { entity: persistent }
    );
}

#[test]
fn missing_persistent_reference_prevents_export_publication() {
    let registry = entity_link_registry();
    let persistent = persistent_reference("33333333-3333-4333-8333-333333333333");
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
        entity_link_type_id(),
        entity_link_record(EntityReference::Persistent { entity: persistent }),
    )]);
    let mut world = World::new();
    assert!(
        !spawn_scene(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );

    let export = export_scene(&world, &registry);

    assert!(export.diagnostics.has_errors());
    assert!(export.output().is_none());
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-entity-reference-rewrite-failed"
            && diagnostic_has_text_field(
                diagnostic,
                "rewrite-error-kind",
                "persistent-target-missing",
            )
    }));
}

#[test]
fn tombstoned_persistent_reference_prevents_export_publication() {
    let registry = entity_link_registry();
    let persistent = persistent_reference("44444444-4444-4444-8444-444444444444");
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
        entity_link_type_id(),
        entity_link_record(EntityReference::Persistent {
            entity: persistent.clone(),
        }),
    )]);
    let mut world = World::new();
    assert!(
        !spawn_scene(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );
    let external = spawn_identity_entity(&mut world).unwrap().entity();
    register_persistent_axis(&mut world, external, persistent);
    world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
        let token = domain.adopt_entity(world, external).unwrap();
        domain
            .retire_entity(world, token, TombstoneCause::Despawned)
            .unwrap();
    });

    let export = export_scene(&world, &registry);

    assert!(export.diagnostics.has_errors());
    assert!(export.output().is_none());
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-entity-reference-rewrite-failed"
            && diagnostic_has_text_field(
                diagnostic,
                "rewrite-error-kind",
                "persistent-target-tombstoned",
            )
    }));
}

#[test]
fn stale_persistent_reference_prevents_export_publication() {
    let registry = entity_link_registry();
    let persistent = persistent_reference("55555555-5555-4555-8555-555555555555");
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
        entity_link_type_id(),
        entity_link_record(EntityReference::Persistent {
            entity: persistent.clone(),
        }),
    )]);
    let mut world = World::new();
    assert!(
        !spawn_scene(&mut world, &registry, &document)
            .diagnostics
            .has_errors()
    );
    let external = spawn_identity_entity(&mut world).unwrap().entity();
    register_persistent_axis(&mut world, external, persistent);
    assert!(world.despawn(external));

    let export = export_scene(&world, &registry);

    assert!(export.diagnostics.has_errors());
    assert!(export.output().is_none());
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-entity-reference-rewrite-failed"
            && diagnostic_has_text_field(
                diagnostic,
                "rewrite-error-kind",
                "persistent-target-stale",
            )
    }));
}

#[test]
fn direct_prefab_spawn_applies_patch_field_overrides() {
    let registry = test_registry();
    let id = scene_id("enemy");
    let prefab =
        PrefabDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id.clone(),
        component: position_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["x"]),
        value: ComponentValue::I64(9),
    }]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let report = spawner.spawn_prefab_with_patch(&mut world, &registry, &prefab, &patch);

    assert!(!report.diagnostics.has_errors());
    let entity = spawned_entity(&world, &report, &id);
    assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 9);
}

#[test]
fn prefab_patch_can_add_and_remove_components() {
    let registry = test_registry();
    let added = scene_id("added");
    let removed = scene_id("removed");
    let prefab = PrefabDocument::new([
        SceneEntityRecord::new(added.clone()),
        SceneEntityRecord::new(removed.clone())
            .with_component(position_type_id(), position_record(4)),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: added.clone(),
            component: position_type_id(),
            value: position_record(7),
        },
        ScenePatchOperation::RemoveComponent {
            entity: removed.clone(),
            component: position_type_id(),
        },
    ]);
    let mut world = World::new();

    let report = spawn_prefab_with_patch(&mut world, &registry, &prefab, &patch);

    assert!(!report.diagnostics.has_errors());
    assert_eq!(
        world
            .get::<TestPosition>(spawned_entity(&world, &report, &added))
            .unwrap()
            .x,
        7
    );
    assert!(
        world
            .get::<TestPosition>(spawned_entity(&world, &report, &removed))
            .is_none()
    );
}

#[test]
fn unknown_prefab_patch_target_prevents_world_mutation() {
    let registry = test_registry();
    let prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("enemy"))
        .with_component(position_type_id(), position_record(1))]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("missing"),
        component: position_type_id(),
        component_version: ComponentSchemaVersion(1),
        path: ComponentFieldPath::from_fields(["x"]),
        value: ComponentValue::I64(9),
    }]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_prefab_with_patch(&mut world, &registry, &prefab, &patch);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-missing-entity"
            && diagnostic_has_u64_field(diagnostic, "operation-index", 0)
            && diagnostic_has_text_field(diagnostic, "entity-id", "missing")
    }));
}

#[test]
fn patch_validation_failure_preserves_sticky_pressure_stats_and_operation_context() {
    let registry = test_registry();
    let components = (0..300_u32)
        .map(|index| {
            (
                ComponentTypeId::new(format!("nara.test.Unknown{index:03}")),
                SceneComponentRecord::new(ComponentSchemaVersion(1), ComponentValue::Null),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let invalid_entity = SceneEntityRecord {
        id: scene_id("invalid"),
        parent: None,
        components,
        prefab: None,
    };
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: invalid_entity,
    }]);
    let mut document = SceneDocument::default();

    let report = patch.apply_to_scene(&mut document, &registry);

    assert!(!report.applied);
    assert!(document.entities.is_empty());
    assert!(report.diagnostics.has_errors());
    let stats = report.diagnostics.stats();
    assert_eq!(stats.observed_errors(), 300);
    assert_eq!(stats.observed_warnings(), 0);
    assert_eq!(stats.observed_info(), 0);
    assert_eq!(stats.published_entries(), 300);
    assert_eq!(stats.rejected_entries(), 0);
    assert_eq!(stats.dropped_fields(), 0);
    assert_eq!(stats.truncated_fields(), 0);
    assert_eq!(stats.truncated_text_bytes(), 0);
    assert!(stats.evicted_entries() > 0);
    assert!(stats.evicted_bytes() > 0);
    assert!(!report.diagnostics.is_retained_empty());
    assert!(report.diagnostics.iter().all(|diagnostic| {
        diagnostic.code().as_str() == "scene.unknown-component"
            && diagnostic_has_u64_field(diagnostic, "operation-index", 0)
    }));
}

#[test]
fn export_drops_parent_that_is_not_in_document() {
    let registry = test_registry();
    let mut world = World::new();
    let child_id = scene_id("child");
    let report = spawn_scene(
        &mut world,
        &registry,
        &SceneDocument::new([SceneEntityRecord::new(child_id.clone())
            .with_component(position_type_id(), position_record(3))]),
    );
    let child = spawned_entity(&world, &report, &child_id);
    let parent = world.spawn_empty().id();
    world.entity_mut(child).insert(Parent(parent));

    let export = export_scene(&world, &registry);
    let output = export.output().unwrap();

    assert_eq!(output.document.entities.len(), 1);
    assert_eq!(output.document.entities[0].id.as_str(), "child");
    assert_eq!(output.document.entities[0].parent, None);
    assert_eq!(world.get::<Parent>(child).unwrap().0, parent);
    assert!(
        export
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.export-parent-skipped")
    );
}

#[test]
fn export_component_encode_failure_is_an_error() {
    let registry = broken_export_registry();
    let mut world = World::new();
    let report = spawn_scene(
        &mut world,
        &registry,
        &SceneDocument::new([SceneEntityRecord::new(scene_id("broken")).with_component(
            broken_export_type_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion(1),
                ComponentValue::map([("broken", ComponentValue::String("value".to_owned()))]),
            ),
        )]),
    );
    assert!(!report.diagnostics.has_errors());

    let export = export_scene(&world, &registry);

    assert!(export.diagnostics.has_errors());
    assert!(export.output().is_none());
    assert!(
        export
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.code().as_str() == "scene.export-component-failed"
                    && diagnostic_has_text_field(diagnostic, "entity-id", "broken")
                    && diagnostic_has_text_field(
                        diagnostic,
                        "component-id",
                        broken_export_type_id().as_str(),
                    )
            )
    );
}

#[cfg(feature = "serde")]
#[test]
fn scene_entity_id_deserialization_validates_shape() {
    let error = serde_json::from_str::<SceneDocument>(
        r#"{"format_version":1,"entities":[{"id":"root/../player","components":{}}]}"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains(".."));
}

#[cfg(feature = "serde")]
#[test]
fn scene_json_rejects_unknown_document_entity_and_component_fields() {
    let document_error =
        SceneDocument::from_json_str(r#"{"format_version":1,"entities":[],"unexpected":true}"#)
            .unwrap_err();
    assert!(document_error.to_string().contains("unknown field"));

    let entity_error = SceneDocument::from_json_str(
        r#"{"format_version":1,"entities":[{"id":"player","components":{},"unexpected":true}]}"#,
    )
    .unwrap_err();
    assert!(entity_error.to_string().contains("unknown field"));

    let component_error = SceneDocument::from_json_str(
        r#"{"format_version":1,"entities":[{"id":"player","components":{"nara.test.Position":{"version":1,"value":{"type":"null"},"unexpected":true}}}]}"#,
    )
    .unwrap_err();
    assert!(component_error.to_string().contains("unknown field"));
}

#[cfg(feature = "serde")]
#[test]
fn prefab_json_rejects_unknown_fields() {
    let prefab_error =
        PrefabDocument::from_json_str(r#"{"format_version":1,"entities":[],"unexpected":true}"#)
            .unwrap_err();

    assert!(prefab_error.to_string().contains("unknown field"));
}

#[cfg(feature = "serde")]
#[test]
fn patch_json_rejects_unknown_document_and_operation_fields() {
    let document_error = serde_json::from_str::<ScenePatchDocument>(
        r#"{"format_version":1,"operations":[],"unexpected":true}"#,
    )
    .unwrap_err();
    assert!(document_error.to_string().contains("unknown field"));

    let operation_error = serde_json::from_str::<ScenePatchDocument>(
        r#"{"format_version":1,"operations":[{"op":"remove_entity","args":{"entity":"player","unexpected":true}}]}"#,
    )
    .unwrap_err();
    assert!(operation_error.to_string().contains("unknown field"));
}

#[test]
fn scene_component_schemas_expose_scalar_fields() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).expect("component registration should succeed");

    let name_schema = registry
        .schema(&ComponentTypeId::new("nara.scene.Name"))
        .unwrap();
    let visibility_schema = registry
        .schema(&ComponentTypeId::new("nara.scene.Visibility"))
        .unwrap();

    assert_eq!(
        name_schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>(),
        vec![("<root>".to_string(), ComponentValueKind::String, true)]
    );
    assert_eq!(
        visibility_schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>(),
        vec![("<root>".to_string(), ComponentValueKind::String, true)]
    );
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_scene_component_with_fields::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["x"]),
                ComponentValueKind::I64,
            )],
            |value| {
                let x = value
                    .get("x")
                    .and_then(ComponentValue::as_i64)
                    .ok_or_else(|| ComponentCodecError::invalid_field("x", "i64"))?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap();
    registry
}

fn entity_link_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_scene_component_with_fields::<TestEntityLink, _, _>(
            entity_link_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )],
            |value| {
                Ok(TestEntityLink {
                    target: value.field_entity_reference("target")?.clone(),
                })
            },
            |link| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(link.target.clone()),
                )]))
            },
        )
        .unwrap();
    registry
}

fn migrated_position_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_scene_component_with_fields::<TestPosition, _, _>(
            position_type_id(),
            ComponentSchemaVersion(2),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["x2"]),
                ComponentValueKind::I64,
            )],
            |value| {
                let x = value.field_i64("x2")?;
                Ok(TestPosition {
                    x: i32::try_from(x)
                        .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                })
            },
            |position| {
                Ok(ComponentValue::map([(
                    "x2",
                    ComponentValue::I64(i64::from(position.x)),
                )]))
            },
        )
        .unwrap()
        .register_component_migration(
            &position_type_id(),
            ComponentSchemaVersion(1),
            ComponentSchemaVersion(2),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let x = fields
                    .remove("x")
                    .ok_or_else(|| ComponentCodecError::missing_field("x"))?;
                fields.insert("x2".to_string(), x);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();
    registry
}

fn test_asset_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_component_codec_with_context_and_fields::<TestAssetLink, _, _>(
            asset_link_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["asset"]),
                ComponentValueKind::AssetRef,
            )],
            |value, context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                let prepared = prepare_test_asset_handle(context, "asset.value", asset_ref)?;
                Ok(PreparedComponent::new(move |context| {
                    let handle = match prepared {
                        PreparedTestAsset::Resolved(handle) => handle,
                        PreparedTestAsset::Deferred(asset_ref) => context
                            .resolve_asset_ref::<TestAsset>(&asset_ref)
                            .map_err(|error| {
                                ComponentCodecError::invalid_asset_ref(
                                    "asset.value",
                                    asset_ref.to_string(),
                                    error.to_string(),
                                )
                            })?,
                    };
                    Ok(TestAssetLink { handle })
                }))
            },
            |world, entity, _context| {
                let Some(link) = world.get::<TestAssetLink>(entity) else {
                    return Ok(None);
                };
                Ok(Some(ComponentValue::map([(
                    "handle",
                    ComponentValue::U64(link.handle.id().raw()),
                )])))
            },
        )
        .unwrap();
    registry
}

fn broken_export_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_component_codec_with_context_and_fields::<TestBrokenExport, _, _>(
            broken_export_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["broken"]),
                ComponentValueKind::String,
            )],
            |_value, _context| Ok(PreparedComponent::insert(TestBrokenExport)),
            |_world, _entity, _context| Err(ComponentCodecError::invalid_field("broken", "boom")),
        )
        .unwrap();
    registry
}

fn failing_apply_registry() -> ComponentRegistry {
    let mut registry = test_registry();
    registry
        .register_component_codec_with_context_and_fields::<TestAssetLink, _, _>(
            apply_resolves_asset_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::from_fields(["asset"]),
                ComponentValueKind::AssetRef,
            )],
            |value, _context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                Ok(PreparedComponent::new(move |context| {
                    let handle =
                        context
                            .resolve_asset_ref::<TestAsset>(&asset_ref)
                            .map_err(|error| {
                                ComponentCodecError::invalid_asset_ref(
                                    "asset.value",
                                    asset_ref.to_string(),
                                    error.to_string(),
                                )
                            })?;
                    Ok(TestAssetLink { handle })
                }))
            },
            |_world, _entity, _context| Ok(None),
        )
        .unwrap();
    registry
        .register_component_codec_with_context_and_fields::<TestApplyFails, _, _>(
            apply_fails_type_id(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::optional(
                ComponentFieldPath::from_fields(["asset"]),
                ComponentValueKind::AssetRef,
            )],
            |value, context| {
                if let Some(asset_value) = value.get("asset") {
                    let asset_ref = read_asset_ref(asset_value, "asset")?;
                    if let Some(result) = context.resolve_asset_ref::<TestAsset>(&asset_ref) {
                        result.map_err(|error| {
                            ComponentCodecError::invalid_asset_ref(
                                "asset.value",
                                asset_ref.to_string(),
                                error.to_string(),
                            )
                        })?;
                    }
                }

                Ok(PreparedComponent::new(|_context| {
                    Err::<TestApplyFails, _>(ComponentCodecError::invalid_field(
                        "apply",
                        "intentional failure",
                    ))
                }))
            },
            |_world, _entity, _context| Ok(None),
        )
        .unwrap();
    registry
}

enum PreparedTestAsset {
    Resolved(Handle<TestAsset>),
    Deferred(AssetRef),
}

fn prepare_test_asset_handle(
    context: &mut ComponentDecodeContext<'_>,
    field: &str,
    asset_ref: AssetRef,
) -> Result<PreparedTestAsset, ComponentCodecError> {
    if let Some(result) = context.resolve_asset_ref::<TestAsset>(&asset_ref) {
        return result.map(PreparedTestAsset::Resolved).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        });
    }

    if let Some(stable_id) = asset_ref.as_stable_id() {
        let Some(database) = context.project_asset_database() else {
            return Err(ComponentCodecError::invalid_asset_ref(
                field,
                asset_ref.to_string(),
                AssetRefError::MissingProjectDatabase(stable_id).to_string(),
            ));
        };
        database.resolve_ref(&asset_ref).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        })?;
    }

    Ok(PreparedTestAsset::Deferred(asset_ref))
}

fn read_asset_ref(value: &ComponentValue, field: &str) -> Result<AssetRef, ComponentCodecError> {
    match value.field_str("kind")? {
        "path" => AssetRef::path(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        "stable_id" => AssetRef::stable_id(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "'path' or 'stable_id'",
        )),
    }
}

fn asset_link_record(asset_ref: AssetRef) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(1),
        ComponentValue::map([("asset", asset_ref_value(&asset_ref))]),
    )
}

fn apply_fails_record(asset_ref: Option<AssetRef>) -> SceneComponentRecord {
    let value = asset_ref
        .as_ref()
        .map(|asset_ref| ComponentValue::map([("asset", asset_ref_value(asset_ref))]))
        .unwrap_or_else(|| ComponentValue::Map(BTreeMap::new()));
    SceneComponentRecord::new(ComponentSchemaVersion(1), value)
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn test_database(stable_id: StableAssetId, path: &str) -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable_id,
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    database
}

fn stable_id(id: &str) -> StableAssetId {
    StableAssetId::parse_str(id).unwrap()
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn prefab_anchor(id: &str, source: AssetRef) -> SceneEntityRecord {
    prefab_anchor_with_overrides(id, source, ScenePatchDocument::default())
}

fn prefab_anchor_with_overrides(
    id: &str,
    source: AssetRef,
    overrides: ScenePatchDocument,
) -> SceneEntityRecord {
    SceneEntityRecord {
        id: scene_id(id),
        parent: None,
        components: BTreeMap::new(),
        prefab: Some(PrefabInstance { source, overrides }),
    }
}

fn asset_link_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.AssetLink")
}

fn broken_export_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.BrokenExport")
}

fn apply_fails_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.ApplyFails")
}

fn apply_resolves_asset_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.ApplyResolvesAsset")
}

fn entity_link_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.EntityLink")
}

fn position_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.Position")
}

fn position_record(x: i32) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(1),
        ComponentValue::map([("x", ComponentValue::I64(i64::from(x)))]),
    )
}

fn entity_link_record(target: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(1),
        ComponentValue::map([("target", ComponentValue::EntityReference(target))]),
    )
}

fn exported_entity_link(record: &SceneEntityRecord) -> &EntityReference {
    record
        .components
        .get(&entity_link_type_id())
        .expect("exported source entity must retain its entity-link component")
        .value
        .field_entity_reference("target")
        .expect("exported entity-link target must remain typed")
}
