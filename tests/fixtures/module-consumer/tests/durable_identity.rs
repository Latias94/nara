#[path = "support/durable_identity.rs"]
mod support;

use bevy_ecs::{component::Component, entity::Entity, world::World};
use nara_asset::{AssetDatabaseError, AssetRef};
use nara_core::SourceUniverseId;
use nara_identity::{
    EntityLookup, EntityReference, IdentityDomainError, PersistentRetirementDisposition,
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    RuntimeEntityReference, SceneEntityId, SpawnedSceneInstance, TombstoneCause,
    WorldIdentityDomain, register_persistent_identity, retire_identity_entity,
    retire_identity_entity_with_disposition, spawn_identity_entity,
};
use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId};
use nara_scene::{
    InMemoryPrefabSourceResolver, Name, PrefabDocument, PrefabInstance, SceneDocument,
    SceneDocumentCandidate, SceneEntityRecord, SceneEntitySource, ScenePatchDocument,
    ScenePatchOperation, SourceDocumentKey, SourceDocumentKind, export_scene,
};
use nara_scene_module_consumer::frozen_scene_registry;
use serde::{Deserialize, Serialize};
use support::{
    ACTOR_ID, ACTOR_PATH, ConversionError, DURABLE_LINKS_ID, DurableLinks, SCENE_ID, SCENE_PATH,
    SHARED_ID, SHARED_PATH, copy_as_new, durable_links_document, durable_links_registry,
    durable_links_value, fixture_bytes, load_prefab, merge_source, merged_database,
    prefab_resolver, resolve_source, stabilize_v1_scene_bytes, stable_id, stable_scene,
};

#[test]
fn two_resolved_sources_merge_and_spawn_through_the_public_scene_module() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    assert_eq!(database.len(), 3);
    let scene = stable_scene(&registry, &database);
    let resolver = prefab_resolver(&database, &registry);
    let mut world = World::new();

    let spawned = {
        let expansion = (scene).expand_prefabs_with_asset_database(&registry, &resolver, &database);
        assert!(
            !expansion.diagnostics.has_errors(),
            "{:?}",
            expansion.diagnostics
        );
        let candidate = expansion
            .candidate
            .unwrap()
            .bind_source(SourceDocumentKey::stable_asset(
                stable_id(SCENE_ID),
                SourceDocumentKind::Scene,
            ));
        nara_scene::spawn_scene(
            &mut world,
            &registry,
            &candidate,
            &scene_occurrence(
                "two_resolved_sources_merge_and_spawn_through_the_public_scene_module/spawned",
            ),
            Some(&database),
        )
    };

    assert!(
        !spawned.diagnostics.has_errors(),
        "{:?}",
        spawned.diagnostics
    );
    let instance = spawned.instance.unwrap();
    assert_eq!(instance.len(), 5);
    let mut projected_shared = world.query::<(Entity, &SceneEntitySource, &Name)>();
    let projected_shared = projected_shared
        .iter(&world)
        .filter(|(_, source, name)| {
            source.instance_id == instance.instance_id()
                && source.provenance().local_entity_id().as_str() == "opaque/shared"
                && name.as_str() == "Shared"
        })
        .collect::<Vec<_>>();
    assert_eq!(projected_shared.len(), 2);
    let mut depths = projected_shared
        .iter()
        .map(|(entity, source, _)| {
            assert_eq!(
                instance.resolve(&world, &source.entity_id),
                EntityLookup::Resolved(*entity)
            );
            source.provenance().projection_path().unwrap().steps().len()
        })
        .collect::<Vec<_>>();
    depths.sort();
    assert_eq!(depths, [1, 2]);
}

