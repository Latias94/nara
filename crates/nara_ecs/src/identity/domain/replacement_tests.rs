use super::*;
use crate::identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, register_persistent_identity,
    register_scene_instance, retire_identity_entity, spawn_identity_entity,
};

struct Fixture {
    world: World,
    current: SpawnedSceneInstance,
    additional: [SpawnedSceneInstance; 2],
    members: Vec<WorldEntityToken>,
    runtime: WorldEntityToken,
    entries: Vec<(SceneEntityId, WorldEntityToken)>,
}

impl Fixture {
    fn new() -> Self {
        let mut world = World::new();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        let mut members = Vec::new();
        let mut instances = Vec::new();
        for _ in 0..3 {
            let entries = ["root", "child"].map(|id| {
                let token = spawn_identity_entity(&mut world).unwrap();
                members.push(token);
                (SceneEntityId::new(id).unwrap(), token)
            });
            instances.push(register_scene_instance(&mut world, entries).unwrap());
        }
        let runtime = spawn_identity_entity(&mut world).unwrap();
        let candidate = spawn_identity_entity(&mut world).unwrap();
        Self {
            world,
            current: instances.remove(0),
            additional: instances.try_into().unwrap(),
            members,
            runtime,
            entries: vec![(SceneEntityId::new("successor").unwrap(), candidate)],
        }
    }

    fn assert_refused(
        &self,
        current: &SpawnedSceneInstance,
        additional: &[SpawnedSceneInstance],
        runtime: &[WorldEntityToken],
        entries: &[(SceneEntityId, WorldEntityToken)],
        expected: IdentityDomainError,
    ) {
        let world = &self.world;
        let domain = world.resource::<WorldIdentityDomain>();
        let stats = domain.stats();
        let next_instance = domain.scene_instance_allocator.clone();
        let next_sequence = domain.retirement_sequence.clone();
        let tombstones = domain.recent_tombstones.clone();
        let lookups = domain
            .registered_locators(world)
            .unwrap()
            .map(|locator| {
                let lookup = domain.lookup(world, locator.entity());
                (locator, lookup)
            })
            .collect::<Vec<_>>();
        let markers = world
            .iter_entities()
            .map(|entity| {
                (
                    entity.id(),
                    entity
                        .get::<IdentityEntityMarker>()
                        .map(|marker| (marker.domain, marker.incarnation)),
                )
            })
            .collect::<Vec<_>>();

        let result = prepare_registered_scene_instance_replacement(
            world,
            current,
            additional,
            runtime,
            entries,
            TombstoneCause::Replaced,
        );
        let Err(error) = result else {
            panic!("invalid aggregate replacement was accepted");
        };
        assert_eq!(error, expected);
        let domain = world.resource::<WorldIdentityDomain>();
        assert_eq!(domain.stats(), stats);
        assert_eq!(domain.scene_instance_allocator, next_instance);
        assert_eq!(domain.retirement_sequence, next_sequence);
        assert_eq!(domain.recent_tombstones, tombstones);
        for (locator, lookup) in lookups {
            assert_eq!(domain.lookup(world, locator.entity()), lookup);
        }
        assert_eq!(
            world
                .iter_entities()
                .map(|entity| {
                    (
                        entity.id(),
                        entity
                            .get::<IdentityEntityMarker>()
                            .map(|marker| (marker.domain, marker.incarnation)),
                    )
                })
                .collect::<Vec<_>>(),
            markers
        );
    }
}

fn persistent(value: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("aggregate-replacement").unwrap(),
        PersistentRuntimeId::parse_str(value).unwrap(),
    )
}

