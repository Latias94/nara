//! Backend-free Rust consumers of guarded complete data groups and scheduled runtime mutation.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{App, CoreStage, RuntimeInstance, StartupStage};
use nara_asset::{AssetRecord, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{
    Component, Entity, Resource, World,
    bevy_ecs::{
        lifecycle::{Add, Insert, Remove},
        observer::On,
    },
};
use nara_identity::{EntityLookup, EntityReference, SpawnedSceneInstance};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryPlugin, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    PersistentComponentSchemaOwnerId, component_registry,
};
use nara_scene::{
    ExpandedSceneCandidate, Name, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings,
    SceneComponentBindingsPlugin, SceneComponentRecord, SceneComponentsPlugin, SceneDataComponents,
    SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind, retire_and_despawn_scene_entity,
    scene_component_bindings,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn component(name: &str) -> ComponentTypeId {
    ComponentTypeId::new(name)
}

fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.guarded-data").unwrap()
}

fn owner() -> ComponentSchemaOwnerSnapshot {
    let root = |name, shape| {
        let reference = match &shape {
            ComponentValueShape::EntityRef => Some(ComponentCapability::EntityRef),
            ComponentValueShape::AssetRef => Some(ComponentCapability::AssetRef),
            _ => None,
        };
        let mut field = ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::empty(),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING);
        if let Some(capability) = reference {
            field = field.with_capability(capability);
        }
        ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field])
    };
    ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![
                root(
                    "game.Score",
                    ComponentValueShape::scalar(ComponentValueKind::U64),
                ),
                root(
                    "game.Enabled",
                    ComponentValueShape::scalar(ComponentValueKind::Bool),
                ),
                root("game.Link", ComponentValueShape::EntityRef),
                root("game.Image", ComponentValueShape::AssetRef),
                ComponentSchema::new(
                    component("game.Defaults"),
                    "Defaults",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("text"),
                    "Text",
                    ComponentFieldPath::from_fields(["text"]),
                    ComponentValueKind::String,
                    ComponentValue::String("abc".into()),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
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
    SceneComponentBindings::new(registry.snapshot().unwrap(), [owner()]).unwrap()
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner_id())
}

fn document() -> Arc<SceneDocument> {
    Arc::new(SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root"))
            .with_component(component("game.Score"), record(ComponentValue::U64(7)))
            .with_component(
                component("game.Enabled"),
                record(ComponentValue::Bool(true)),
            )
            .with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("native".into()),
                ),
            ),
        SceneEntityRecord::new(id("target")),
    ]))
}

fn candidate(document: Arc<SceneDocument>) -> ExpandedSceneCandidate {
    ExpandedSceneCandidate::plain(
        SourceDocumentKey::named(
            "a4519cc6-e3a1-43bf-803b-4d667af48628".parse().unwrap(),
            "guarded.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        document,
    )
}

fn spawn(
    bindings: &SceneComponentBindings,
    world: &mut World,
    source: &ExpandedSceneCandidate,
) -> SpawnedSceneInstance {
    let report = bindings
        .prepare_scene(
            world,
            source,
            &SceneOccurrenceKey::new("game.guarded", "main").unwrap(),
            None,
        )
        .unwrap()
        .commit(world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.instance.unwrap()
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, name: &str) -> Entity {
    match instance.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected a live Scene entity: {lookup:?}"),
    }
}

fn limits(nodes: usize, bytes: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(8).unwrap(),
    )
}

