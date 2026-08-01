use std::{collections::BTreeSet, error::Error, fmt};

use bevy_ecs::{
    component::{Component, ComponentId},
    entity::Entity,
    relationship::{Relationship, RelationshipTarget},
    world::World,
};

use crate::__private::{
    validate_entity_despawn, validate_entity_despawn_with_non_linked_relationship,
    validate_entity_insertion, validate_non_linked_relationship_teardown,
};

/// Rejection while preparing an exclusive despawn with no lifecycle side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleFreeDespawnError {
    DuplicateEntity,
    EntityMissing,
    LifecycleWork,
}

impl fmt::Display for LifecycleFreeDespawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateEntity => "despawn transaction contains a duplicate entity",
            Self::EntityMissing => "despawn transaction contains a missing entity",
            Self::LifecycleWork => "despawn transaction would run lifecycle work",
        })
    }
}

impl Error for LifecycleFreeDespawnError {}

/// Exclusive proof that the selected entities can be despawned without lifecycle side effects.
///
/// The guard retains the only mutable `World` borrow between validation and commit, preventing
/// later hook or observer registration from invalidating the proof.
pub struct LifecycleFreeDespawn<'world, 'entities> {
    world: &'world mut World,
    entities: &'entities [Entity],
}

/// Prepares an exclusive, lifecycle-free despawn transaction.
pub fn prepare_lifecycle_free_despawn<'world, 'entities>(
    world: &'world mut World,
    entities: &'entities [Entity],
) -> Result<LifecycleFreeDespawn<'world, 'entities>, LifecycleFreeDespawnError> {
    let mut unique = BTreeSet::new();
    for entity in entities.iter().copied() {
        if !unique.insert(entity) {
            return Err(LifecycleFreeDespawnError::DuplicateEntity);
        }
    }
    // Deferred work belongs to the pre-transaction baseline. Flushing it before validation keeps
    // `despawn()` from discovering older commands after the proof has been issued.
    world.flush();
    for entity in entities.iter().copied() {
        let entity_ref = world
            .get_entity(entity)
            .map_err(|_| LifecycleFreeDespawnError::EntityMissing)?;
        validate_entity_despawn(world, entity, entity_ref.archetype().components())
            .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    }

    Ok(LifecycleFreeDespawn { world, entities })
}

/// Validates a future despawn whose only provisional lifecycle work is the intrinsic teardown of
/// one known non-linked relationship pair.
///
/// This function does not detach relationships or weaken the strict lifecycle-free despawn guard.
/// The relationship owner must keep exclusive World access, apply its private prevalidated detach,
/// and then use [`prepare_lifecycle_free_despawn`] for the final exact retirement set.
#[doc(hidden)]
pub(crate) fn validate_lifecycle_free_relationship_despawn<R>(
    world: &mut World,
    entities: &[Entity],
    relationship_targets: &[Entity],
) -> Result<(), LifecycleFreeDespawnError>
where
    R: Relationship,
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    let mut unique = BTreeSet::new();
    for entity in entities.iter().copied() {
        if !unique.insert(entity) {
            return Err(LifecycleFreeDespawnError::DuplicateEntity);
        }
    }
    world.register_component::<R>();
    world.register_component::<R::RelationshipTarget>();
    world.flush();
    validate_non_linked_relationship_teardown::<R>(world, relationship_targets)
        .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    for entity in entities.iter().copied() {
        let entity_ref = world
            .get_entity(entity)
            .map_err(|_| LifecycleFreeDespawnError::EntityMissing)?;
        validate_entity_despawn_with_non_linked_relationship::<R>(
            world,
            entity,
            entity_ref.archetype().components(),
        )
        .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    }

    Ok(())
}

/// Rejection while preparing or applying a lifecycle-free component insertion transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleFreeInsertionError {
    DuplicateComponent,
    EntityMissing,
    ComponentAlreadyPresent,
    LifecycleWork,
}

