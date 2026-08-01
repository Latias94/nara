use std::collections::BTreeMap;

use super::*;
use nara_asset::{
    AssetId, AssetPath, AssetRecord, AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticFieldClass, DiagnosticReport, DiagnosticValueRef};
use nara_ecs::{
    Commands, Component, Entity, Mut, Resource, World,
    lifecycle::{Add, Despawn, HookContext, Remove},
    observer::{Observer, On},
    system::ResMut,
    world::DeferredWorld,
};
use nara_hierarchy::{Children, HierarchyConstructionWriter, Parent};
use nara_identity::{
    EntityLookup, EntityReference, IdentityDomainError, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, SpawnedSceneInstance, TombstoneCause,
    WorldEntityLocator, WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
};
use nara_reflect::bevy_reflect;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryError, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PreparedComponentCandidate, Reflect,
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
struct TestAlternateAssetLink;

#[derive(Clone, Debug, PartialEq, Component)]
struct DeferredTransform(i64);

#[derive(Clone, Debug, PartialEq, Component)]
struct TestBrokenExport;

#[derive(Clone, Debug, PartialEq, Component)]
struct TestApplyFails;

#[derive(Clone, Debug, PartialEq, Component)]
struct RuntimeRetirementProbe;

#[derive(Debug, Default, Resource)]
struct PersistentApplyCanary(u32);

fn persistent_apply_hook(mut world: DeferredWorld<'_>, _context: HookContext) {
    world.resource_mut::<PersistentApplyCanary>().0 += 1;
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestEntityLink {
    target: EntityReference,
}

#[derive(Clone, Debug, PartialEq, Component)]
struct TestLargeData;

#[derive(Debug)]
struct TestAsset;

fn test_component_schema(
    id: ComponentTypeId,
    alias: &str,
    version: ComponentSchemaVersion,
    fields: impl IntoIterator<Item = ComponentFieldSchema>,
) -> ComponentSchema {
    ComponentSchema::new(id, alias, version)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(fields)
}

fn test_component_field(
    id: &str,
    alias: &str,
    path: ComponentFieldPath,
    value_kind: ComponentValueKind,
    required: bool,
) -> ComponentFieldSchema {
    let field = if required {
        ComponentFieldSchema::required(ComponentFieldId::new(id), alias, path, value_kind)
    } else {
        ComponentFieldSchema::optional(ComponentFieldId::new(id), alias, path, value_kind)
    }
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);

    match value_kind {
        ComponentValueKind::AssetRef => field.with_capability(ComponentCapability::AssetRef),
        ComponentValueKind::EntityRef => field.with_capability(ComponentCapability::EntityRef),
        _ => field,
    }
}

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
fn construction_writer_publishes_parent_child_links_immediately() {
    let mut world = World::new();
    let parent = world.spawn((Name::new("parent"),)).id();
    let child = world.spawn((Name::new("child"),)).id();
    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap();

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
        field: ComponentFieldId::new("x"),
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
fn repeated_prefab_instances_namespace_declared_scene_local_references() {
    let registry = entity_link_registry();
    let source = AssetRef::path("prefabs/linked-unit.ron").unwrap();
    let persistent = persistent_reference("66666666-6666-4666-8666-666666666666");
    let prefab = PrefabDocument::new([
        SceneEntityRecord::new(scene_id("source")).with_component(
            entity_link_type_id(),
            entity_link_record(EntityReference::SceneLocal {
                entity: scene_id("target"),
            }),
        ),
        SceneEntityRecord::new(scene_id("target")),
        SceneEntityRecord::new(scene_id("external")).with_component(
            entity_link_type_id(),
            entity_link_record(EntityReference::Persistent {
                entity: persistent.clone(),
            }),
        ),
    ]);
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    let document = SceneDocument::new([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    for anchor in ["left", "right"] {
        let source = expanded
            .entities
            .iter()
            .find(|entity| entity.id == scene_id(&format!("{anchor}/source")))
            .unwrap();
        assert_eq!(
            exported_entity_link(source),
            &EntityReference::SceneLocal {
                entity: scene_id(&format!("{anchor}/target")),
            }
        );

        let external = expanded
            .entities
            .iter()
            .find(|entity| entity.id == scene_id(&format!("{anchor}/external")))
            .unwrap();
        assert_eq!(
            exported_entity_link(external),
            &EntityReference::Persistent {
                entity: persistent.clone(),
            }
        );
    }
    let source_record = prefab
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("source"))
        .unwrap();
    assert_eq!(
        exported_entity_link(source_record),
        &EntityReference::SceneLocal {
            entity: scene_id("target"),
        }
    );
}

#[test]
fn nested_prefabs_namespace_declared_scene_local_references_through_each_anchor() {
    let registry = entity_link_registry();
    let inner_source = AssetRef::path("prefabs/linked-inner.ron").unwrap();
    let outer_source = AssetRef::path("prefabs/linked-outer.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            inner_source.clone(),
            PrefabDocument::new([
                SceneEntityRecord::new(scene_id("source")).with_component(
                    entity_link_type_id(),
                    entity_link_record(EntityReference::SceneLocal {
                        entity: scene_id("target"),
                    }),
                ),
                SceneEntityRecord::new(scene_id("target")),
            ]),
        )
        .with_prefab(
            outer_source.clone(),
            PrefabDocument::new([prefab_anchor("nested", inner_source)]),
        );
    let document = SceneDocument::new([prefab_anchor("root", outer_source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    let source = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("root/nested/source"))
        .unwrap();
    assert_eq!(
        exported_entity_link(source),
        &EntityReference::SceneLocal {
            entity: scene_id("root/nested/target"),
        }
    );
}

#[test]
fn nested_prefab_generated_identifier_budget_charges_only_final_projection() {
    let registry = entity_link_registry();
    let inner_source = AssetRef::path("prefabs/linked-inner.ron").unwrap();
    let outer_source = AssetRef::path("prefabs/linked-outer.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            inner_source.clone(),
            PrefabDocument::new([
                SceneEntityRecord::new(scene_id("source")).with_component(
                    entity_link_type_id(),
                    entity_link_record(EntityReference::SceneLocal {
                        entity: scene_id("target"),
                    }),
                ),
                SceneEntityRecord::new(scene_id("target")),
            ]),
        )
        .with_prefab(
            outer_source.clone(),
            PrefabDocument::new([prefab_anchor("nested", inner_source)]),
        );
    let document = SceneDocument::new([prefab_anchor("root", outer_source)]);
    let exact_limits = PrefabExpansionLimits::default()
        .with_generated_identifier_bytes(nara_core::ByteLimit::new(91).unwrap());

    let exact = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(exact_limits),
    );

    assert!(!exact.diagnostics.has_errors());
    assert!(exact.document.is_some());

    let failing_limits = PrefabExpansionLimits::default()
        .with_generated_identifier_bytes(nara_core::ByteLimit::new(90).unwrap());
    let failing = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(failing_limits),
    );

    assert!(failing.document.is_none());
    assert!(failing.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "generated-identifier-bytes")
            && diagnostic_has_u64_field(diagnostic, "observed", 91)
            && diagnostic_has_u64_field(diagnostic, "maximum", 90)
    }));
}

