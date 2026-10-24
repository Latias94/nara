use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::{AssetRef, AssetServer, Handle};
use nara_ecs::{Component, World};
use nara_identity::{EntityLookup, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PreparedComponentCandidate,
};
use nara_scene::{
    ExpandedSceneCandidate, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneEntitySource, SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind, prepare_scene,
    prepare_scene_replacement, retire_and_despawn_scene_entity, retire_and_despawn_scene_instance,
    spawn_scene,
};

#[derive(Component)]
struct Probe {
    value: i64,
    asset: Option<Handle<String>>,
    drops: Arc<AtomicUsize>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn registry(runs: &Arc<AtomicUsize>, drops: &Arc<AtomicUsize>, assets: bool) -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let schema = ComponentSchema::new(component(), "Prepared probe", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let runs = Arc::clone(runs);
    let drops = Arc::clone(drops);
    registry
        .register_persistent_component_codec_with_context::<Probe, _, _>(
            schema,
            move |value, _| {
                let value = value
                    .get("value")
                    .and_then(ComponentValue::as_i64)
                    .ok_or_else(|| ComponentCodecError::invalid_field("value", "i64"))?;
                let runs = Arc::clone(&runs);
                let drops = Arc::clone(&drops);
                if assets {
                    Ok(PreparedComponentCandidate::with_asset_server(
                        move |context| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            let asset = context
                                .resolve_asset_ref::<String>(
                                    &AssetRef::path("textures/prepared.png").unwrap(),
                                )
                                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                            Ok(Probe {
                                value,
                                asset: Some(asset),
                                drops,
                            })
                        },
                    ))
                } else {
                    Ok(PreparedComponentCandidate::deferred(move || {
                        runs.fetch_add(1, Ordering::SeqCst);
                        if value < 0 {
                            return Err(ComponentCodecError::Message(
                                "rejected native value".into(),
                            ));
                        }
                        Ok(Probe {
                            value,
                            asset: None,
                            drops,
                        })
                    }))
                }
            },
            |probe, _, _| {
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::I64(probe.value),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("consumer.PreparedProbe")
}
fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}
fn slot(name: &str) -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("consumer.prepare", name).unwrap()
}

fn candidate(name: &str, values: &[i64]) -> ExpandedSceneCandidate {
    let source = SourceDocumentKey::named(
        "90d048a8-122e-4a23-9cf0-13ab9d3178e5".parse().unwrap(),
        name,
        SourceDocumentKind::Scene,
    )
    .unwrap();
    ExpandedSceneCandidate::plain(
        source,
        Arc::new(SceneDocument::from_ordered_roots(
            values.iter().enumerate().map(|(index, value)| {
                SceneEntityRecord::new(id(&format!("e{index}"))).with_component(
                    component(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([("value", ComponentValue::I64(*value))]),
                    ),
                )
            }),
        )),
    )
}

fn resolve(
    world: &World,
    receipt: &nara_scene::SpawnedSceneInstance,
    name: &str,
) -> nara_ecs::Entity {
    match receipt.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected live Scene entity: {other:?}"),
    }
}

#[test]
fn preparation_owns_actual_values_and_cancels_without_world_cleanup_or_target_allocations() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let mut world = World::new();
    let mut control = World::new();
    // Hierarchy's query baseline lazily installs Bevy's normal query-filter resource.
    control.init_resource::<nara_ecs::bevy_ecs::entity_disabling::DefaultQueryFilters>();
    let pending = prepare_scene(
        &mut world,
        &registry,
        &candidate("cancel", &[1, 2]),
        &slot("cancel"),
        None,
    )
    .unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        control
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>()
    );
    assert!(world.contains_resource::<nara_ecs::bevy_ecs::entity_disabling::DefaultQueryFilters>());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    drop(pending);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());

    let pending = prepare_scene(
        &mut world,
        &registry,
        &candidate("after-world", &[3]),
        &slot("after-world"),
        None,
    )
    .unwrap();
    drop(world);
    drop(pending);
    assert_eq!(drops.load(Ordering::SeqCst), 3);
}