#[test]
fn aggregate_replacement_retires_all_groups_in_one_sequence_batch() {
    let mut fixture = Fixture::new();
    let persistent = persistent("11111111-1111-4111-8111-111111111111");
    let old_persistent =
        register_persistent_identity(&mut fixture.world, fixture.members[2], persistent.clone())
            .unwrap();
    let unrelated_token = spawn_identity_entity(&mut fixture.world).unwrap();
    let unrelated = register_scene_instance(
        &mut fixture.world,
        [(SceneEntityId::new("unrelated").unwrap(), unrelated_token)],
    )
    .unwrap();
    fixture
        .world
        .resource_mut::<WorldIdentityDomain>()
        .set_next_retirement_sequence_for_test(20)
        .unwrap();
    let before = fixture.world.resource::<WorldIdentityDomain>().stats();
    let mut expected = fixture
        .members
        .iter()
        .map(|token| token.entity())
        .chain([fixture.runtime.entity()])
        .collect::<Vec<_>>();
    expected.sort_by_key(|entity| entity.to_bits());

    let prepared = prepare_exact_scene_instance_replacement(
        &mut fixture.world,
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        TombstoneCause::Replaced,
    )
    .unwrap();
    assert_eq!(prepared.retiring_entities(), expected);
    assert_eq!(
        fixture.world.resource::<WorldIdentityDomain>().stats(),
        before
    );
    for (receipt, tokens) in std::iter::once(&fixture.current)
        .chain(&fixture.additional)
        .zip(fixture.members.chunks_exact(2))
    {
        assert_eq!(
            receipt.resolve(&fixture.world, &SceneEntityId::new("root").unwrap()),
            EntityLookup::Resolved(tokens[0].entity())
        );
    }

    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let (replacement, retired) = prepared.commit(&mut domain);
    assert_eq!(retired, expected);
    assert_eq!(domain.stats().active_scene_instances, 2);
    assert_eq!(domain.stats().active_scene_entities, 2);
    assert_eq!(domain.stats().active_persistent_entities, 0);
    assert_eq!(domain.stats().unloaded_persistent_entities, 1);
    assert_eq!(
        domain.lookup(&fixture.world, old_persistent.entity()),
        EntityLookup::Unloaded
    );
    for receipt in std::iter::once(&fixture.current).chain(&fixture.additional) {
        let subject = IdentityTombstoneSubject::SceneInstance(receipt.instance_id());
        assert_eq!(
            domain.recent_tombstones[&subject].cause(),
            TombstoneCause::Replaced
        );
        assert!(
            !domain
                .active_scene_instances
                .contains_key(&receipt.instance_id())
        );
        for id in receipt.entity_ids() {
            assert!(matches!(
                domain.lookup(&fixture.world, &receipt.runtime_reference(id).unwrap()),
                EntityLookup::Tombstoned(Some(_))
            ));
        }
    }
    for entity in retired {
        assert!(fixture.world.despawn(entity));
    }
    fixture.world.insert_resource(domain);
    assert_eq!(
        unrelated.resolve(&fixture.world, &SceneEntityId::new("unrelated").unwrap()),
        EntityLookup::Resolved(unrelated_token.entity())
    );
    let rebound =
        register_persistent_identity(&mut fixture.world, fixture.entries[0].1, persistent).unwrap();
    assert_ne!(rebound, old_persistent);
    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    domain
        .retire_scene_instance(&mut fixture.world, &replacement, TombstoneCause::Unloaded)
        .unwrap();
    let mut sequences = domain
        .recent_tombstones
        .values()
        .map(|tombstone| tombstone.sequence().get())
        .collect::<Vec<_>>();
    sequences.sort_unstable();
    assert_eq!(sequences, (20..33).collect::<Vec<_>>());
    assert_eq!(domain.retirement_sequence.peek().unwrap().get(), 33);
    fixture.world.insert_resource(domain);
}

#[test]
fn aggregate_replacement_rejects_duplicate_receipts_tokens_and_candidate_overlap() {
    let fixture = Fixture::new();
    for additional in [
        vec![fixture.current.clone()],
        vec![fixture.additional[0].clone(), fixture.additional[0].clone()],
    ] {
        let duplicate = additional.last().unwrap().instance_id();
        fixture.assert_refused(
            &fixture.current,
            &additional,
            &[],
            &fixture.entries,
            IdentityDomainError::DuplicateSceneInstance {
                instance: duplicate,
            },
        );
    }
    for runtime in [
        vec![fixture.runtime, fixture.runtime],
        vec![fixture.members[2]],
        vec![fixture.entries[0].1],
    ] {
        fixture.assert_refused(
            &fixture.current,
            &fixture.additional,
            &runtime,
            &fixture.entries,
            IdentityDomainError::DuplicateRuntimeEntity,
        );
    }
}

