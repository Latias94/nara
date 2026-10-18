use std::collections::BTreeSet;

use nara_asset::{AssetRef, AssetRefExportPolicy};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{Component, World};
use nara_identity::{EntityLookup, EntityReference, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, export_scene, spawn_scene,
};
#[cfg(feature = "serde")]
use nara_scene::{SceneDocumentCandidate, SceneFileLimits};

#[derive(Debug, PartialEq, Component)]
struct DefaultLink {
    target: EntityReference,
}

fn component_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.DefaultLink")
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn local_target() -> EntityReference {
    EntityReference::SceneLocal {
        entity: scene_id("target"),
    }
}

fn registry(omit_default_on_encode: bool) -> ComponentRegistry {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    field.default_value = Some(ComponentValue::EntityReference(local_target()));
    let schema = ComponentSchema::new(component_id(), "Default link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<DefaultLink, _, _>(
            schema,
            |value| {
                Ok(DefaultLink {
                    target: value.field_entity_reference("target")?.clone(),
                })
            },
            move |link| {
                if omit_default_on_encode && link.target == local_target() {
                    Ok(ComponentValue::Map(Default::default()))
                } else {
                    Ok(ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(link.target.clone()),
                    )]))
                }
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn linked_source(omit_target: bool) -> SceneEntityRecord {
    let value = if omit_target {
        ComponentValue::Map(Default::default())
    } else {
        ComponentValue::map([("target", ComponentValue::EntityReference(local_target()))])
    };
    SceneEntityRecord::new(scene_id("source")).with_component(
        component_id(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )
}

fn linked_scene(omit_target: bool) -> SceneDocument {
    SceneDocument::from_ordered_roots([
        linked_source(omit_target),
        SceneEntityRecord::new(scene_id("target")),
    ])
}

#[test]
fn scene_spawn_materializes_default_reference_without_rewriting_source() {
    let registry = registry(false);
    let document = linked_scene(true);
    let source = document.clone();
    let mut world = World::new();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report.instance.unwrap();
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &scene_id("source")) else {
        panic!("the admitted scene must expose its source entity");
    };
    assert_eq!(
        world.get::<DefaultLink>(entity).unwrap().target,
        local_target(),
    );
    assert_eq!(document, source);
    assert!(
        document.entities[0].components[&component_id()]
            .value
            .get("target")
            .is_none()
    );
}

#[test]
fn scene_spawn_rejects_missing_local_target_before_world_mutation() {
    let registry = registry(false);
    for omit_target in [false, true] {
        let document = SceneDocument::from_ordered_roots([linked_source(omit_target)]);
        let source = document.clone();
        let mut world = World::new();
        let canary = world.spawn_empty().id();
        let before_entities = world.iter_entities().count();

        let report = spawn_scene(&mut world, &registry, &document);

        assert!(
            report.diagnostics.has_errors(),
            "missing effective local targets must reject, omission={omit_target}",
        );
        assert!(report.instance.is_none());
        eprintln!(
            "omit={omit_target} diagnostics={:?} entities={:?}",
            report.diagnostics,
            world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<Vec<_>>()
        );
        assert_eq!(world.iter_entities().count(), before_entities);
        assert!(world.get_entity(canary).is_ok());
        assert!(!world.contains_resource::<WorldIdentityDomain>());
        assert_eq!(document, source);
    }
}

#[test]
fn repeated_prefabs_remap_default_local_reference_and_preserve_source_omission() {
    let registry = registry(false);
    let source = AssetRef::path("prefabs/default-link.ron").unwrap();
    let prefab = PrefabDocument::from_ordered_roots([
        linked_source(true),
        SceneEntityRecord::new(scene_id("target")),
    ]);
    let original_prefab = prefab.clone();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), prefab.clone());
    let document = SceneDocument::from_ordered_roots(["left", "right"].map(|anchor| {
        let mut entity = SceneEntityRecord::new(scene_id(anchor));
        entity.prefab = Some(PrefabInstance {
            source: source.clone(),
            overrides: ScenePatchDocument::default(),
        });
        entity
    }));

    let expansion = document.expand_prefabs(&registry, &resolver);

    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics,
    );
    let expanded = expansion.document.as_ref().unwrap();
    let mut mapped_targets = BTreeSet::new();
    for anchor in ["left", "right"] {
        let projected = |source_entity: &str| {
            expansion
                .provenance
                .iter()
                .find(|(_, provenance)| {
                    provenance.projection_path().is_some_and(|path| {
                        path.root_anchor().as_str() == anchor
                            && path.source_chain().len() == 1
                            && path.source_chain()[0].source_entity().as_str() == source_entity
                    })
                })
                .map(|(entity, _)| entity)
                .unwrap()
        };
        let source_entity = projected("source");
        let target_entity = projected("target");
        let record = expanded
            .entities
            .iter()
            .find(|entity| &entity.id == source_entity)
            .unwrap();
        assert_eq!(
            record.components[&component_id()]
                .value
                .field_entity_reference("target")
                .unwrap(),
            &EntityReference::SceneLocal {
                entity: target_entity.clone(),
            },
        );
        mapped_targets.insert(target_entity.clone());
    }
    assert_eq!(mapped_targets.len(), 2);
    let mut world = World::new();
    let report = spawn_scene(&mut world, &registry, expanded);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(prefab, original_prefab);
    assert!(
        prefab.entities[0].components[&component_id()]
            .value
            .get("target")
            .is_none()
    );
}

