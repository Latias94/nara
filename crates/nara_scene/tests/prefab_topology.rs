use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::{AssetRef, AssetServer, ProjectAssetDatabase};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Component, World};
use nara_hierarchy::HierarchyRoots;
use nara_identity::WorldIdentityDomain;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, PrefabSourceResolver,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, spawn_scene, spawn_scene_with_prefab_resolver,
    spawn_scene_with_prefab_resolver_and_asset_database,
};

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn anchor(source: AssetRef) -> SceneEntityRecord {
    let mut anchor = SceneEntityRecord::new(scene_id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source,
        overrides: ScenePatchDocument::default(),
    });
    anchor
}

struct RawPrefabSource {
    source: AssetRef,
    document: PrefabDocument,
}

impl PrefabSourceResolver for RawPrefabSource {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        (source == &self.source).then_some(&self.document)
    }
}

#[derive(Component, Debug, PartialEq)]
struct ExistingValue(u32);

fn assert_bounded_errors(diagnostics: &DiagnosticReport, code: &str) {
    assert!(diagnostics.has_errors(), "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{diagnostics:?}"
    );
    assert!(diagnostics.retained_len() <= diagnostics.settings().entry_limit().get());
    assert!(diagnostics.retained_bytes() <= diagnostics.settings().byte_limit().get());
}

fn assert_rejected_without_mutation(
    document: &SceneDocument,
    resolver: &impl PrefabSourceResolver,
    code: &str,
) {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let database = ProjectAssetDatabase::default();
    let before_document = document.clone();

    let expansion = document.expand_prefabs(&registry, resolver);
    assert!(expansion.candidate.is_none());
    assert_bounded_errors(&expansion.diagnostics, code);
    let expansion = document.expand_prefabs_with_asset_database(&registry, resolver, &database);
    assert!(expansion.candidate.is_none());
    assert_bounded_errors(&expansion.diagnostics, code);

    for with_existing_identity in [false, true] {
        let mut world = World::new();
        let sentinel = world.spawn(ExistingValue(73)).id();
        if with_existing_identity {
            let existing =
                SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("existing"))]);
            let report = spawn_scene(&mut world, &registry, &existing);
            assert!(report.instance.is_some(), "{:?}", report.diagnostics);
        }
        world.increment_change_tick();
        let before_entities = world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>();
        let before_identity = world
            .get_resource::<WorldIdentityDomain>()
            .map(|domain| domain.stats());
        let before_ticks = world
            .get_resource_change_ticks::<WorldIdentityDomain>()
            .map(|ticks| (ticks.added, ticks.changed));
        let before_roots = world
            .get_resource::<HierarchyRoots>()
            .map(|roots| roots.as_slice().to_vec());

        for with_database in [false, true] {
            let report = if with_database {
                spawn_scene_with_prefab_resolver_and_asset_database(
                    &mut world, &registry, document, resolver, &database,
                )
            } else {
                spawn_scene_with_prefab_resolver(&mut world, &registry, document, resolver)
            };
            assert!(report.instance.is_none());
            assert_bounded_errors(&report.diagnostics, code);
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<Vec<_>>(),
                before_entities
            );
            assert_eq!(
                world.get::<ExistingValue>(sentinel),
                Some(&ExistingValue(73))
            );
            assert_eq!(
                world
                    .get_resource::<WorldIdentityDomain>()
                    .map(|domain| domain.stats()),
                before_identity
            );
            assert_eq!(
                world
                    .get_resource_change_ticks::<WorldIdentityDomain>()
                    .map(|ticks| (ticks.added, ticks.changed)),
                before_ticks
            );
            assert_eq!(
                world
                    .get_resource::<HierarchyRoots>()
                    .map(|roots| roots.as_slice().to_vec()),
                before_roots
            );
            assert!(!world.contains_resource::<AssetServer>());
        }
    }
    assert_eq!(document, &before_document);
}

fn assert_raw_source_rejected(prefab: PrefabDocument, code: &str) {
    let source = AssetRef::path("prefabs/raw.prefab.ron").unwrap();
    let document = SceneDocument::from_ordered_roots([anchor(source.clone())]);
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    assert_rejected_without_mutation(&document, &resolver, code);
    assert_eq!(resolver.resolve_prefab(&source), Some(&prefab));

    let resolver = RawPrefabSource {
        source,
        document: prefab.clone(),
    };
    assert_rejected_without_mutation(&document, &resolver, code);
    assert_eq!(resolver.document, prefab);
}