#[test]
fn read_enumerate_replace_remove_preserve_siblings_and_immutable_source() {
    let bindings = bindings();
    let source_document = document();
    let before = source_document.as_ref().clone();
    let source = candidate(Arc::clone(&source_document));
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &source);
    let entity = resolve(&world, &instance, "root");
    let score = component("game.Score");
    assert_eq!(bindings.data_components(&world, entity).unwrap().count(), 2);
    assert_eq!(
        bindings
            .data_component(&world, entity, &score)
            .unwrap()
            .unwrap()
            .value,
        ComponentValue::U64(7)
    );
    assert!(
        bindings
            .data_component(&world, entity, &component("missing"))
            .is_err()
    );
    assert!(
        bindings
            .data_component(&world, entity, &component("nara.scene.Name"))
            .is_err()
    );
    bindings
        .set_data_component(
            &mut world,
            entity,
            &score,
            ComponentValue::U64(9),
            None,
            limits(2, 9),
        )
        .unwrap();
    assert_eq!(
        bindings
            .data_component(&world, entity, &component("game.Enabled"))
            .unwrap()
            .unwrap()
            .value,
        ComponentValue::Bool(true)
    );
    assert_eq!(world.get::<Name>(entity), Some(&Name::new("native")));
    assert!(
        bindings
            .remove_data_component(&mut world, entity, &score)
            .unwrap()
    );
    assert!(
        !bindings
            .remove_data_component(&mut world, entity, &score)
            .unwrap()
    );
    assert!(
        bindings
            .data_component(&world, entity, &score)
            .unwrap()
            .is_none()
    );
    assert_eq!(bindings.data_components(&world, entity).unwrap().count(), 1);
    assert!(
        bindings
            .remove_data_component(&mut world, entity, &component("game.Enabled"))
            .unwrap()
    );
    assert!(world.get::<SceneDataComponents>(entity).is_none());
    bindings
        .set_data_component(
            &mut world,
            entity,
            &score,
            ComponentValue::U64(10),
            None,
            limits(1, 8),
        )
        .unwrap();
    assert_eq!(bindings.data_components(&world, entity).unwrap().count(), 1);
    assert_eq!(source.document(), &before);
    assert_eq!(source_document.as_ref(), &before);
}

#[test]
fn exact_aggregate_nodes_bytes_defaults_and_zero_residual_reject_without_writes() {
    let bindings = bindings();
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &candidate(document()));
    let entity = resolve(&world, &instance, "root");
    let defaults = component("game.Defaults");
    let before = bindings.export_scene(&world).into_output().unwrap();
    for budget in [limits(1, 9), limits(2, 8)] {
        assert!(
            bindings
                .set_data_component(
                    &mut world,
                    entity,
                    &component("game.Score"),
                    ComponentValue::U64(42),
                    None,
                    budget,
                )
                .is_err()
        );
    }
    // Existing siblings cost 2 nodes/9 bytes. The defaulted map adds 2 nodes/7 bytes.
    for budget in [limits(3, 16), limits(4, 15), limits(4, 9)] {
        assert!(
            bindings
                .set_data_component(
                    &mut world,
                    entity,
                    &defaults,
                    ComponentValue::Map(Default::default()),
                    None,
                    budget,
                )
                .is_err()
        );
    }
    assert_eq!(bindings.export_scene(&world).into_output().unwrap(), before);
    bindings
        .set_data_component(
            &mut world,
            entity,
            &defaults,
            ComponentValue::Map(Default::default()),
            None,
            limits(4, 16),
        )
        .unwrap();
    assert_eq!(
        bindings
            .data_component(&world, entity, &defaults)
            .unwrap()
            .unwrap()
            .value,
        ComponentValue::map([("text", ComponentValue::String("abc".into()))])
    );
}

#[test]
fn invalid_payload_and_declared_references_leave_the_complete_group_unchanged() {
    let bindings = bindings();
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &candidate(document()));
    let entity = resolve(&world, &instance, "root");
    let before = bindings.export_scene(&world).into_output().unwrap();
    for (name, value) in [
        ("game.Score", ComponentValue::Bool(false)),
        (
            "game.Link",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("missing"),
            }),
        ),
        (
            "game.Image",
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("../bad.png".into())),
            ]),
        ),
    ] {
        assert!(
            bindings
                .set_data_component(
                    &mut world,
                    entity,
                    &component(name),
                    value,
                    None,
                    limits(100, 4096)
                )
                .is_err()
        );
    }
    assert_eq!(bindings.export_scene(&world).into_output().unwrap(), before);
    bindings
        .set_data_component(
            &mut world,
            entity,
            &component("game.Link"),
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("target"),
            }),
            None,
            limits(100, 4096),
        )
        .unwrap();
    let target = resolve(&world, &instance, "target");
    retire_and_despawn_scene_entity(&mut world, target).unwrap();
    assert!(
        bindings
            .data_component(&world, entity, &component("game.Score"))
            .is_err()
    );
    assert!(bindings.data_components(&world, entity).is_err());
    assert!(
        bindings
            .remove_data_component(&mut world, entity, &component("game.Link"))
            .is_err()
    );
}

