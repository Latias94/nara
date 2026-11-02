#[path = "support/compiled_scene.rs"]
mod compiled_scene;

#[path = "support/prefab_source.rs"]
mod prefab_source;

use std::collections::BTreeSet;

use nara_asset::{AssetRef, AssetRefExportPolicy};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{Component, World};
use nara_identity::{EntityLookup, EntityReference, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, export_scene,
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

fn registry(omit_default_on_encode: bool) -> ComponentRegistrySnapshot {
    registry_with_extra_fields(omit_default_on_encode, [])
}

fn registry_with_extra_fields(
    omit_default_on_encode: bool,
    fields: impl IntoIterator<Item = ComponentFieldSchema>,
) -> ComponentRegistrySnapshot {
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
        .with_fields([field])
        .with_fields(fields);
    let mut registry = nara_reflect::ComponentRegistryBuilder::new();
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
    registry.freeze().unwrap()
}

fn linked_source(omit_target: bool) -> SceneEntityRecord {
    let value = if omit_target {
        ComponentValue::Map(Default::default())
    } else {
        ComponentValue::map([("target", ComponentValue::EntityReference(local_target()))])
    };
    SceneEntityRecord::new(scene_id("source")).with_component(
        component_id(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_contextual_default(
            ComponentFieldId::new("target"),
            ComponentValue::EntityReference(local_target()),
        ),
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

    let report = nara_scene::spawn_scene(
        &mut world,
        &registry,
        &compiled_scene::native(
            &registry,
            &std::sync::Arc::new((document).clone()),
            scene_source_key(
                "scene_spawn_materializes_default_reference_without_rewriting_source/document",
            ),
            None,
        ),
        &scene_occurrence(
            "scene_spawn_materializes_default_reference_without_rewriting_source/report",
        ),
        None,
    );

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
fn scene_compilation_rejects_missing_local_target_before_world_mutation() {
    let registry = registry(false);
    for omit_target in [false, true] {
        let document = SceneDocument::from_ordered_roots([linked_source(omit_target)]);
        let source = document.clone();
        let mut world = World::new();
        let canary = world.spawn_empty().id();
        let before_entities = world.iter_entities().count();

        let report = document.compile(&registry, &nara_scene::InMemoryPrefabSourceResolver::new());

        assert!(
            report.diagnostics.has_errors(),
            "missing effective local targets must reject, omission={omit_target}",
        );
        assert!(report.candidate.is_none());
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
    let mut source_entity = linked_source(true);
    source_entity
        .components
        .get_mut(&component_id())
        .unwrap()
        .contextual_defaults
        .insert(
            ComponentFieldId::new("target"),
            ComponentValue::EntityReference(local_target()),
        );
    let prefab = PrefabDocument::from_ordered_roots([
        source_entity,
        SceneEntityRecord::new(scene_id("target")),
    ]);
    let original_prefab = prefab.clone();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        prefab_source::key("effective-references.default-link"),
        prefab.clone(),
    );
    let document = SceneDocument::from_ordered_roots(["left", "right"].map(|anchor| {
        let mut entity = SceneEntityRecord::new(scene_id(anchor));
        entity.prefab = Some(PrefabInstance {
            binding: nara_scene::PrefabInstanceBinding::Forest,
            source: source.clone(),
            arguments: None,
            overrides: None,
        });
        entity
    }));

    let expansion = document.compile(&registry, &resolver);

    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics,
    );
    let expanded = expansion.document().unwrap();
    let mut mapped_targets = BTreeSet::new();
    for anchor in ["left", "right"] {
        let projected = |source_entity: &str| {
            expansion
                .provenance()
                .unwrap()
                .iter()
                .find(|(_, provenance)| {
                    provenance.projection_path().is_some_and(|path| {
                        path.root_anchor().as_str() == anchor
                            && path.steps().len() == 1
                            && path.steps()[0].entity_id().as_str() == source_entity
                    })
                })
                .map(|(entity, _)| entity)
                .unwrap()
        };
        let source_entity = projected("source");
        let target_entity = projected("target");
        let record = expanded
            .entities()
            .iter()
            .find(|entity| entity.id() == source_entity)
            .unwrap();
        assert_eq!(
            record.components()[&component_id()]
                .value()
                .clone()
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
    let report = nara_scene::spawn_scene(
        &mut world,
        &registry,
        &expansion.candidate.unwrap().bind_source(scene_source_key(
            "repeated_prefabs_remap_default_local_reference_and_preserve_source_omission/source",
        )),
        &scene_occurrence(
            "repeated_prefabs_remap_default_local_reference_and_preserve_source_omission/report",
        ),
        None,
    );
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
    for slot in ["first", "second"] {
        let report = nara_scene::spawn_scene(
            &mut world,
            &registry,
            &compiled_scene::native(
                &registry,
                &std::sync::Arc::new((document).clone()),
                scene_source_key(
                    "export_materializes_omitted_default_reference_before_instance_remap/document",
                ),
                None,
            ),
            &scene_occurrence(slot),
            None,
        );
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
        let component = &source_record.components[&component_id()];
        let expected = EntityReference::SceneLocal {
            entity: target.clone(),
        };
        assert_eq!(
            component.value.field_entity_reference("target").unwrap(),
            &expected,
        );
        assert_eq!(
            component.contextual_defaults[&ComponentFieldId::new("target")],
            ComponentValue::EntityReference(expected),
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

    let compiled = output
        .document
        .compile(&registry, &InMemoryPrefabSourceResolver::new());
    assert!(
        !compiled.diagnostics.has_errors(),
        "{:?}",
        compiled.diagnostics,
    );
    assert!(compiled.candidate.is_some());
    #[cfg(feature = "serde")]
    for candidate in [
        SceneDocumentCandidate::decode_json_str(&output.document.to_json_string().unwrap())
            .unwrap(),
        SceneDocumentCandidate::decode_ron_str(&output.document.to_ron_string().unwrap()).unwrap(),
    ] {
        let published = candidate.publish(&registry).unwrap();
        assert_eq!(published.document(), &output.document);
    }
}

#[test]
fn export_aggregate_value_budget_has_exact_boundary_and_preserves_world() {
    let registry = registry(true);
    let document = linked_scene(true);
    let mut world = World::new();
    for slot in ["first", "second"] {
        let report = nara_scene::spawn_scene(
            &mut world,
            &registry,
            &compiled_scene::native(
                &registry,
                &std::sync::Arc::new((document).clone()),
                scene_source_key(
                    "export_aggregate_value_budget_has_exact_boundary_and_preserves_world/document",
                ),
                None,
            ),
            &scene_occurrence(slot),
            None,
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    }
    let before_entities = world.iter_entities().count();
    // Two effective maps each retain their field key and one generated entity_N reference.
    // Two independent contextual maps add a map node, the stable field ID and that reference.
    // Derive this from the fixture and export ID format, independently of the output's cost.
    let values = 2;
    let field_bytes = "target".len();
    let reference_bytes = "entity_1".len();
    let nodes = values * 2 + values * (1 + 1);
    let bytes = values * (field_bytes + reference_bytes) + values * (field_bytes + reference_bytes);
    let exact_limits = nara_reflect::ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(2).unwrap(),
    );
    let exact = export_scene_with_limits(&world, &registry, exact_limits);
    assert!(!exact.diagnostics.has_errors(), "{:?}", exact.diagnostics);
    assert!(exact.output().is_some());
    assert_eq!(world.iter_entities().count(), before_entities);

    for (nodes, bytes, depth) in [
        (nodes - 1, bytes, 2),
        (nodes, bytes - 1, 2),
        (nodes, bytes, 1),
    ] {
        let below = nara_reflect::ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        );
        let rejected = export_scene_with_limits(&world, &registry, below);
        assert!(rejected.output().is_none());
        assert!(rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.export-value-budget-exceeded"
        }));
        assert_eq!(world.iter_entities().count(), before_entities);
    }
}

fn export_scene_with_limits(
    world: &World,
    registry: &ComponentRegistrySnapshot,
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
    // Five Null defaults add six effective nodes (list plus elements) without payload bytes.
    // This keeps publication cost larger than the now-explicit source placement declarations.
    let registry = registry_with_extra_fields(
        false,
        [ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("padding"),
            "Padding",
            ComponentFieldPath::from_fields(["padding"]),
            nara_reflect::ComponentValueShape::list(nara_reflect::ComponentValueShape::optional(
                nara_reflect::ComponentValueShape::scalar(ComponentValueKind::U64),
            )),
            ComponentValue::List(vec![ComponentValue::Null; 5]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)],
    );
    let mut document = linked_scene(true);
    document.entities[0]
        .components
        .get_mut(&component_id())
        .unwrap()
        .contextual_defaults
        .insert(
            ComponentFieldId::new("target"),
            ComponentValue::EntityReference(local_target()),
        );
    let bytes = document.to_json_string().unwrap();
    // Two placement/target/location declarations (6), the raw map (1) and retained
    // contextual entry/reference (2) total nine raw nodes. Publication retains the contextual
    // map/reference (2) plus an effective map/target/list/five Nulls (8): ten nodes.
    let raw_nodes = 2 * 3 + 1 + 2;
    let effective_nodes = 2 + 1 + 1 + 1 + 5;
    // Both root IDs occur in placement targets and entity records; root tags cost three bytes
    // per placement. The remaining bytes belong to the component ID, record metadata,
    // contextual field name and its SceneLocal reference. This is the exact raw byte cost.
    let raw_bytes = 2 * ("source".len() + "target".len())
        + 2 * 3
        + component_id().as_str().len()
        + 5
        + "target".len()
        + "target".len();
    assert_eq!((raw_nodes, effective_nodes, raw_bytes), (9, 10, 68));
    let tight = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(raw_nodes).unwrap())
        .with_component_value_bytes(ByteLimit::new(raw_bytes).unwrap());
    for (limits, kind, observed, maximum) in [
        (
            tight.with_component_value_nodes(ItemLimit::new(raw_nodes - 1).unwrap()),
            nara_scene::SceneFileBudgetKind::ComponentValueNodes,
            raw_nodes,
            raw_nodes - 1,
        ),
        (
            tight.with_component_value_bytes(ByteLimit::new(raw_bytes - 1).unwrap()),
            nara_scene::SceneFileBudgetKind::ComponentValueBytes,
            raw_bytes,
            raw_bytes - 1,
        ),
    ] {
        let nara_scene::SceneFormatError::Budget(error) =
            SceneDocumentCandidate::decode_json_bytes_with_limits(bytes.as_bytes(), limits)
                .unwrap_err()
        else {
            panic!("raw source must reject exactly one missing node or byte");
        };
        assert_eq!(
            (error.kind, error.observed, error.maximum),
            (kind, observed, maximum)
        );
    }
    let candidate = SceneDocumentCandidate::decode_json_bytes_with_limits(bytes.as_bytes(), tight)
        .unwrap()
        .canonicalize(&registry)
        .unwrap();
    let rejected = candidate.publish(&registry).unwrap_err();
    assert!(
        rejected
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.invalid-component-payload" }),
        "{:?}",
        rejected.diagnostics()
    );

    let wide = tight.with_component_value_nodes(ItemLimit::new(effective_nodes).unwrap());
    let published = SceneDocumentCandidate::decode_json_bytes_with_limits(bytes.as_bytes(), wide)
        .unwrap()
        .canonicalize(&registry)
        .unwrap()
        .publish(&registry)
        .unwrap();
    assert_eq!(published.document(), &document);
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