#[test]
fn raw_prefab_dangling_root_rejects_expansion_and_spawn_without_mutation() {
    assert_raw_source_rejected(
        PrefabDocument::new([scene_id("missing")], []),
        "scene.dangling-root",
    );
}

#[test]
fn raw_prefab_dangling_child_rejects_expansion_and_spawn_without_mutation() {
    assert_raw_source_rejected(
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(scene_id("root")).with_children([scene_id("missing")])
        ]),
        "scene.dangling-child",
    );
}

#[test]
fn raw_prefab_duplicate_id_rejects_expansion_and_spawn_without_mutation() {
    assert_raw_source_rejected(
        PrefabDocument::new(
            [scene_id("root")],
            [
                SceneEntityRecord::new(scene_id("root")),
                SceneEntityRecord::new(scene_id("root")),
            ],
        ),
        "scene.duplicate-entity-id",
    );
}

#[test]
fn raw_prefab_placement_cycle_rejects_expansion_and_spawn_without_mutation() {
    assert_raw_source_rejected(
        PrefabDocument::new(
            [],
            [
                SceneEntityRecord::new(scene_id("first")).with_children([scene_id("second")]),
                SceneEntityRecord::new(scene_id("second")).with_children([scene_id("first")]),
            ],
        ),
        "scene.placement-cycle",
    );
}

#[test]
fn nested_raw_prefab_is_checked_before_projection() {
    let outer = AssetRef::path("prefabs/outer.prefab.ron").unwrap();
    let inner = AssetRef::path("prefabs/inner.prefab.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            outer.clone(),
            PrefabDocument::from_ordered_roots([anchor(inner.clone())]),
        )
        .with_prefab(inner, PrefabDocument::new([scene_id("missing")], []));
    let document = SceneDocument::from_ordered_roots([anchor(outer)]);
    assert_rejected_without_mutation(&document, &resolver, "scene.dangling-root");
}

#[test]
fn raw_prefab_topology_diagnostics_preserve_bounded_loss_accounting() {
    let source = AssetRef::path("prefabs/many-missing.prefab.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::new(
            (0..1_024).map(|index| scene_id(&format!("missing_{index}"))),
            [],
        ),
    );
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let document = SceneDocument::from_ordered_roots([anchor(source)]);
    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(expansion.candidate.is_none());
    assert_bounded_errors(&expansion.diagnostics, "scene.dangling-root");
    assert_eq!(expansion.diagnostics.stats().observed_errors(), 1_024);
    assert!(expansion.diagnostics.stats().evicted_entries() > 0);
}
#[derive(Component)]
struct DefaultAmount(i64);

#[test]
fn raw_prefab_topology_admission_preserves_defaults_and_overrides_without_extra_native_decode() {
    let component_id = ComponentTypeId::new("nara.test.DefaultAmount");
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(ComponentValue::I64(5));
    let schema = ComponentSchema::new(
        component_id.clone(),
        "Default amount",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let decode_count = Arc::new(AtomicUsize::new(0));
    let decode_probe = Arc::clone(&decode_count);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DefaultAmount, _, _>(
            schema,
            move |value| {
                decode_probe.fetch_add(1, Ordering::Relaxed);
                Ok(DefaultAmount(value.field_i64("amount")?))
            },
            |amount| {
                Ok(ComponentValue::map([(
                    "amount",
                    ComponentValue::I64(amount.0),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root"))
        .with_component(
            component_id.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(Default::default()),
            ),
        )]);
    let source = AssetRef::path("prefabs/defaulted.prefab.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    let mut document = SceneDocument::from_ordered_roots([anchor(source.clone())]);

    let expansion = document.expand_prefabs(&registry, &resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    assert_eq!(decode_count.load(Ordering::Relaxed), 1);
    let expanded = expansion.document().unwrap();
    let component = expanded
        .entities
        .iter()
        .find_map(|entity| entity.components.get(&component_id))
        .unwrap();
    assert_eq!(component.value.field_i64("amount").unwrap(), 5);

    document.entities[0].prefab.as_mut().unwrap().overrides =
        ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: scene_id("root"),
            component: component_id.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("amount"),
            value: ComponentValue::I64(9),
        }]);
    let expansion = document.expand_prefabs(&registry, &resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    let expanded = expansion.document().unwrap();
    let component = expanded
        .entities
        .iter()
        .find_map(|entity| entity.components.get(&component_id))
        .unwrap();
    assert_eq!(component.value.field_i64("amount").unwrap(), 9);
    assert_eq!(resolver.resolve_prefab(&source), Some(&prefab));
    assert!(
        prefab.entities[0].components[&component_id]
            .value
            .get("amount")
            .is_none()
    );
}