#[test]
fn disposable_projection_keeps_repeated_prefab_provenance_in_bounded_epochs() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    let scene = stable_scene(&registry, &database);
    let resolver = prefab_resolver(&database, &registry);
    let mut world = World::new();
    let settings = nara_identity::WorldIdentityDomainSettings::new(
        nara_core::ItemLimit::new(6).unwrap(),
        nara_core::ItemLimit::ONE,
        nara_core::ItemLimit::ONE,
    );
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    world.insert_resource(domain);
    let occurrence = scene_occurrence("durable-identity/disposable-projection");
    let mut projection = nara_scene::SceneProjection::new(occurrence.clone());
    let expansion = scene.expand_prefabs_with_asset_database(&registry, &resolver, &database);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    let source_key =
        SourceDocumentKey::stable_asset(stable_id(SCENE_ID), SourceDocumentKind::Scene);
    let candidate = expansion.candidate.unwrap().bind_source(source_key.clone());
    let mut previous: Option<SpawnedSceneInstance> = None;
    for _ in 0..16 {
        let report = projection.publish(&mut world, &registry, &candidate, &database);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let instance = report.instance.unwrap();
        assert_eq!(projection.instance(), Some(&instance));
        assert_eq!(instance.len(), 5);
        let domain = world.resource::<WorldIdentityDomain>();
        assert_eq!(domain.stats().lifetime_claims, 6);
        assert_eq!(domain.stats().active_scene_entities, 5);
        assert_eq!(domain.settings(), settings);
        if let Some(previous) = &previous {
            assert_ne!(previous.domain_id(), instance.domain_id());
            for id in previous.entity_ids() {
                assert!(matches!(
                    previous.resolve(&world, id),
                    EntityLookup::WrongDomain { .. }
                ));
            }
        }
        let mut projected = world.query::<(&SceneEntitySource, &Name)>();
        let mut depths = projected
            .iter(&world)
            .filter_map(|(source, name)| {
                assert_eq!(source.containing_source(), &source_key);
                assert_eq!(source.occurrence(), &occurrence);
                (source.provenance().local_entity_id().as_str() == "opaque/shared"
                    && name.as_str() == "Shared")
                    .then(|| source.provenance().projection_path().unwrap().steps().len())
            })
            .collect::<Vec<_>>();
        depths.sort();
        assert_eq!(depths, [1, 2]);
        previous = Some(instance);
    }
    assert_eq!(projection.clear(&mut world).unwrap(), 5);
    assert!(projection.instance().is_none());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn duplicate_uuid_in_second_resolved_source_preserves_the_published_database() {
    let first = resolve_source(&[SCENE_PATH, ACTOR_PATH]);
    let second = resolve_source(&[SHARED_PATH, "source-b/collision.prefab.ron"]);
    let mut published = first.database;
    let before = published.clone();

    let error = merge_source(&mut published, &second).unwrap_err();

    assert!(matches!(
        error,
        AssetDatabaseError::DuplicateStableId { stable_id: id, .. } if id == stable_id(ACTOR_ID)
    ));
    assert_eq!(published, before);
    assert!(
        published
            .record_for_stable_id(stable_id(SHARED_ID))
            .is_none()
    );
}

