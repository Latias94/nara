use nara_core::ItemLimit;
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_ecs::{Entity, World};

use crate::{
    EntityLookup, EntityReference, EntityReferenceRemap, IdentityAllocationError,
    IdentityDomainError, IdentityRemapError, MonotonicNonZeroU64Allocator, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, RuntimeEntityReference,
    SceneEntityId, SceneInstanceId, TombstoneCause, WorldEntityLocator, WorldEntityToken,
    WorldIdentityDomain, WorldIdentityDomainSettings, allocate_world_identity_domain_id_from,
    resolve_in_world, spawn_identity_entity,
};

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn persistent_ref(value: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("runtime").unwrap(),
        PersistentRuntimeId::parse_str(value).unwrap(),
    )
}

fn settings(claims: usize, tombstones: usize) -> WorldIdentityDomainSettings {
    WorldIdentityDomainSettings::new(
        ItemLimit::new(claims).unwrap(),
        ItemLimit::new(tombstones).unwrap(),
    )
    .unwrap()
}

fn world_with_domain(claims: usize, tombstones: usize) -> World {
    let mut world = World::new();
    let domain = WorldIdentityDomain::new(&world, settings(claims, tombstones)).unwrap();
    world.insert_resource(domain);
    world
}

fn spawn_token(world: &mut World) -> WorldEntityToken {
    spawn_identity_entity(world).unwrap()
}

fn with_domain<T>(
    world: &mut World,
    mutate: impl FnOnce(&World, &mut WorldIdentityDomain) -> T,
) -> T {
    let mut domain = world.remove_resource::<WorldIdentityDomain>().unwrap();
    let result = mutate(world, &mut domain);
    world.insert_resource(domain);
    result
}

#[test]
fn monotonic_allocator_rejects_zero_and_exhaustion_atomically() {
    assert_eq!(
        MonotonicNonZeroU64Allocator::from_next_raw(0),
        Err(IdentityAllocationError::Zero)
    );

    let mut allocator = MonotonicNonZeroU64Allocator::from_next_raw(u64::MAX).unwrap();
    assert_eq!(allocator.allocate().unwrap().get(), u64::MAX);
    let exhausted = allocator.clone();
    assert_eq!(
        allocator.allocate(),
        Err(IdentityAllocationError::Exhausted)
    );
    assert_eq!(allocator, exhausted);
    assert_eq!(
        allocator.allocate(),
        Err(IdentityAllocationError::Exhausted)
    );
    assert_eq!(allocator, exhausted);
}

#[test]
fn world_domain_allocator_issues_the_last_id_once_then_fails_atomically() {
    let allocator = AtomicU64::new(u64::MAX);
    assert_eq!(
        allocate_world_identity_domain_id_from(&allocator)
            .unwrap()
            .get(),
        u64::MAX
    );
    assert_eq!(allocator.load(Ordering::Relaxed), 0);
    assert_eq!(
        allocate_world_identity_domain_id_from(&allocator),
        Err(IdentityDomainError::WorldDomainIdExhausted)
    );
    assert_eq!(allocator.load(Ordering::Relaxed), 0);
}

#[test]
fn scene_instance_allocator_issues_the_last_id_once_then_fails_atomically() {
    let mut world = world_with_domain(3, 1);
    with_domain(&mut world, |world, domain| {
        domain.set_next_scene_instance_for_test(u64::MAX).unwrap();
        let last = domain.register_new_scene_instance(world, []).unwrap();
        assert_eq!(last.instance_id().get(), u64::MAX);
        let before = domain.stats();
        assert_eq!(
            domain.register_new_scene_instance(world, []),
            Err(IdentityDomainError::SceneInstanceExhausted)
        );
        assert_eq!(domain.stats(), before);
    });
}