#[test]
fn independent_tokens_commit_in_reverse_order_after_registry_drop_without_reexecuting_codecs() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let mut world = World::new();
    let first = prepare_scene(
        &mut world,
        &registry,
        &candidate("first", &[1]),
        &slot("first"),
        None,
    )
    .unwrap();
    let second = prepare_scene(
        &mut world,
        &registry,
        &candidate("second", &[2]),
        &slot("second"),
        None,
    )
    .unwrap();
    drop(registry);
    let second = second.commit(&mut world).instance.unwrap();
    let domain = world.resource::<WorldIdentityDomain>().id();
    let first = first.commit(&mut world).instance.unwrap();
    assert_eq!(world.resource::<WorldIdentityDomain>().id(), domain);
    assert_ne!(first.instance_id(), second.instance_id());
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_eq!(
        world
            .get::<Probe>(resolve(&world, &first, "e0"))
            .unwrap()
            .value,
        1
    );
    assert_eq!(
        world
            .get::<Probe>(resolve(&world, &second, "e0"))
            .unwrap()
            .value,
        2
    );
    retire_and_despawn_scene_instance(&mut world, &first).unwrap();
    retire_and_despawn_scene_instance(&mut world, &second).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}

#[test]
fn replacement_targets_the_activation_after_individual_retirement_and_rejects_a_stale_receipt() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let mut world = World::new();
    let old = spawn_scene(
        &mut world,
        &registry,
        &candidate("old", &[1, 2]),
        &slot("replace"),
        None,
    )
    .instance
    .unwrap();
    let replacement_source = candidate("new", &[3]);
    let replacement =
        prepare_scene_replacement(&mut world, &registry, &replacement_source, &old, None).unwrap();
    let stale =
        prepare_scene_replacement(&mut world, &registry, &candidate("stale", &[4]), &old, None)
            .unwrap();
    let removed = resolve(&world, &old, "e1");
    retire_and_despawn_scene_entity(&mut world, removed).unwrap();
    let current = replacement.commit(&mut world).instance.unwrap();
    let entity = resolve(&world, &current, "e0");
    let source = world.get::<SceneEntitySource>(entity).unwrap();
    assert_eq!(source.containing_source(), replacement_source.source_key());
    assert_eq!(source.occurrence(), &slot("replace"));
    let before = world.resource::<WorldIdentityDomain>().stats();
    let report = stale.commit(&mut world);
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
    assert_eq!(world.get::<Probe>(entity).unwrap().value, 3);
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
}

#[test]
fn native_prepare_failure_releases_partial_values_without_allocating_scene_targets() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let mut world = World::new();
    let report = match prepare_scene(
        &mut world,
        &registry,
        &candidate("failure", &[1, -1]),
        &slot("failure"),
        None,
    ) {
        Ok(_) => panic!("the final native value must reject"),
        Err(report) => report,
    };
    assert!(report.diagnostics.has_errors());
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(world.spawn_empty().id(), World::new().spawn_empty().id());
}

#[test]
fn late_hierarchy_observer_rejects_before_scene_target_allocation() {
    use nara_ecs::bevy_ecs::{lifecycle::Add, observer::On};
    use nara_hierarchy::Parent;

    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let source = candidate("tree", &[]).source_key().clone();
    let tree = ExpandedSceneCandidate::plain(
        source,
        Arc::new(SceneDocument {
            roots: vec![id("parent")],
            entities: vec![
                SceneEntityRecord::new(id("parent")).with_children([id("child")]),
                SceneEntityRecord::new(id("child")),
            ],
            ..Default::default()
        }),
    );
    let mut world = World::new();
    let mut control = World::new();
    let pending = prepare_scene(&mut world, &registry, &tree, &slot("tree"), None).unwrap();
    drop(prepare_scene(&mut control, &registry, &tree, &slot("tree"), None).unwrap());
    for world in [&mut world, &mut control] {
        world.add_observer(|_: On<Add<Parent>>| {});
        world.flush();
    }
    let report = pending.commit(&mut world);
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
}