#[test]
fn copy_as_new_changes_only_references_inside_the_explicit_copied_closure() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    let scene = stable_scene(&registry, &database);
    let actor = load_prefab(&database, &registry, stable_id(ACTOR_ID));
    let shared = load_prefab(&database, &registry, stable_id(SHARED_ID));
    let original_bytes =
        [SCENE_PATH, ACTOR_PATH, SHARED_PATH].map(|path| (path, fixture_bytes(path)));

    let copied = copy_as_new(&scene, &actor, &database);

    assert_eq!(copied.stable_ids.len(), 2);
    assert_ne!(copied.stable_ids[&stable_id(SCENE_ID)], stable_id(SCENE_ID));
    let copied_actor = copied.stable_ids[&stable_id(ACTOR_ID)];
    let copied_scene = copied.stable_ids[&stable_id(SCENE_ID)];
    assert_ne!(copied_actor, stable_id(ACTOR_ID));
    assert_ne!(copied_actor, copied.stable_ids[&stable_id(SCENE_ID)]);
    assert_eq!(copied.database.len(), 5);
    assert_eq!(
        scene.entities[0].prefab.as_ref().unwrap().source,
        AssetRef::StableId(stable_id(ACTOR_ID))
    );
    assert_eq!(
        copied.scene.entities[0].prefab.as_ref().unwrap().source,
        AssetRef::StableId(copied_actor)
    );
    assert_eq!(copied.scene.entities[1], scene.entities[1]);
    assert_eq!(copied.actor, actor);
    let copied_name = copied.actor.entities[0].components.values().next().unwrap();
    assert_eq!(copied_name.value.as_str().unwrap(), ACTOR_PATH);
    for (path, bytes) in original_bytes {
        assert_eq!(fixture_bytes(path), bytes);
    }

    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            AssetRef::StableId(copied_actor),
            SourceDocumentKey::stable_asset(copied_actor, SourceDocumentKind::Prefab),
            copied.actor,
        )
        .with_prefab(
            AssetRef::StableId(stable_id(SHARED_ID)),
            SourceDocumentKey::stable_asset(stable_id(SHARED_ID), SourceDocumentKind::Prefab),
            shared,
        );
    let mut world = World::new();
    let spawned = {
        let expansion = (copied.scene).expand_prefabs_with_asset_database(
            &registry,
            &resolver,
            &copied.database,
        );
        assert!(
            !expansion.diagnostics.has_errors(),
            "{:?}",
            expansion.diagnostics
        );
        let candidate = expansion
            .candidate
            .unwrap()
            .bind_source(SourceDocumentKey::stable_asset(
                copied_scene,
                SourceDocumentKind::Scene,
            ));
        nara_scene::spawn_scene(
            &mut world,
            &registry,
            &candidate,
            &scene_occurrence(
                "copy_as_new_changes_only_references_inside_the_explicit_copied_closure/spawned",
            ),
            Some(&copied.database),
        )
    };
    assert!(
        !spawned.diagnostics.has_errors(),
        "{:?}",
        spawned.diagnostics
    );
    assert_eq!(spawned.instance.unwrap().len(), 5);
}

#[test]
fn explicit_v1_conversion_publishes_a_real_candidate_and_preserves_unresolved_bytes() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    let mut encoded = fixture_bytes(SCENE_PATH);
    let original = encoded.clone();

    stabilize_v1_scene_bytes(&mut encoded, &registry, &database).unwrap();

    assert_ne!(encoded, original);
    let converted = SceneDocumentCandidate::decode_ron_bytes(&encoded)
        .unwrap()
        .canonicalize(&registry)
        .unwrap()
        .publish_with_asset_database(&registry, &database)
        .unwrap();
    let after = SceneDocumentCandidate::decode_ron_bytes(&fixture_bytes("after/startup.scene.ron"))
        .unwrap()
        .canonicalize(&registry)
        .unwrap()
        .publish_with_asset_database(&registry, &database)
        .unwrap();
    assert_eq!(converted.document(), after.document());

    let mut unresolved = original;
    let before = unresolved.clone();
    let first_only = resolve_source(&[SCENE_PATH, ACTOR_PATH]);
    let error =
        stabilize_v1_scene_bytes(&mut unresolved, &registry, &first_only.database).unwrap_err();
    assert!(
        matches!(error, ConversionError::UnresolvedPrefab { ref entity, .. }
        if entity.as_str() == "shared-instance")
    );
    assert!(error.to_string().contains(SHARED_PATH));
    assert_eq!(unresolved, before);
}

/// Application-private save data binds an authored local ID to one persistent instance owner.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedAuthoredBinding {
    instance_owner: PersistentRuntimeReference,
    authored_entity: SceneEntityId,
}

#[derive(Component)]
struct InstanceOwner(SpawnedSceneInstance);

