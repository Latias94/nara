use bevy_ecs::{
    lifecycle::Remove,
    observer::{Observer, On},
    prelude::{ResMut, Resource},
    world::World,
};
use nara_core::ItemLimit;
use nara_identity::{
    EntityLookup, EntityReference, IdentityDomainError, PersistentRetirementDisposition,
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference, TombstoneCause,
    WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
};

fn persistent_reference() -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("reactivation-test").unwrap(),
        PersistentRuntimeId::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
    )
}

fn world_with_identity_domain() -> World {
    let mut world = World::new();
    let domain = WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
    world.insert_resource(domain);
    world
}

fn with_domain<T>(
    world: &mut World,
    operation: impl FnOnce(&mut World, &mut WorldIdentityDomain) -> T,
) -> T {
    let mut domain = world.remove_resource::<WorldIdentityDomain>().unwrap();
    let result = operation(world, &mut domain);
    world.insert_resource(domain);
    result
}

#[test]
fn reactivation_on_the_same_entity_revokes_the_old_token() {
    let persistent = persistent_reference();
    let mut world = world_with_identity_domain();
    let token_a = spawn_identity_entity(&mut world).unwrap();
    let locator_a = with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, token_a, persistent.clone())
            .unwrap()
    });
    let runtime_a = locator_a.entity().clone();

    with_domain(&mut world, |world, domain| {
        domain
            .retire_entity(world, token_a, TombstoneCause::Unloaded)
            .unwrap();
        let token_b = domain.adopt_entity(world, token_a.entity()).unwrap();
        assert_eq!(token_a.entity(), token_b.entity());
        assert_ne!(token_a, token_b);

        let locator_b = domain
            .register_persistent(world, token_b, persistent.clone())
            .unwrap();
        assert!(matches!(
            domain.lookup(world, &runtime_a),
            EntityLookup::StaleRuntime
        ));
        assert!(matches!(
            domain.lookup(world, locator_b.entity()),
            EntityLookup::Resolved(_)
        ));
        assert!(matches!(
            domain.resolve_entity_reference(
                world,
                &EntityReference::Persistent {
                    entity: persistent.clone(),
                },
                None,
            ),
            EntityLookup::Resolved(_)
        ));
        assert!(
            domain
                .retire_entity(world, token_a, TombstoneCause::Despawned)
                .is_err()
        );
    });
}

#[test]
fn an_unloaded_persistent_identity_rebinds_to_a_fresh_entity() {
    let persistent = persistent_reference();
    let mut world = world_with_identity_domain();
    let token_a = spawn_identity_entity(&mut world).unwrap();
    let locator_a = with_domain(&mut world, |world, domain| {
        let locator = domain
            .register_persistent(world, token_a, persistent.clone())
            .unwrap();
        domain
            .retire_entity(world, token_a, TombstoneCause::Unloaded)
            .unwrap();
        locator
    });
    assert!(world.despawn(token_a.entity()));
    let token_b = spawn_identity_entity(&mut world).unwrap();
    let locator_b = with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, token_b, persistent)
            .unwrap()
    });
    assert_ne!(locator_a, locator_b);
    let domain = world.resource::<WorldIdentityDomain>();
    assert!(matches!(
        domain.lookup(&world, locator_a.entity()),
        EntityLookup::StaleRuntime
    ));
    assert_eq!(
        domain.lookup(&world, locator_b.entity()),
        EntityLookup::Resolved(token_b.entity())
    );
}