#[test]
fn aggregate_replacement_rejects_foreign_receipts_tokens_and_worlds() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    fixture.assert_refused(
        &fixture.current,
        std::slice::from_ref(&foreign.current),
        &[],
        &fixture.entries,
        IdentityDomainError::WrongDomain {
            expected: fixture.current.domain_id(),
            actual: foreign.current.domain_id(),
        },
    );
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[foreign.runtime],
        &fixture.entries,
        IdentityDomainError::WrongDomain {
            expected: fixture.current.domain_id(),
            actual: foreign.runtime.domain_id(),
        },
    );
    let mut world = World::new();
    let mut fixture = fixture;
    let domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let next_instance = domain.scene_instance_allocator.clone();
    let next_sequence = domain.retirement_sequence.clone();
    world.insert_resource(domain);
    let result = prepare_registered_scene_instance_replacement(
        &world,
        &fixture.current,
        &fixture.additional,
        &[],
        &[],
        TombstoneCause::Replaced,
    );
    assert!(matches!(
        result,
        Err(IdentityDomainError::WorldBindingMismatch)
    ));
    let domain = world.resource::<WorldIdentityDomain>();
    assert_eq!(domain.scene_instance_allocator, next_instance);
    assert_eq!(domain.retirement_sequence, next_sequence);
}

#[test]
fn aggregate_replacement_rejects_stale_receipts_and_member_incarnations() {
    let mut fixture = Fixture::new();
    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    domain
        .retire_scene_instance(
            &mut fixture.world,
            &fixture.additional[1],
            TombstoneCause::Unloaded,
        )
        .unwrap();
    fixture.world.insert_resource(domain);
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[],
        &fixture.entries,
        IdentityDomainError::SceneInstanceNotActive {
            instance: fixture.additional[1].instance_id(),
        },
    );
    let mut stale = fixture.additional[0].clone();
    stale.incarnations.insert(
        SceneEntityId::new("root").unwrap(),
        fixture.runtime.incarnation(),
    );
    fixture.assert_refused(
        &fixture.current,
        &[stale],
        &[],
        &fixture.entries,
        IdentityDomainError::SceneInstanceMembershipMismatch {
            instance: fixture.additional[0].instance_id(),
        },
    );
}

#[test]
fn aggregate_replacement_rejects_runtime_tokens_with_semantic_axes_or_stale_ownership() {
    let mut fixture = Fixture::new();
    fixture.assert_refused(
        &fixture.current,
        &[],
        &[fixture.members[2]],
        &fixture.entries,
        IdentityDomainError::EntityAxisAlreadyRegistered {
            axis: EntityIdentityAxis::Scene,
        },
    );
    register_persistent_identity(
        &mut fixture.world,
        fixture.runtime,
        persistent("22222222-2222-4222-8222-222222222222"),
    )
    .unwrap();
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        IdentityDomainError::EntityAxisAlreadyRegistered {
            axis: EntityIdentityAxis::Persistent,
        },
    );
    retire_identity_entity(
        &mut fixture.world,
        fixture.runtime,
        TombstoneCause::Unloaded,
    )
    .unwrap();
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        IdentityDomainError::EntityTokenNotOwned,
    );
}