#[test]
fn bidirectional_registration_rejects_both_collision_directions_atomically() {
    let mut world = world_with_domain(16, 4);
    let first = spawn_token(&mut world);
    let second = spawn_token(&mut world);
    let first_ref = persistent_ref("11111111-1111-4111-8111-111111111111");
    let second_ref = persistent_ref("22222222-2222-4222-8222-222222222222");

    with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, first, first_ref.clone())
            .unwrap();
        let before = domain.stats();

        assert!(matches!(
            domain.register_persistent(world, second, first_ref.clone()),
            Err(IdentityDomainError::ReferenceAlreadyClaimed { .. })
        ));
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain.lookup(
                world,
                &RuntimeEntityReference::persistent(first_ref.clone()),
            ),
            EntityLookup::Resolved(first.entity())
        );

        assert!(matches!(
            domain.register_persistent(world, first, second_ref.clone()),
            Err(IdentityDomainError::EntityAxisAlreadyRegistered { .. })
        ));
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain.lookup(world, &RuntimeEntityReference::persistent(second_ref)),
            EntityLookup::Missing
        );
    });
}

#[test]
fn scene_batch_rejects_duplicate_ids_and_entities_without_claiming_an_instance() {
    let mut world = world_with_domain(16, 4);
    let first = spawn_token(&mut world);
    let second = spawn_token(&mut world);
    with_domain(&mut world, |world, domain| {
        let before = domain.stats();

        assert!(matches!(
            domain.register_new_scene_instance(
                world,
                [
                    (scene_id("duplicate"), first),
                    (scene_id("duplicate"), second),
                ],
            ),
            Err(IdentityDomainError::DuplicateSceneEntityId { .. })
        ));
        assert_eq!(domain.stats(), before);

        assert_eq!(
            domain.register_new_scene_instance(
                world,
                [(scene_id("a"), first), (scene_id("b"), first)],
            ),
            Err(IdentityDomainError::DuplicateRuntimeEntity)
        );
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain
                .register_new_scene_instance(world, [(scene_id("valid"), first)])
                .unwrap()
                .instance_id()
                .get(),
            1
        );
    });
}

#[test]
fn equal_entity_bits_in_two_worlds_never_alias_and_scene_local_requires_context() {
    let mut first_world = world_with_domain(16, 4);
    let mut second_world = world_with_domain(16, 4);
    let first_entity = spawn_token(&mut first_world);
    let second_entity = spawn_token(&mut second_world);
    assert_eq!(
        first_entity.entity().to_bits(),
        second_entity.entity().to_bits()
    );

    let first_instance = with_domain(&mut first_world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("player"), first_entity)])
            .unwrap()
    });
    let second_instance = with_domain(&mut second_world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("player"), second_entity)])
            .unwrap()
    });

    let first_locator = first_instance.locator(&scene_id("player")).unwrap();
    let second_locator = second_instance.locator(&scene_id("player")).unwrap();
    assert_ne!(first_locator, second_locator);
    assert!(matches!(
        resolve_in_world(&second_world, &first_locator),
        EntityLookup::WrongDomain { .. }
    ));
    assert!(matches!(
        first_instance.resolve(&second_world, &scene_id("not-in-instance")),
        EntityLookup::WrongDomain { .. }
    ));
    assert_eq!(
        resolve_in_world(&second_world, &second_locator),
        EntityLookup::Resolved(second_entity.entity())
    );

    let durable = EntityReference::SceneLocal {
        entity: scene_id("player"),
    };
    assert_eq!(
        second_world
            .resource::<WorldIdentityDomain>()
            .resolve_entity_reference(&second_world, &durable, None),
        EntityLookup::ContextRequired
    );
    assert_eq!(
        second_world
            .resource::<WorldIdentityDomain>()
            .resolve_entity_reference(
                &second_world,
                &durable,
                Some(second_instance.instance_id()),
            ),
        EntityLookup::Resolved(second_entity.entity())
    );
}