#[test]
fn a_saved_authored_binding_selects_one_restored_instance_after_spawn_order_changes() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    let scene = stable_scene(&registry, &database);
    let resolver = prefab_resolver(&database, &registry);
    let alpha = persistent_owner("11111111-1111-4111-8111-111111111111");
    let beta = persistent_owner("22222222-2222-4222-8222-222222222222");
    let local = SceneEntityId::new("actor-instance").unwrap();
    let mut original = World::new();
    let original_alpha = spawn_owned_instance(
        &mut original,
        &registry,
        &scene,
        &resolver,
        &database,
        alpha.clone(),
        &scene_occurrence("durable-identity/alpha"),
    );
    let original_beta = spawn_owned_instance(
        &mut original,
        &registry,
        &scene,
        &resolver,
        &database,
        beta.clone(),
        &scene_occurrence("durable-identity/beta"),
    );
    assert_ne!(original_alpha.instance_id(), original_beta.instance_id());
    let saved = SavedAuthoredBinding {
        instance_owner: alpha.clone(),
        authored_entity: local.clone(),
    };
    let encoded = ron::ser::to_string(&saved).unwrap();
    let saved: SavedAuthoredBinding = ron::from_str(&encoded).unwrap();
    drop(original);

    let mut restored = World::new();
    let restored_beta = spawn_owned_instance(
        &mut restored,
        &registry,
        &scene,
        &resolver,
        &database,
        beta,
        &scene_occurrence("durable-identity/beta"),
    );
    let restored_alpha = spawn_owned_instance(
        &mut restored,
        &registry,
        &scene,
        &resolver,
        &database,
        alpha,
        &scene_occurrence("durable-identity/alpha"),
    );
    assert_ne!(original_alpha.instance_id(), restored_alpha.instance_id());
    assert_eq!(original_alpha.instance_id(), restored_beta.instance_id());
    let EntityLookup::Resolved(owner_entity) = restored
        .resource::<WorldIdentityDomain>()
        .resolve_entity_reference(
            &restored,
            &EntityReference::Persistent {
                entity: saved.instance_owner,
            },
            None,
        )
    else {
        panic!("the saved instance owner UUID must resolve");
    };
    let selected = &restored.get::<InstanceOwner>(owner_entity).unwrap().0;
    assert_eq!(selected.instance_id(), restored_alpha.instance_id());
    let target = selected.resolve(&restored, &saved.authored_entity);
    assert_eq!(target, restored_alpha.resolve(&restored, &local));
    assert_ne!(target, restored_beta.resolve(&restored, &local));
    assert!(matches!(target, EntityLookup::Resolved(_)));
}

#[derive(Debug, Component, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NpcState {
    visits: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedNpc {
    identity: PersistentRuntimeReference,
    state: NpcState,
}