#[test]
fn managed_registry_loss_is_not_a_standalone_definition() {
    use nara_app::{App, RuntimeInstance};
    use nara_ecs::ScheduleLabel;
    use nara_reflect::{
        ComponentRegistryAuthorityError, ComponentRegistryPlugin, component_registry,
        validate_component_registry_authority,
    };

    // Corrupt one public resource at a time, without naming Reflect's private resource type.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct CorruptRegistry;
    let mut baseline = App::new();
    baseline.add_plugin(ComponentRegistryPlugin).unwrap();
    let count = baseline
        .seal()
        .unwrap()
        .world()
        .resource_entities()
        .iter()
        .count();
    let found_registry = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::new(AtomicUsize::new(0));
    for (index, retained) in (0..count).flat_map(|index| [(index, false), (index, true)]) {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        let found = Arc::clone(&found_registry);
        let accepted = Arc::clone(&accepted);
        app.add_systems(CorruptRegistry, move |world: &mut World| {
            let registry = ComponentRegistry::from_snapshot(
                component_registry(world).unwrap().snapshot().unwrap(),
            );
            let pending = retained.then(|| {
                prepare_scene(
                    world,
                    &registry,
                    &candidate("managed-missing", &[]),
                    &slot("managed"),
                    None,
                )
                .unwrap()
            });
            let (component, entity) = world.resource_entities().iter().nth(index).unwrap();
            world.entity_mut(entity).remove_by_id(component);
            if validate_component_registry_authority(world)
                == Err(ComponentRegistryAuthorityError::MissingRegistry)
            {
                found.fetch_add(1, Ordering::SeqCst);
                let was_accepted = match pending {
                    Some(pending) => pending.commit(world).instance.is_some(),
                    None => prepare_scene(
                        world,
                        &registry,
                        &candidate("managed-missing", &[]),
                        &slot("managed"),
                        None,
                    )
                    .is_ok(),
                };
                accepted.fetch_add(usize::from(was_accepted), Ordering::SeqCst);
            }
        })
        .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let _ = runtime.run_schedule(CorruptRegistry);
    }
    assert_eq!(
        found_registry.load(Ordering::SeqCst),
        2,
        "both probes must reach the actual registry owner"
    );
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        0,
        "a broken managed owner cannot become standalone"
    );
}

#[test]
fn managed_definition_requires_the_exact_snapshot_not_a_structurally_equal_registry() {
    use nara_app::{App, RuntimeInstance};
    use nara_ecs::ScheduleLabel;
    use nara_reflect::{ComponentRegistryPlugin, component_registry};
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct CompareDefinitions;
    let result = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&result);
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    app.add_systems(CompareDefinitions, move |world: &mut World| {
        let compatible = ComponentRegistry::from_snapshot(
            component_registry(world).unwrap().snapshot().unwrap(),
        );
        let mut different = ComponentRegistry::new();
        different.freeze().unwrap();
        let source = candidate("managed", &[]);
        let rejected = prepare_scene(world, &different, &source, &slot("managed"), None).is_err();
        let prepared = prepare_scene(world, &compatible, &source, &slot("managed"), None).unwrap();
        drop(compatible);
        let published = prepared.commit(world).instance.unwrap();
        retire_and_despawn_scene_instance(world, &published).unwrap();
        observed.store(1 + usize::from(rejected), Ordering::SeqCst);
    })
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.run_schedule(CompareDefinitions).unwrap();
    assert_eq!(result.load(Ordering::SeqCst), 2);
}

#[test]
fn foreign_world_rejection_does_not_flush_its_pending_commands() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let mut origin = World::new();
    let pending = prepare_scene(
        &mut origin,
        &registry,
        &candidate("foreign", &[1]),
        &slot("foreign"),
        None,
    )
    .unwrap();
    let mut foreign = World::new();
    let flushed = Arc::new(AtomicUsize::new(0));
    let flag = Arc::clone(&flushed);
    foreign.commands().queue(move |_: &mut World| {
        flag.fetch_add(1, Ordering::SeqCst);
    });
    let report = pending.commit(&mut foreign);
    assert!(report.instance.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.prepared-wrong-world")
    );
    assert_eq!(flushed.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    foreign.flush();
    assert_eq!(flushed.load(Ordering::SeqCst), 1);
}

#[test]
fn later_slot_occupancy_including_empty_instances_rejects_before_allocating_targets() {
    for empty in [false, true] {
        let runs = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let registry = registry(&runs, &drops, false);
        let source = candidate("contender", &[1]);
        let occupying = candidate("occupier", if empty { &[] } else { &[2] });
        let mut world = World::new();
        let mut control = World::new();
        let pending = prepare_scene(&mut world, &registry, &source, &slot("shared"), None).unwrap();
        drop(prepare_scene(&mut control, &registry, &source, &slot("shared"), None).unwrap());
        let current = spawn_scene(&mut world, &registry, &occupying, &slot("shared"), None)
            .instance
            .unwrap();
        let control_current =
            spawn_scene(&mut control, &registry, &occupying, &slot("shared"), None)
                .instance
                .unwrap();
        let before = world.resource::<WorldIdentityDomain>().stats();
        let report = pending.commit(&mut world);
        assert!(report.instance.is_none());
        assert!(report.diagnostics.has_errors());
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
        assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
        retire_and_despawn_scene_instance(&mut world, &current).unwrap();
        retire_and_despawn_scene_instance(&mut control, &control_current).unwrap();
    }
}

