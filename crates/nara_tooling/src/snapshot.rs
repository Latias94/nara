use nara_core::ItemLimit;
use nara_ecs::{Resource, World, resource::IsResource};
use nara_identity::{
    EntityLookup, IdentityDomainError, WorldEntityLocator, WorldIdentityDomain,
    WorldIdentityDomainId,
};

pub const DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT: ItemLimit =
    ItemLimit::new(4_096).expect("the default tooling snapshot limit is non-zero");

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct WorldIdentitySnapshot {
    pub domain_id: Option<WorldIdentityDomainId>,
    pub locator_limit: ItemLimit,
    pub total_entity_count: usize,
    pub identified_entity_count: usize,
    pub runtime_only_entity_count: usize,
    pub returned_locator_count: usize,
    pub omitted_locator_count: usize,
    pub locators: Vec<WorldEntityLocator>,
}

impl WorldIdentitySnapshot {
    pub fn capture(world: &World, locator_limit: ItemLimit) -> Result<Self, IdentityDomainError> {
        let total_entity_count = world
            .iter_entities()
            .filter(|entity| !entity.contains::<IsResource>())
            .count();
        let Some(domain) = world.get_resource::<WorldIdentityDomain>() else {
            return Ok(Self {
                domain_id: None,
                locator_limit,
                total_entity_count,
                identified_entity_count: 0,
                runtime_only_entity_count: total_entity_count,
                returned_locator_count: 0,
                omitted_locator_count: 0,
                locators: Vec::new(),
            });
        };

        let stats = domain.stats();
        let mut observed_locator_count = 0_usize;
        let mut locators = Vec::new();
        for locator in domain.registered_locators(world)? {
            if !matches!(
                domain.lookup(world, locator.entity()),
                EntityLookup::Resolved(_)
            ) {
                return Err(IdentityDomainError::StaleRegistration);
            }
            observed_locator_count = observed_locator_count.saturating_add(1);
            if locators.len() < locator_limit.get() {
                locators.push(locator);
            }
        }

        let identified_entity_count = stats.registered_entities;
        if identified_entity_count > total_entity_count {
            return Err(IdentityDomainError::StaleRegistration);
        }
        let returned_locator_count = locators.len();
        Ok(Self {
            domain_id: Some(domain.id()),
            locator_limit,
            total_entity_count,
            identified_entity_count,
            runtime_only_entity_count: total_entity_count - identified_entity_count,
            returned_locator_count,
            omitted_locator_count: observed_locator_count - returned_locator_count,
            locators,
        })
    }

    pub fn capture_default(world: &World) -> Result<Self, IdentityDomainError> {
        Self::capture(world, DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use nara_ecs::{Mut, World};
    use nara_identity::{
        PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
        SceneEntityId, WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
    };

    use super::*;

    #[test]
    fn worlds_without_an_identity_domain_are_count_only() {
        let mut world = World::new();
        world.spawn_empty();
        world.spawn_empty();

        let snapshot = WorldIdentitySnapshot::capture(&world, ItemLimit::ONE).unwrap();

        assert_eq!(snapshot.domain_id, None);
        assert_eq!(snapshot.locator_limit, ItemLimit::ONE);
        assert_eq!(snapshot.total_entity_count, 2);
        assert_eq!(snapshot.identified_entity_count, 0);
        assert_eq!(snapshot.runtime_only_entity_count, 2);
        assert_eq!(snapshot.returned_locator_count, 0);
        assert_eq!(snapshot.omitted_locator_count, 0);
        assert!(snapshot.locators.is_empty());
    }

    #[test]
    fn identity_snapshots_bound_semantically_sorted_locators() {
        let mut world = World::new();
        world.spawn_empty();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        let first = spawn_identity_entity(&mut world).unwrap();
        let second = spawn_identity_entity(&mut world).unwrap();
        let instance = world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_new_scene_instance(
                    world,
                    [
                        (SceneEntityId::new("zeta").unwrap(), first),
                        (SceneEntityId::new("alpha").unwrap(), second),
                    ],
                )
            })
            .unwrap();

        let snapshot = WorldIdentitySnapshot::capture(&world, ItemLimit::ONE).unwrap();

        assert_eq!(snapshot.domain_id, Some(instance.domain_id()));
        assert_eq!(snapshot.locator_limit, ItemLimit::ONE);
        assert_eq!(snapshot.total_entity_count, 3);
        assert_eq!(snapshot.identified_entity_count, 2);
        assert_eq!(snapshot.runtime_only_entity_count, 1);
        assert_eq!(snapshot.returned_locator_count, 1);
        assert_eq!(
            snapshot.locators,
            vec![
                instance
                    .locator(&SceneEntityId::new("alpha").unwrap())
                    .unwrap()
            ]
        );
        assert_eq!(snapshot.omitted_locator_count, 1);
    }

