//! Public, backend-free consumers of atomic native/data Scene publication.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_app::{App, RuntimeInstance, StartupStage};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{
    Component, Entity, Resource, World,
    bevy_ecs::{
        lifecycle::{HookContext, Insert},
        observer::On,
        world::DeferredWorld,
    },
};
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryPlugin, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    ExpandedSceneCandidate, Name, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings,
    SceneComponentBindingsPlugin, SceneComponentRecord, SceneComponentsPlugin, SceneDataComponents,
    SceneDocument, SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, SceneSpawnReport,
    SourceDocumentKey, SourceDocumentKind, retire_and_despawn_scene_instance,
    scene_component_bindings,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn component(name: &str) -> ComponentTypeId {
    ComponentTypeId::new(name)
}

fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.runtime-data").unwrap()
}

fn data_owner() -> ComponentSchemaOwnerSnapshot {
    let schema = |name, kind| {
        ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::empty(),
                kind,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
    };
    ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![
                schema("game.Score", ComponentValueKind::U64),
                schema("game.Visible", ComponentValueKind::Bool),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn bindings() -> SceneComponentBindings {
    let mut registry = ComponentRegistry::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap()
}

fn candidate(score: ComponentValue) -> ExpandedSceneCandidate {
    let data = |value| {
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner_id())
    };
    ExpandedSceneCandidate::plain(
        SourceDocumentKey::named(
            "8bfe5e0f-ecc0-4dd8-af51-aed9317735bb".parse().unwrap(),
            "runtime.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("entity"),
        )
        .with_component(component("game.Score"), data(score))
        .with_component(component("game.Visible"), data(ComponentValue::Bool(true)))
        .with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("root".into()),
            ),
        )])),
    )
}

fn occurrence(name: &str) -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("game.runtime", name).unwrap()
}

fn limits(nodes: usize, bytes: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(8).unwrap(),
    )
}

fn succeeded(report: SceneSpawnReport) -> SpawnedSceneInstance {
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.instance.unwrap()
}

fn resolve(world: &World, instance: &SpawnedSceneInstance) -> Entity {
    match instance.resolve(world, &id("entity")) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected current Scene entity, got {lookup:?}"),
    }
}

#[test]
fn preparation_is_private_and_commit_publishes_the_complete_group() {
    let bindings = bindings();
    let mut world = World::new();
    let before = world.entities().len();
    let prepared = bindings
        .prepare_scene(
            &mut world,
            &candidate(ComponentValue::U64(7)),
            &occurrence("first"),
            None,
        )
        .unwrap();
    assert_eq!(world.entities().len(), before);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    let instance = succeeded(prepared.commit(&mut world));
    let entity = resolve(&world, &instance);
    assert_eq!(world.get::<Name>(entity), Some(&Name::new("root")));
    assert!(world.get::<SceneDataComponents>(entity).is_some());
    let output = bindings.export_scene(&world).into_output().unwrap();
    let records = &output.document.entities[0].components;
    assert_eq!(records.len(), 3);
    assert_eq!(
        records[&component("game.Score")].value,
        ComponentValue::U64(7)
    );
    assert_eq!(
        records[&component("game.Visible")].value,
        ComponentValue::Bool(true)
    );
    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &instance).unwrap(),
        1
    );
    assert!(world.get_entity(entity).is_err());
}

#[test]
fn invalid_final_data_record_does_not_publish_or_consume_target_entities() {
    let bindings = bindings();
    let mut document = candidate(ComponentValue::U64(7)).document().clone();
    // Make the invalid record sort after the native Name, so decoding a preceding native record
    // cannot accidentally publish it before the complete data group has passed preflight.
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component("z.Last"), "Last", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::empty(),
                        ComponentValueKind::Bool,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    document.entities[0]
        .components
        .retain(|component, _| component.as_str() == "nara.scene.Name");
    document.entities[0].components.insert(
        component("z.Last"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(9))
            .with_owner(owner_id()),
    );
    let bindings =
        SceneComponentBindings::new(bindings.native_snapshot().clone(), [owner]).unwrap();
    let candidate = ExpandedSceneCandidate::plain(
        candidate(ComponentValue::U64(7)).source_key().clone(),
        Arc::new(document),
    );
    let mut world = World::new();
    let mut control = World::new();
    let before = world.entities().len();
    let report = bindings
        .prepare_scene(&mut world, &candidate, &occurrence("invalid"), None)
        .unwrap_err();
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.entities().len(), before);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
}