#[test]
fn retained_assets_use_only_the_original_touched_catalogue_basis() {
    for touches_assets in [false, true] {
        for changes_assets in [false, true] {
            let runs = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let registry = registry(&runs, &drops, touches_assets);
            let mut world = World::new();
            world.init_resource::<AssetServer>();
            let pending = prepare_scene(
                &mut world,
                &registry,
                &candidate("assets", &[1]),
                &slot("assets"),
                None,
            )
            .unwrap();
            assert_eq!(world.resource::<AssetServer>(), &AssetServer::default());
            let mut expected = world.resource::<AssetServer>().clone();
            if changes_assets {
                expected
                    .reserve::<String>("textures/unrelated.png")
                    .unwrap();
                let successor = expected.clone();
                world.commands().queue(move |world: &mut World| {
                    *world.resource_mut::<AssetServer>() = successor;
                });
            }
            let report = pending.commit(&mut world);
            assert_eq!(runs.load(Ordering::SeqCst), 1);
            if touches_assets && changes_assets {
                assert!(report.instance.is_none());
                assert!(report.diagnostics.has_errors());
                assert_eq!(drops.load(Ordering::SeqCst), 1);
                assert!(!world.contains_resource::<WorldIdentityDomain>());
                assert_eq!(world.resource::<AssetServer>(), &expected);
            } else {
                let receipt = report.instance.unwrap();
                let probe = world.get::<Probe>(resolve(&world, &receipt, "e0")).unwrap();
                assert_eq!(probe.asset.is_some(), touches_assets);
                if !touches_assets && changes_assets {
                    assert_eq!(world.resource::<AssetServer>(), &expected);
                }
                retire_and_despawn_scene_instance(&mut world, &receipt).unwrap();
            }
        }
    }
}

#[test]
fn queued_native_hook_rejects_before_target_allocation_and_drops_values_once() {
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let registry = registry(&runs, &drops, false);
    let source = candidate("late-native", &[1]);
    let mut world = World::new();
    let mut control = World::new();
    let pending =
        prepare_scene(&mut world, &registry, &source, &slot("late-native"), None).unwrap();
    drop(prepare_scene(&mut control, &registry, &source, &slot("late-native"), None).unwrap());
    world.commands().queue(|world: &mut World| {
        world.register_component_hooks::<Probe>().on_add(|_, _| {});
    });
    let report = pending.commit(&mut world);
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn explicit_projection_definition_preserves_the_distinct_managed_host_basis() {
    use nara_app::{App, RuntimeInstance};
    use nara_ecs::ScheduleLabel;
    use nara_reflect::{
        ComponentRegistryPlugin, component_registry, validate_component_registry_authority,
    };
    use nara_scene::SceneProjection;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct Project;
    let runs = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let native = registry(&runs, &drops, false);
    let observed = Arc::new(AtomicUsize::new(0));
    let result = Arc::clone(&observed);
    let prepared_runs = Arc::clone(&runs);
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    app.add_systems(Project, move |world: &mut World| {
        let host = component_registry(world).unwrap().snapshot().unwrap();
        assert!(!native.shares_snapshot(&host));
        let source = candidate("explicit-projection", &[7]);
        assert!(prepare_scene(world, &native, &source, &slot("explicit"), None).is_err());
        assert_eq!(prepared_runs.load(Ordering::SeqCst), 0);
        let mut projection = SceneProjection::new(slot("explicit"));
        let report = projection.publish(world, &native, &source, &Default::default());
        let receipt = report
            .instance
            .expect("explicit preview definition is not Host definition selection");
        assert_eq!(
            world
                .get::<Probe>(resolve(world, &receipt, "e0"))
                .unwrap()
                .value,
            7
        );
        assert!(component_registry(world).unwrap().shares_snapshot(&host));
        validate_component_registry_authority(world).unwrap();
        projection.clear(world).unwrap();
        result.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.run_schedule(Project).unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
