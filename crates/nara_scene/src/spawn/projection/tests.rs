use super::*;
use crate::{
    Name, SceneComponentRecord, SceneDocument, SceneEntityRecord, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind, register_scene_components, spawn_scene,
};
use nara_asset::ProjectAssetDatabase;
use nara_core::ItemLimit;
use nara_ecs::{Resource, bevy_ecs::lifecycle::HookContext, bevy_ecs::world::DeferredWorld};
use nara_identity::{
    EntityLookup, PersistentRetirementDisposition, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, register_persistent_identity,
    resolve_in_world, retire_identity_entity_with_disposition, spawn_identity_entity,
};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Resource)]
struct RetainedResource(Arc<AtomicUsize>);

#[derive(Component)]
struct Unmanaged(u32);

#[derive(Component)]
#[component(on_remove = record_removal)]
struct NativeState;

#[derive(Resource, Default)]
struct Removals(usize);

fn record_removal(mut world: DeferredWorld<'_>, _context: HookContext) {
    world.resource_mut::<Removals>().0 += 1;
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn named_scene(name: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root")).with_component(
        ComponentTypeId::new("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(name.to_owned()),
        ),
    )])
}

fn bound_scene(document: SceneDocument) -> ExpandedSceneCandidate {
    let source = SourceDocumentKey::named(
        "a2e25c7c-56c1-4d70-983a-bcc09985ced3".parse().unwrap(),
        "projection.source",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    ExpandedSceneCandidate::plain(source, Arc::new(document))
}

fn occurrence(slot: &str) -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("projection-tests", slot).unwrap()
}

fn resolved_entity(world: &World, instance: &SpawnedSceneInstance, id: &SceneEntityId) -> Entity {
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("published projection must resolve: {lookup:?}"),
    }
}

fn assert_rejected(report: &SceneSpawnReport, code: &str) {
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(report.retired_entities(), 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "expected {code}: {:?}",
        report.diagnostics
    );
}

struct ProjectionFixture {
    world: World,
    registry: ComponentRegistry,
    database: ProjectAssetDatabase,
    projection: SceneProjection,
    id: SceneEntityId,
    entity: Entity,
}

impl ProjectionFixture {
    fn new() -> Self {
        let mut world = World::new();
        let registry = registry();
        let database = ProjectAssetDatabase::default();
        let mut projection = SceneProjection::new(occurrence("projection"));
        let report = projection.publish(
            &mut world,
            &registry,
            &bound_scene(named_scene("last-good")),
            database.resolver(&nara_asset::AssetPathScope::ROOT),
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let id = scene_id("root");
        let entity = resolved_entity(&world, projection.instance().unwrap(), &id);
        Self {
            world,
            registry,
            database,
            projection,
            id,
            entity,
        }
    }

    fn prepare(&mut self, candidate: &ExpandedSceneCandidate) -> PreparedScenePublication {
        let bindings = SceneAssetBindings::from_world(&self.world);
        SceneSpawner::new()
            .prepare_with_asset_context_and_product_bindings(
                &mut self.world,
                &self.registry,
                candidate,
                &self.projection.occurrence,
                Some(self.database.resolver(&nara_asset::AssetPathScope::ROOT)),
                SceneIdentityCommit::Projection(self.projection.instance()),
                None,
                bindings,
                &SceneNativeRetirementClaims::default(),
            )
            .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics))
    }

    fn assert_last_good(&self, current: &SpawnedSceneInstance) {
        assert_eq!(self.projection.instance(), Some(current));
        assert_eq!(
            self.world.resource::<WorldIdentityDomain>().id(),
            current.domain_id()
        );
        assert_eq!(
            current.resolve(&self.world, &self.id),
            EntityLookup::Resolved(self.entity)
        );
        assert_eq!(
            resolve_in_world(&self.world, &current.locator(&self.id).unwrap()),
            EntityLookup::Resolved(self.entity)
        );
        assert_eq!(
            self.world.get::<Name>(self.entity).unwrap().as_str(),
            "last-good"
        );
    }
}