#[test]
fn invalid_replacement_preserves_the_last_good_activation_and_data() {
    let bindings = bindings();
    let mut world = World::new();
    let current = succeeded(
        bindings
            .prepare_scene(
                &mut world,
                &candidate(ComponentValue::U64(7)),
                &occurrence("active"),
                None,
            )
            .unwrap()
            .commit(&mut world),
    );
    let old_entity = resolve(&world, &current);
    let old_stats = world.resource::<WorldIdentityDomain>().stats();
    assert!(
        bindings
            .prepare_scene_replacement(
                &mut world,
                &candidate(ComponentValue::String("invalid".into())),
                &current,
                None,
            )
            .is_err()
    );
    assert_eq!(resolve(&world, &current), old_entity);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), old_stats);
    let output = bindings.export_scene(&world).into_output().unwrap();
    assert_eq!(
        output.document.entities[0].components[&component("game.Score")].value,
        ComponentValue::U64(7)
    );
    let replacement = succeeded(
        bindings
            .prepare_scene_replacement(
                &mut world,
                &candidate(ComponentValue::U64(19)),
                &current,
                None,
            )
            .unwrap()
            .commit(&mut world),
    );
    assert_ne!(resolve(&world, &replacement), old_entity);
    assert!(world.get_entity(old_entity).is_err());
}

#[test]
fn native_and_data_preparation_share_one_exact_value_allowance() {
    let bindings = bindings();
    // Score, Visible and Name have three nodes and 8 + 1 + 4 logical bytes together.
    for allowance in [limits(2, 13), limits(3, 12)] {
        let mut world = World::new();
        assert!(
            bindings
                .prepare_scene_with_limits(
                    &mut world,
                    &candidate(ComponentValue::U64(7)),
                    &occurrence("too-small"),
                    None,
                    allowance,
                )
                .is_err()
        );
        assert!(!world.contains_resource::<WorldIdentityDomain>());
    }
    let mut world = World::new();
    let prepared = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate(ComponentValue::U64(7)),
            &occurrence("exact"),
            None,
            limits(3, 13),
        )
        .unwrap();
    succeeded(prepared.commit(&mut world));
}

#[test]
fn late_carrier_observer_rejects_before_binding_and_never_runs() {
    let bindings = bindings();
    let mut world = World::new();
    let prepared = bindings
        .prepare_scene(
            &mut world,
            &candidate(ComponentValue::U64(7)),
            &occurrence("late-observer"),
            None,
        )
        .unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&hits);
    world.add_observer(move |_: On<Insert<SceneDataComponents>>| {
        observed.fetch_add(1, Ordering::Relaxed);
    });
    let before = world.entities().len();
    let report = prepared.commit(&mut world);
    assert!(report.instance.is_none(), "{report:?}");
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.entities().len(), before);
    assert_eq!(hits.load(Ordering::Relaxed), 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn wrong_world_commit_cannot_publish_even_when_entity_generations_match() {
    let bindings = bindings();
    let mut origin = World::new();
    let prepared = bindings
        .prepare_scene(
            &mut origin,
            &candidate(ComponentValue::U64(7)),
            &occurrence("wrong-world"),
            None,
        )
        .unwrap();
    let mut other = World::new();
    let before = other.entities().len();
    let report = prepared.commit(&mut other);
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(other.entities().len(), before);
    assert!(!other.contains_resource::<WorldIdentityDomain>());
    assert!(!origin.contains_resource::<WorldIdentityDomain>());
}

fn exercise_managed_preparation(world: &mut World) {
    let selected = scene_component_bindings(world).unwrap().clone();
    let same = SceneComponentBindings::new(
        selected.native_snapshot().clone(),
        selected.data_owners().iter().cloned(),
    )
    .unwrap();
    let prepared = same
        .prepare_scene(
            world,
            &candidate(ComponentValue::U64(7)),
            &occurrence("managed"),
            None,
        )
        .unwrap();
    let current = succeeded(prepared.commit(world));
    let old_entity = resolve(world, &current);
    let foreign =
        SceneComponentBindings::new(selected.native_snapshot().clone(), [data_owner()]).unwrap();
    let before = world.entities().len();
    assert!(
        foreign
            .prepare_scene_replacement(world, &candidate(ComponentValue::U64(9)), &current, None,)
            .is_err()
    );
    assert_eq!(world.entities().len(), before);
    assert_eq!(resolve(world, &current), old_entity);

    // A separately frozen native registry must fail through the advanced replacement entry as
    // well as ordinary prepare. An empty candidate cannot hide the mismatched definition.
    let mut foreign_native = ComponentRegistry::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut foreign_native)
        .unwrap();
    foreign_native.freeze().unwrap();
    let empty = ExpandedSceneCandidate::plain(
        candidate(ComponentValue::U64(7)).source_key().clone(),
        Arc::new(SceneDocument::default()),
    );
    let assets = nara_scene::advanced::SceneAssetBindings::from_world(world);
    let result = nara_scene::advanced::prepare_expanded_scene_replacement_with_claims(
        world,
        &foreign_native,
        &empty,
        None,
        &current,
        assets,
        &nara_scene::advanced::SceneNativeRetirementClaims::default(),
        nara_scene::advanced::SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| {},
    );
    assert!(result.is_err());
    assert_eq!(resolve(world, &current), old_entity);
}

