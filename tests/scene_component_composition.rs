use nara::{
    ecs::{
        lifecycle::{Add, HookContext, Remove},
        observer::On,
        system::ResMut,
        world::DeferredWorld,
    },
    identity::{EntityLookup, WorldIdentityDomain},
    prelude::{
        Component, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponent, Resource, World,
    },
    reflect::{
        ComponentRegistryError, PersistentApplyRejection, PersistentLifecycleEvent,
        PersistentObserverScope,
    },
    scene::{
        PrefabDocument, SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneEntityId,
        SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, export_scene, spawn_prefab,
        spawn_scene,
    },
};

const PERSISTENT_ID: &str = "nara.test.CompositionProbe";

#[derive(Component, PersistentComponent, Clone, Debug, PartialEq)]
#[nara(
    id = "nara.test.CompositionProbe",
    version = 2,
    alias = "Composition probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct CompositionProbe {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

#[derive(Component, Default)]
struct ImplicitDependency;

#[derive(Component, PersistentComponent)]
#[require(ImplicitDependency)]
#[nara(
    id = "nara.test.RequiredCompositionProbe",
    version = 1,
    alias = "Required composition probe",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct RequiredCompositionProbe {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

#[derive(Component, PersistentComponent)]
#[component(on_add = intrinsic_persistent_hook)]
#[nara(
    id = "nara.test.HookedCompositionProbe",
    version = 1,
    alias = "Hooked composition probe",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct HookedCompositionProbe {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

#[derive(Component)]
struct RuntimeProjection;

#[derive(Component, Default)]
struct RuntimeDependency;

#[derive(Component)]
#[require(RuntimeDependency)]
#[component(on_add = runtime_only_hook)]
struct RuntimeOnly;

#[derive(Resource, Default)]
struct HookCanary(u32);

fn intrinsic_persistent_hook(_world: DeferredWorld<'_>, _context: HookContext) {}

fn runtime_only_hook(mut world: DeferredWorld<'_>, _context: HookContext) {
    world.resource_mut::<HookCanary>().0 += 1;
}

fn dynamic_persistent_hook(mut world: DeferredWorld<'_>, _context: HookContext) {
    world.resource_mut::<HookCanary>().0 += 1;
}

fn component_id() -> ComponentTypeId {
    ComponentTypeId::new(PERSISTENT_ID)
}

fn component_record(version: ComponentSchemaVersion, value: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        version,
        ComponentValue::map([("value", ComponentValue::I64(value))]),
    )
}

fn frozen_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<CompositionProbe>()
        .unwrap();
    registry
        .register_component_migration(
            &component_id(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).unwrap(),
            Ok,
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn record(id: &SceneEntityId, version: ComponentSchemaVersion, value: i64) -> SceneEntityRecord {
    SceneEntityRecord::new(id.clone())
        .with_component(component_id(), component_record(version, value))
}

fn resolved_entity(
    world: &World,
    report: &nara::scene::SceneSpawnReport,
    id: &SceneEntityId,
) -> nara::prelude::Entity {
    let instance = report.instance.as_ref().expect("scene spawn must succeed");
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("scene entity did not resolve: {lookup:?}"),
    }
}

#[test]
fn provider_validation_rejects_implicit_composition_but_runtime_ecs_keeps_bevy_behavior() {
    let registry = ComponentRegistry::new();

    assert!(matches!(
        registry.validate_persistent_component::<RequiredCompositionProbe>(),
        Err(ComponentRegistryError::PersistentComponentRequiresImplicitComponents { .. })
    ));
    assert!(matches!(
        registry.validate_persistent_component::<HookedCompositionProbe>(),
        Err(ComponentRegistryError::PersistentComponentHasLifecycleHook { .. })
    ));
    assert!(registry.catalog_candidate().components().is_empty());

    let mut world = World::new();
    world.init_resource::<HookCanary>();
    let entity = world.spawn(RuntimeOnly).id();
    assert!(world.get::<RuntimeDependency>(entity).is_some());
    assert_eq!(world.resource::<HookCanary>().0, 1);
}

#[test]
fn every_public_authoring_route_materializes_only_the_explicit_persistent_set() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("probe").unwrap();

    let mut scene_world = World::new();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::ONE, 11)]);
    let scene_report = spawn_scene(&mut scene_world, &registry, &scene);
    assert!(!scene_report.diagnostics.has_errors());
    let scene_entity = resolved_entity(&scene_world, &scene_report, &id);
    assert_eq!(
        scene_world.get::<CompositionProbe>(scene_entity),
        Some(&CompositionProbe { value: 11 })
    );
    assert!(scene_world.get::<RuntimeProjection>(scene_entity).is_none());

    scene_world
        .entity_mut(scene_entity)
        .insert(RuntimeProjection);
    let exported = export_scene(&scene_world, &registry)
        .into_output()
        .expect("eligible scene must export");
    assert_eq!(exported.document.entities.len(), 1);
    assert_eq!(exported.document.entities[0].components.len(), 1);
    assert!(
        exported.document.entities[0]
            .components
            .contains_key(&component_id())
    );

    let mut prefab_world = World::new();
    let prefab = PrefabDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 12)]);
    let prefab_report = spawn_prefab(&mut prefab_world, &registry, &prefab);
    assert!(!prefab_report.diagnostics.has_errors());
    let prefab_entity = resolved_entity(&prefab_world, &prefab_report, &id);
    assert_eq!(
        prefab_world.get::<CompositionProbe>(prefab_entity),
        Some(&CompositionProbe { value: 12 })
    );
    assert!(
        prefab_world
            .get::<RuntimeProjection>(prefab_entity)
            .is_none()
    );

    let mut session =
        SceneAuthoringSession::new(SceneDocument::new([SceneEntityRecord::new(id.clone())]));
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: id.clone(),
        component: component_id(),
        value: component_record(ComponentSchemaVersion::new(2).unwrap(), 13),
    }]);
    assert!(session.apply_patch(&patch, &registry).applied);
    let mut authoring_world = World::new();
    let sync = session.sync_world(&mut authoring_world, &registry);
    assert!(sync.synced);
    let authoring_instance = sync.live_instance.as_ref().unwrap();
    let authoring_entity = match authoring_instance.resolve(&authoring_world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("authoring entity did not resolve: {lookup:?}"),
    };
    assert_eq!(
        authoring_world.get::<CompositionProbe>(authoring_entity),
        Some(&CompositionProbe { value: 13 })
    );
    assert!(
        authoring_world
            .get::<RuntimeProjection>(authoring_entity)
            .is_none()
    );

    let mut direct_world = World::new();
    let direct_entity = direct_world.spawn_empty().id();
    registry
        .preflight_component(
            &component_id(),
            &ComponentValue::map([("value", ComponentValue::I64(14))]),
        )
        .unwrap()
        .unwrap()
        .apply(&mut direct_world, direct_entity)
        .unwrap();
    assert_eq!(
        direct_world.get::<CompositionProbe>(direct_entity),
        Some(&CompositionProbe { value: 14 })
    );
    assert!(
        direct_world
            .get::<RuntimeProjection>(direct_entity)
            .is_none()
    );
}