#[test]
fn repeated_epochs_bound_claims_without_replacing_the_world_or_unmanaged_state() {
    let mut world = World::new();
    let world_id = world.id();
    let retained = Arc::new(AtomicUsize::new(7));
    world.insert_resource(RetainedResource(Arc::clone(&retained)));
    let unmanaged = world.spawn(Unmanaged(42)).id();
    let settings = WorldIdentityDomainSettings::new(
        ItemLimit::new(2).unwrap(),
        ItemLimit::ONE,
        ItemLimit::ONE,
    );
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    let mut previous_domain = domain.id();
    world.insert_resource(domain);
    let registry = registry();
    let database = ProjectAssetDatabase::default();
    let mut projection = SceneProjection::new(occurrence("projection"));
    let id = scene_id("root");
    let mut history: Vec<(SpawnedSceneInstance, Entity)> = Vec::new();
    let mut initial_population = None;

    for epoch in 0..16 {
        let name = format!("epoch-{epoch}");
        let report = projection.publish(
            &mut world,
            &registry,
            &bound_scene(named_scene(&name)),
            database.resolver(&nara_asset::AssetPathScope::ROOT),
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(report.retired_entities(), usize::from(epoch != 0));
        let current = report.instance.as_ref().unwrap();
        assert_eq!(projection.instance(), Some(current));
        let entity = resolved_entity(&world, current, &id);
        let domain = world.resource::<WorldIdentityDomain>();
        assert_ne!(domain.id(), previous_domain);
        assert_eq!(domain.id(), current.domain_id());
        assert_eq!(domain.settings(), settings);
        let stats = domain.stats();
        assert_eq!(stats.lifetime_claims, 2);
        assert_eq!(stats.claimed_scene_instances, 1);
        assert_eq!(stats.active_scene_instances, 1);
        assert_eq!(stats.active_scene_entities, 1);
        assert_eq!(stats.registered_entities, 1);
        assert_eq!(stats.persistent_logical_records, 0);
        assert_eq!(stats.recent_tombstones, 0);
        let population = world.iter_entities().count();
        assert_eq!(*initial_population.get_or_insert(population), population);
        assert_eq!(world.get::<Name>(entity).unwrap().as_str(), name);
        assert_eq!(world.get::<Unmanaged>(unmanaged).unwrap().0, 42);
        assert_eq!(world.id(), world_id);
        assert!(Arc::ptr_eq(
            &world.resource::<RetainedResource>().0,
            &retained
        ));
        assert_eq!(retained.load(Ordering::Relaxed), 7);
        for (old, old_entity) in &history {
            assert_eq!(
                resolve_in_world(&world, &old.locator(&id).unwrap()),
                EntityLookup::WrongDomain {
                    expected: current.domain_id(),
                    actual: old.domain_id(),
                }
            );
            assert_ne!(old.runtime_reference(&id), current.runtime_reference(&id));
            assert!(world.get_entity(*old_entity).is_err());
        }
        previous_domain = domain.id();
        history.push((current.clone(), entity));
    }
}

#[test]
fn empty_projection_is_an_owned_epoch_that_can_be_replaced_cleared_and_republished() {
    let mut fixture = ProjectionFixture::new();
    let nonempty = fixture.projection.instance().unwrap().clone();
    let empty = bound_scene(SceneDocument::default());
    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &empty,
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 1);
    let first_empty = report.instance.unwrap();
    assert!(first_empty.is_empty());
    assert_ne!(first_empty.domain_id(), nonempty.domain_id());
    assert!(fixture.world.get_entity(fixture.entity).is_err());

    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &empty,
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 0);
    let second_empty = report.instance.unwrap();
    assert!(second_empty.is_empty());
    assert_ne!(second_empty.domain_id(), first_empty.domain_id());
    assert_eq!(fixture.projection.instance(), Some(&second_empty));
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    assert_eq!(stats.lifetime_claims, 1);
    assert_eq!(stats.active_scene_instances, 1);
    assert_eq!(stats.registered_entities, 0);

    assert_eq!(fixture.projection.clear(&mut fixture.world).unwrap(), 0);
    assert!(fixture.projection.instance().is_none());
    assert!(!fixture.world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(fixture.projection.clear(&mut fixture.world).unwrap(), 0);
    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &empty,
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let fresh = report.instance.unwrap();
    assert!(fresh.is_empty());
    assert_ne!(fresh.domain_id(), second_empty.domain_id());
    assert_eq!(fixture.projection.instance(), Some(&fresh));

    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &bound_scene(named_scene("nonempty")),
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 0);
    let nonempty = report.instance.unwrap();
    assert_eq!(nonempty.len(), 1);
    assert_ne!(nonempty.domain_id(), fresh.domain_id());
    assert_eq!(fixture.projection.instance(), Some(&nonempty));
    assert_eq!(
        fixture
            .world
            .resource::<WorldIdentityDomain>()
            .stats()
            .lifetime_claims,
        2
    );
    let entity = resolved_entity(&fixture.world, &nonempty, &fixture.id);
    assert_eq!(
        fixture.world.get::<Name>(entity).unwrap().as_str(),
        "nonempty"
    );
}