#[test]
fn foreign_world_token_never_aliases_equal_entity_bits_in_the_target_world() {
    let mut source_world = world_with_domain(8, 2);
    let mut target_world = world_with_domain(8, 2);
    let source_token = spawn_token(&mut source_world);
    let target_token = spawn_token(&mut target_world);
    assert_eq!(
        source_token.entity().to_bits(),
        target_token.entity().to_bits()
    );

    with_domain(&mut target_world, |world, domain| {
        let before = domain.stats();
        assert!(matches!(
            domain.register_new_scene_instance(world, [(scene_id("foreign"), source_token)]),
            Err(IdentityDomainError::WrongDomain { .. })
        ));
        assert!(matches!(
            domain.locators_for_token(world, source_token),
            Err(IdentityDomainError::WrongDomain { .. })
        ));
        assert_eq!(domain.stats(), before);

        let instance = domain
            .register_new_scene_instance(world, [(scene_id("local"), target_token)])
            .unwrap();
        assert_eq!(instance.instance_id().get(), 1);
    });
}

#[test]
fn moving_a_domain_resource_cannot_rebind_its_tokens_to_another_world() {
    let mut source_world = world_with_domain(8, 2);
    let source_token = spawn_token(&mut source_world);
    let source_domain = source_world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let mut target_world = World::new();
    target_world.insert_resource(source_domain);

    assert_eq!(
        spawn_identity_entity(&mut target_world),
        Err(IdentityDomainError::WorldBindingMismatch)
    );
    with_domain(&mut target_world, |world, domain| {
        let before = domain.stats();
        assert_eq!(
            domain.register_new_scene_instance(world, [(scene_id("foreign"), source_token)]),
            Err(IdentityDomainError::WorldBindingMismatch)
        );
        assert_eq!(
            domain.register_new_scene_instance(world, []),
            Err(IdentityDomainError::WorldBindingMismatch)
        );
        assert_eq!(
            domain.resolve_entity_reference(
                world,
                &EntityReference::SceneLocal {
                    entity: scene_id("foreign"),
                },
                None,
            ),
            EntityLookup::WrongWorldBinding
        );
        assert_eq!(domain.stats(), before);
    });
}

#[test]
fn moving_a_domain_resource_cannot_resolve_an_equal_bits_unrelated_entity() {
    let mut source_world = world_with_domain(8, 2);
    let source_token = spawn_token(&mut source_world);
    let instance = with_domain(&mut source_world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("source"), source_token)])
            .unwrap()
    });
    let locator = instance.locator(&scene_id("source")).unwrap();
    let source_domain = source_world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();

    let mut target_world = world_with_domain(8, 2);
    let unrelated = spawn_token(&mut target_world);
    assert_eq!(
        source_token.entity().to_bits(),
        unrelated.entity().to_bits()
    );
    let _target_domain = target_world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    target_world.insert_resource(source_domain);

    assert_eq!(
        resolve_in_world(&target_world, &locator),
        EntityLookup::WrongWorldBinding
    );
}

#[test]
fn despawned_token_cannot_register_or_consume_identity_claims() {
    let mut world = world_with_domain(8, 2);
    let stale = spawn_token(&mut world);
    let live = spawn_token(&mut world);
    assert!(world.despawn(stale.entity()));

    with_domain(&mut world, |world, domain| {
        let before = domain.stats();
        assert_eq!(
            domain.register_new_scene_instance(world, [(scene_id("stale"), stale)]),
            Err(IdentityDomainError::EntityTokenNotAlive)
        );
        assert_eq!(domain.stats(), before);

        let instance = domain
            .register_new_scene_instance(world, [(scene_id("live"), live)])
            .unwrap();
        assert_eq!(instance.instance_id().get(), 1);
    });
}

#[test]
fn world_resolution_detects_direct_despawn_as_a_stale_registration() {
    let mut world = world_with_domain(8, 2);
    let entity = spawn_token(&mut world);
    let instance = with_domain(&mut world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("stale"), entity)])
            .unwrap()
    });
    let locator = instance.locator(&scene_id("stale")).unwrap();
    assert!(world.despawn(entity.entity()));
    assert_eq!(
        resolve_in_world(&world, &locator),
        EntityLookup::StaleRegistration
    );
}