#[test]
fn prefab_reference_rewrite_uses_the_migrated_component_schema() {
    let registry = migrated_entity_link_registry();
    let source = AssetRef::path("prefabs/migrated-link.ron").unwrap();
    let source_record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "old_target",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: scene_id("target"),
            }),
        )]),
    );
    let prefab = PrefabDocument::new([
        SceneEntityRecord::new(scene_id("source"))
            .with_component(entity_link_type_id(), source_record),
        SceneEntityRecord::new(scene_id("target")),
    ]);
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    let document = SceneDocument::new([prefab_anchor("root", source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    let source = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("root/source"))
        .unwrap();
    let component = source.components.get(&entity_link_type_id()).unwrap();
    assert_eq!(component.version, ComponentSchemaVersion(2));
    assert_eq!(
        component.value.field_entity_reference("target").unwrap(),
        &EntityReference::SceneLocal {
            entity: scene_id("root/target"),
        }
    );
    let source_component = prefab.entities[0]
        .components
        .get(&entity_link_type_id())
        .unwrap();
    assert_eq!(source_component.version, ComponentSchemaVersion::ONE);
    assert!(source_component.value.field("old_target").is_ok());
}

#[test]
fn prefab_expansion_budgets_namespaced_component_references_before_publication() {
    let registry = entity_link_registry();
    let source = AssetRef::path("prefabs/linked-unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([
            SceneEntityRecord::new(scene_id("source")).with_component(
                entity_link_type_id(),
                entity_link_record(EntityReference::SceneLocal {
                    entity: scene_id("target"),
                }),
            ),
            SceneEntityRecord::new(scene_id("target")),
        ]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source)]);
    let exact_limits = PrefabExpansionLimits::default()
        .with_generated_identifier_bytes(nara_core::ByteLimit::new(41).unwrap());
    let exact = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(exact_limits),
    );
    assert!(!exact.diagnostics.has_errors());
    assert!(exact.document.is_some());

    let limits = PrefabExpansionLimits::default()
        .with_generated_identifier_bytes(nara_core::ByteLimit::new(40).unwrap());
    let expansion = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(limits),
    );

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "generated-identifier-bytes")
            && diagnostic_has_u64_field(diagnostic, "observed", 41)
            && diagnostic_has_u64_field(diagnostic, "maximum", 40)
    }));
}

#[test]
fn prefab_namespace_preflight_uses_the_public_materialized_node_budget() {
    let registry = large_data_registry();
    let source = AssetRef::path("prefabs/large-data.ron").unwrap();
    let large_value = ComponentValue::map([(
        "values",
        ComponentValue::List(
            (0..17_000)
                .map(|value| ComponentValue::I64(i64::from(value)))
                .collect(),
        ),
    )]);
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
            large_data_type_id(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, large_value),
        )]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    let expanded = expansion.document.unwrap();
    let source = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("root/source"))
        .unwrap();
    let component = source.components.get(&large_data_type_id()).unwrap();
    let ComponentValue::List(values) = component.value.field("values").unwrap() else {
        panic!("large data values must remain a list");
    };
    assert_eq!(values.len(), 17_000);
}

#[test]
fn prefab_namespace_rejects_undeclared_references_without_mutating_the_source() {
    let registry = large_data_registry();
    let source = AssetRef::path("prefabs/hidden-reference.ron").unwrap();
    let prefab = PrefabDocument::new([
        SceneEntityRecord::new(scene_id("source")).with_component(
            large_data_type_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "values",
                    ComponentValue::List(vec![ComponentValue::EntityReference(
                        EntityReference::SceneLocal {
                            entity: scene_id("target"),
                        },
                    )]),
                )]),
            ),
        ),
        SceneEntityRecord::new(scene_id("target")),
    ]);
    let source_before = prefab.clone();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    let document = SceneDocument::new([prefab_anchor("root", source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-entity-reference-rewrite-failed"
            && diagnostic_has_text_field(diagnostic, "rewrite-error-kind", "undeclared-reference")
            && diagnostic_has_text_field(diagnostic, "entity-id", "source")
            && diagnostic_has_text_field(diagnostic, "component-id", "nara.test.LargeData")
    }));
    assert_eq!(prefab, source_before);
}

#[test]
fn prefab_expansion_budgets_namespaced_reference_value_growth_before_publication() {
    let registry = entity_link_registry();
    let source = AssetRef::path("prefabs/linked-unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([
            SceneEntityRecord::new(scene_id("source")).with_component(
                entity_link_type_id(),
                entity_link_record(EntityReference::SceneLocal {
                    entity: scene_id("target"),
                }),
            ),
            SceneEntityRecord::new(scene_id("target")),
        ]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source)]);
    let exact_limits = PrefabExpansionLimits::default()
        .with_materialized_value_bytes(nara_core::ByteLimit::new(17).unwrap());

    let exact = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(exact_limits),
    );

    assert!(!exact.diagnostics.has_errors());
    assert!(exact.document.is_some());

    let failing_limits = PrefabExpansionLimits::default()
        .with_materialized_value_bytes(nara_core::ByteLimit::new(16).unwrap());
    let failing = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(failing_limits),
    );

    assert!(failing.document.is_none());
    assert!(failing.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "materialized-value-bytes")
            && diagnostic_has_u64_field(diagnostic, "observed", 17)
            && diagnostic_has_u64_field(diagnostic, "maximum", 16)
    }));
}

#[test]
fn prefab_namespace_traversal_accepts_persistent_reference_at_logical_byte_limit() {
    let registry = entity_link_registry();
    let source = AssetRef::path("prefabs/persistent-link.ron").unwrap();
    let persistent = persistent_reference("77777777-7777-4777-8777-777777777777");
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("source")).with_component(
            entity_link_type_id(),
            entity_link_record(EntityReference::Persistent {
                entity: persistent.clone(),
            }),
        )]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source)]);
    let limits = PrefabExpansionLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(2).unwrap())
        .with_materialized_value_bytes(nara_core::ByteLimit::new(26).unwrap());

    let expansion = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(limits),
    );

    assert!(!expansion.diagnostics.has_errors());
    let expanded = expansion.document.unwrap();
    let source = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("root/source"))
        .unwrap();
    assert_eq!(
        exported_entity_link(source),
        &EntityReference::Persistent { entity: persistent }
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
        PrefabExpansionOptions::default().with_max_depth(1),
    );

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-depth-exceeded"
            && diagnostic_has_text_field(diagnostic, "entity-id", "enemy/weapon")
            && diagnostic_has_u64_field(diagnostic, "maximum-depth", 1)
    }));
}

#[test]
fn prefab_expansion_entity_budget_is_aggregate_across_reused_sources() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("body"))]),
    );
    let document = SceneDocument::new([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);
    let exact = PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_materialized_entities(ItemLimit::new(4).unwrap()),
    );
    assert!(
        document
            .expand_prefabs_with_options(&registry, &resolver, exact)
            .document
            .is_some()
    );

    let over = PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_materialized_entities(ItemLimit::new(3).unwrap()),
    );
    let expansion = document.expand_prefabs_with_options(&registry, &resolver, over);
    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
            && diagnostic_has_text_field(diagnostic, "budget-kind", "materialized-entities")
            && diagnostic_has_u64_field(diagnostic, "observed", 4)
            && diagnostic_has_u64_field(diagnostic, "maximum", 3)
    }));
}