#[test]
fn fresh_scene_apply_flushes_and_rejects_dynamic_hooks_before_allocation() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("hooked").unwrap();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 21)]);
    let mut world = World::new();
    world.init_resource::<HookCanary>();
    world.register_component::<CompositionProbe>();
    world.commands().queue(|world: &mut World| {
        world
            .register_component_hooks::<CompositionProbe>()
            .on_add(dynamic_persistent_hook);
    });
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &scene);

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
    assert_eq!(world.resource::<HookCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn late_required_component_metadata_rejects_public_apply_before_persistent_mutation() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("required").unwrap();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 22)]);
    let mut scene_world = World::new();
    scene_world.register_required_components::<CompositionProbe, ImplicitDependency>();
    scene_world.flush();
    let baseline = scene_world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut scene_world, &registry, &scene);

    assert!(report.instance.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
        })
    );
    assert_eq!(
        scene_world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert!(!scene_world.contains_resource::<WorldIdentityDomain>());

    let mut direct_world = World::new();
    let target = direct_world.spawn_empty().id();
    direct_world.register_required_components::<CompositionProbe, ImplicitDependency>();
    let prepared = registry
        .preflight_component(
            &component_id(),
            &ComponentValue::map([("value", ComponentValue::I64(23))]),
        )
        .unwrap()
        .unwrap();

    let error = prepared.apply(&mut direct_world, target).unwrap_err();

    assert!(matches!(
        error,
        nara::reflect::ComponentCodecError::PersistentApplyRejected {
            reason: PersistentApplyRejection::RequiredComponents,
            ..
        }
    ));
    assert!(direct_world.get::<CompositionProbe>(target).is_none());
    assert!(direct_world.get::<ImplicitDependency>(target).is_none());
}

#[test]
fn post_publication_runtime_observer_runs_but_blocks_the_next_persistent_apply() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("observed").unwrap();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 31)]);
    let mut world = World::new();
    world.init_resource::<HookCanary>();
    let report = spawn_scene(&mut world, &registry, &scene);
    assert!(!report.diagnostics.has_errors());
    let entity = resolved_entity(&world, &report, &id);

    world.entity_mut(entity).observe(
        |_: On<Remove, CompositionProbe>, mut canary: ResMut<HookCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    world.entity_mut(entity).remove::<CompositionProbe>();
    assert_eq!(world.resource::<HookCanary>().0, 1);

    let prepared = registry
        .preflight_component(
            &component_id(),
            &ComponentValue::map([("value", ComponentValue::I64(32))]),
        )
        .unwrap()
        .unwrap();
    let error = prepared.apply(&mut world, entity).unwrap_err();
    assert!(matches!(
        error,
        nara::reflect::ComponentCodecError::PersistentApplyRejected {
            reason: PersistentApplyRejection::Observer {
                event: PersistentLifecycleEvent::Remove,
                scope: PersistentObserverScope::EntityComponent,
            },
            ..
        }
    ));
    assert!(world.get::<CompositionProbe>(entity).is_none());
    assert_eq!(world.resource::<HookCanary>().0, 1);
}

#[test]
fn component_global_observer_rejects_a_fresh_scene_without_running() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("observed").unwrap();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 41)]);
    let mut world = World::new();
    world.init_resource::<HookCanary>();
    world.add_observer(
        |_: On<Add, CompositionProbe>, mut canary: ResMut<HookCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &scene);

    assert!(report.instance.is_none());
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline
    );
    assert_eq!(world.resource::<HookCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn identity_resource_observer_rejects_before_scene_allocation() {
    let registry = frozen_registry();
    let id = SceneEntityId::new("identity-observed").unwrap();
    let scene = SceneDocument::new([record(&id, ComponentSchemaVersion::new(2).unwrap(), 51)]);
    let mut world = World::new();
    world.init_resource::<HookCanary>();
    world.add_observer(
        |_: On<Add, WorldIdentityDomain>, mut canary: ResMut<HookCanary>| {
            canary.0 += 1;
        },
    );
    world.flush();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report = spawn_scene(&mut world, &registry, &scene);

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
    assert_eq!(world.resource::<HookCanary>().0, 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn prepared_component_cannot_be_forged_outside_the_registry() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail(
        "tests/ui/scene_component_composition/prepared_component_not_constructible.rs",
    );
}