#[test]
fn invalid_final_component_preserves_the_receipt_domain_and_old_lookups() {
    let mut fixture = ProjectionFixture::new();
    let current = fixture.projection.instance().unwrap().clone();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let population = fixture.world.iter_entities().count();
    let mut invalid = named_scene("candidate");
    invalid.entities.push(
        SceneEntityRecord::new(scene_id("zz-invalid")).with_component(
            ComponentTypeId::new("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("invalid-visibility".to_owned()),
            ),
        ),
    );
    invalid.roots.push(scene_id("zz-invalid"));

    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &bound_scene(invalid),
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );

    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(report.retired_entities(), 0);
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );
    assert_eq!(fixture.world.iter_entities().count(), population);
}

#[test]
fn foreign_active_and_retired_scene_claims_reject_projection_replacement() {
    for retired in [false, true] {
        let mut fixture = ProjectionFixture::new();
        let current = fixture.projection.instance().unwrap().clone();
        let report = spawn_scene(
            &mut fixture.world,
            &fixture.registry,
            &bound_scene(named_scene("foreign")),
            &occurrence("foreign"),
            None,
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let foreign = report.instance.unwrap();
        if retired {
            retire_scene_instance_exact(&mut fixture.world, &foreign, TombstoneCause::Unloaded)
                .unwrap_or_else(|_| panic!("foreign Scene retirement must succeed"));
        }
        let foreign_lookup = foreign.resolve(&fixture.world, &fixture.id);
        assert!(if retired {
            matches!(foreign_lookup, EntityLookup::Tombstoned(Some(_)))
        } else {
            matches!(foreign_lookup, EntityLookup::Resolved(_))
        });
        let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
        assert_eq!(stats.active_scene_instances, if retired { 1 } else { 2 });
        assert_eq!(stats.claimed_scene_instances, 2);
        let population = fixture.world.iter_entities().count();
        assert!(
            nara_identity::__private::prepare_scene_projection_epoch_retirement(
                &mut fixture.world,
                Some(&current),
            )
            .is_err(),
            "foreign active and historical Scene responsibility must both fence the epoch"
        );

        let report = fixture.projection.publish(
            &mut fixture.world,
            &fixture.registry,
            &bound_scene(named_scene("candidate")),
            fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
        );

        assert_rejected(
            &report,
            if retired {
                "scene.projection-epoch-ineligible"
            } else {
                "scene.occurrence-association-changed"
            },
        );
        fixture.assert_last_good(&current);
        assert_eq!(
            fixture.world.resource::<WorldIdentityDomain>().stats(),
            stats
        );
        assert_eq!(fixture.world.iter_entities().count(), population);
        assert_eq!(foreign.resolve(&fixture.world, &fixture.id), foreign_lookup);
    }
}

#[test]
fn fresh_owner_cannot_take_another_owners_empty_projection() {
    let mut world = World::new();
    let registry = registry();
    let database = ProjectAssetDatabase::default();
    let mut owner = SceneProjection::new(occurrence("owner"));
    let empty = bound_scene(SceneDocument::default());
    let report = owner.publish(
        &mut world,
        &registry,
        &empty,
        database.resolver(&nara_asset::AssetPathScope::ROOT),
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let current = report.instance.unwrap();
    assert!(current.is_empty());
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let population = world.iter_entities().count();
    let mut other = SceneProjection::new(occurrence("other"));
    assert!(
        nara_identity::__private::prepare_scene_projection_epoch_retirement(&mut world, None)
            .is_err(),
        "an unbound owner must not replace another owner's empty epoch"
    );

    let report = other.publish(
        &mut world,
        &registry,
        &empty,
        database.resolver(&nara_asset::AssetPathScope::ROOT),
    );

    assert_rejected(&report, "scene.occurrence-association-changed");
    assert!(other.instance().is_none());
    assert_eq!(other.clear(&mut world).unwrap(), 0);
    assert_eq!(owner.instance(), Some(&current));
    assert_eq!(
        world.resource::<WorldIdentityDomain>().id(),
        current.domain_id()
    );
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    assert_eq!(world.iter_entities().count(), population);
    assert_eq!(owner.clear(&mut world).unwrap(), 0);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn persistent_active_unloaded_and_deleted_responsibilities_are_not_discarded() {
    for disposition in [
        None,
        Some(PersistentRetirementDisposition::Unload),
        Some(PersistentRetirementDisposition::Delete),
    ] {
        let mut fixture = ProjectionFixture::new();
        let current = fixture.projection.instance().unwrap().clone();
        let token = spawn_identity_entity(&mut fixture.world).unwrap();
        let locator = register_persistent_identity(
            &mut fixture.world,
            token,
            PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new("test").unwrap(),
                PersistentRuntimeId::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            ),
        )
        .unwrap();
        if let Some(disposition) = disposition {
            retire_identity_entity_with_disposition(
                &mut fixture.world,
                token,
                TombstoneCause::Unloaded,
                disposition,
            )
            .unwrap();
            fixture.world.entity_mut(token.entity()).despawn();
        }
        let expected = match disposition {
            None => EntityLookup::Resolved(token.entity()),
            Some(PersistentRetirementDisposition::Unload) => EntityLookup::Unloaded,
            Some(PersistentRetirementDisposition::Delete) => EntityLookup::Deleted,
        };
        assert_eq!(resolve_in_world(&fixture.world, &locator), expected);
        let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
        assert_eq!(stats.persistent_logical_records, 1);
        let population = fixture.world.iter_entities().count();

        let report = fixture.projection.publish(
            &mut fixture.world,
            &fixture.registry,
            &bound_scene(named_scene("candidate")),
            fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
        );

        assert_rejected(&report, "scene.projection-epoch-ineligible");
        fixture.assert_last_good(&current);
        assert_eq!(
            fixture.world.resource::<WorldIdentityDomain>().stats(),
            stats
        );
        assert_eq!(fixture.world.iter_entities().count(), population);
        assert_eq!(resolve_in_world(&fixture.world, &locator), expected);
    }
}

#[test]
fn unregistered_identity_refuses_publish_and_clear_until_its_owner_retires_it() {
    let mut fixture = ProjectionFixture::new();
    let current = fixture.projection.instance().unwrap().clone();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let foreign = spawn_identity_entity(&mut fixture.world).unwrap();
    let population = fixture.world.iter_entities().count();
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );

    let report = fixture.projection.publish(
        &mut fixture.world,
        &fixture.registry,
        &bound_scene(named_scene("candidate")),
        fixture.database.resolver(&nara_asset::AssetPathScope::ROOT),
    );

    assert_rejected(&report, "scene.projection-epoch-ineligible");
    let diagnostics = fixture.projection.clear(&mut fixture.world).unwrap_err();
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.projection-epoch-ineligible"
        })
    );
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );
    assert_eq!(
        fixture
            .world
            .resource::<WorldIdentityDomain>()
            .locators_for_token(&fixture.world, foreign),
        Ok(None)
    );
    assert_eq!(fixture.world.iter_entities().count(), population);

    fixture.world.entity_mut(foreign.entity()).despawn();
    assert_eq!(fixture.projection.clear(&mut fixture.world).unwrap(), 1);
    assert!(fixture.projection.instance().is_none());
    assert!(!fixture.world.contains_resource::<WorldIdentityDomain>());
    assert!(fixture.world.get_entity(fixture.entity).is_err());
    assert!(fixture.world.get_entity(foreign.entity()).is_err());
    assert_eq!(
        current.resolve(&fixture.world, &fixture.id),
        EntityLookup::DomainUnavailable
    );
}