#[test]
fn replacing_a_valid_link_with_a_retired_target_preserves_the_complete_group() {
    let bindings = bindings();
    let mut source = document().as_ref().clone();
    let link = record(ComponentValue::EntityReference(
        EntityReference::SceneLocal { entity: id("root") },
    ));
    source.entities[0]
        .components
        .insert(component("game.Link"), link.clone());
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &candidate(Arc::new(source)));
    let entity = resolve(&world, &instance, "root");
    let target = resolve(&world, &instance, "target");
    retire_and_despawn_scene_entity(&mut world, target).unwrap();
    assert!(world.get_entity(target).is_err());
    // The existing self-link remains valid, so refusal must come from the proposed dead target.
    let before = bindings.export_scene(&world).into_output().unwrap();

    assert!(matches!(
        bindings.set_data_component(
            &mut world,
            entity,
            &component("game.Link"),
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("target"),
            }),
            None,
            limits(100, 4096),
        ),
        Err(nara_scene::SceneDataComponentError::LocalReferenceInvalid { component: rejected })
            if rejected == component("game.Link")
    ));
    assert_eq!(
        bindings
            .data_component(&world, entity, &component("game.Link"))
            .unwrap(),
        Some(&link)
    );
    assert_eq!(
        bindings
            .data_component(&world, entity, &component("game.Score"))
            .unwrap(),
        Some(&record(ComponentValue::U64(7)))
    );
    assert_eq!(world.get::<Name>(entity), Some(&Name::new("native")));
    assert_eq!(bindings.export_scene(&world).into_output().unwrap(), before);
}

#[test]
fn declared_asset_metadata_is_checked_without_native_handles() {
    let bindings = bindings();
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &candidate(document()));
    let entity = resolve(&world, &instance, "root");
    let value = ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String("images/fixture.png".into())),
    ]);
    let mut database = ProjectAssetDatabase::default();
    assert!(
        bindings
            .set_data_component(
                &mut world,
                entity,
                &component("game.Image"),
                value.clone(),
                Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
                limits(100, 4096),
            )
            .is_err()
    );
    database
        .insert(AssetRecord::new(
            "b69d5990-ef57-478a-92f3-94557862ec56".parse().unwrap(),
            AssetSourcePath::new("images/fixture.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    bindings
        .set_data_component(
            &mut world,
            entity,
            &component("game.Image"),
            value,
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
            limits(100, 4096),
        )
        .unwrap();
    assert!(!world.contains_resource::<nara_asset::AssetServer>());
    assert!(
        bindings
            .set_data_component(
                &mut world,
                entity,
                &component("game.Score"),
                ComponentValue::U64(8),
                Some(ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT)),
                limits(100, 4096),
            )
            .is_err()
    );
    assert_eq!(
        bindings
            .data_component(&world, entity, &component("game.Score"))
            .unwrap()
            .unwrap()
            .value,
        ComponentValue::U64(7)
    );
}

#[test]
fn foreign_owner_admissions_unowned_entities_and_moved_carriers_reject() {
    let bindings = bindings();
    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, &candidate(document()));
    let entity = resolve(&world, &instance, "root");
    let target = resolve(&world, &instance, "target");
    let foreign =
        SceneComponentBindings::new(bindings.native_snapshot().clone(), [owner()]).unwrap();
    assert!(foreign.data_components(&world, entity).is_err());
    assert!(
        foreign
            .remove_data_component(&mut world, entity, &component("game.Score"))
            .is_err()
    );
    let unowned = world.spawn_empty().id();
    let source = world.get::<SceneEntitySource>(entity).unwrap().clone();
    world.entity_mut(unowned).insert(source);
    assert!(
        bindings
            .set_data_component(
                &mut world,
                unowned,
                &component("game.Score"),
                ComponentValue::U64(1),
                None,
                limits(2, 9)
            )
            .is_err()
    );
    let carrier = world
        .entity_mut(entity)
        .take::<SceneDataComponents>()
        .unwrap();
    world.entity_mut(target).insert(carrier);
    assert!(bindings.data_components(&world, target).is_err());
    assert!(
        bindings
            .set_data_component(
                &mut world,
                target,
                &component("game.Score"),
                ComponentValue::U64(1),
                None,
                limits(2, 9)
            )
            .is_err()
    );
    assert!(
        bindings
            .remove_data_component(&mut world, target, &component("game.Score"))
            .is_err()
    );
}

