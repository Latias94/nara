//! Live export admits complete native Scene capabilities before any encoder runs.

use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_ecs::identity::{EntityLookup, WorldIdentityDomain};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::hierarchy::HierarchyRoots;
use nara_scene::{
    InMemoryPrefabSourceResolver, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind, export_live_scene, prepare_scene,
};

const EARLIER_COMPONENT: &str = "test.EarlierNative";
const LATER_COMPONENT: &str = "test.LaterNative";

#[derive(Debug, PartialEq, Eq, Component)]
struct EarlierNative(u64);

#[derive(Debug, PartialEq, Eq, Component)]
struct LaterNative(u64);

fn schema(component: &str, field_has_scene: bool) -> ComponentSchema {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities([ComponentCapability::Inspect, ComponentCapability::Edit]);
    let field = if field_has_scene {
        field.with_capability(ComponentCapability::Scene)
    } else {
        field
    };
    ComponentSchema::new(
        ComponentTypeId::new(component),
        component,
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field])
}

fn registry(
    later_field_has_scene: bool,
    encoder_calls: &Arc<AtomicUsize>,
) -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    let earlier_calls = Arc::clone(encoder_calls);
    builder
        .register_persistent_component_with_codec::<EarlierNative, _, _>(
            schema(EARLIER_COMPONENT, true),
            |value| Ok(EarlierNative(value.field_u64("value")?)),
            move |value| {
                earlier_calls.fetch_add(1, Ordering::Relaxed);
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::U64(value.0),
                )]))
            },
        )
        .unwrap();
    let later_calls = Arc::clone(encoder_calls);
    builder
        .register_persistent_component_with_codec::<LaterNative, _, _>(
            schema(LATER_COMPONENT, later_field_has_scene),
            |value| Ok(LaterNative(value.field_u64("value")?)),
            move |value| {
                later_calls.fetch_add(1, Ordering::Relaxed);
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::U64(value.0),
                )]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

#[test]
fn later_native_field_without_scene_capability_refuses_before_any_encoder() {
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let initial = registry(true, &encoder_calls);
    let earlier_id = SceneEntityId::new("a-earlier").unwrap();
    let later_id = SceneEntityId::new("z-later").unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(earlier_id.clone()).with_component(
            ComponentTypeId::new(EARLIER_COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::U64(7))]),
            ),
        ),
        SceneEntityRecord::new(later_id.clone()).with_component(
            ComponentTypeId::new(LATER_COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::U64(11))]),
            ),
        ),
    ]);
    let compiled = document.compile(&initial, &InMemoryPrefabSourceResolver::new());
    assert!(!compiled.diagnostics.has_errors(), "{compiled:?}");
    let source = SourceDocumentKey::named(
        "61258ee5-bb94-436f-a589-1507ce77d7ac".parse().unwrap(),
        "live-export-capabilities",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    let candidate = compiled.candidate.unwrap().bind_source(source);
    let occurrence = SceneOccurrenceKey::new("scene.tests", "live-export-capabilities").unwrap();
    let mut world = World::new();
    let spawned = prepare_scene(&mut world, &initial, &candidate, &occurrence, None)
        .unwrap_or_else(|report| panic!("preparation failed: {report:?}"))
        .commit(&mut world);
    assert!(!spawned.diagnostics.has_errors(), "{spawned:?}");
    let instance = spawned.instance.unwrap();
    let locators = [
        instance.locator(&earlier_id).unwrap(),
        instance.locator(&later_id).unwrap(),
    ];
    assert!(locators[0] < locators[1]);
    let entities = locators.each_ref().map(|locator| {
        match world
            .resource::<WorldIdentityDomain>()
            .lookup(&world, locator.entity())
        {
            EntityLookup::Resolved(entity) => entity,
            lookup => panic!("expected a live entity, got {lookup:?}"),
        }
    });
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), &entities);
    let sources = entities.map(|entity| world.get::<SceneEntitySource>(entity).unwrap().clone());
    let entities_before = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<BTreeSet<_>>();
    let tick_before = world.read_change_tick();
    let identities_before = world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&world, &instance)
        .unwrap();
    let assert_unchanged = || {
        assert_eq!(
            world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<BTreeSet<_>>(),
            entities_before,
        );
        assert_eq!(world.read_change_tick(), tick_before);
        assert_eq!(world.resource::<HierarchyRoots>().as_slice(), &entities);
        assert_eq!(
            world.get::<EarlierNative>(entities[0]),
            Some(&EarlierNative(7))
        );
        assert_eq!(
            world.get::<LaterNative>(entities[1]),
            Some(&LaterNative(11))
        );
        let domain = world.resource::<WorldIdentityDomain>();
        assert_eq!(
            domain.scene_identity_snapshot(&world, &instance).unwrap(),
            identities_before,
        );
        assert_eq!(
            domain
                .registered_locators(&world)
                .unwrap()
                .collect::<BTreeSet<_>>(),
            locators.iter().cloned().collect::<BTreeSet<_>>(),
        );
        for ((locator, entity), source) in locators.iter().zip(entities).zip(&sources) {
            assert_eq!(
                domain.lookup(&world, locator.entity()),
                EntityLookup::Resolved(entity)
            );
            assert_eq!(world.get::<SceneEntitySource>(entity), Some(source));
        }
    };
    assert_unchanged();

    let incomplete = registry(false, &encoder_calls);
    let later_schema = incomplete
        .schema(&ComponentTypeId::new(LATER_COMPONENT))
        .unwrap();
    assert!(later_schema.has_capability(ComponentCapability::Scene));
    assert!(!later_schema.fields()[0].has_capability(ComponentCapability::Scene));
    encoder_calls.store(0, Ordering::Relaxed);
    let refused = export_live_scene(&world, &incomplete);
    assert!(refused.output().is_none(), "{refused:?}");
    assert!(
        refused.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.export-native-binding-unavailable"
        }),
        "{refused:?}",
    );
    assert_eq!(encoder_calls.load(Ordering::Relaxed), 0);
    assert_unchanged();

    let complete = registry(true, &encoder_calls);
    assert!(!complete.ptr_eq(&initial));
    assert_eq!(complete.catalog(), initial.catalog());
    let exported = export_live_scene(&world, &complete);
    assert!(!exported.diagnostics.has_errors(), "{exported:?}");
    let output = exported.output().unwrap();
    assert_eq!(encoder_calls.load(Ordering::Relaxed), 2);
    assert_eq!(output.document, document);
    assert_eq!(output.remap.len(), 2);
    assert_eq!(output.remap.get(&locators[0]), Some(&earlier_id));
    assert_eq!(output.remap.get(&locators[1]), Some(&later_id));
    assert_unchanged();
}