    #[test]
    fn dual_identity_axes_do_not_double_count_entities() {
        let mut world = World::new();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        let token = spawn_identity_entity(&mut world).unwrap();
        world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_new_scene_instance(
                    world,
                    [(SceneEntityId::new("entity").unwrap(), token)],
                )
            })
            .unwrap();
        let persistent = PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        );
        world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_persistent(world, token, persistent)
            })
            .unwrap();

        let snapshot = WorldIdentitySnapshot::capture(&world, ItemLimit::ONE).unwrap();

        assert_eq!(snapshot.total_entity_count, 1);
        assert_eq!(snapshot.identified_entity_count, 1);
        assert_eq!(snapshot.runtime_only_entity_count, 0);
        assert_eq!(snapshot.returned_locator_count, 1);
        assert_eq!(snapshot.omitted_locator_count, 1);
    }

    #[test]
    fn equal_entity_bits_in_different_worlds_produce_distinct_snapshots() {
        let mut first_world = World::new();
        let first_domain =
            WorldIdentityDomain::new(&first_world, WorldIdentityDomainSettings::default()).unwrap();
        first_world.insert_resource(first_domain);
        let first_token = spawn_identity_entity(&mut first_world).unwrap();
        first_world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_new_scene_instance(
                    world,
                    [(SceneEntityId::new("entity").unwrap(), first_token)],
                )
            })
            .unwrap();

        let mut second_world = World::new();
        let second_domain =
            WorldIdentityDomain::new(&second_world, WorldIdentityDomainSettings::default())
                .unwrap();
        second_world.insert_resource(second_domain);
        let second_token = spawn_identity_entity(&mut second_world).unwrap();
        second_world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_new_scene_instance(
                    world,
                    [(SceneEntityId::new("entity").unwrap(), second_token)],
                )
            })
            .unwrap();

        assert_eq!(
            first_token.entity().to_bits(),
            second_token.entity().to_bits()
        );
        let first = WorldIdentitySnapshot::capture_default(&first_world).unwrap();
        let second = WorldIdentitySnapshot::capture_default(&second_world).unwrap();
        assert_ne!(first.domain_id, second.domain_id);
        assert_ne!(first.locators, second.locators);
    }

    #[test]
    fn a_domain_moved_to_another_world_fails_snapshot_capture() {
        let mut source = World::new();
        let domain =
            WorldIdentityDomain::new(&source, WorldIdentityDomainSettings::default()).unwrap();
        source.insert_resource(domain);
        let domain = source.remove_resource::<WorldIdentityDomain>().unwrap();
        let mut target = World::new();
        target.insert_resource(domain);

        assert_eq!(
            WorldIdentitySnapshot::capture_default(&target),
            Err(IdentityDomainError::WorldBindingMismatch)
        );
    }

    #[test]
    fn stale_identity_registration_fails_snapshot_capture() {
        let mut world = World::new();
        let domain =
            WorldIdentityDomain::new(&world, WorldIdentityDomainSettings::default()).unwrap();
        world.insert_resource(domain);
        let token = spawn_identity_entity(&mut world).unwrap();
        world
            .resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
                domain.register_new_scene_instance(
                    world,
                    [(SceneEntityId::new("entity").unwrap(), token)],
                )
            })
            .unwrap();
        assert!(world.despawn(token.entity()));

        assert_eq!(
            WorldIdentitySnapshot::capture_default(&world),
            Err(IdentityDomainError::StaleRegistration)
        );
    }
}