#[test]
fn persistent_uuid_identity_is_scoped_by_namespace() {
    let mut world = world_with_domain(8, 2);
    let uuid = PersistentRuntimeId::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
    let first =
        PersistentRuntimeReference::new(PersistentRuntimeNamespaceId::new("save_a").unwrap(), uuid);
    let second =
        PersistentRuntimeReference::new(PersistentRuntimeNamespaceId::new("save_b").unwrap(), uuid);
    let first_entity = spawn_token(&mut world);
    let second_entity = spawn_token(&mut world);

    with_domain(&mut world, |world, domain| {
        domain
            .register_persistent(world, first_entity, first.clone())
            .unwrap();
        domain
            .register_persistent(world, second_entity, second.clone())
            .unwrap();
        assert_eq!(
            domain.lookup(world, &RuntimeEntityReference::persistent(first)),
            EntityLookup::Resolved(first_entity.entity())
        );
        assert_eq!(
            domain.lookup(world, &RuntimeEntityReference::persistent(second)),
            EntityLookup::Resolved(second_entity.entity())
        );
    });
}

#[test]
fn parallel_fork_remaps_every_scene_reference_as_a_group() {
    let mut source_world = world_with_domain(32, 8);
    let source_a = spawn_token(&mut source_world);
    let source_b = spawn_token(&mut source_world);
    let source = with_domain(&mut source_world, |world, domain| {
        domain
            .register_new_scene_instance(
                world,
                [(scene_id("a"), source_a), (scene_id("b"), source_b)],
            )
            .unwrap()
    });
    let source_snapshot = source_world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&source_world, &source)
        .unwrap();

    let mut target_world = world_with_domain(32, 8);
    let target_a = spawn_token(&mut target_world);
    let target_b = spawn_token(&mut target_world);
    let (target, remap) = with_domain(&mut target_world, |world, domain| {
        domain
            .register_parallel_scene_fork(
                world,
                &source_snapshot,
                [
                    (scene_id("a"), target_a, None),
                    (scene_id("b"), target_b, None),
                ],
            )
            .unwrap()
    });

    assert_ne!(source.domain_id(), target.domain_id());
    assert_eq!(remap.references().len(), 2);
    for id in [scene_id("a"), scene_id("b")] {
        assert_ne!(source.locator(&id), target.locator(&id));
        assert_eq!(
            remap.rewrite(&source.locator(&id).unwrap()).unwrap(),
            target.locator(&id).unwrap()
        );
    }
    assert_eq!(
        target.resolve(&target_world, &scene_id("b")),
        EntityLookup::Resolved(target_b.entity())
    );
}

#[test]
fn same_timeline_restore_resolves_recorded_reference_to_a_fresh_entity_slot() {
    let mut source_world = world_with_domain(16, 4);
    let source_entity = spawn_token(&mut source_world);
    let source = with_domain(&mut source_world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("player"), source_entity)])
            .unwrap()
    });
    let recorded = source.runtime_reference(&scene_id("player")).unwrap();
    let source_snapshot = source_world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&source_world, &source)
        .unwrap();

    let mut restored_world = world_with_domain(16, 4);
    let _occupied = restored_world.spawn_empty().id();
    let restored_entity = spawn_token(&mut restored_world);
    assert_ne!(
        source_entity.entity().to_bits(),
        restored_entity.entity().to_bits()
    );
    let (restored, remap) = with_domain(&mut restored_world, |world, domain| {
        domain
            .register_restored_scene_instance(
                world,
                &source_snapshot,
                [(scene_id("player"), restored_entity, None)],
            )
            .unwrap()
    });

    assert_ne!(source.domain_id(), restored.domain_id());
    assert_eq!(source.instance_id(), restored.instance_id());
    let restored_locator = remap
        .rewrite(&WorldEntityLocator::new(source.domain_id(), recorded))
        .unwrap();
    assert_eq!(
        resolve_in_world(&restored_world, &restored_locator),
        EntityLookup::Resolved(restored_entity.entity())
    );
}