#[test]
fn prefab_expansion_value_budgets_are_aggregate_across_reused_sources() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    let source = AssetRef::path("prefabs/unit.ron").unwrap();
    let payload = "x".repeat(64 * 1024);
    let payload_bytes = payload.len();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("body")).with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::String(payload)),
        )]),
    );
    let document = SceneDocument::new([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);

    let exact_limits = PrefabExpansionLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(2).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(payload_bytes * 2).unwrap());
    assert!(
        document
            .expand_prefabs_with_options(
                &registry,
                &resolver,
                PrefabExpansionOptions::default().with_limits(exact_limits),
            )
            .document
            .is_some()
    );

    let node_limits =
        PrefabExpansionLimits::default().with_materialized_value_nodes(ItemLimit::ONE);
    let node_report = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(node_limits),
    );
    assert!(node_report.document.is_none());
    assert!(node_report.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "materialized-value-nodes")
            && diagnostic_has_u64_field(diagnostic, "observed", 2)
            && diagnostic_has_u64_field(diagnostic, "maximum", 1)
    }));

    let byte_limits = PrefabExpansionLimits::default()
        .with_materialized_value_bytes(ByteLimit::new(payload_bytes * 2 - 1).unwrap());
    let byte_report = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(byte_limits),
    );
    assert!(byte_report.document.is_none());
    assert!(byte_report.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "materialized-value-bytes")
            && diagnostic_has_u64_field(diagnostic, "observed", (payload_bytes * 2) as u64)
            && diagnostic_has_u64_field(diagnostic, "maximum", (payload_bytes * 2 - 1) as u64)
    }));
}

#[test]
fn prefab_expansion_component_and_instance_budgets_are_independent() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([
            SceneEntityRecord::new(scene_id("first"))
                .with_component(position_type_id(), position_record(1)),
            SceneEntityRecord::new(scene_id("second"))
                .with_component(position_type_id(), position_record(2)),
        ]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source.clone())]);
    let component_limited = PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_materialized_components(ItemLimit::new(1).unwrap()),
    );
    let expansion = document.expand_prefabs_with_options(&registry, &resolver, component_limited);
    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "materialized-components")
            && diagnostic_has_u64_field(diagnostic, "observed", 2)
    }));

    let two_instances = SceneDocument::new([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);
    let instance_limited = PrefabExpansionOptions::default()
        .with_limits(PrefabExpansionLimits::default().with_resolved_instances(ItemLimit::ONE));
    let expansion =
        two_instances.expand_prefabs_with_options(&registry, &resolver, instance_limited);
    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "resolved-instances")
            && diagnostic_has_u64_field(diagnostic, "observed", 2)
            && diagnostic_has_u64_field(diagnostic, "maximum", 1)
    }));
}

#[test]
fn prefab_expansion_counts_override_operations_per_instance() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("body"))
            .with_component(position_type_id(), position_record(1))]),
    );
    let overrides = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("body"),
        component: position_type_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(2),
    }]);
    let document = SceneDocument::new([
        prefab_anchor_with_overrides("left", source.clone(), overrides.clone()),
        prefab_anchor_with_overrides("right", source, overrides),
    ]);
    let options = PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_applied_patch_operations(ItemLimit::ONE),
    );

    let expansion = document.expand_prefabs_with_options(&registry, &resolver, options);

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "applied-patch-operations")
            && diagnostic_has_u64_field(diagnostic, "observed", 2)
            && diagnostic_has_u64_field(diagnostic, "maximum", 1)
    }));
}

#[test]
fn prefab_expansion_bounds_generated_identifier_bytes_before_publication() {
    let registry = test_registry();
    let source = AssetRef::path("prefabs/unit.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new([SceneEntityRecord::new(scene_id("child"))]),
    );
    let document = SceneDocument::new([prefab_anchor("root", source)]);
    let limits = PrefabExpansionLimits::default()
        .with_generated_identifier_bytes(nara_core::ByteLimit::new(13).unwrap());

    let expansion = document.expand_prefabs_with_options(
        &registry,
        &resolver,
        PrefabExpansionOptions::default().with_limits(limits),
    );

    assert!(expansion.document.is_none());
    assert!(expansion.diagnostics.iter().any(|diagnostic| {
        diagnostic_has_text_field(diagnostic, "budget-kind", "generated-identifier-bytes")
            && diagnostic_has_u64_field(diagnostic, "observed", 14)
            && diagnostic_has_u64_field(diagnostic, "maximum", 13)
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
fn component_observer_rejects_fresh_scene_before_target_allocation() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("observed"))
        .with_component(position_type_id(), position_record(4))]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(
        |_: On<Add, TestPosition>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(
                diagnostic,
                "persistent-apply-reason",
                "lifecycle-observer",
            )
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "add")
            && diagnostic_has_text_field(diagnostic, "observer-scope", "component-global")
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn deferred_dynamic_hook_rejects_fresh_scene_before_target_allocation() {
    let registry = test_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("hooked"))
        .with_component(position_type_id(), position_record(5))]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.register_component::<TestPosition>();
    world.commands().queue(|world: &mut World| {
        world
            .register_component_hooks::<TestPosition>()
            .on_add(persistent_apply_hook);
    });
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(diagnostic, "persistent-apply-reason", "lifecycle-hook")
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "add")
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn hierarchy_observer_rejects_publication_before_candidate_mutation() {
    let registry = test_registry();
    let parent_id = scene_id("parent");
    let child_id = scene_id("child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(parent_id.clone())
            .with_component(position_type_id(), position_record(1)),
        SceneEntityRecord::new(child_id)
            .with_parent(parent_id)
            .with_component(position_type_id(), position_record(2)),
    ]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(|_: On<Add, Parent>, mut commands: Commands| {
        commands.add_observer(
            |_: On<Add, TestPosition>, mut canary: ResMut<PersistentApplyCanary>| {
                canary.0 += 1;
            },
        );
    });
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.runtime-projection-ineligible"
        })
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
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
    let schema = test_component_schema(
        position_type_id(),
        "Position",
        ComponentSchemaVersion(2),
        [test_component_field(
            "x",
            "X",
            ComponentFieldPath::from_fields(["x2"]),
            ComponentValueKind::I64,
            true,
        )],
    );
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
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
    registry.freeze().unwrap();
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
fn asset_server_observer_rejects_scene_before_target_allocation() {
    let registry = test_asset_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::path("textures/player.png").unwrap()),
    )]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(
        |_: On<Add, AssetServer>, mut canary: ResMut<PersistentApplyCanary>| canary.0 += 1,
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "add")
            && diagnostic_has_text_field(diagnostic, "observer-scope", "component-global")
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert!(!world.contains_resource::<AssetServer>());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
}

#[test]
fn deferred_asset_prepare_declares_resource_access_before_target_allocation() {
    let registry = deferred_asset_registry();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("player")).with_component(
        asset_link_type_id(),
        asset_link_record(AssetRef::path("textures/player.png").unwrap()),
    )]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(
        |_: On<Add, AssetServer>, mut canary: ResMut<PersistentApplyCanary>| canary.0 += 1,
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
        })
    );
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert!(!world.contains_resource::<AssetServer>());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
}

