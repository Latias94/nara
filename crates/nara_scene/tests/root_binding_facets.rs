//! Public compiler acceptance for aliased declaration slots and finite routed recursion.

use std::{collections::BTreeMap, convert::Infallible};

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    CompiledSceneDocument, CompiledSceneEntity, InMemoryPrefabSourceResolver,
    PrefabComponentIntent, PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep,
    PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceArguments, PrefabSourceResolver,
    SceneCompilationFailureKind, SceneCompilationLimits, SceneCompilationReport,
    SceneCompilationWorkingSet, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    ScenePlacementSlot, SourceDocumentKey, SourceDocumentKind,
};

#[path = "root_binding_facets/instance_slots.rs"]
mod instance_slots;
#[path = "root_binding_facets/local_hierarchy.rs"]
mod local_hierarchy;
#[path = "root_binding_facets/recursive_placements.rs"]
mod recursive_placements;

#[derive(Component)]
struct Link(EntityReference);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/root_binding_facets/{name}.prefab.ron")).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "54f63bd4-e218-4c06-a38f-5531a71f0c1e".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn input() -> ReferenceInputId {
    ReferenceInputId::new("u").unwrap()
}

fn link_type() -> ComponentTypeId {
    ComponentTypeId::new("test.RootBindingFacetLink")
}

fn registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<Link, _, _>(
            ComponentSchema::new(link_type(), "Facet link", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueKind::EntityRef,
                )
                .with_capabilities([
                    ComponentCapability::Scene,
                    ComponentCapability::EntityRef,
                    ComponentCapability::Inspect,
                    ComponentCapability::Edit,
                ])]),
            |value| Ok(Link(value.field_entity_reference("target")?.clone())),
            |value| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

fn local(name: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(name) }
}

fn hops(route: &[(&str, &str)]) -> Vec<PrefabInstanceHop> {
    route
        .iter()
        .map(|(anchor, base)| PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: key(base),
        })
        .collect()
}

fn target(route: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: hops(route),
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn reference(route: &[(&str, &str)], entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: hops(route),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn input_reference(route: &[(&str, &str)]) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: hops(route),
            target: AuthoredReferenceTarget::Input(input()),
        },
    }
}

fn link(reference: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(reference))]),
    )
}

fn reader(name: &str, reference: EntityReference) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(link_type(), link(reference))
}

fn root_instance(base: &str, root: &str) -> PrefabInstance {
    PrefabInstance::root(source(base), key(base), id(root))
}

fn with_argument(
    mut instance: PrefabInstance,
    base: &str,
    value: EntityReference,
) -> PrefabInstance {
    instance.arguments = Some(PrefabReferenceArguments {
        base: key(base),
        values: BTreeMap::from([(input(), value)]),
    });
    instance
}

fn child(entity: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        target(&[], entity),
        ScenePlacement::last(ScenePlacementParent::Entity(target(&[], parent))),
    )
}

fn pin_link(target: PrefabEntityTarget, value: EntityReference) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: link_type(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(value)),
        }],
    }
}

fn compile_with_requests(
    scene: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
    registry: &ComponentRegistrySnapshot,
) -> (SceneCompilationReport, Vec<AssetRef>) {
    let before = scene.clone();
    let mut requests = Vec::new();
    let report = scene
        .try_compile_with_loader(
            registry,
            Default::default(),
            None,
            |requested: &AssetRef, _: &mut SceneCompilationWorkingSet| {
                requests.push(requested.clone());
                Ok::<_, Infallible>(sources.resolve_prefab(requested))
            },
        )
        .unwrap();
    assert_eq!(
        scene, &before,
        "compilation must preserve source declarations"
    );
    (report, requests)
}

fn compiled(report: SceneCompilationReport) -> CompiledSceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report
        .candidate
        .expect("valid composition must seal a candidate")
}

fn entity<'a>(
    document: &'a CompiledSceneDocument,
    entity_id: &SceneEntityId,
) -> &'a CompiledSceneEntity {
    document
        .entities()
        .iter()
        .find(|entity| entity.id() == entity_id)
        .expect("expected a materialized entity")
}

fn projected(
    document: &CompiledSceneDocument,
    anchor: &str,
    path: &[(&str, &str)],
) -> SceneEntityId {
    let steps = path
        .iter()
        .map(|(base, entity)| PrefabProjectionStep::SourceEntity {
            source_document: source(base),
            canonical_source: key(base),
            entity: id(entity),
        })
        .collect::<Vec<_>>();
    let mut matches = document
        .canonical_provenance()
        .iter()
        .filter(|(_, origin)| {
            origin.projection_path().is_some_and(|projection| {
                projection.root_anchor() == &id(anchor) && projection.steps() == steps.as_slice()
            })
        });
    let (entity, _) = matches
        .next()
        .expect("expected the exact public source projection");
    assert!(
        matches.next().is_none(),
        "the occurrence path must be unique"
    );
    entity.clone()
}

fn assert_link(
    document: &CompiledSceneDocument,
    entity_id: &SceneEntityId,
    target: &SceneEntityId,
) {
    assert_eq!(
        entity(document, entity_id).components()[&link_type()]
            .value()
            .field_entity_reference("target")
            .unwrap(),
        &EntityReference::SceneLocal {
            entity: target.clone(),
        },
        "unexpected lowered target for {entity_id:?}",
    );
}