impl fmt::Display for LifecycleFreeInsertionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateComponent => "component insertion transaction contains a duplicate",
            Self::EntityMissing => "component insertion transaction contains a missing entity",
            Self::ComponentAlreadyPresent => {
                "component insertion target already contains the component"
            }
            Self::LifecycleWork => "component insertion transaction would run lifecycle work",
        })
    }
}

impl Error for LifecycleFreeInsertionError {}

struct PlannedComponentInsertion {
    target: Entity,
    register: fn(&mut World) -> ComponentId,
    apply: Box<dyn FnOnce(&mut World)>,
}

/// Owned component values validated together before any insertion is applied.
#[derive(Default)]
pub struct LifecycleFreeInsertionPlan {
    insertions: Vec<PlannedComponentInsertion>,
}

impl LifecycleFreeInsertionPlan {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            insertions: Vec::new(),
        }
    }

    pub fn push<T: Component>(&mut self, target: Entity, component: T) {
        self.insertions.push(PlannedComponentInsertion {
            target,
            register: World::register_component::<T>,
            apply: Box::new(move |world| {
                world.entity_mut(target).insert(component);
            }),
        });
    }

    /// Flushes the pre-transaction baseline and validates every value before applying any value.
    pub fn commit(self, world: &mut World) -> Result<&mut World, LifecycleFreeInsertionError> {
        let mut identities = Vec::with_capacity(self.insertions.len());
        let mut unique = BTreeSet::new();
        for insertion in &self.insertions {
            let component_id = (insertion.register)(world);
            if !unique.insert((insertion.target, component_id)) {
                return Err(LifecycleFreeInsertionError::DuplicateComponent);
            }
            identities.push((insertion.target, component_id));
        }
        world.flush();
        for (target, component_id) in &identities {
            let entity = world
                .get_entity(*target)
                .map_err(|_| LifecycleFreeInsertionError::EntityMissing)?;
            if entity.contains_id(*component_id) {
                return Err(LifecycleFreeInsertionError::ComponentAlreadyPresent);
            }
            validate_entity_insertion(world, *target, &[*component_id])
                .map_err(|_| LifecycleFreeInsertionError::LifecycleWork)?;
        }
        for insertion in self.insertions {
            (insertion.apply)(world);
        }
        Ok(world)
    }
}