#[test]
fn identity_resource_scope_observer_rejects_scene_before_target_allocation() {
    let registry = test_registry();
    let first_document = SceneDocument::new([SceneEntityRecord::new(scene_id("first"))
        .with_component(position_type_id(), position_record(1))]);
    let second_document = SceneDocument::new([SceneEntityRecord::new(scene_id("second"))
        .with_component(position_type_id(), position_record(2))]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let first = spawn_scene(&mut world, &registry, &first_document);
    assert!(!first.diagnostics.has_errors());
    world.add_observer(
        |_: On<Remove, WorldIdentityDomain>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &second_document);

    assert!(report.instance.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.identity-support-ineligible"
        })
    );
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
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
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
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
fn empty_scene_replacement_handles_empty_and_nonempty_candidates() {
    let registry = test_registry();
    let empty = SceneDocument::new([]);
    let populated_id = scene_id("populated");
    let populated = SceneDocument::new([SceneEntityRecord::new(populated_id.clone())]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();

    let initial = spawner.spawn(&mut world, &registry, &empty);
    assert!(!initial.diagnostics.has_errors());
    let initial = spawned_instance(&initial).clone();

    let empty_replacement = spawner.replace(&mut world, &registry, &empty, &initial);
    assert!(!empty_replacement.diagnostics.has_errors());
    assert_eq!(empty_replacement.retired_entities(), 0);
    let empty_replacement = spawned_instance(&empty_replacement).clone();

    let populated_replacement =
        spawner.replace(&mut world, &registry, &populated, &empty_replacement);
    assert!(!populated_replacement.diagnostics.has_errors());
    assert_eq!(populated_replacement.retired_entities(), 0);
    let populated_entity = spawned_entity(&world, &populated_replacement, &populated_id);
    assert!(world.get_entity(populated_entity).is_ok());
}

#[test]
fn individual_scene_retirement_tombstones_identity_before_despawn() {
    let registry = test_registry();
    let id = scene_id("retired");
    let document =
        SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, &document);
    let instance = spawned_instance(&report).clone();
    let entity = spawned_entity(&world, &report, &id);

    retire_and_despawn_scene_entity(&mut world, entity).unwrap();

    assert!(world.get_entity(entity).is_err());
    let EntityLookup::Tombstoned(Some(tombstone)) = instance.resolve(&world, &id) else {
        panic!("retired entity must resolve to a retained tombstone");
    };
    assert_eq!(tombstone.cause(), TombstoneCause::Despawned);
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_entities,
        0
    );
}

#[test]
fn scene_retirement_rejection_keeps_the_target_alive() {
    let registry = test_registry();
    let id = scene_id("retained");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone())]);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, &document);
    let entity = spawned_entity(&world, &report, &id);

    let source = world.get::<SceneEntitySource>(entity).unwrap().clone();
    let forged = world
        .spawn(SceneEntitySource {
            instance_id: source.instance_id,
            entity_id: scene_id("forged"),
        })
        .id();
    let forged_components = world.entity(forged).archetype().components().len();
    assert!(matches!(
        retire_and_despawn_scene_entity(&mut world, forged),
        Err(SceneEntityRetirementError::Identity(
            IdentityDomainError::EntityNotRegistered
        ))
    ));
    assert_eq!(
        world.entity(forged).archetype().components().len(),
        forged_components
    );

    world.remove_resource::<WorldIdentityDomain>();

    assert!(matches!(
        retire_and_despawn_scene_entity(&mut world, entity),
        Err(SceneEntityRetirementError::Identity(
            IdentityDomainError::WorldDomainUnavailable
        ))
    ));
    assert!(world.get_entity(entity).is_ok());

    let unrelated = world.spawn_empty().id();
    assert!(matches!(
        retire_and_despawn_scene_entity(&mut world, unrelated),
        Err(SceneEntityRetirementError::NotSceneEntity)
    ));
    assert!(world.get_entity(unrelated).is_ok());
}

#[test]
fn scene_retirement_rejects_a_source_that_disagrees_with_registered_identity() {
    let registry = test_registry();
    let id = scene_id("retained");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone())]);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, &document);
    let instance = spawned_instance(&report).clone();
    let entity = spawned_entity(&world, &report, &id);
    let stats = world.resource::<WorldIdentityDomain>().stats();

    world
        .get_mut::<SceneEntitySource>(entity)
        .unwrap()
        .entity_id = scene_id("forged");

    assert!(matches!(
        retire_and_despawn_scene_entity(&mut world, entity),
        Err(SceneEntityRetirementError::Identity(
            IdentityDomainError::StaleRegistration
        ))
    ));
    assert!(world.get_entity(entity).is_ok());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    assert_eq!(
        instance.resolve(&world, &id),
        EntityLookup::Resolved(entity)
    );
}

#[test]
fn individual_scene_retirement_rejects_parents_and_unlinks_retired_children() {
    let registry = test_registry();
    let parent_id = scene_id("parent");
    let child_id = scene_id("parent/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(parent_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(parent_id.clone()),
    ]);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, &document);
    let instance = spawned_instance(&report).clone();
    let parent = spawned_entity(&world, &report, &parent_id);
    let child = spawned_entity(&world, &report, &child_id);

    assert!(matches!(
        retire_and_despawn_scene_entity(&mut world, parent),
        Err(SceneEntityRetirementError::HasChildren)
    ));
    assert!(world.get_entity(parent).is_ok());
    assert!(world.get_entity(child).is_ok());
    assert!(matches!(
        instance.resolve(&world, &parent_id),
        EntityLookup::Resolved(_)
    ));

    retire_and_despawn_scene_entity(&mut world, child).unwrap();

    assert!(world.get_entity(child).is_err());
    assert!(world.get::<Children>(parent).is_none_or(Children::is_empty));
    assert!(matches!(
        instance.resolve(&world, &child_id),
        EntityLookup::Tombstoned(Some(_))
    ));
}

#[test]
fn empty_persistent_set_rejects_event_global_observer_before_target_allocation() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let document = SceneDocument::new([SceneEntityRecord::new(scene_id("empty"))]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(|_: On<Add>, mut canary: ResMut<PersistentApplyCanary>| canary.0 += 1);
    world.flush();
    world.resource_mut::<PersistentApplyCanary>().0 = 0;
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "add")
            && diagnostic_has_text_field(diagnostic, "observer-scope", "event-global")
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
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
fn hierarchy_aware_replacement_retires_exact_scene_membership_only() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id.clone()),
    ]);
    let mut world = World::new();
    let mut spawner = SceneSpawner::new();
    let first = spawner.spawn(&mut world, &registry, &document);
    assert!(!first.diagnostics.has_errors());
    let current = spawned_instance(&first).clone();
    let old_root = spawned_entity(&world, &first, &root_id);
    let old_child = spawned_entity(&world, &first, &child_id);
    let external_descendant = world.spawn_empty().id();
    HierarchyConstructionWriter::new(&mut world)
        .attach(external_descendant, old_child)
        .unwrap();

    let replacement = spawner.replace(&mut world, &registry, &document, &current);

    assert!(!replacement.diagnostics.has_errors());
    assert_eq!(replacement.retired_entities(), 2);
    assert!(world.get_entity(old_root).is_err());
    assert!(world.get_entity(old_child).is_err());
    assert!(world.get_entity(external_descendant).is_ok());
    assert!(world.get::<Parent>(external_descendant).is_none());
    let new_root = spawned_entity(&world, &replacement, &root_id);
    let new_child = spawned_entity(&world, &replacement, &child_id);
    assert_eq!(world.get::<Parent>(new_child).unwrap().parent(), new_root);
}