#[test]
fn aggregate_replacement_accepts_legally_retired_members_and_empty_instances() {
    let mut fixture = Fixture::new();
    for token in &fixture.members[2..] {
        retire_identity_entity(&mut fixture.world, *token, TombstoneCause::Despawned).unwrap();
        assert!(fixture.world.despawn(token.entity()));
    }
    let prepared = prepare_registered_scene_instance_replacement(
        &fixture.world,
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        TombstoneCause::Replaced,
    )
    .unwrap();
    assert_eq!(prepared.retiring_entities().len(), 3);
    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let (_, retired) = prepared.commit(&mut domain);
    assert_eq!(retired.len(), 3);
    assert_eq!(domain.stats().active_scene_instances, 1);
    for receipt in &fixture.additional {
        assert!(
            domain
                .recent_tombstones
                .contains_key(&IdentityTombstoneSubject::SceneInstance(
                    receipt.instance_id()
                ))
        );
    }
    fixture.world.insert_resource(domain);
}

#[test]
fn aggregate_replacement_sequence_exhaustion_preserves_all_groups_and_allocators() {
    let mut fixture = Fixture::new();
    fixture
        .world
        .resource_mut::<WorldIdentityDomain>()
        .set_next_retirement_sequence_for_test(u64::MAX - 7)
        .unwrap();
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        IdentityDomainError::RetirementSequenceExhausted,
    );
    fixture
        .world
        .resource_mut::<WorldIdentityDomain>()
        .set_next_retirement_sequence_for_test(u64::MAX - 8)
        .unwrap();
    let prepared = prepare_registered_scene_instance_replacement(
        &fixture.world,
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        TombstoneCause::Replaced,
    )
    .unwrap();
    assert_eq!(prepared.instance_id().get(), 4);
    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let (replacement, _) = prepared.commit(&mut domain);
    assert_eq!(
        domain.retirement_sequence.peek(),
        Err(IdentityAllocationError::Exhausted)
    );
    assert_eq!(domain.stats().active_scene_instances, 1);
    assert_eq!(domain.recent_tombstones.len(), 9);
    fixture.world.insert_resource(domain);
    assert_eq!(
        replacement.resolve(&fixture.world, &fixture.entries[0].0),
        EntityLookup::Resolved(fixture.entries[0].1.entity())
    );
}

#[test]
fn aggregate_replacement_preserves_candidate_persistent_axis_and_scene_conflict_checks() {
    let mut fixture = Fixture::new();
    let candidate = fixture.entries[0].1;
    let persistent = register_persistent_identity(
        &mut fixture.world,
        candidate,
        persistent("33333333-3333-4333-8333-333333333333"),
    )
    .unwrap();
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional[..1],
        &[],
        &[(fixture.entries[0].0.clone(), fixture.members[4])],
        IdentityDomainError::EntityAxisAlreadyRegistered {
            axis: EntityIdentityAxis::Scene,
        },
    );
    let prepared = prepare_registered_scene_instance_replacement(
        &fixture.world,
        &fixture.current,
        &fixture.additional,
        &[fixture.runtime],
        &fixture.entries,
        TombstoneCause::Replaced,
    )
    .unwrap();
    let mut domain = fixture
        .world
        .remove_resource::<WorldIdentityDomain>()
        .unwrap();
    let (replacement, _) = prepared.commit(&mut domain);
    assert_eq!(
        domain.lookup(&fixture.world, persistent.entity()),
        EntityLookup::Resolved(candidate.entity())
    );
    let axes = domain
        .locators_for_token(&fixture.world, candidate)
        .unwrap()
        .unwrap();
    assert_eq!(axes.persistent(), Some(&persistent));
    assert_eq!(
        axes.scene(),
        replacement.locator(&fixture.entries[0].0).as_ref()
    );
    fixture.world.insert_resource(domain);
}

#[test]
fn aggregate_replacement_rejects_missing_members_and_revoked_member_ownership() {
    let mut fixture = Fixture::new();
    fixture
        .world
        .entity_mut(fixture.members[2].entity())
        .remove::<IdentityEntityMarker>();
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[],
        &fixture.entries,
        IdentityDomainError::StaleRegistration,
    );
    assert!(fixture.world.despawn(fixture.members[2].entity()));
    fixture.assert_refused(
        &fixture.current,
        &fixture.additional,
        &[],
        &fixture.entries,
        IdentityDomainError::StaleRegistration,
    );
}