#[test]
fn a_persistent_npc_rematerializes_without_reloading_its_scene_owner() {
    let registry = frozen_scene_registry().unwrap();
    let database = merged_database();
    let scene = stable_scene(&registry, &database);
    let resolver = prefab_resolver(&database, &registry);
    let mut world = World::new();
    let instance = spawn_owned_instance(
        &mut world,
        &registry,
        &scene,
        &resolver,
        &database,
        persistent_owner("11111111-1111-4111-8111-111111111111"),
        &scene_occurrence("durable-identity/instance"),
    );
    let scene_claims = world
        .resource::<WorldIdentityDomain>()
        .stats()
        .lifetime_claims;
    let mut saved = SavedNpc {
        identity: persistent_owner("33333333-3333-4333-8333-333333333333"),
        state: NpcState { visits: 0 },
    };
    let mut previous_runtime = None;

    for expected_visit in 0..32 {
        let saved_bytes = ron::ser::to_string(&saved).unwrap();
        assert!(!saved_bytes.contains("incarnation"));
        saved = ron::from_str(&saved_bytes).unwrap();
        let token = spawn_identity_entity(&mut world).unwrap();
        world.entity_mut(token.entity()).insert(saved.state.clone());
        let locator =
            register_persistent_identity(&mut world, token, saved.identity.clone()).unwrap();
        let domain = world.resource::<WorldIdentityDomain>();
        if let Some(previous) = &previous_runtime {
            assert_eq!(domain.lookup(&world, previous), EntityLookup::StaleRuntime);
        }
        assert_eq!(domain.stats().persistent_logical_records, 2);
        assert_eq!(domain.stats().lifetime_claims, scene_claims);
        assert_eq!(
            domain.resolve_entity_reference(
                &world,
                &EntityReference::Persistent {
                    entity: saved.identity.clone()
                },
                None
            ),
            EntityLookup::Resolved(token.entity())
        );
        assert!(matches!(
            instance.resolve(&world, &SceneEntityId::new("actor-instance").unwrap()),
            EntityLookup::Resolved(_)
        ));
        let mut state = world.get_mut::<NpcState>(token.entity()).unwrap();
        assert_eq!(state.visits, expected_visit);
        state.visits += 1;
        saved.state = state.clone();
        retire_identity_entity(&mut world, token, TombstoneCause::Unloaded).unwrap();
        assert_eq!(
            world
                .resource::<WorldIdentityDomain>()
                .resolve_entity_reference(
                    &world,
                    &EntityReference::Persistent {
                        entity: saved.identity.clone()
                    },
                    None
                ),
            EntityLookup::Unloaded
        );
        assert!(world.despawn(token.entity()));
        previous_runtime = Some(locator.entity().clone());
    }
    assert_eq!(saved.state.visits, 32);
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .persistent_logical_records,
        2
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .lifetime_claims,
        scene_claims
    );
}