#[test]
fn hierarchy_aware_unload_detaches_external_descendants() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id),
    ]);
    let mut session = SceneAuthoringSession::new(document);
    let mut world = World::new();
    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    let live = sync.live_instance.unwrap();
    let scene_child = match live.resolve(&world, &child_id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("scene child did not resolve: {lookup:?}"),
    };
    let external_descendant = world.spawn_empty().id();
    HierarchyConstructionWriter::new(&mut world)
        .attach(external_descendant, scene_child)
        .unwrap();

    let clear = session.clear_live_world(&mut world);

    assert!(clear.cleared);
    assert_eq!(clear.removed_entities, 2);
    assert!(world.get_entity(external_descendant).is_ok());
    assert!(world.get::<Parent>(external_descendant).is_none());
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
fn runtime_component_despawn_observer_rejects_scene_replacement_atomically() {
    let registry = test_registry();
    let id = scene_id("player");
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let mut spawner = SceneSpawner::new();
    let first = spawner.spawn(
        &mut world,
        &registry,
        &SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]),
    );
    assert!(!first.diagnostics.has_errors());
    let current = spawned_instance(&first).clone();
    let old_entity = spawned_entity(&world, &first, &id);
    world.entity_mut(old_entity).insert(RuntimeRetirementProbe);
    let runtime_component = world
        .component_id::<RuntimeRetirementProbe>()
        .expect("the runtime retirement component should be registered");
    world.spawn(
        Observer::new(|_: On<Despawn>, mut canary: ResMut<PersistentApplyCanary>| canary.0 += 1)
            .with_entity(old_entity)
            .with_component(runtime_component),
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();

    let failed = spawner.replace(
        &mut world,
        &registry,
        &SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(2))]),
        &current,
    );

    assert!(failed.instance.is_none());
    assert_eq!(failed.retired_entities(), 0);
    assert!(failed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.identity-replacement-ineligible"
    }));
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(
        current.resolve(&world, &id),
        EntityLookup::Resolved(old_entity)
    );
    assert_eq!(world.get::<TestPosition>(old_entity).unwrap().x, 1);
}

#[test]
fn hierarchy_remove_observer_rejects_scene_replacement_before_detach() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id),
    ]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let mut spawner = SceneSpawner::new();
    let first = spawner.spawn(&mut world, &registry, &document);
    assert!(!first.diagnostics.has_errors());
    let current = spawned_instance(&first).clone();
    let old_child = spawned_entity(&world, &first, &child_id);
    world.entity_mut(old_child).observe(
        |_: On<Remove, Parent>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();

    let failed = spawner.replace(&mut world, &registry, &document, &current);

    assert!(failed.instance.is_none());
    assert_eq!(failed.retired_entities(), 0);
    assert!(failed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.identity-replacement-ineligible"
    }));
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert!(world.get::<Parent>(old_child).is_some());
}

#[test]
fn children_add_observer_rejects_fresh_scene_publication() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id),
        SceneEntityRecord::new(child_id).with_parent(scene_id("root")),
    ]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(
        |_: On<Add, Children>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world.iter_entities().count();
    let mut spawner = SceneSpawner::new();

    let failed = spawner.spawn(&mut world, &registry, &document);

    assert!(failed.instance.is_none());
    assert_eq!(failed.retired_entities(), 0);
    assert!(
        failed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.runtime-projection-ineligible"
        })
    );
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(world.iter_entities().count(), baseline_entities);
}

#[test]
fn children_add_observer_does_not_reject_a_flat_scene() {
    let registry = test_registry();
    let id = scene_id("flat");
    let document = SceneDocument::new([SceneEntityRecord::new(id.clone())]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    world.add_observer(
        |_: On<Add, Children>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let mut spawner = SceneSpawner::new();

    let report = spawner.spawn(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors());
    assert!(report.instance.is_some());
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert!(
        world
            .get_entity(spawned_entity(&world, &report, &id))
            .is_ok()
    );
}

#[test]
fn deferred_hierarchy_projection_rejects_every_materialization_entry_point() {
    let registry = deferred_transform_registry();
    let flat_id = scene_id("flat");
    let flat_document = SceneDocument::new([SceneEntityRecord::new(flat_id)
        .with_component(deferred_transform_type_id(), deferred_transform_record(1))]);
    assert!(!flat_document.validate(&registry).iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable"
    }));

    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone())
            .with_parent(root_id)
            .with_component(deferred_transform_type_id(), deferred_transform_record(3)),
    ]);
    let has_unavailable_projection = |diagnostics: &DiagnosticReport| {
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable"
                && diagnostic_has_text_field(diagnostic, "entity-id", child_id.as_str())
        })
    };

    assert!(has_unavailable_projection(&document.validate(&registry)));

    let mut direct_world = World::new();
    let direct_baseline = direct_world.iter_entities().count();
    let direct = SceneSpawner::new().spawn(&mut direct_world, &registry, &document);
    assert!(direct.instance.is_none());
    assert!(has_unavailable_projection(&direct.diagnostics));
    assert_eq!(direct_world.iter_entities().count(), direct_baseline);
    assert!(!direct_world.contains_resource::<WorldIdentityDomain>());

    let prefab = PrefabDocument::new(document.entities.clone());
    let mut prefab_world = World::new();
    let prefab_baseline = prefab_world.iter_entities().count();
    let prefab_report = SceneSpawner::new().spawn_prefab(&mut prefab_world, &registry, &prefab);
    assert!(prefab_report.instance.is_none());
    assert!(has_unavailable_projection(&prefab_report.diagnostics));
    assert_eq!(prefab_world.iter_entities().count(), prefab_baseline);
    assert!(!prefab_world.contains_resource::<WorldIdentityDomain>());

    let current_id = scene_id("current");
    let current_document = SceneDocument::new([SceneEntityRecord::new(current_id.clone())]);
    let mut replacement_world = World::new();
    let mut replacement_spawner = SceneSpawner::new();
    let current = replacement_spawner.spawn(&mut replacement_world, &registry, &current_document);
    assert!(!current.diagnostics.has_errors());
    let current_instance = spawned_instance(&current).clone();
    let current_entity = spawned_entity(&replacement_world, &current, &current_id);
    let baseline_entities = replacement_world.iter_entities().count();
    let replacement = replacement_spawner.replace(
        &mut replacement_world,
        &registry,
        &document,
        &current_instance,
    );
    assert!(replacement.instance.is_none());
    assert!(has_unavailable_projection(&replacement.diagnostics));
    assert_eq!(replacement_world.iter_entities().count(), baseline_entities);
    assert_eq!(
        current_instance.resolve(&replacement_world, &current_id),
        EntityLookup::Resolved(current_entity)
    );

    let mut authoring_world = World::new();
    let authoring_baseline = authoring_world.iter_entities().count();
    let mut session = SceneAuthoringSession::new(document);
    let authoring = session.sync_world(&mut authoring_world, &registry);
    assert!(!authoring.synced);
    assert!(authoring.live_instance.is_none());
    assert!(has_unavailable_projection(&authoring.diagnostics));
    assert_eq!(authoring_world.iter_entities().count(), authoring_baseline);
    assert!(!authoring_world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn inherited_visibility_is_rejected_by_the_shared_scene_preflight() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    let flat_document = SceneDocument::new([SceneEntityRecord::new(scene_id("flat"))
        .with_component(
            ComponentTypeId::new("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("hidden".to_owned()),
            ),
        )]);
    assert!(!flat_document.validate(&registry).iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable"
    }));

    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()).with_component(
            ComponentTypeId::new("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("hidden".to_owned()),
            ),
        ),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id),
    ]);

    let diagnostics = document.validate(&registry);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable"
            && diagnostic_has_text_field(diagnostic, "entity-id", child_id.as_str())
    }));
}