impl<'world> LifecycleFreeDespawn<'world, '_> {
    /// Returns a read-only view while preserving the exclusive validation guard.
    #[doc(hidden)]
    #[must_use]
    pub fn world(&self) -> &World {
        self.world
    }

    /// Cancels the transaction without despawning and returns the exclusive world borrow.
    #[doc(hidden)]
    #[must_use]
    pub fn cancel(self) -> &'world mut World {
        self.world
    }

    /// Despawns every validated entity and returns the still-exclusively-borrowed world.
    #[must_use]
    pub fn commit(self) -> &'world mut World {
        for entity in self.entities {
            // The guard preserves exclusive access after proving every unique entity exists and
            // cannot run lifecycle work that removes another member.
            self.world.entity_mut(*entity).despawn();
        }
        self.world
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::{
        component::Component,
        entity::Entity,
        lifecycle::{Despawn, Remove},
        observer::On,
        relationship::Relationship,
        resource::Resource,
    };

    #[derive(Component)]
    struct Probe;

    #[derive(Component, Debug, PartialEq, Eq)]
    struct Value(u32);

    #[derive(Component)]
    #[relationship(relationship_target = TestChildren)]
    struct TestParent(Entity);

    #[derive(Component)]
    #[relationship_target(relationship = TestParent)]
    struct TestChildren(Vec<Entity>);

    #[derive(Resource, Default)]
    struct ObserverRuns(u32);

    #[test]
    fn commits_unique_entities_without_lifecycle_work() {
        let mut world = World::new();
        let first = world.spawn(Probe).id();
        let second = world.spawn(Probe).id();

        let _ = prepare_lifecycle_free_despawn(&mut world, &[first, second])
            .unwrap()
            .commit();

        assert!(world.get_entity(first).is_err());
        assert!(world.get_entity(second).is_err());
    }

    #[test]
    fn rejects_missing_and_duplicate_entities_without_mutation() {
        let mut world = World::new();
        let first = world.spawn(Probe).id();
        let missing = world.spawn(Probe).id();
        world.despawn(missing);

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut world, &[first, first])
                .err()
                .expect("duplicate retirement should reject"),
            LifecycleFreeDespawnError::DuplicateEntity
        );
        assert_eq!(
            prepare_lifecycle_free_despawn(&mut world, &[first, missing])
                .err()
                .expect("missing retirement should reject"),
            LifecycleFreeDespawnError::EntityMissing
        );
        assert!(world.get_entity(first).is_ok());
    }

    #[test]
    fn rejects_component_and_entity_retirement_observers_without_running_them() {
        let mut component_world = World::new();
        component_world.init_resource::<ObserverRuns>();
        let component_target = component_world.spawn(Probe).id();
        component_world.add_observer(
            |_: On<Remove, Probe>, mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| runs.0 += 1,
        );

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut component_world, &[component_target])
                .err()
                .expect("component observer should reject"),
            LifecycleFreeDespawnError::LifecycleWork
        );
        assert_eq!(component_world.resource::<ObserverRuns>().0, 0);
        assert!(component_world.get_entity(component_target).is_ok());

        let mut entity_world = World::new();
        entity_world.init_resource::<ObserverRuns>();
        let entity_target = entity_world.spawn(Probe).id();
        let observer = entity_world
            .spawn(
                bevy_ecs::observer::Observer::new(
                    |_: On<Despawn>, mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| runs.0 += 1,
                )
                .with_entity(entity_target),
            )
            .id();
        assert!(entity_world.get_entity(observer).is_ok());

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut entity_world, &[entity_target])
                .err()
                .expect("entity observer should reject"),
            LifecycleFreeDespawnError::LifecycleWork
        );
        assert_eq!(entity_world.resource::<ObserverRuns>().0, 0);
        assert!(entity_world.get_entity(entity_target).is_ok());
    }

    #[test]
    fn despawn_preparation_flushes_pending_commands_before_issuing_the_proof() {
        let mut missing_world = World::new();
        let retained = missing_world.spawn(Probe).id();
        let queued_for_despawn = missing_world.spawn(Probe).id();
        missing_world.commands().queue(move |world: &mut World| {
            world.despawn(queued_for_despawn);
        });

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut missing_world, &[retained, queued_for_despawn])
                .err()
                .expect("the flushed despawn should invalidate the requested set"),
            LifecycleFreeDespawnError::EntityMissing
        );
        assert!(missing_world.get_entity(retained).is_ok());
        assert!(missing_world.get_entity(queued_for_despawn).is_err());

        let mut observer_world = World::new();
        observer_world.init_resource::<ObserverRuns>();
        let target = observer_world.spawn(Probe).id();
        observer_world.commands().queue(move |world: &mut World| {
            world.spawn(
                bevy_ecs::observer::Observer::new(
                    |_: On<Despawn>, mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| {
                        runs.0 += 1;
                    },
                )
                .with_entity(target),
            );
        });

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut observer_world, &[target])
                .err()
                .expect("the flushed observer should invalidate the proof"),
            LifecycleFreeDespawnError::LifecycleWork
        );
        assert_eq!(observer_world.resource::<ObserverRuns>().0, 0);
        assert!(observer_world.get_entity(target).is_ok());
    }

    #[test]
    fn relationship_retirement_detaches_non_linked_children_before_exact_despawn() {
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let child = world.spawn(<TestParent as Relationship>::from(parent)).id();
        world.flush();

        assert_eq!(
            prepare_lifecycle_free_despawn(&mut world, &[parent])
                .err()
                .expect("relationship hooks must remain ineligible for the strict guard"),
            LifecycleFreeDespawnError::LifecycleWork
        );

        validate_lifecycle_free_relationship_despawn::<TestParent>(
            &mut world,
            &[parent],
            &[parent, child],
        )
        .expect("the exact intrinsic non-linked relationship should be admitted");
        world.entity_mut(child).remove::<TestParent>();
        let retirement = [parent];
        let ready = prepare_lifecycle_free_despawn(&mut world, &retirement)
            .expect("the detached parent should now satisfy the strict guard");
        let world = ready.commit();

        assert!(world.get_entity(parent).is_err());
        assert!(world.get_entity(child).is_ok());
        assert!(world.get::<TestParent>(child).is_none());
    }

    #[test]
    fn relationship_retirement_rejects_user_observers_before_detach() {
        let mut world = World::new();
        world.init_resource::<ObserverRuns>();
        let parent = world.spawn_empty().id();
        let child = world.spawn(<TestParent as Relationship>::from(parent)).id();
        world.flush();
        world.add_observer(
            |_: On<Remove, TestParent>, mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| {
                runs.0 += 1;
            },
        );

        assert_eq!(
            validate_lifecycle_free_relationship_despawn::<TestParent>(
                &mut world,
                &[parent],
                &[parent, child],
            )
            .expect_err("a relationship observer must reject before mutation"),
            LifecycleFreeDespawnError::LifecycleWork
        );
        assert_eq!(world.resource::<ObserverRuns>().0, 0);
        assert_eq!(
            world
                .get::<TestParent>(child)
                .map(|relationship| relationship.get()),
            Some(parent)
        );
    }

    #[test]
    fn insertion_plan_applies_the_complete_prevalidated_component_set() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let mut insertion = LifecycleFreeInsertionPlan::new();
        insertion.push(target, Probe);
        insertion.push(target, Value(7));
        let world = insertion.commit(&mut world).unwrap();

        assert!(world.get::<Probe>(target).is_some());
        assert_eq!(world.get::<Value>(target), Some(&Value(7)));

        let duplicate_target = world.spawn_empty().id();
        let mut duplicate = LifecycleFreeInsertionPlan::new();
        duplicate.push(duplicate_target, Probe);
        duplicate.push(duplicate_target, Probe);
        let error = match duplicate.commit(world) {
            Ok(_) => panic!("duplicate component insertion should reject"),
            Err(error) => error,
        };
        assert_eq!(error, LifecycleFreeInsertionError::DuplicateComponent);
        assert!(world.get::<Probe>(duplicate_target).is_none());
    }

    #[test]
    fn insertion_plan_rejects_add_observers_before_component_mutation() {
        let mut world = World::new();
        world.init_resource::<ObserverRuns>();
        let target = world.spawn_empty().id();
        world.add_observer(
            |_: On<bevy_ecs::lifecycle::Add, Probe>,
             mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| { runs.0 += 1 },
        );
        let mut insertion = LifecycleFreeInsertionPlan::new();
        insertion.push(target, Probe);

        let error = match insertion.commit(&mut world) {
            Ok(_) => panic!("the component observer should reject before insertion"),
            Err(error) => error,
        };
        assert_eq!(error, LifecycleFreeInsertionError::LifecycleWork);
        assert!(world.get::<Probe>(target).is_none());
        assert_eq!(world.resource::<ObserverRuns>().0, 0);
    }

    #[test]
    fn insertion_plan_flushes_and_rejects_a_later_value_before_any_insertion() {
        let mut world = World::new();
        world.init_resource::<ObserverRuns>();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        world.commands().queue(|world: &mut World| {
            world.add_observer(
                |_: On<bevy_ecs::lifecycle::Add, Value>,
                 mut runs: bevy_ecs::prelude::ResMut<ObserverRuns>| { runs.0 += 1 },
            );
        });
        let mut insertion = LifecycleFreeInsertionPlan::new();
        insertion.push(first, Probe);
        insertion.push(second, Value(7));

        let error = match insertion.commit(&mut world) {
            Ok(_) => panic!("the queued observer should reject the complete insertion plan"),
            Err(error) => error,
        };

        assert_eq!(error, LifecycleFreeInsertionError::LifecycleWork);
        assert!(world.get::<Probe>(first).is_none());
        assert!(world.get::<Value>(second).is_none());
        assert_eq!(world.resource::<ObserverRuns>().0, 0);
    }
}