#[test]
fn carrier_world_and_entity_generation_witnesses_survive_raw_ecs_moves() {
    let bindings = bindings();
    let source = candidate(document());
    let mut first = World::new();
    let mut second = World::new();
    let first_instance = spawn(&bindings, &mut first, &source);
    let second_instance = spawn(&bindings, &mut second, &source);
    let old = resolve(&first, &first_instance, "root");
    let other = resolve(&second, &second_instance, "root");
    assert_eq!(old, other);
    let carrier = first.entity_mut(old).take::<SceneDataComponents>().unwrap();
    second.entity_mut(other).insert(carrier);
    assert!(
        bindings
            .data_component(&second, other, &component("game.Score"))
            .is_err()
    );
    assert!(
        bindings
            .remove_data_component(&mut second, other, &component("game.Score"))
            .is_err()
    );
    assert!(first.despawn(old));
    let recycled = first.spawn_empty().id();
    assert_ne!(old, recycled);
    assert!(bindings.data_components(&first, old).is_err());
}

#[test]
fn managed_native_registry_never_auto_installs_caller_data_bindings() {
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    let app = app.seal().unwrap();
    let caller = SceneComponentBindings::new(
        component_registry(app.world()).unwrap().snapshot().unwrap(),
        [owner()],
    )
    .unwrap();
    assert!(
        caller
            .data_components(app.world(), Entity::PLACEHOLDER)
            .is_err()
    );
    assert!(scene_component_bindings(app.world()).is_none());
}

#[derive(Component, Default)]
struct Required;