#[test]
fn children_remove_observer_rejects_scene_replacement_before_detach() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id.clone()),
    ]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let mut spawner = SceneSpawner::new();
    let first = spawner.spawn(&mut world, &registry, &document);
    assert!(!first.diagnostics.has_errors());
    let current = spawned_instance(&first).clone();
    let old_root = spawned_entity(&world, &first, &root_id);
    let old_child = spawned_entity(&world, &first, &child_id);
    world.entity_mut(old_root).observe(
        |_: On<Remove, Children>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();

    let failed = spawner.replace(&mut world, &registry, &document, &current);

    assert!(failed.instance.is_none());
    assert_eq!(failed.retired_entities(), 0);
    assert!(failed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.identity-replacement-ineligible"
    }));
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(
        world.get::<Parent>(old_child).map(Parent::parent),
        Some(old_root)
    );
    assert_eq!(
        world.get::<Children>(old_root).map(Children::as_slice),
        Some([old_child].as_slice())
    );
}

#[test]
fn children_remove_observer_rejects_authoring_unload_before_detach() {
    let registry = test_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let mut session = SceneAuthoringSession::new(SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id.clone()),
    ]));
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    let live = sync.live_instance.unwrap();
    let root = match live.resolve(&world, &root_id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("authoring root did not resolve: {lookup:?}"),
    };
    let child = match live.resolve(&world, &child_id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("authoring child did not resolve: {lookup:?}"),
    };
    world.entity_mut(root).observe(
        |_: On<Remove, Children>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();
    let baseline_revision = session.revision();

    let clear = session.clear_live_world(&mut world);

    assert!(!clear.cleared);
    assert_eq!(clear.removed_entities, 0);
    assert_eq!(clear.live_instance.as_ref(), Some(&live));
    assert_eq!(session.live_instance(), Some(&live));
    assert_eq!(session.revision(), baseline_revision);
    assert!(clear.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.lifecycle-retirement-ineligible"
    }));
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(root));
    assert_eq!(
        world.get::<Children>(root).map(Children::as_slice),
        Some([child].as_slice())
    );
}

#[test]
fn replacement_relationship_rejection_precedes_asset_and_binding_publication() {
    let registry = test_asset_registry();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let current_document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone()).with_parent(root_id.clone()),
    ]);
    let replacement_document = SceneDocument::new([
        SceneEntityRecord::new(root_id.clone()),
        SceneEntityRecord::new(child_id.clone())
            .with_parent(root_id.clone())
            .with_component(
                asset_link_type_id(),
                asset_link_record(AssetRef::path("textures/generated.png").unwrap()),
            ),
    ]);
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();
    let mut spawner = SceneSpawner::new();
    let first = spawner.spawn(&mut world, &registry, &current_document);
    assert!(!first.diagnostics.has_errors());
    let current = spawned_instance(&first).clone();
    let old_root = spawned_entity(&world, &first, &root_id);
    let old_child = spawned_entity(&world, &first, &child_id);
    world.entity_mut(old_child).observe(
        |_: On<Remove, Parent>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();

    let failed = spawner.replace(&mut world, &registry, &replacement_document, &current);

    assert!(failed.instance.is_none());
    assert_eq!(failed.retired_entities(), 0);
    assert!(failed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.identity-replacement-ineligible"
    }));
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
    assert!(!world.contains_resource::<AssetServer>());
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(
        world.get::<Parent>(old_child).map(Parent::parent),
        Some(old_root)
    );

    let alternate_registry = alternate_asset_registry();
    let alternate_id = scene_id("alternate");
    let alternate_document = SceneDocument::new([SceneEntityRecord::new(alternate_id.clone())
        .with_component(
            asset_link_type_id(),
            asset_link_record(AssetRef::path("textures/alternate.png").unwrap()),
        )]);
    let alternate = SceneSpawner::new().spawn(&mut world, &alternate_registry, &alternate_document);
    assert!(!alternate.diagnostics.has_errors());
    let alternate_entity = spawned_entity(&world, &alternate, &alternate_id);
    assert!(
        world
            .get::<TestAlternateAssetLink>(alternate_entity)
            .is_some()
    );
}

#[test]
fn entity_component_observer_rejects_authoring_replacement_before_retirement() {
    let registry = test_registry();
    let id = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]));
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();

    let first_sync = session.sync_world(&mut world, &registry);
    assert!(first_sync.synced);
    let first = first_sync.live_instance.unwrap();
    let first_entity = match first.resolve(&world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("first authoring instance did not resolve: {lookup:?}"),
    };
    world.entity_mut(first_entity).observe(
        |_: On<Remove, TestPosition>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();

    session
        .replace_document(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(2))]));
    let baseline_revision = session.revision();
    let failed = session.sync_world(&mut world, &registry);

    assert!(!failed.synced);
    assert!(failed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(
                diagnostic,
                "persistent-apply-reason",
                "lifecycle-observer",
            )
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "remove")
            && diagnostic_has_text_field(diagnostic, "observer-scope", "entity-component")
    }));
    assert_eq!(failed.removed_entities, 0);
    assert_eq!(failed.live_instance.as_ref(), Some(&first));
    assert_eq!(session.live_instance(), Some(&first));
    assert_eq!(session.revision(), baseline_revision);
    assert!(session.is_live_dirty());
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(
        first.resolve(&world, &id),
        EntityLookup::Resolved(first_entity)
    );
    assert_eq!(world.get::<TestPosition>(first_entity).unwrap().x, 1);
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
}

#[test]
fn entity_component_observer_rejects_authoring_clear_before_retirement() {
    let registry = test_registry();
    let id = scene_id("player");
    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(position_type_id(), position_record(1))]));
    let mut world = World::new();
    world.init_resource::<PersistentApplyCanary>();

    let sync = session.sync_world(&mut world, &registry);
    assert!(sync.synced);
    let live = sync.live_instance.unwrap();
    let entity = match live.resolve(&world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("authoring instance did not resolve: {lookup:?}"),
    };
    world.entity_mut(entity).observe(
        |_: On<Remove, TestPosition>, mut canary: ResMut<PersistentApplyCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let baseline_stats = world.resource::<WorldIdentityDomain>().stats();
    let baseline_revision = session.revision();

    let clear = session.clear_live_world(&mut world);

    assert!(!clear.cleared);
    assert_eq!(clear.removed_entities, 0);
    assert_eq!(clear.live_instance.as_ref(), Some(&live));
    assert_eq!(session.live_instance(), Some(&live));
    assert_eq!(session.revision(), baseline_revision);
    assert!(clear.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
            && diagnostic_has_text_field(diagnostic, "lifecycle-event", "remove")
            && diagnostic_has_text_field(diagnostic, "observer-scope", "entity-component")
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        baseline_stats
    );
    assert_eq!(world.get::<TestPosition>(entity).unwrap().x, 1);
    assert_eq!(world.resource::<PersistentApplyCanary>().0, 0);
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
fn spawns_hierarchy_and_keeps_runtime_topology_out_of_export() {
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

    assert!(export.output().is_none());
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-runtime-topology-unsupported"
    }));
}