#[test]
fn managed_publication_uses_installed_definitions_including_advanced_replacement() {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentsPlugin,
        SceneComponentBindingsPlugin::new([data_owner()]),
    ))
    .unwrap();
    app.add_systems(StartupStage::Scene, exercise_managed_preparation)
        .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    assert!(scene_component_bindings(runtime.world()).is_some());
    runtime.finish().unwrap();
}

#[derive(Component)]
#[component(on_remove = record_native_retirement)]
struct NativeRetirementState;

#[derive(Resource, Default)]
struct NativeRetirements(usize);

fn record_native_retirement(mut world: DeferredWorld<'_>, _: HookContext) {
    world.resource_mut::<NativeRetirements>().0 += 1;
}

#[test]
fn mixed_claimed_replacement_retains_candidate_until_native_owner_finishes() {
    let bindings = bindings();
    let mut world = World::new();
    world.init_resource::<NativeRetirements>();
    let current = succeeded(
        bindings
            .prepare_scene(
                &mut world,
                &candidate(ComponentValue::U64(7)),
                &occurrence("native-owner"),
                None,
            )
            .unwrap()
            .commit(&mut world),
    );
    let old = resolve(&world, &current);
    world.entity_mut(old).insert(NativeRetirementState);
    let claims = nara_scene::advanced::SceneNativeRetirementClaims::from_component_ids([world
        .component_id::<NativeRetirementState>()
        .unwrap()]);
    let assets = nara_scene::advanced::SceneAssetBindings::from_world(&world);
    let prepared = bindings
        .prepare_scene_replacement_with_claims(
            &mut world,
            &candidate(ComponentValue::U64(19)),
            None,
            &current,
            assets,
            &claims,
            nara_scene::advanced::SceneProductTransactionLimits::new(
                ItemLimit::ONE,
                ItemLimit::ONE,
            ),
            &[],
            |_| {},
        )
        .unwrap();
    let (prepared, report) = prepared.commit(&mut world).unwrap_err();
    assert!(report.diagnostics.has_errors());
    assert_eq!(resolve(&world, &current), old);
    assert_eq!(world.resource::<NativeRetirements>().0, 0);
    world.entity_mut(old).remove::<NativeRetirementState>();
    assert_eq!(world.resource::<NativeRetirements>().0, 1);
    let replacement = succeeded(prepared.commit(&mut world).unwrap_or_else(|_| {
        panic!("completed native retirement should allow the retained candidate")
    }));
    assert!(world.get_entity(old).is_err());
    let entity = resolve(&world, &replacement);
    assert_eq!(
        bindings
            .data_component(&world, entity, &component("game.Score"))
            .unwrap()
            .unwrap()
            .value,
        ComponentValue::U64(19)
    );
    assert_eq!(world.resource::<NativeRetirements>().0, 1);
}

#[test]
fn native_codec_cannot_claim_the_scene_owned_carrier_type() {
    let selected = bindings();
    let mut donor = World::new();
    let instance = succeeded(
        selected
            .prepare_scene(
                &mut donor,
                &candidate(ComponentValue::U64(7)),
                &occurrence("donor"),
                None,
            )
            .unwrap()
            .commit(&mut donor),
    );
    let donor_entity = resolve(&donor, &instance);
    let held = Arc::new(std::sync::Mutex::new(Some(
        donor
            .entity_mut(donor_entity)
            .take::<SceneDataComponents>()
            .unwrap(),
    )));
    let decoder = Arc::clone(&held);
    let mut native = ComponentRegistry::new();
    native
        .register_persistent_component_with_codec::<SceneDataComponents, _, _>(
            ComponentSchema::new(
                component("game.ForgedCarrier"),
                "Forged carrier",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            move |_| {
                decoder
                    .lock()
                    .unwrap()
                    .take()
                    .ok_or_else(|| nara_reflect::ComponentCodecError::missing_field("carrier"))
            },
            |_| Ok(ComponentValue::map([] as [(String, ComponentValue); 0])),
        )
        .unwrap();
    native.freeze().unwrap();
    assert!(matches!(
        SceneComponentBindings::new(native.snapshot().unwrap(), []),
        Err(nara_scene::SceneComponentBindingError::ReservedNativeComponent { .. }),
    ));
    let input = ExpandedSceneCandidate::plain(
        candidate(ComponentValue::U64(7)).source_key().clone(),
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("forged"),
        )
        .with_component(
            component("game.ForgedCarrier"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(String, ComponentValue); 0]),
            ),
        )])),
    );
    let mut world = World::new();
    let before = world.entities().len();
    let report =
        nara_scene::prepare_scene(&mut world, &native, &input, &occurrence("forged"), None)
            .unwrap_err();
    assert!(report.diagnostics.has_errors());
    assert!(
        held.lock().unwrap().is_some(),
        "native decoder must never run"
    );
    assert_eq!(world.entities().len(), before);
}