#[test]
fn scene_snapshot_remap_covers_persistent_axes_or_fails_as_incomplete() {
    let persistent = persistent_ref("77777777-7777-4777-8777-777777777777");
    let mut source_world = world_with_domain(16, 4);
    let source_token = spawn_token(&mut source_world);
    let (source, source_persistent) = with_domain(&mut source_world, |world, domain| {
        let source = domain
            .register_new_scene_instance(world, [(scene_id("player"), source_token)])
            .unwrap();
        let locator = domain
            .register_persistent(world, source_token, persistent.clone())
            .unwrap();
        (source, locator)
    });
    let source_snapshot = source_world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&source_world, &source)
        .unwrap();

    let mut target_world = world_with_domain(16, 4);
    let target_token = spawn_token(&mut target_world);
    let (target, remap) = with_domain(&mut target_world, |world, domain| {
        let before = domain.stats();
        assert!(matches!(
            domain.register_parallel_scene_fork(
                world,
                &source_snapshot,
                [(scene_id("player"), target_token, None)],
            ),
            Err(IdentityDomainError::IncompleteSceneForkIdentityAxes { .. })
        ));
        assert_eq!(domain.stats(), before);

        domain
            .register_parallel_scene_fork(
                world,
                &source_snapshot,
                [(scene_id("player"), target_token, Some(persistent.clone()))],
            )
            .unwrap()
    });
    assert_eq!(target.instance_id().get(), 1);
    let target_persistent = WorldEntityLocator::new(
        target.domain_id(),
        RuntimeEntityReference::persistent(persistent),
    );

    assert_eq!(
        remap
            .rewrite(&source.locator(&scene_id("player")).unwrap())
            .unwrap(),
        target.locator(&scene_id("player")).unwrap()
    );
    assert_eq!(
        remap.rewrite(&source_persistent).unwrap(),
        target_persistent
    );
}

#[test]
fn retirement_remains_typed_after_detail_eviction_and_identity_is_never_reused() {
    let mut world = world_with_domain(32, 1);
    let first_entity = spawn_token(&mut world);
    let first = with_domain(&mut world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("first"), first_entity)])
            .unwrap()
    });
    let first_ref = first.runtime_reference(&scene_id("first")).unwrap();
    let first_snapshot = world
        .resource::<WorldIdentityDomain>()
        .scene_identity_snapshot(&world, &first)
        .unwrap();
    with_domain(&mut world, |world, domain| {
        domain
            .retire_scene_instance(world, &first, TombstoneCause::Unloaded)
            .unwrap();
        assert!(matches!(
            domain.lookup(world, &first_ref),
            EntityLookup::Tombstoned(Some(_))
        ));
    });

    let second_entity = spawn_token(&mut world);
    let second = with_domain(&mut world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("second"), second_entity)])
            .unwrap()
    });
    let replacement_entity = spawn_token(&mut world);
    with_domain(&mut world, |world, domain| {
        domain
            .retire_scene_instance(world, &second, TombstoneCause::Unloaded)
            .unwrap();
        assert_eq!(
            domain.lookup(world, &first_ref),
            EntityLookup::Tombstoned(None)
        );
        assert_eq!(
            domain.lookup(
                world,
                &RuntimeEntityReference::scene(
                    SceneInstanceId::new(99).unwrap(),
                    scene_id("missing")
                ),
            ),
            EntityLookup::Missing
        );
        let before = domain.stats();
        assert!(matches!(
            domain.register_restored_scene_instance(
                world,
                &first_snapshot,
                [(scene_id("first"), replacement_entity, None)]
            ),
            Err(IdentityDomainError::SceneInstanceAlreadyClaimed { .. })
        ));
        assert_eq!(domain.stats(), before);
    });
}

#[test]
fn retirement_sequence_exhaustion_is_failure_atomic() {
    let mut world = world_with_domain(8, 2);
    let token = spawn_token(&mut world);
    with_domain(&mut world, |world, domain| {
        let instance = domain
            .register_new_scene_instance(world, [(scene_id("entity"), token)])
            .unwrap();
        let reference = instance.runtime_reference(&scene_id("entity")).unwrap();
        domain
            .set_next_retirement_sequence_for_test(u64::MAX)
            .unwrap();
        let before = domain.stats();

        assert_eq!(
            domain.retire_scene_instance(world, &instance, TombstoneCause::Unloaded),
            Err(IdentityDomainError::RetirementSequenceExhausted)
        );
        assert_eq!(domain.stats(), before);
        assert_eq!(
            domain.lookup(world, &reference),
            EntityLookup::Resolved(token.entity())
        );

        domain
            .set_next_retirement_sequence_for_test(u64::MAX - 1)
            .unwrap();
        assert_eq!(
            domain
                .retire_scene_instance(world, &instance, TombstoneCause::Unloaded)
                .unwrap(),
            [token.entity()]
        );
    });
}