#[test]
fn export_rejects_a_building_component_registry() {
    let frozen_registry = test_registry();
    let entity_id = scene_id("entity");
    let document =
        SceneDocument::new([SceneEntityRecord::new(entity_id)
            .with_component(position_type_id(), position_record(1))]);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &frozen_registry, &document);
    assert!(!report.diagnostics.has_errors());
    let mut building_registry = ComponentRegistry::new();
    register_test_position(&mut building_registry).unwrap();

    let export = export_scene(&world, &building_registry);

    assert!(export.output().is_none());
    assert!(
        export.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.component-registry-not-frozen"
        })
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
        field: ComponentFieldId::new("x"),
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
        field: ComponentFieldId::new("x"),
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
fn unpublished_patch_path_returns_no_inverse_and_canonicalizes_empty_patch() {
    let registry = test_registry();
    let mut document = SceneDocument {
        entities: vec![
            SceneEntityRecord::new(scene_id("z")),
            SceneEntityRecord::new(scene_id("a")),
        ],
    };

    let report =
        ScenePatchDocument::default().apply_owned_to_unpublished_scene(&mut document, &registry);

    assert!(report.applied);
    assert!(report.inverse.is_none());
    assert!(!report.diagnostics.has_errors());
    assert_eq!(
        document
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
}

#[test]
fn owned_unpublished_patch_moves_component_payload_into_candidate() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    let entity_id = scene_id("named");
    let payload = "owned-prefab-override-payload".repeat(128);
    let payload_pointer = payload.as_ptr();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: entity_id.clone(),
        component: ComponentTypeId::new("nara.scene.Name"),
        value: SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(payload),
        ),
    }]);
    let mut document = SceneDocument::new([SceneEntityRecord::new(entity_id)]);

    let report = patch.apply_owned_to_unpublished_scene(&mut document, &registry);

    assert!(report.applied);
    assert!(report.inverse.is_none());
    let stored = document.entities[0]
        .components
        .get(&ComponentTypeId::new("nara.scene.Name"))
        .unwrap()
        .value
        .as_str()
        .unwrap();
    assert_eq!(stored.as_ptr(), payload_pointer);
}

#[test]
fn unpublished_patch_failure_isolated_from_public_atomic_patch_contract() {
    let registry = test_registry();
    let source = SceneDocument::new([SceneEntityRecord::new(scene_id("enemy"))
        .with_component(position_type_id(), position_record(1))]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::SetField {
            entity: scene_id("enemy"),
            component: position_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(9),
        },
        ScenePatchOperation::SetField {
            entity: scene_id("missing"),
            component: position_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(12),
        },
    ]);
    let public_patch = patch.clone();
    let mut unpublished = source.clone();

    let unpublished_report = patch.apply_owned_to_unpublished_scene(&mut unpublished, &registry);

    assert!(!unpublished_report.applied);
    assert!(unpublished_report.inverse.is_none());
    assert_eq!(
        unpublished.entities[0]
            .components
            .get(&position_type_id())
            .unwrap()
            .value
            .field_i64("x")
            .unwrap(),
        9
    );

    let mut published = source;
    let published_report = public_patch.apply_to_scene(&mut published, &registry);
    assert!(!published_report.applied);
    assert_eq!(
        published.entities[0]
            .components
            .get(&position_type_id())
            .unwrap()
            .value
            .field_i64("x")
            .unwrap(),
        1
    );
}

#[test]
fn owned_unpublished_patch_matches_public_apply_for_supported_operations() {
    let registry = test_registry();
    let source = SceneDocument::new([
        SceneEntityRecord::new(scene_id("root"))
            .with_component(position_type_id(), position_record(1)),
        SceneEntityRecord::new(scene_id("editable"))
            .with_component(position_type_id(), position_record(2)),
        SceneEntityRecord::new(scene_id("empty")),
        SceneEntityRecord::new(scene_id("doomed"))
            .with_component(position_type_id(), position_record(7)),
        SceneEntityRecord::new(scene_id("doomed-child")).with_parent(scene_id("doomed")),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(scene_id("added"))
                .with_component(position_type_id(), position_record(3)),
        },
        ScenePatchOperation::AddComponent {
            entity: scene_id("empty"),
            component: position_type_id(),
            value: position_record(4),
        },
        ScenePatchOperation::ReplaceComponent {
            entity: scene_id("editable"),
            component: position_type_id(),
            value: position_record(5),
        },
        ScenePatchOperation::SetField {
            entity: scene_id("root"),
            component: position_type_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(6),
        },
        ScenePatchOperation::Reparent {
            entity: scene_id("empty"),
            parent: Some(scene_id("root")),
        },
        ScenePatchOperation::RemoveComponent {
            entity: scene_id("empty"),
            component: position_type_id(),
        },
        ScenePatchOperation::RemoveEntity {
            entity: scene_id("doomed"),
        },
    ]);
    let mut published = source.clone();
    let mut unpublished = source;

    let public_report = patch.apply_to_scene(&mut published, &registry);
    let owned_report = patch.apply_owned_to_unpublished_scene(&mut unpublished, &registry);

    assert!(public_report.applied);
    assert!(public_report.inverse.is_some());
    assert!(owned_report.applied);
    assert!(owned_report.inverse.is_none());
    assert_eq!(unpublished, published);
}

#[test]
fn failed_nested_prefab_override_does_not_poison_a_sibling_source_expansion() {
    let registry = test_registry();
    let inner_source = AssetRef::path("prefabs/inner.ron").unwrap();
    let outer_source = AssetRef::path("prefabs/outer.ron").unwrap();
    let invalid_override = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("missing"),
        component: position_type_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("x"),
        value: ComponentValue::I64(9),
    }]);
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            inner_source.clone(),
            PrefabDocument::new([SceneEntityRecord::new(scene_id("target"))]),
        )
        .with_prefab(
            outer_source.clone(),
            PrefabDocument::new([
                prefab_anchor_with_overrides("bad", inner_source.clone(), invalid_override),
                prefab_anchor("good", inner_source),
            ]),
        );
    let document = SceneDocument::new([prefab_anchor("root", outer_source)]);

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(expansion.document.is_none());
    assert!(
        expansion
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.patch-missing-entity" })
    );
    assert!(
        !expansion
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-cycle" })
    );
}

#[test]
fn export_rejects_runtime_parent_instead_of_inventing_persistent_topology() {
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
    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap();

    let export = export_scene(&world, &registry);
    assert!(export.output().is_none());
    assert_eq!(world.get::<Parent>(child).unwrap().parent(), parent);
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.export-runtime-topology-unsupported"
    }));
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
    let error = SceneDocumentCandidate::decode_json_str(
        r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"entities":[{"id":"root/../player","components":{}}]}}"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains(".."));
}

#[cfg(feature = "serde")]
#[test]
fn scene_json_rejects_unknown_document_entity_and_component_fields() {
    let document_error = SceneDocumentCandidate::decode_json_str(
        r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"entities":[],"unexpected":true}}"#,
    )
    .unwrap_err();
    assert!(document_error.to_string().contains("unknown field"));

    let entity_error = SceneDocumentCandidate::decode_json_str(
        r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"entities":[{"id":"player","components":{},"unexpected":true}]}}"#,
    )
    .unwrap_err();
    assert!(entity_error.to_string().contains("unknown field"));

    let component_error = SceneDocumentCandidate::decode_json_str(
        r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"entities":[{"id":"player","components":{"nara.test.Position":{"version":1,"value":{"type":"null"},"unexpected":true}}}]}}"#,
    )
    .unwrap_err();
    assert!(component_error.to_string().contains("unknown field"));
}

#[cfg(feature = "serde")]
#[test]
fn prefab_json_rejects_unknown_fields() {
    let prefab_error = PrefabDocumentCandidate::decode_json_str(
        r#"{"kind":"prefab","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"entities":[],"unexpected":true}}"#,
    )
    .unwrap_err();

    assert!(prefab_error.to_string().contains("unknown field"));
}

#[cfg(feature = "serde")]
#[test]
fn patch_json_rejects_unknown_document_and_operation_fields() {
    let document_error = ScenePatchDocumentCandidate::decode_json_str(
        r#"{"kind":"scene_patch","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"format_version":1,"operations":[],"unexpected":true}}"#,
    )
    .unwrap_err();
    assert!(document_error.to_string().contains("unknown field"));

    let operation_error = ScenePatchDocumentCandidate::decode_json_str(
        r#"{"kind":"scene_patch","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"format_version":1,"operations":[{"op":"remove_entity","args":{"entity":"player","unexpected":true}}]}}"#,
    )
    .unwrap_err();
    assert!(operation_error.to_string().contains("unknown field"));
}