#[test]
fn prepared_successor_rechecks_foreign_responsibility_and_discards_its_detached_tokens() {
    let mut fixture = ProjectionFixture::new();
    let current = fixture.projection.instance().unwrap().clone();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let population = fixture.world.iter_entities().count();
    let prepared = fixture.prepare(&bound_scene(named_scene("candidate")));
    let candidate = prepared.identity_tokens[&fixture.id];
    assert_ne!(candidate.domain_id(), current.domain_id());
    let PreparedSceneIdentityCommit::Projection { domain, .. } = &prepared.prepared_identity else {
        panic!("projection preparation must retain its detached domain");
    };
    assert_eq!(
        domain.locators_for_token(&fixture.world, candidate),
        Ok(None)
    );
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );
    let foreign = spawn_identity_entity(&mut fixture.world).unwrap();

    let (prepared, report) = match prepared.commit(&mut fixture.world) {
        Err(rejected) => rejected,
        Ok(_) => panic!("new predecessor responsibility must invalidate the prepared epoch"),
    };

    assert_rejected(&report, "scene.prepared-baseline-changed");
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );
    assert!(fixture.world.get_entity(candidate.entity()).is_ok());
    assert!(prepared.discard(&mut fixture.world).is_ok());
    assert!(fixture.world.get_entity(candidate.entity()).is_err());
    assert_eq!(fixture.world.iter_entities().count(), population + 1);
    assert_eq!(
        fixture
            .world
            .resource::<WorldIdentityDomain>()
            .locators_for_token(&fixture.world, foreign),
        Ok(None)
    );
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );
}

