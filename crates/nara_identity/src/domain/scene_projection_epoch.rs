use std::collections::BTreeSet;

use nara_ecs::World;

use super::{
    IdentityDomainError, IdentityEntityMarker, PreparedSceneInstanceRetirement,
    RuntimeEntityReference, SpawnedSceneInstance, TombstoneCause, WorldEntityToken,
    WorldIdentityDomain, prepare_registered_scene_instance_retirement,
    validate_identity_support_topology, validate_registered_identity_support_topology,
};

/// Proves that the installed domain's entire semantic lifetime belongs to one projection.
///
/// This prepares exact retirement only; it never replaces the installed domain, clears its
/// claims, or commits retirement. The Scene owner must retain exclusive World access through
/// publication, or repeat admission after further preparation. Shared support admission may
/// register support components and flush the World, as for ordinary exact Scene retirement.
#[doc(hidden)]
pub fn prepare_scene_projection_epoch_retirement(
    world: &mut World,
    current: Option<&SpawnedSceneInstance>,
) -> Result<Option<PreparedSceneInstanceRetirement>, IdentityDomainError> {
    validate_identity_support_topology(world, [])
        .map_err(|_| IdentityDomainError::LifecycleConflict)?;
    prepare_registered_scene_projection_epoch_retirement(world, current)
}