#[test]
fn scene_instance_can_retire_after_one_member_was_already_despawned() {
    let mut world = world_with_domain(16, 8);
    let first = spawn_token(&mut world);
    let second = spawn_token(&mut world);
    let persistent = persistent_ref("88888888-8888-4888-8888-888888888888");
    with_domain(&mut world, |world, domain| {
        let instance = domain
            .register_new_scene_instance(
                world,
                [(scene_id("first"), first), (scene_id("second"), second)],
            )
            .unwrap();
        let persistent_locator = domain
            .register_persistent(world, first, persistent)
            .unwrap();

        domain
            .retire_entity(world, first, TombstoneCause::Despawned)
            .unwrap();
        assert_eq!(
            domain
                .retire_scene_instance(world, &instance, TombstoneCause::Unloaded)
                .unwrap(),
            [second.entity()]
        );
        assert!(matches!(
            domain.lookup(
                world,
                &instance.runtime_reference(&scene_id("first")).unwrap(),
            ),
            EntityLookup::Tombstoned(_)
        ));
        assert!(matches!(
            domain.lookup(
                world,
                &instance.runtime_reference(&scene_id("second")).unwrap(),
            ),
            EntityLookup::Tombstoned(_)
        ));
        assert!(matches!(
            domain.lookup(world, persistent_locator.entity()),
            EntityLookup::Tombstoned(_)
        ));
    });
}

#[test]
fn lifetime_claim_budget_charges_instances_and_entity_axes_atomically() {
    let mut world = world_with_domain(2, 1);
    let scene_entity = spawn_token(&mut world);
    with_domain(&mut world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("only"), scene_entity)])
            .unwrap();
        let before = domain.stats();
        assert!(matches!(
            domain.register_persistent(
                world,
                scene_entity,
                persistent_ref("33333333-3333-4333-8333-333333333333")
            ),
            Err(IdentityDomainError::LifetimeClaimLimit { .. })
        ));
        assert_eq!(domain.stats(), before);
    });
}

#[test]
fn scene_registration_stops_consuming_input_when_the_claim_budget_is_exceeded() {
    let mut world = world_with_domain(3, 1);
    let first = spawn_token(&mut world);
    let second = spawn_token(&mut world);
    let third = spawn_token(&mut world);
    let fourth = spawn_token(&mut world);
    let consumed = Cell::new(0);
    let entries = [
        (scene_id("first"), first),
        (scene_id("second"), second),
        (scene_id("third"), third),
        (scene_id("must-not-be-read"), fourth),
    ]
    .into_iter()
    .inspect(|_| consumed.set(consumed.get() + 1));

    with_domain(&mut world, |world, domain| {
        let before = domain.stats();
        assert_eq!(
            domain.register_new_scene_instance(world, entries),
            Err(IdentityDomainError::LifetimeClaimLimit {
                requested: 4,
                maximum: 3,
            })
        );
        assert_eq!(consumed.get(), 3);
        assert_eq!(domain.stats(), before);

        let instance = domain.register_new_scene_instance(world, []).unwrap();
        assert_eq!(instance.instance_id().get(), 1);
    });
}