#[test]
fn late_native_state_refuses_prepared_publish_and_clear_then_clear_can_retry() {
    let mut fixture = ProjectionFixture::new();
    fixture.world.init_resource::<Removals>();
    let current = fixture.projection.instance().unwrap().clone();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let prepared = fixture.prepare(&bound_scene(named_scene("candidate")));
    let candidate = prepared.identity_tokens[&fixture.id];
    fixture.world.entity_mut(fixture.entity).insert(NativeState);

    let (prepared, report) = match prepared.commit(&mut fixture.world) {
        Err(rejected) => rejected,
        Ok(_) => panic!("late native state must not run during projection publication"),
    };

    assert_rejected(&report, "scene.prepared-baseline-changed");
    assert!(prepared.discard(&mut fixture.world).is_ok());
    assert!(fixture.world.get_entity(candidate.entity()).is_err());
    let diagnostics = fixture.projection.clear(&mut fixture.world).unwrap_err();
    assert!(diagnostics.has_errors());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.projection-retirement-ineligible"
    }));
    assert_eq!(fixture.world.resource::<Removals>().0, 0);
    assert!(fixture.world.get::<NativeState>(fixture.entity).is_some());
    fixture.assert_last_good(&current);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        stats
    );

    fixture
        .world
        .entity_mut(fixture.entity)
        .remove::<NativeState>();
    assert_eq!(fixture.world.resource::<Removals>().0, 1);
    assert_eq!(fixture.projection.clear(&mut fixture.world).unwrap(), 1);
    assert_eq!(fixture.world.resource::<Removals>().0, 1);
    assert!(fixture.projection.instance().is_none());
    assert!(!fixture.world.contains_resource::<WorldIdentityDomain>());
    assert!(fixture.world.get_entity(fixture.entity).is_err());
}