#[test]
fn export_materializes_omitted_default_reference_before_instance_remap() {
    let registry = registry(true);
    let document = linked_scene(false);
    let mut world = World::new();
    let mut instances = Vec::new();
    for _instance in 0..2 {
        let report = spawn_scene(&mut world, &registry, &document);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        instances.push(report.instance.unwrap());
    }

    let export = export_scene(&world, &registry);

    assert!(!export.diagnostics.has_errors(), "{:?}", export.diagnostics);
    let output = export.output().unwrap();
    let mut mapped_targets = BTreeSet::new();
    for instance in instances {
        let source = output
            .remap
            .get(&instance.locator(&scene_id("source")).unwrap())
            .unwrap();
        let target = output
            .remap
            .get(&instance.locator(&scene_id("target")).unwrap())
            .unwrap();
        let source_record = output
            .document
            .entities
            .iter()
            .find(|entity| &entity.id == source)
            .unwrap();
        assert_eq!(
            source_record.components[&component_id()]
                .value
                .field_entity_reference("target")
                .unwrap(),
            &EntityReference::SceneLocal {
                entity: target.clone(),
            },
        );
        assert_ne!(source, target);
        assert!(
            output
                .document
                .entities
                .iter()
                .any(|entity| &entity.id == target && entity.components.is_empty())
        );
        mapped_targets.insert(target.clone());
    }
    assert_eq!(mapped_targets.len(), 2);
}

#[test]
fn export_aggregate_value_budget_has_exact_boundary_and_preserves_world() {
    let registry = registry(true);
    let document = linked_scene(true);
    let mut world = World::new();
    for _instance in 0..2 {
        let report = spawn_scene(&mut world, &registry, &document);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    }
    let before_entities = world.iter_entities().count();
    let exact_limits = nara_reflect::ComponentValueMaterializationLimits::new(
        ItemLimit::new(4).unwrap(),
        ByteLimit::new(28).unwrap(),
        DepthLimit::new(3).unwrap(),
    );
    let exact = export_scene_with_limits(&world, &registry, exact_limits);
    assert!(!exact.diagnostics.has_errors(), "{:?}", exact.diagnostics);
    assert!(exact.output().is_some());
    assert_eq!(world.iter_entities().count(), before_entities);

    let below_bytes = nara_reflect::ComponentValueMaterializationLimits::new(
        ItemLimit::new(4).unwrap(),
        ByteLimit::new(27).unwrap(),
        DepthLimit::new(3).unwrap(),
    );
    let rejected = export_scene_with_limits(&world, &registry, below_bytes);
    assert!(rejected.output().is_none());
    assert!(
        rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.export-value-budget-exceeded"
        })
    );
    assert_eq!(world.iter_entities().count(), before_entities);
}

fn export_scene_with_limits(
    world: &World,
    registry: &ComponentRegistry,
    value_limits: nara_reflect::ComponentValueMaterializationLimits,
) -> nara_scene::SceneExportReport {
    nara_scene::export_scene_with_options(
        world,
        registry,
        nara_scene::SceneExportOptions {
            asset_ref_export_policy: AssetRefExportPolicy::Path,
            value_limits,
        },
    )
}

#[cfg(feature = "serde")]
#[test]
fn scene_file_value_limits_survive_canonical_publish() {
    let registry = registry(false);
    let document = linked_scene(true);
    let bytes = document.to_json_string().unwrap();
    let tight = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(1).unwrap())
        .with_component_value_bytes(ByteLimit::new(64).unwrap());
    let candidate = SceneDocumentCandidate::decode_json_bytes_with_limits(bytes.as_bytes(), tight)
        .unwrap()
        .canonicalize(&registry)
        .unwrap();
    assert!(candidate.publish(&registry).is_err());

    let wide = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(2).unwrap())
        .with_component_value_bytes(ByteLimit::new(64).unwrap());
    let published = SceneDocumentCandidate::decode_json_bytes_with_limits(bytes.as_bytes(), wide)
        .unwrap()
        .canonicalize(&registry)
        .unwrap()
        .publish(&registry);
    assert!(published.is_ok());
}