#[test]
fn reverse_lookup_returns_every_registered_identity_axis() {
    let mut world = world_with_domain(4, 2);
    let token = spawn_token(&mut world);
    let persistent = persistent_ref("66666666-6666-4666-8666-666666666666");

    with_domain(&mut world, |world, domain| {
        let instance = domain
            .register_new_scene_instance(world, [(scene_id("dual"), token)])
            .unwrap();
        let persistent_locator = domain
            .register_persistent(world, token, persistent)
            .unwrap();

        let locators = domain.locators_for_token(world, token).unwrap().unwrap();
        assert_eq!(
            locators.scene(),
            instance.locator(&scene_id("dual")).as_ref()
        );
        assert_eq!(locators.persistent(), Some(&persistent_locator));
        assert_eq!(locators.iter().count(), 2);

        let registered = domain
            .registered_locators(world)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(registered, locators.iter().cloned().collect::<Vec<_>>());
    });
}

#[test]
fn registered_locators_are_sorted_by_semantic_identity() {
    let mut world = world_with_domain(8, 2);
    let z_entity = spawn_token(&mut world);
    let a_entity = spawn_token(&mut world);

    with_domain(&mut world, |world, domain| {
        let instance = domain
            .register_new_scene_instance(
                world,
                [(scene_id("z"), z_entity), (scene_id("a"), a_entity)],
            )
            .unwrap();
        let registered = domain
            .registered_locators(world)
            .unwrap()
            .collect::<Vec<_>>();

        assert_eq!(
            registered,
            [
                instance.locator(&scene_id("a")).unwrap(),
                instance.locator(&scene_id("z")).unwrap(),
            ]
        );
    });
}

#[test]
fn explicit_reference_remap_rejects_an_incomplete_source_set() {
    let first = RuntimeEntityReference::scene(SceneInstanceId::new(1).unwrap(), scene_id("first"));
    let second =
        RuntimeEntityReference::scene(SceneInstanceId::new(1).unwrap(), scene_id("second"));

    assert_eq!(
        EntityReferenceRemap::complete([first.clone(), second], [(first.clone(), first)],),
        Err(IdentityRemapError::IncompleteSourceSet)
    );
}

#[test]
fn empty_scene_instance_is_a_non_reusable_lifetime_claim() {
    let mut world = world_with_domain(1, 1);
    with_domain(&mut world, |world, domain| {
        let empty = domain.register_new_scene_instance(world, []).unwrap();
        assert!(empty.is_empty());
        let empty_snapshot = domain.scene_identity_snapshot(world, &empty).unwrap();
        let before = domain.stats();
        assert!(matches!(
            domain.register_restored_scene_instance(world, &empty_snapshot, []),
            Err(IdentityDomainError::SceneInstanceAlreadyClaimed { .. })
        ));
        assert_eq!(domain.stats(), before);
        assert!(matches!(
            domain.register_new_scene_instance(world, []),
            Err(IdentityDomainError::LifetimeClaimLimit { .. })
        ));
        assert_eq!(domain.stats(), before);
    });
}

#[cfg(feature = "serde")]
#[test]
fn durable_and_runtime_references_roundtrip_without_runtime_entity_bits() {
    let durable = EntityReference::Persistent {
        entity: persistent_ref("44444444-4444-4444-8444-444444444444"),
    };
    let encoded = serde_json::to_string(&durable).unwrap();
    assert!(!encoded.contains("index"));
    assert!(!encoded.contains("generation"));
    assert_eq!(
        serde_json::from_str::<EntityReference>(&encoded).unwrap(),
        durable
    );

    let runtime =
        RuntimeEntityReference::scene(SceneInstanceId::new(7).unwrap(), scene_id("root/player"));
    let encoded = serde_json::to_string(&runtime).unwrap();
    assert!(!encoded.contains("Entity"));
    assert_eq!(
        serde_json::from_str::<RuntimeEntityReference>(&encoded).unwrap(),
        runtime
    );
}

#[test]
fn entity_type_remains_local_to_lookup_results() {
    fn assert_runtime_entity(_: Entity) {}
    let mut world = world_with_domain(4, 1);
    let entity = spawn_token(&mut world);
    let instance = with_domain(&mut world, |world, domain| {
        domain
            .register_new_scene_instance(world, [(scene_id("entity"), entity)])
            .unwrap()
    });
    let EntityLookup::Resolved(resolved) = instance.resolve(&world, &scene_id("entity")) else {
        panic!("registered identity should resolve")
    };
    assert_runtime_entity(resolved);
}