#[test]
fn initial_detached_candidate_rejects_empty_installed_domain_replacement_or_removal() {
    for (with_domain, replace_with_empty) in [(false, true), (true, true), (true, false)] {
        let mut world = World::new();
        let settings = WorldIdentityDomainSettings::default();
        let predecessor_id = if with_domain {
            let predecessor = WorldIdentityDomain::new(&world, settings).unwrap();
            let id = predecessor.id();
            world.insert_resource(predecessor);
            Some(id)
        } else {
            None
        };
        let registry = registry();
        let database = ProjectAssetDatabase::default();
        let projection = SceneProjection::new(occurrence("projection"));
        let bindings = SceneAssetBindings::from_world(&world);
        let prepared = SceneSpawner::new()
            .prepare_with_asset_context_and_product_bindings(
                &mut world,
                &registry,
                &bound_scene(named_scene("candidate")),
                &projection.occurrence,
                Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
                SceneIdentityCommit::Projection(projection.instance()),
                None,
                bindings,
                &SceneNativeRetirementClaims::default(),
            )
            .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics));
        let candidate = prepared.identity_tokens[&scene_id("root")];
        assert_ne!(Some(candidate.domain_id()), predecessor_id);
        assert_eq!(
            world
                .get_resource::<WorldIdentityDomain>()
                .map(WorldIdentityDomain::id),
            predecessor_id
        );
        let predecessor = world.remove_resource::<WorldIdentityDomain>();
        if let Some(predecessor) = predecessor {
            assert_eq!(predecessor.stats().lifetime_claims, 0);
        }
        let installed_id = if replace_with_empty {
            let replacement = WorldIdentityDomain::new(&world, settings).unwrap();
            let id = replacement.id();
            assert_ne!(Some(id), predecessor_id);
            world.insert_resource(replacement);
            Some(id)
        } else {
            None
        };
        let retained_entities = world
            .iter_entities()
            .map(|entity| entity.id())
            .filter(|entity| *entity != candidate.entity())
            .collect::<Vec<_>>();

        let (prepared, report) = match prepared.commit(&mut world) {
            Err(rejected) => rejected,
            Ok(_) => panic!("the initial epoch must retain its exact installed-domain baseline"),
        };

        assert_rejected(&report, "scene.prepared-baseline-changed");
        assert!(projection.instance().is_none());
        assert!(world.get_entity(candidate.entity()).is_ok());
        assert_eq!(
            world
                .get_resource::<WorldIdentityDomain>()
                .map(WorldIdentityDomain::id),
            installed_id
        );
        assert!(prepared.discard(&mut world).is_ok());
        assert!(world.get_entity(candidate.entity()).is_err());
        assert_eq!(
            world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<Vec<_>>(),
            retained_entities
        );
        assert_eq!(
            world
                .get_resource::<WorldIdentityDomain>()
                .map(WorldIdentityDomain::id),
            installed_id
        );
        if let Some(domain) = world.get_resource::<WorldIdentityDomain>() {
            assert_eq!(domain.stats().lifetime_claims, 0);
        }
    }
}