#[test]
fn late_carrier_hooks_requirements_and_observers_reject_without_running() {
    for mode in 0..5 {
        let bindings = bindings();
        let mut world = World::new();
        let empty_source = candidate(Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")),
        ])));
        let instance = spawn(&bindings, &mut world, &empty_source);
        let entity = resolve(&world, &instance, "root");
        let calls = Arc::new(AtomicUsize::new(0));
        match mode {
            0 => {
                world
                    .register_component_hooks::<SceneDataComponents>()
                    .on_insert(|_, _| panic!("guarded insertion ran a hook"));
            }
            1 => {
                world.register_required_components::<SceneDataComponents, Required>();
            }
            2 => {
                let calls = Arc::clone(&calls);
                world.add_observer(move |_: On<Insert<SceneDataComponents>>| {
                    calls.fetch_add(1, Ordering::SeqCst);
                });
            }
            3 => {
                let calls = Arc::clone(&calls);
                world.add_observer(move |_: On<Remove<SceneDataComponents>>| {
                    calls.fetch_add(1, Ordering::SeqCst);
                });
            }
            _ => {
                let calls = Arc::clone(&calls);
                world.add_observer(move |_: On<Add<()>>| {
                    calls.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
        let before = calls.load(Ordering::SeqCst);
        assert!(
            bindings
                .set_data_component(
                    &mut world,
                    entity,
                    &component("game.Score"),
                    ComponentValue::U64(1),
                    None,
                    limits(1, 8)
                )
                .is_err()
        );
        assert!(
            bindings
                .remove_data_component(&mut world, entity, &component("game.Score"))
                .is_err()
        );
        assert!(bindings.data_components(&world, entity).is_err());
        assert!(world.get::<SceneDataComponents>(entity).is_none());
        assert!(world.get::<Required>(entity).is_none());
        assert_eq!(calls.load(Ordering::SeqCst), before);
    }
}

#[derive(Resource)]
struct ScheduledConsumer {
    source: ExpandedSceneCandidate,
    retained_source: Arc<SceneDocument>,
    instance: Option<SpawnedSceneInstance>,
    entity: Option<Entity>,
    updates: usize,
}

fn startup(world: &mut World) {
    let bindings = scene_component_bindings(world).unwrap().clone();
    world.resource_scope(|world, mut consumer: nara_ecs::Mut<ScheduledConsumer>| {
        let instance = spawn(&bindings, world, &consumer.source);
        consumer.entity = Some(resolve(world, &instance, "root"));
        consumer.instance = Some(instance);
    });
}

fn update(world: &mut World) {
    let bindings = scene_component_bindings(world).unwrap().clone();
    world.resource_scope(|world, mut consumer: nara_ecs::Mut<ScheduledConsumer>| {
        let entity = consumer.entity.unwrap();
        if consumer.updates == 0 {
            assert_eq!(
                bindings
                    .data_component(world, entity, &component("game.Score"))
                    .unwrap()
                    .unwrap()
                    .value,
                ComponentValue::U64(7)
            );
            bindings
                .set_data_component(
                    world,
                    entity,
                    &component("game.Score"),
                    ComponentValue::U64(42),
                    None,
                    limits(2, 9),
                )
                .unwrap();
            assert_eq!(
                bindings
                    .data_component(world, entity, &component("game.Enabled"))
                    .unwrap()
                    .unwrap()
                    .value,
                ComponentValue::Bool(true)
            );
            assert_eq!(bindings.data_components(world, entity).unwrap().count(), 2);
            assert_eq!(world.get::<Name>(entity), Some(&Name::new("native")));
            let foreign =
                SceneComponentBindings::new(bindings.native_snapshot().clone(), [owner()]).unwrap();
            assert!(
                foreign
                    .set_data_component(
                        world,
                        entity,
                        &component("game.Score"),
                        ComponentValue::U64(0),
                        None,
                        limits(2, 9)
                    )
                    .is_err()
            );
        } else {
            let report = bindings
                .prepare_scene_replacement(
                    world,
                    &consumer.source,
                    consumer.instance.as_ref().unwrap(),
                    None,
                )
                .unwrap()
                .commit(world);
            assert!(!report.diagnostics.has_errors(), "{report:?}");
            let instance = report.instance.unwrap();
            assert!(
                bindings
                    .data_component(world, entity, &component("game.Score"))
                    .is_err()
            );
            assert!(
                bindings
                    .set_data_component(
                        world,
                        entity,
                        &component("game.Score"),
                        ComponentValue::U64(0),
                        None,
                        limits(2, 9)
                    )
                    .is_err()
            );
            assert!(
                bindings
                    .remove_data_component(world, entity, &component("game.Score"))
                    .is_err()
            );
            let current = resolve(world, &instance, "root");
            assert_eq!(
                bindings
                    .data_component(world, current, &component("game.Score"))
                    .unwrap()
                    .unwrap()
                    .value,
                ComponentValue::U64(7)
            );
            consumer.entity = Some(current);
            consumer.instance = Some(instance);
        }
        // Guarded runtime edits never mutate the source candidate or its retained history basis.
        assert_eq!(
            consumer.source.document(),
            consumer.retained_source.as_ref()
        );
        assert_eq!(
            consumer.source.document().entities[0].components[&component("game.Score")].value,
            ComponentValue::U64(7)
        );
        consumer.updates += 1;
    });
}

#[test]
fn actual_app_startup_and_update_use_guarded_data_without_changing_source_history() {
    let retained_source = document();
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentsPlugin,
        SceneComponentBindingsPlugin::new([owner()]),
    ))
    .unwrap();
    app.insert_resource(ScheduledConsumer {
        source: candidate(Arc::clone(&retained_source)),
        retained_source,
        instance: None,
        entity: None,
        updates: 0,
    })
    .unwrap();
    app.add_systems(StartupStage::Scene, startup).unwrap();
    app.add_systems(CoreStage::Update, update).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<ScheduledConsumer>().updates, 2);
    runtime.finish().unwrap();
}