#[test]
fn nested_durable_references_preserve_values_across_residency_and_default_matrix() {
    let persistent = persistent_owner("44444444-4444-4444-8444-444444444444");
    let reference = EntityReference::Persistent {
        entity: persistent.clone(),
    };
    let component = ComponentTypeId::new(DURABLE_LINKS_ID);
    let source_id = SceneEntityId::new("source").unwrap();
    for required in [true, false] {
        let registry = durable_links_registry(required, &reference);
        for omitted in [true, false] {
            let document = durable_links_document(omitted, &reference);
            let original = document.clone();
            let encoded = document.to_json_string().unwrap();
            assert!(!encoded.contains("incarnation"));
            let published = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
                .unwrap()
                .canonicalize(&registry)
                .unwrap()
                .publish(&registry)
                .unwrap();
            assert_eq!(published.document(), &document);
            let mut world = World::new();
            let spawned = nara_scene::spawn_scene(
                &mut world,
                &registry,
                &nara_scene::ExpandedSceneCandidate::plain(
                    scene_source_key(
                        "nested_durable_references_preserve_values_across_residency_and_default_matrix/document",
                    ),
                    std::sync::Arc::new((document).clone()),
                ),
                &scene_occurrence(
                    "nested_durable_references_preserve_values_across_residency_and_default_matrix/spawned",
                ),
                None,
            );
            assert!(
                !spawned.diagnostics.has_errors(),
                "{:?}",
                spawned.diagnostics
            );
            let instance = spawned.instance.unwrap();
            let EntityLookup::Resolved(source) = instance.resolve(&world, &source_id) else {
                panic!("source must spawn independently of durable target residency");
            };
            assert_eq!(
                world.get::<DurableLinks>(source).unwrap().0,
                durable_links_value(&reference)
            );
            assert_eq!(
                world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(&world, &reference, None),
                EntityLookup::Missing,
            );
            let missing_export = export_scene(&world, &registry);
            assert!(missing_export.output().is_none());
            assert!(missing_export.diagnostics.has_errors());

            let token = spawn_identity_entity(&mut world).unwrap();
            world
                .entity_mut(token.entity())
                .insert(NpcState { visits: 0 });
            let first =
                register_persistent_identity(&mut world, token, persistent.clone()).unwrap();
            assert_eq!(
                world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(&world, &reference, None),
                EntityLookup::Resolved(token.entity()),
            );
            apply_npc_visit(&mut world, first.entity()).unwrap();
            let live_export = export_scene(&world, &registry);
            assert!(
                !live_export.diagnostics.has_errors(),
                "{:?}",
                live_export.diagnostics
            );
            let exported =
                &live_export.output().unwrap().document.entities[0].components[&component].value;
            assert_eq!(
                exported.field("links").unwrap(),
                &durable_links_value(&reference)
            );

            retire_identity_entity(&mut world, token, TombstoneCause::Unloaded).unwrap();
            assert_eq!(
                apply_npc_visit(&mut world, first.entity()),
                Err(EntityLookup::Unloaded)
            );
            assert_eq!(world.get::<NpcState>(token.entity()).unwrap().visits, 1);
            assert!(world.despawn(token.entity()));
            assert_eq!(
                world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(&world, &reference, None),
                EntityLookup::Unloaded,
            );
            assert_eq!(
                world.get::<DurableLinks>(source).unwrap().0,
                durable_links_value(&reference)
            );
            assert_eq!(document, original);
            assert_eq!(document.to_json_string().unwrap(), encoded);
            let unloaded_spawn = nara_scene::spawn_scene(
                &mut world,
                &registry,
                &nara_scene::ExpandedSceneCandidate::plain(
                    scene_source_key(
                        "nested_durable_references_preserve_values_across_residency_and_default_matrix/document",
                    ),
                    std::sync::Arc::new((document).clone()),
                ),
                &scene_occurrence(
                    "nested_durable_references_preserve_values_across_residency_and_default_matrix/unloaded_spawn",
                ),
                None,
            );
            assert!(
                !unloaded_spawn.diagnostics.has_errors(),
                "{:?}",
                unloaded_spawn.diagnostics
            );
            assert!(export_scene(&world, &registry).output().is_none());

            let prefab_source = AssetRef::path("prefabs/durable-links.prefab.ron").unwrap();
            let universe: SourceUniverseId =
                "cc25ede9-e5af-48d7-b105-63a0b33bc74a".parse().unwrap();
            let key = SourceDocumentKey::named(
                universe,
                "consumer.durable.links",
                SourceDocumentKind::Prefab,
            )
            .unwrap();
            let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
                prefab_source.clone(),
                key,
                PrefabDocument::new(document.roots.clone(), document.entities.clone()),
            );
            let anchors = ["first", "second"].map(|id| {
                let mut anchor = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
                anchor.prefab = Some(PrefabInstance {
                    source: prefab_source.clone(),
                    overrides: None,
                });
                anchor
            });
            let expansion =
                SceneDocument::from_ordered_roots(anchors).expand_prefabs(&registry, &resolver);
            assert!(
                !expansion.diagnostics.has_errors(),
                "{:?}",
                expansion.diagnostics
            );
            let expanded = expansion.document().unwrap();
            let linked = expanded
                .entities
                .iter()
                .filter_map(|entity| entity.components.get(&component))
                .collect::<Vec<_>>();
            assert_eq!(linked.len(), 2);
            for record in linked {
                assert_eq!(
                    record.value.field("links").unwrap(),
                    &durable_links_value(&reference)
                );
            }
            let mut patched = durable_links_document(false, &reference);
            let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
                entity: source_id.clone(),
                component: component.clone(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("links"),
            }]);
            let patch_report = patch.apply_to_scene(&mut patched, &registry);
            assert!(patch_report.applied, "{:?}", patch_report.diagnostics);
            assert!(
                patched.entities[0].components[&component]
                    .value
                    .get("links")
                    .is_none()
            );

            let replacement = spawn_identity_entity(&mut world).unwrap();
            world
                .entity_mut(replacement.entity())
                .insert(NpcState { visits: 10 });
            let current =
                register_persistent_identity(&mut world, replacement, persistent.clone()).unwrap();
            assert_eq!(
                apply_npc_visit(&mut world, first.entity()),
                Err(EntityLookup::StaleRuntime)
            );
            assert_eq!(
                world.get::<NpcState>(replacement.entity()).unwrap().visits,
                10
            );
            apply_npc_visit(&mut world, current.entity()).unwrap();
            assert_eq!(
                world.get::<NpcState>(replacement.entity()).unwrap().visits,
                11
            );
            retire_identity_entity_with_disposition(
                &mut world,
                replacement,
                TombstoneCause::Despawned,
                PersistentRetirementDisposition::Delete,
            )
            .unwrap();
            assert_eq!(
                apply_npc_visit(&mut world, current.entity()),
                Err(EntityLookup::Deleted)
            );
            assert_eq!(
                apply_npc_visit(&mut world, first.entity()),
                Err(EntityLookup::StaleRuntime)
            );
            assert_eq!(
                world.get::<NpcState>(replacement.entity()).unwrap().visits,
                11
            );
            assert_eq!(
                world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(&world, &reference, None),
                EntityLookup::Deleted,
            );
            assert_eq!(
                world.get::<DurableLinks>(source).unwrap().0,
                durable_links_value(&reference)
            );
            assert_eq!(document, original);
            assert!(!document.validate(&registry).has_errors());
            let deleted_spawn = nara_scene::spawn_scene(
                &mut world,
                &registry,
                &nara_scene::ExpandedSceneCandidate::plain(
                    scene_source_key(
                        "nested_durable_references_preserve_values_across_residency_and_default_matrix/document",
                    ),
                    std::sync::Arc::new((document).clone()),
                ),
                &scene_occurrence(
                    "nested_durable_references_preserve_values_across_residency_and_default_matrix/deleted_spawn",
                ),
                None,
            );
            assert!(
                !deleted_spawn.diagnostics.has_errors(),
                "{:?}",
                deleted_spawn.diagnostics
            );
            assert!(export_scene(&world, &registry).output().is_none());
            let forbidden = spawn_identity_entity(&mut world).unwrap();
            assert!(matches!(
                register_persistent_identity(&mut world, forbidden, persistent.clone()),
                Err(IdentityDomainError::PersistentIdentityDeleted { .. }),
            ));
        }
    }
}