#[test]
fn scene_component_schemas_expose_scalar_fields() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).expect("component registration should succeed");
    registry.freeze().expect("component registry should freeze");

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

fn register_test_position(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    let schema = test_component_schema(
        position_type_id(),
        "Position",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "x",
            "X",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::I64,
            true,
        )],
    );
    registry.register_persistent_component_with_codec::<TestPosition, _, _>(
        schema,
        |value| {
            let x = value
                .get("x")
                .and_then(ComponentValue::as_i64)
                .ok_or_else(|| ComponentCodecError::invalid_field("x", "i64"))?;
            Ok(TestPosition {
                x: i32::try_from(x).map_err(|_| ComponentCodecError::invalid_field("x", "i32"))?,
            })
        },
        |position| {
            Ok(ComponentValue::map([(
                "x",
                ComponentValue::I64(i64::from(position.x)),
            )]))
        },
    )?;
    Ok(())
}

fn test_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_test_position(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

fn deferred_transform_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DeferredTransform, _, _>(
            test_component_schema(
                deferred_transform_type_id(),
                "Deferred transform",
                ComponentSchemaVersion::ONE,
                [test_component_field(
                    "value",
                    "Value",
                    ComponentFieldPath::from_fields(["value"]),
                    ComponentValueKind::I64,
                    true,
                )],
            ),
            |value| Ok(DeferredTransform(value.field_i64("value")?)),
            |transform| {
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::I64(transform.0),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn deferred_transform_record(value: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("value", ComponentValue::I64(value))]),
    )
}

fn deferred_transform_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.transform.Transform2d")
}

fn entity_link_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        entity_link_type_id(),
        "Entity link",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "target",
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
            true,
        )],
    );
    registry
        .register_persistent_component_with_codec::<TestEntityLink, _, _>(
            schema,
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
    registry.freeze().unwrap();
    registry
}

fn large_data_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        large_data_type_id(),
        "Large data",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "values",
            "Values",
            ComponentFieldPath::from_fields(["values"]),
            ComponentValueKind::List,
            true,
        )],
    );
    registry
        .register_persistent_component_with_codec::<TestLargeData, _, _>(
            schema,
            |value| {
                if matches!(value.field("values")?, ComponentValue::List(_)) {
                    Ok(TestLargeData)
                } else {
                    Err(ComponentCodecError::invalid_field("values", "list"))
                }
            },
            |_| {
                Ok(ComponentValue::map([(
                    "values",
                    ComponentValue::List(Vec::new()),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrated_entity_link_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        entity_link_type_id(),
        "Entity link",
        ComponentSchemaVersion(2),
        [test_component_field(
            "target",
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
            true,
        )],
    );
    registry
        .register_persistent_component_with_codec::<TestEntityLink, _, _>(
            schema,
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
        .unwrap()
        .register_component_migration(
            &entity_link_type_id(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            |value| {
                let ComponentValue::Map(mut fields) = value else {
                    return Err(ComponentCodecError::invalid_field("<root>", "map"));
                };
                let target = fields
                    .remove("old_target")
                    .ok_or_else(|| ComponentCodecError::missing_field("old_target"))?;
                fields.insert("target".to_string(), target);
                Ok(ComponentValue::Map(fields))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrated_position_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        position_type_id(),
        "Position",
        ComponentSchemaVersion(2),
        [test_component_field(
            "x",
            "X",
            ComponentFieldPath::from_fields(["x2"]),
            ComponentValueKind::I64,
            true,
        )],
    );
    registry
        .register_persistent_component_with_codec::<TestPosition, _, _>(
            schema,
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
    registry.freeze().unwrap();
    registry
}

fn test_asset_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        asset_link_type_id(),
        "Asset link",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "asset",
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
            true,
        )],
    );
    registry
        .register_persistent_component_codec_with_context::<TestAssetLink, _, _>(
            schema,
            |value, context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                let prepared = prepare_test_asset_handle(context, "asset.value", asset_ref)?;
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
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
                    },
                ))
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
    registry.freeze().unwrap();
    registry
}

fn alternate_asset_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        asset_link_type_id(),
        "Alternate asset link",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "asset",
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
            true,
        )],
    );
    registry
        .register_persistent_component_codec::<TestAlternateAssetLink, _, _>(
            schema,
            |value| {
                let _ = read_asset_ref(value.field("asset")?, "asset")?;
                Ok(PreparedComponentCandidate::insert(TestAlternateAssetLink))
            },
            |_world, _entity| Ok(None),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn deferred_asset_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        asset_link_type_id(),
        "Deferred asset link",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "asset",
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
            true,
        )],
    );
    registry
        .register_persistent_component_codec_with_context::<TestAssetLink, _, _>(
            schema,
            |value, _context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        let handle = context.resolve_asset_ref::<TestAsset>(&asset_ref).map_err(
                            |error| {
                                ComponentCodecError::invalid_asset_ref(
                                    "asset.value",
                                    asset_ref.to_string(),
                                    error.to_string(),
                                )
                            },
                        )?;
                        Ok(TestAssetLink { handle })
                    },
                ))
            },
            |_world, _entity, _context| Ok(None),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn broken_export_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = test_component_schema(
        broken_export_type_id(),
        "Broken export",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "broken",
            "Broken",
            ComponentFieldPath::from_fields(["broken"]),
            ComponentValueKind::String,
            true,
        )],
    );
    registry
        .register_persistent_component_codec_with_context::<TestBrokenExport, _, _>(
            schema,
            |_value, _context| Ok(PreparedComponentCandidate::insert(TestBrokenExport)),
            |_world, _entity, _context| Err(ComponentCodecError::invalid_field("broken", "boom")),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn failing_apply_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_test_position(&mut registry).unwrap();
    let resolves_asset_schema = test_component_schema(
        apply_resolves_asset_type_id(),
        "Apply resolves asset",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "asset",
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
            true,
        )],
    );
    registry
        .register_persistent_component_codec_with_context::<TestAssetLink, _, _>(
            resolves_asset_schema,
            |value, _context| {
                let asset_ref = read_asset_ref(value.field("asset")?, "asset")?;
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        let handle = context.resolve_asset_ref::<TestAsset>(&asset_ref).map_err(
                            |error| {
                                ComponentCodecError::invalid_asset_ref(
                                    "asset.value",
                                    asset_ref.to_string(),
                                    error.to_string(),
                                )
                            },
                        )?;
                        Ok(TestAssetLink { handle })
                    },
                ))
            },
            |_world, _entity, _context| Ok(None),
        )
        .unwrap();
    let apply_fails_schema = test_component_schema(
        apply_fails_type_id(),
        "Apply fails",
        ComponentSchemaVersion::ONE,
        [test_component_field(
            "asset",
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
            false,
        )],
    );
    registry
        .register_persistent_component_codec_with_context::<TestApplyFails, _, _>(
            apply_fails_schema,
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

                Ok(PreparedComponentCandidate::deferred(|| {
                    Err::<TestApplyFails, _>(ComponentCodecError::invalid_field(
                        "apply",
                        "intentional failure",
                    ))
                }))
            },
            |_world, _entity, _context| Ok(None),
        )
        .unwrap();
    registry.freeze().unwrap();
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

fn large_data_type_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.LargeData")
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