/// Checks the complete epoch against registered support without flushing deferred World work.
#[doc(hidden)]
pub fn prepare_registered_scene_projection_epoch_retirement(
    world: &World,
    current: Option<&SpawnedSceneInstance>,
) -> Result<Option<PreparedSceneInstanceRetirement>, IdentityDomainError> {
    let prepared = match current {
        Some(current) => {
            let prepared = prepare_registered_scene_instance_retirement(
                world,
                current,
                TombstoneCause::Replaced,
            )?;
            validate_registered_identity_support_topology(
                world,
                prepared.retiring_entities().iter().copied(),
            )
            .map_err(|_| IdentityDomainError::LifecycleConflict)?;
            Some(prepared)
        }
        None => {
            validate_registered_identity_support_topology(world, std::iter::empty())
                .map_err(|_| IdentityDomainError::LifecycleConflict)?;
            None
        }
    };
    let Some(domain) = world.get_resource::<WorldIdentityDomain>() else {
        return if current.is_none() {
            Ok(None)
        } else {
            Err(IdentityDomainError::WorldDomainUnavailable)
        };
    };
    domain.validate_world_binding(world)?;

    let retiring_entities = prepared
        .as_ref()
        .into_iter()
        .flat_map(|prepared| prepared.retiring_entities().iter().copied())
        .collect::<BTreeSet<_>>();

    if !domain.persistent_records.is_empty() || !domain.persistent_entities.is_empty() {
        return Err(IdentityDomainError::ProjectionEpochNotExclusive);
    }
    match current {
        None => {
            if !domain.claimed_scene_instances.is_empty()
                || !domain.claimed_references.is_empty()
                || !domain.active_scene_instances.is_empty()
                || !domain.scene_entities.is_empty()
                || !domain.identities_by_entity.is_empty()
                || !domain.recent_tombstones.is_empty()
                || !domain.tombstone_order.is_empty()
            {
                return Err(IdentityDomainError::ProjectionEpochNotExclusive);
            }
        }
        Some(current) => {
            if current.domain_id() != domain.id() {
                return Err(IdentityDomainError::WrongDomain {
                    expected: domain.id(),
                    actual: current.domain_id(),
                });
            }
            if domain.active_scene_instances.len() != 1
                || domain.active_scene_instances.get(&current.instance_id())
                    != Some(&current.incarnations)
                || !current.entity_ids.iter().eq(current.incarnations.keys())
                || domain.claimed_scene_instances.len() != 1
                || !domain
                    .claimed_scene_instances
                    .contains(&current.instance_id())
                || domain.claimed_references.len() != current.len()
                || domain.scene_entities.len() != current.len()
                || domain.identities_by_entity.len() != current.len()
                || retiring_entities.len() != current.len()
            {
                return Err(IdentityDomainError::ProjectionEpochNotExclusive);
            }
            for (entity_id, incarnation) in &current.incarnations {
                let reference = RuntimeEntityReference::scene(
                    current.instance_id(),
                    entity_id.clone(),
                    *incarnation,
                );
                let Some(&entity) = domain.scene_entities.get(&reference) else {
                    return Err(IdentityDomainError::ProjectionEpochNotExclusive);
                };
                if !domain.claimed_references.contains(&reference)
                    || !retiring_entities.contains(&entity)
                    || !domain
                        .identities_by_entity
                        .get(&entity)
                        .is_some_and(|axes| {
                            axes.scene.as_ref() == Some(&reference) && axes.persistent.is_none()
                        })
                {
                    return Err(IdentityDomainError::ProjectionEpochNotExclusive);
                }
                domain.validate_live_token(
                    world,
                    WorldEntityToken {
                        domain: domain.id(),
                        entity,
                        incarnation: *incarnation,
                    },
                )?;
            }
        }
    }

    // Unregistered tokens still belong to the predecessor. Detached successor markers do not.
    if world.iter_entities().any(|entity| {
        entity
            .get::<IdentityEntityMarker>()
            .is_some_and(|marker| marker.domain == domain.id())
            && !retiring_entities.contains(&entity.id())
    }) {
        return Err(IdentityDomainError::ProjectionEpochNotExclusive);
    }

    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use nara_ecs::{
        Resource,
        bevy_ecs::lifecycle::{Insert, Remove},
        bevy_ecs::observer::{Observer, On},
        system::ResMut,
    };

    use super::*;
    use crate::{
        EntityLookup, PersistentRetirementDisposition, PersistentRuntimeId,
        PersistentRuntimeNamespaceId, PersistentRuntimeReference, SceneEntityId,
        WorldEntityLocator, WorldIdentityDomainSettings, adopt_identity_entity,
        register_persistent_identity, register_scene_instance, resolve_in_world,
        retire_identity_entity, retire_identity_entity_with_disposition, spawn_identity_entity,
    };

    fn world_with_domain() -> World {
        let mut world = World::new();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        world
    }

    fn scene_id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn spawn_scene(
        world: &mut World,
        ids: &[&str],
    ) -> (SpawnedSceneInstance, Vec<WorldEntityToken>) {
        let tokens = ids
            .iter()
            .map(|_| spawn_identity_entity(world).unwrap())
            .collect::<Vec<_>>();
        let entries = ids
            .iter()
            .zip(&tokens)
            .map(|(id, token)| (scene_id(id), *token))
            .collect::<Vec<_>>();
        (register_scene_instance(world, entries).unwrap(), tokens)
    }

    fn retire_scene(world: &mut World, current: &SpawnedSceneInstance) {
        let mut domain = world.remove_resource::<WorldIdentityDomain>().unwrap();
        domain
            .retire_scene_instance(world, current, TombstoneCause::Unloaded)
            .unwrap();
        world.insert_resource(domain);
    }

    fn assert_refusal_preserves_domain(
        world: &mut World,
        current: Option<&SpawnedSceneInstance>,
        expected: IdentityDomainError,
        locators: &[WorldEntityLocator],
    ) {
        let domain = world.resource::<WorldIdentityDomain>();
        let id = domain.id();
        let stats = domain.stats();
        let next_instance = domain.scene_instance_allocator.peek();
        let next_retirement = domain.retirement_sequence.peek();
        let lookups = locators
            .iter()
            .map(|locator| resolve_in_world(world, locator))
            .collect::<Vec<_>>();
        let entities = world.iter_entities().count();
        let markers = world
            .iter_entities()
            .filter_map(|entity| {
                entity
                    .get::<IdentityEntityMarker>()
                    .map(|marker| WorldEntityToken {
                        domain: marker.domain,
                        entity: entity.id(),
                        incarnation: marker.incarnation,
                    })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            prepare_scene_projection_epoch_retirement(world, current).err(),
            Some(expected)
        );

        let domain = world.resource::<WorldIdentityDomain>();
        assert_eq!(domain.id(), id);
        assert_eq!(domain.stats(), stats);
        assert_eq!(domain.scene_instance_allocator.peek(), next_instance);
        assert_eq!(domain.retirement_sequence.peek(), next_retirement);
        assert_eq!(world.iter_entities().count(), entities);
        for (locator, before) in locators.iter().zip(lookups) {
            assert_eq!(resolve_in_world(world, locator), before);
        }
        for token in markers {
            let marker = world.get::<IdentityEntityMarker>(token.entity()).unwrap();
            assert_eq!(marker.domain, token.domain_id());
            assert_eq!(marker.incarnation, token.incarnation());
        }
    }

    #[test]
    fn absent_domain_without_projection_needs_no_retirement_or_domain_installation() {
        let mut world = World::new();
        let unrelated = world.spawn_empty().id();

        assert!(
            prepare_scene_projection_epoch_retirement(&mut world, None)
                .unwrap()
                .is_none()
        );

        assert!(!world.contains_resource::<WorldIdentityDomain>());
        assert!(world.get_entity(unrelated).is_ok());
    }

    #[test]
    fn absent_domain_with_projection_is_unavailable() {
        let mut source = world_with_domain();
        let (current, _) = spawn_scene(&mut source, &["current"]);
        let mut world = World::new();
        let before = source.resource::<WorldIdentityDomain>().stats();

        assert_eq!(
            prepare_scene_projection_epoch_retirement(&mut world, Some(&current)).err(),
            Some(IdentityDomainError::WorldDomainUnavailable)
        );

        assert!(!world.contains_resource::<WorldIdentityDomain>());
        assert_eq!(source.resource::<WorldIdentityDomain>().stats(), before);
    }

    #[test]
    fn installed_empty_domain_without_projection_is_preserved() {
        let mut world = world_with_domain();
        let before = world.resource::<WorldIdentityDomain>().stats();
        let id = world.resource::<WorldIdentityDomain>().id();

        assert!(
            prepare_scene_projection_epoch_retirement(&mut world, None)
                .unwrap()
                .is_none()
        );

        assert_eq!(world.resource::<WorldIdentityDomain>().id(), id);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
    }

    #[test]
    fn complete_projection_prepares_exact_retirement_without_committing_it() {
        let mut world = world_with_domain();
        let (current, tokens) = spawn_scene(&mut world, &["first", "second"]);
        let unrelated = world.spawn_empty().id();
        let before = world.resource::<WorldIdentityDomain>().stats();
        let id = world.resource::<WorldIdentityDomain>().id();

        let prepared = prepare_scene_projection_epoch_retirement(&mut world, Some(&current))
            .unwrap()
            .unwrap();

        assert_eq!(
            prepared
                .retiring_entities()
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            tokens
                .iter()
                .map(|token| token.entity())
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(world.resource::<WorldIdentityDomain>().id(), id);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
        for (entity_id, token) in current.entity_ids().iter().zip(tokens) {
            assert_eq!(
                current.resolve(&world, entity_id),
                EntityLookup::Resolved(token.entity())
            );
        }
        assert!(world.get_entity(unrelated).is_ok());
    }

    #[test]
    fn empty_scene_still_prepares_its_instance_retirement() {
        let mut world = world_with_domain();
        let (current, _) = spawn_scene(&mut world, &[]);
        let before = world.resource::<WorldIdentityDomain>().stats();

        let prepared = prepare_scene_projection_epoch_retirement(&mut world, Some(&current))
            .unwrap()
            .unwrap();

        assert!(prepared.retiring_entities().is_empty());
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
        assert_eq!(before.lifetime_claims, 1);
        assert_eq!(before.active_scene_instances, 1);
    }

    #[test]
    fn detached_successor_markers_are_not_predecessor_responsibility() {
        for with_current in [false, true] {
            let mut world = world_with_domain();
            let current = with_current.then(|| spawn_scene(&mut world, &["current"]).0);
            let mut successor =
                WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
            let candidate = world.spawn_empty().id();
            let token = successor.adopt_entity(&mut world, candidate).unwrap();
            let candidate_id = scene_id("candidate");
            let candidate_scene = successor
                .register_new_scene_instance(&world, [(candidate_id.clone(), token)])
                .unwrap();
            let before = world.resource::<WorldIdentityDomain>().stats();
            let successor_before = successor.stats();

            let prepared =
                prepare_scene_projection_epoch_retirement(&mut world, current.as_ref()).unwrap();

            assert_eq!(prepared.is_some(), with_current);
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
            assert_eq!(successor.stats(), successor_before);
            assert_eq!(
                successor.lookup(
                    &world,
                    &candidate_scene.runtime_reference(&candidate_id).unwrap()
                ),
                EntityLookup::Resolved(candidate)
            );
            assert_eq!(
                world.get::<IdentityEntityMarker>(candidate).unwrap().domain,
                successor.id()
            );
        }
    }

    #[test]
    fn unrelated_active_scene_prevents_epoch_retirement() {
        let mut world = world_with_domain();
        let (current, _) = spawn_scene(&mut world, &["current"]);
        let (unrelated, _) = spawn_scene(&mut world, &["unrelated"]);

        assert_refusal_preserves_domain(
            &mut world,
            Some(&current),
            IdentityDomainError::ProjectionEpochNotExclusive,
            &[
                current.locator(&scene_id("current")).unwrap(),
                unrelated.locator(&scene_id("unrelated")).unwrap(),
            ],
        );
    }

    #[test]
    fn previously_retired_scene_claims_prevent_retiring_the_only_active_scene() {
        for empty_unrelated in [false, true] {
            let mut world = world_with_domain();
            let (current, _) = spawn_scene(&mut world, &["current"]);
            let ids: &[&str] = if empty_unrelated { &[] } else { &["unrelated"] };
            let (unrelated, _) = spawn_scene(&mut world, ids);
            retire_scene(&mut world, &unrelated);
            let before = world.resource::<WorldIdentityDomain>().stats();
            assert_eq!(before.active_scene_instances, 1);
            assert_eq!(before.claimed_scene_instances, 2);
            let mut locators = vec![current.locator(&scene_id("current")).unwrap()];
            locators.extend(unrelated.locator(&scene_id("unrelated")));

            assert_refusal_preserves_domain(
                &mut world,
                Some(&current),
                IdentityDomainError::ProjectionEpochNotExclusive,
                &locators,
            );
        }
    }

    #[test]
    fn retired_member_reference_is_not_part_of_a_fresh_projection_epoch() {
        let mut world = world_with_domain();
        let (original, tokens) = spawn_scene(&mut world, &["retained", "retired"]);
        retire_identity_entity(&mut world, tokens[1], TombstoneCause::Unloaded).unwrap();
        let current = world
            .resource::<WorldIdentityDomain>()
            .active_scene_instance(&world, original.instance_id())
            .unwrap();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        assert_eq!(stats.claimed_scene_instances, 1);
        assert_eq!(stats.lifetime_claims, 3);
        assert_eq!(current.len(), 1);

        assert_refusal_preserves_domain(
            &mut world,
            Some(&current),
            IdentityDomainError::ProjectionEpochNotExclusive,
            &[
                original.locator(&scene_id("retained")).unwrap(),
                original.locator(&scene_id("retired")).unwrap(),
            ],
        );
    }

    #[test]
    fn no_current_projection_cannot_discard_active_or_retired_empty_scene_claims() {
        for retired in [false, true] {
            let mut world = world_with_domain();
            let (unrelated, _) = spawn_scene(&mut world, &[]);
            if retired {
                retire_scene(&mut world, &unrelated);
            }

            assert_refusal_preserves_domain(
                &mut world,
                None,
                IdentityDomainError::ProjectionEpochNotExclusive,
                &[],
            );
        }
    }

    fn persistent_reference() -> PersistentRuntimeReference {
        PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("runtime").unwrap(),
            PersistentRuntimeId::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        )
    }

    #[test]
    fn persistent_active_unloaded_and_deleted_records_all_prevent_epoch_retirement() {
        for with_current in [false, true] {
            for disposition in [
                None,
                Some(PersistentRetirementDisposition::Unload),
                Some(PersistentRetirementDisposition::Delete),
            ] {
                let mut world = world_with_domain();
                let current = with_current.then(|| spawn_scene(&mut world, &["current"]).0);
                let token = spawn_identity_entity(&mut world).unwrap();
                let persistent =
                    register_persistent_identity(&mut world, token, persistent_reference())
                        .unwrap();
                if let Some(disposition) = disposition {
                    retire_identity_entity_with_disposition(
                        &mut world,
                        token,
                        TombstoneCause::Unloaded,
                        disposition,
                    )
                    .unwrap();
                }
                assert_eq!(
                    resolve_in_world(&world, &persistent),
                    match disposition {
                        None => EntityLookup::Resolved(token.entity()),
                        Some(PersistentRetirementDisposition::Unload) => EntityLookup::Unloaded,
                        Some(PersistentRetirementDisposition::Delete) => EntityLookup::Deleted,
                    }
                );
                assert_eq!(
                    world
                        .resource::<WorldIdentityDomain>()
                        .stats()
                        .persistent_logical_records,
                    1
                );
                let mut locators = vec![persistent];
                if let Some(current) = &current {
                    locators.push(current.locator(&scene_id("current")).unwrap());
                }

                assert_refusal_preserves_domain(
                    &mut world,
                    current.as_ref(),
                    IdentityDomainError::ProjectionEpochNotExclusive,
                    &locators,
                );
            }
        }
    }

    #[test]
    fn projection_members_cannot_have_an_additional_persistent_axis() {
        let mut world = world_with_domain();
        let (current, tokens) = spawn_scene(&mut world, &["current"]);
        let persistent =
            register_persistent_identity(&mut world, tokens[0], persistent_reference()).unwrap();

        assert_refusal_preserves_domain(
            &mut world,
            Some(&current),
            IdentityDomainError::ProjectionEpochNotExclusive,
            &[current.locator(&scene_id("current")).unwrap(), persistent],
        );
    }

    #[test]
    fn adopted_unregistered_predecessor_token_prevents_epoch_retirement_even_with_zero_stats() {
        for with_current in [false, true] {
            let mut world = world_with_domain();
            let current = with_current.then(|| spawn_scene(&mut world, &["current"]).0);
            let before = world.resource::<WorldIdentityDomain>().stats();
            let entity = world.spawn_empty().id();
            let token = adopt_identity_entity(&mut world, entity).unwrap();
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
            assert!(
                world
                    .resource::<WorldIdentityDomain>()
                    .locators_for_token(&world, token)
                    .unwrap()
                    .is_none()
            );
            let locators = current
                .as_ref()
                .and_then(|current| current.locator(&scene_id("current")))
                .into_iter()
                .collect::<Vec<_>>();

            assert_refusal_preserves_domain(
                &mut world,
                current.as_ref(),
                IdentityDomainError::ProjectionEpochNotExclusive,
                &locators,
            );
        }
    }

    #[test]
    fn moved_domain_resource_cannot_be_retired_from_a_different_world() {
        for with_current in [false, true] {
            let mut source = world_with_domain();
            let current = with_current.then(|| spawn_scene(&mut source, &["current"]).0);
            let domain = source.remove_resource::<WorldIdentityDomain>().unwrap();
            let mut world = World::new();
            world.insert_resource(domain);
            let locators = current
                .as_ref()
                .and_then(|current| current.locator(&scene_id("current")))
                .into_iter()
                .collect::<Vec<_>>();

            assert_refusal_preserves_domain(
                &mut world,
                current.as_ref(),
                IdentityDomainError::WorldBindingMismatch,
                &locators,
            );
        }
    }

    #[test]
    fn receipt_must_belong_to_the_installed_domain() {
        let mut source = world_with_domain();
        let (foreign, _) = spawn_scene(&mut source, &["current"]);
        let mut world = world_with_domain();
        let (current, _) = spawn_scene(&mut world, &["current"]);
        let expected = world.resource::<WorldIdentityDomain>().id();

        assert_refusal_preserves_domain(
            &mut world,
            Some(&foreign),
            IdentityDomainError::WrongDomain {
                expected,
                actual: foreign.domain_id(),
            },
            &[current.locator(&scene_id("current")).unwrap()],
        );
    }

    #[test]
    fn current_receipt_incarnations_must_match_the_entire_active_group() {
        let mut world = world_with_domain();
        let (current, _) = spawn_scene(&mut world, &["current"]);
        let mut stale = current.clone();
        stale.incarnations.insert(
            scene_id("current"),
            super::super::allocate_runtime_entity_incarnation().unwrap(),
        );

        assert_refusal_preserves_domain(
            &mut world,
            Some(&stale),
            IdentityDomainError::ProjectionEpochNotExclusive,
            &[current.locator(&scene_id("current")).unwrap()],
        );
    }

    #[test]
    fn live_marker_must_still_own_the_exact_projection_incarnation() {
        let mut world = world_with_domain();
        let (current, tokens) = spawn_scene(&mut world, &["current"]);
        let replacement_incarnation = super::super::allocate_runtime_entity_incarnation().unwrap();
        world
            .get_mut::<IdentityEntityMarker>(tokens[0].entity())
            .unwrap()
            .incarnation = replacement_incarnation;

        assert_refusal_preserves_domain(
            &mut world,
            Some(&current),
            IdentityDomainError::EntityTokenNotOwned,
            &[current.locator(&scene_id("current")).unwrap()],
        );
    }

    #[derive(Resource, Default)]
    struct LifecycleRuns(u32);

    #[test]
    fn resource_lifecycle_rejection_preserves_both_empty_and_active_predecessors() {
        for with_current in [false, true] {
            for insert in [false, true] {
                let mut world = world_with_domain();
                world.init_resource::<LifecycleRuns>();
                let current = with_current.then(|| spawn_scene(&mut world, &["current"]).0);
                if insert {
                    world.add_observer(
                        |_: On<Insert<WorldIdentityDomain>>, mut runs: ResMut<LifecycleRuns>| {
                            runs.0 += 1;
                        },
                    );
                } else {
                    world.add_observer(
                        |_: On<Remove<WorldIdentityDomain>>, mut runs: ResMut<LifecycleRuns>| {
                            runs.0 += 1;
                        },
                    );
                }
                world.flush();
                let locators = current
                    .as_ref()
                    .and_then(|current| current.locator(&scene_id("current")))
                    .into_iter()
                    .collect::<Vec<_>>();

                assert_refusal_preserves_domain(
                    &mut world,
                    current.as_ref(),
                    IdentityDomainError::LifecycleConflict,
                    &locators,
                );

                assert_eq!(world.resource::<LifecycleRuns>().0, 0);
            }
        }
    }

    #[test]
    fn absent_domain_still_requires_support_lifecycle_admission() {
        let mut world = World::new();
        world.init_resource::<LifecycleRuns>();
        world.add_observer(
            |_: On<Insert<WorldIdentityDomain>>, mut runs: ResMut<LifecycleRuns>| runs.0 += 1,
        );
        world.flush();
        let entities = world.iter_entities().count();

        assert_eq!(
            prepare_scene_projection_epoch_retirement(&mut world, None).err(),
            Some(IdentityDomainError::LifecycleConflict)
        );

        assert!(!world.contains_resource::<WorldIdentityDomain>());
        assert_eq!(world.iter_entities().count(), entities);
        assert_eq!(world.resource::<LifecycleRuns>().0, 0);
    }

    #[test]
    fn target_specific_marker_lifecycle_rejection_preserves_the_projection() {
        let mut world = world_with_domain();
        world.init_resource::<LifecycleRuns>();
        let (current, tokens) = spawn_scene(&mut world, &["current"]);
        let marker = world.component_id::<IdentityEntityMarker>().unwrap();
        world.spawn(
            Observer::new(|_: On<Insert<()>>, mut runs: ResMut<LifecycleRuns>| runs.0 += 1)
                .with_entity(tokens[0].entity())
                .with_component(marker),
        );
        world.flush();

        assert_refusal_preserves_domain(
            &mut world,
            Some(&current),
            IdentityDomainError::LifecycleConflict,
            &[current.locator(&scene_id("current")).unwrap()],
        );

        assert_eq!(world.resource::<LifecycleRuns>().0, 0);
    }
}