fn apply_npc_visit(world: &mut World, target: &RuntimeEntityReference) -> Result<(), EntityLookup> {
    let outcome = world
        .resource::<WorldIdentityDomain>()
        .lookup(world, target);
    let EntityLookup::Resolved(entity) = outcome else {
        return Err(outcome);
    };
    world
        .get_mut::<NpcState>(entity)
        .expect("visit commands target NPCs")
        .visits += 1;
    Ok(())
}

fn persistent_owner(id: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("durable-identity-consumer").unwrap(),
        PersistentRuntimeId::parse_str(id).unwrap(),
    )
}

fn spawn_owned_instance(
    world: &mut World,
    registry: &nara_reflect::ComponentRegistry,
    scene: &nara_scene::SceneDocument,
    resolver: &InMemoryPrefabSourceResolver,
    database: &nara_asset::ProjectAssetDatabase,
    owner: PersistentRuntimeReference,
    occurrence: &nara_scene::SceneOccurrenceKey,
) -> SpawnedSceneInstance {
    let report = {
        let expansion = (scene).expand_prefabs_with_asset_database(registry, resolver, database);
        assert!(
            !expansion.diagnostics.has_errors(),
            "{:?}",
            expansion.diagnostics
        );
        let candidate = expansion
            .candidate
            .unwrap()
            .bind_source(SourceDocumentKey::stable_asset(
                stable_id(SCENE_ID),
                SourceDocumentKind::Scene,
            ));
        nara_scene::spawn_scene(world, registry, &candidate, occurrence, Some(database))
    };
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report.instance.unwrap();
    let token = spawn_identity_entity(world).unwrap();
    register_persistent_identity(world, token, owner).unwrap();
    world
        .entity_mut(token.entity())
        .insert(InstanceOwner(instance.clone()));
    instance
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