#[test]
fn repeated_residency_keeps_the_logical_budget_bounded_and_deletion_terminal() {
    let mut world = World::new();
    let settings = WorldIdentityDomainSettings::new(
        ItemLimit::new(4).unwrap(),
        ItemLimit::new(1).unwrap(),
        ItemLimit::new(1).unwrap(),
    );
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    world.insert_resource(domain);
    let persistent = persistent_reference();
    let durable = EntityReference::Persistent {
        entity: persistent.clone(),
    };
    for _cycle in 0..32 {
        let token = spawn_identity_entity(&mut world).unwrap();
        with_domain(&mut world, |world, domain| {
            domain
                .register_persistent(world, token, persistent.clone())
                .unwrap();
            assert_eq!(domain.stats().persistent_logical_records, 1);
            assert_eq!(domain.stats().lifetime_claims, 0);
            domain
                .retire_entity(world, token, TombstoneCause::Unloaded)
                .unwrap();
            assert_eq!(
                domain.resolve_entity_reference(world, &durable, None),
                EntityLookup::Unloaded
            );
        });
        assert!(world.despawn(token.entity()));
    }
    let final_token = spawn_identity_entity(&mut world).unwrap();
    with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, final_token, persistent.clone())
            .unwrap();
        domain
            .retire_entity_with_disposition(
                world,
                final_token,
                TombstoneCause::Despawned,
                PersistentRetirementDisposition::Delete,
            )
            .unwrap();
        let empty_scene = domain.register_new_scene_instance(world, []).unwrap();
        domain
            .retire_scene_instance(world, &empty_scene, TombstoneCause::Unloaded)
            .unwrap();
        assert_eq!(
            domain.resolve_entity_reference(world, &durable, None),
            EntityLookup::Deleted
        );
        assert_eq!(domain.stats().deleted_persistent_entities, 1);
    });
    assert!(world.despawn(final_token.entity()));
    let rejected = spawn_identity_entity(&mut world).unwrap();
    with_domain(&mut world, |world, domain| {
        let before = domain.stats();
        assert!(matches!(
            domain.register_persistent(world, rejected, persistent.clone()),
            Err(IdentityDomainError::PersistentIdentityDeleted { .. })
        ));
        let different = PersistentRuntimeReference::new(
            persistent.namespace.clone(),
            PersistentRuntimeId::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap(),
        );
        assert!(matches!(
            domain.register_persistent(world, rejected, different),
            Err(IdentityDomainError::PersistentLogicalRecordLimit {
                requested: 2,
                maximum: 1
            })
        ));
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain.resolve_entity_reference(world, &durable, None),
            EntityLookup::Deleted
        );
    });
}

#[test]
fn an_unloaded_logical_identity_can_be_deleted_without_becoming_reusable() {
    let mut world = world_with_identity_domain();
    let persistent = persistent_reference();
    let token = spawn_identity_entity(&mut world).unwrap();
    with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, token, persistent.clone())
            .unwrap();
        assert!(matches!(
            domain.delete_unloaded_persistent(world, &persistent),
            Err(IdentityDomainError::PersistentIdentityActive { .. })
        ));
        domain
            .retire_entity(world, token, TombstoneCause::Unloaded)
            .unwrap();
        domain
            .delete_unloaded_persistent(world, &persistent)
            .unwrap();
        assert_eq!(
            domain.resolve_entity_reference(
                world,
                &EntityReference::Persistent { entity: persistent },
                None
            ),
            EntityLookup::Deleted
        );
    });
}

#[derive(Resource, Default)]
struct RetirementObserverRuns(u32);

#[test]
fn identity_retirement_rejects_marker_lifecycle_work_before_revocation() {
    let mut world = world_with_identity_domain();
    world.init_resource::<RetirementObserverRuns>();
    let token = spawn_identity_entity(&mut world).unwrap();
    let locator = with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, token, persistent_reference())
            .unwrap()
    });
    world.spawn(
        Observer::new(|_: On<Remove<()>>, mut runs: ResMut<RetirementObserverRuns>| runs.0 += 1)
            .with_entity(token.entity()),
    );
    world.flush();
    with_domain(&mut world, |world, domain| {
        let before = domain.stats();
        assert_eq!(
            domain.retire_entity(world, token, TombstoneCause::Unloaded),
            Err(IdentityDomainError::LifecycleConflict)
        );
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain.lookup(world, locator.entity()),
            EntityLookup::Resolved(token.entity())
        );
    });
    assert_eq!(world.resource::<RetirementObserverRuns>().0, 0);
}
