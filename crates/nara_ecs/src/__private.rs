//! Internal workspace contracts coupled to the selected Bevy ECS version.

use bevy_ecs::{
    component::{Component, ComponentId},
    entity::Entity,
    event::EventKey,
    lifecycle::{ADD, ComponentHooks, DESPAWN, DISCARD, HookContext, INSERT, REMOVE},
    relationship::{Relationship, RelationshipAccessor, RelationshipTarget},
    resource::{IS_RESOURCE, IsResource, Resource},
    world::{DeferredWorld, World},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentLifecycleEvent {
    Add,
    Insert,
    Discard,
    Remove,
    Despawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentObserverScope {
    EventGlobal,
    ComponentGlobal,
    Entity,
    EntityComponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentComponentMetadataError {
    ComponentMissing,
    RequiredComponents,
    LifecycleHook(PersistentLifecycleEvent),
    Observer {
        event: PersistentLifecycleEvent,
        scope: PersistentObserverScope,
    },
}

pub fn validate_persistent_component_registration<T: Component>()
-> Result<(), PersistentComponentMetadataError> {
    let mut world = World::new();
    let component_id = world.register_component::<T>();
    validate_persistent_component_apply(&world, component_id, None)
}

pub fn register_persistent_component<T: Component>(world: &mut World) -> ComponentId {
    world.register_component::<T>()
}

pub fn validate_registered_persistent_component_apply<T: Component>(
    world: &World,
    target: Option<Entity>,
) -> Result<(), PersistentComponentMetadataError> {
    let component_id = world
        .component_id::<T>()
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    validate_persistent_component_apply(world, component_id, target)
}

fn validate_persistent_component_apply(
    world: &World,
    component_id: ComponentId,
    target: Option<Entity>,
) -> Result<(), PersistentComponentMetadataError> {
    let info = world
        .components()
        .get_info(component_id)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;

    if info.required_components().iter_ids().next().is_some() {
        return Err(PersistentComponentMetadataError::RequiredComponents);
    }
    if let Some(event) = first_hook(info.hooks()) {
        return Err(PersistentComponentMetadataError::LifecycleHook(event));
    }

    validate_component_observers(world, component_id, target)
}

/// Validates the lifecycle work performed when a resource is first inserted.
///
/// Bevy resources intentionally require `IsResource`, whose intrinsic hooks maintain the resource
/// entity cache. Those built-in semantics are allowed; extra requirements, hooks, or observers are
/// not allowed to run inside a guarded Nara transaction.
pub fn validate_resource_insertion<R: Resource>(
    world: &World,
) -> Result<(), PersistentComponentMetadataError> {
    let resource_id = world
        .component_id::<R>()
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    validate_resource_metadata(world, resource_id)?;
    validate_component_observers(world, resource_id, None)?;
    validate_is_resource_support(world)
}

/// Validates the remove/reinsert lifecycle performed by `World::resource_scope`.
pub fn validate_resource_scope<R: Resource>(
    world: &World,
) -> Result<(), PersistentComponentMetadataError> {
    let resource_id = world
        .component_id::<R>()
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    validate_resource_metadata(world, resource_id)?;
    let target = world
        .resource_entities()
        .get(resource_id)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    validate_component_observers(world, resource_id, Some(target))
}

/// Validates that despawning one entity cannot run component hooks or lifecycle observers.
pub fn validate_entity_despawn(
    world: &World,
    target: Entity,
    component_ids: &[ComponentId],
) -> Result<(), PersistentComponentMetadataError> {
    const EVENTS: [(PersistentLifecycleEvent, EventKey); 3] = [
        (PersistentLifecycleEvent::Discard, DISCARD),
        (PersistentLifecycleEvent::Remove, REMOVE),
        (PersistentLifecycleEvent::Despawn, DESPAWN),
    ];

    for component_id in component_ids {
        let info = world
            .components()
            .get_info(*component_id)
            .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
        for (event, _) in EVENTS {
            if hook_is_registered(info.hooks(), event) {
                return Err(PersistentComponentMetadataError::LifecycleHook(event));
            }
        }
    }

    validate_entity_despawn_observers(world, target, component_ids)
}

/// Validates a despawn while allowing only the intrinsic hooks of one known non-linked
/// relationship pair.
///
/// This is intentionally coupled to the selected Bevy ECS version. It accepts the exact hook and
/// relationship metadata generated for `R` and its target, while continuing to reject additional
/// hooks or lifecycle observers.
pub fn validate_entity_despawn_with_non_linked_relationship<R: Relationship>(
    world: &World,
    target: Entity,
    component_ids: &[ComponentId],
) -> Result<(), PersistentComponentMetadataError>
where
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    let (source_id, relationship_target_id) =
        validate_non_linked_relationship_metadata::<R>(world)?;

    for component_id in component_ids {
        if *component_id == source_id || *component_id == relationship_target_id {
            continue;
        }
        let info = world
            .components()
            .get_info(*component_id)
            .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
        for event in [
            PersistentLifecycleEvent::Discard,
            PersistentLifecycleEvent::Remove,
            PersistentLifecycleEvent::Despawn,
        ] {
            if hook_is_registered(info.hooks(), event) {
                return Err(PersistentComponentMetadataError::LifecycleHook(event));
            }
        }
    }

    validate_entity_despawn_observers(world, target, component_ids)
}

/// Validates every lifecycle observer that relationship teardown could reach on `targets`.
pub fn validate_non_linked_relationship_teardown<R: Relationship>(
    world: &World,
    targets: &[Entity],
) -> Result<(), PersistentComponentMetadataError>
where
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    let (source_id, relationship_target_id) =
        validate_non_linked_relationship_metadata::<R>(world)?;
    for target in targets {
        validate_component_observers(world, source_id, Some(*target))?;
        validate_component_observers(world, relationship_target_id, Some(*target))?;
    }
    Ok(())
}

/// Validates the insertion work reachable while constructing one known non-linked relationship
/// pair.
///
/// Component registration and the existing deferred baseline are completed before the proof is
/// returned. The caller must retain exclusive World access until every edge is inserted.
pub fn validate_non_linked_relationship_insertion<R: Relationship>(
    world: &mut World,
    edges: impl IntoIterator<Item = (Entity, Entity)>,
) -> Result<(), PersistentComponentMetadataError>
where
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    world.register_component::<R>();
    world.register_component::<R::RelationshipTarget>();
    world.flush();

    let (source_id, relationship_target_id) =
        validate_non_linked_relationship_metadata::<R>(world)?;
    for (child, parent) in edges {
        validate_entity_insertion_observers(world, child, &[source_id])?;
        validate_entity_insertion_observers(world, parent, &[relationship_target_id])?;
    }
    Ok(())
}

fn validate_non_linked_relationship_metadata<R: Relationship>(
    world: &World,
) -> Result<(ComponentId, ComponentId), PersistentComponentMetadataError>
where
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    let source_id = world
        .component_id::<R>()
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    let relationship_target_id = world
        .component_id::<R::RelationshipTarget>()
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    let source = world
        .components()
        .get_info(source_id)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    let relationship_target = world
        .components()
        .get_info(relationship_target_id)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;

    if source.required_components().iter_ids().next().is_some()
        || relationship_target
            .required_components()
            .iter_ids()
            .next()
            .is_some()
    {
        return Err(PersistentComponentMetadataError::RequiredComponents);
    }

    match source.relationship_accessor() {
        Some(RelationshipAccessor::Relationship {
            linked_spawn: false,
            allow_self_referential: false,
            relationship_target,
            ..
        }) if *relationship_target == relationship_target_id => {}
        _ => return Err(PersistentComponentMetadataError::ComponentMissing),
    }
    match relationship_target.relationship_accessor() {
        Some(RelationshipAccessor::RelationshipTarget {
            linked_spawn: false,
            allow_self_referential: false,
            relationship,
            ..
        }) if *relationship == source_id => {}
        _ => return Err(PersistentComponentMetadataError::ComponentMissing),
    }

    validate_exact_hooks(
        source.hooks(),
        &[
            PersistentLifecycleEvent::Insert,
            PersistentLifecycleEvent::Discard,
        ],
    )?;
    validate_exact_hooks(
        relationship_target.hooks(),
        &[PersistentLifecycleEvent::Discard],
    )?;

    Ok((source_id, relationship_target_id))
}

fn validate_exact_hooks(
    hooks: &ComponentHooks,
    expected: &[PersistentLifecycleEvent],
) -> Result<(), PersistentComponentMetadataError> {
    for event in [
        PersistentLifecycleEvent::Add,
        PersistentLifecycleEvent::Insert,
        PersistentLifecycleEvent::Discard,
        PersistentLifecycleEvent::Remove,
        PersistentLifecycleEvent::Despawn,
    ] {
        if hook_is_registered(hooks, event) != expected.contains(&event) {
            return Err(PersistentComponentMetadataError::LifecycleHook(event));
        }
    }
    Ok(())
}

fn validate_entity_despawn_observers(
    world: &World,
    target: Entity,
    component_ids: &[ComponentId],
) -> Result<(), PersistentComponentMetadataError> {
    for (event, event_key) in [
        (PersistentLifecycleEvent::Discard, DISCARD),
        (PersistentLifecycleEvent::Remove, REMOVE),
        (PersistentLifecycleEvent::Despawn, DESPAWN),
    ] {
        let Some(observers) = world.observers().try_get_observers(event_key) else {
            continue;
        };
        if !observers.global_observers().is_empty() {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::EventGlobal,
            });
        }
        if observers
            .entity_observers()
            .get(&target)
            .is_some_and(|observers| !observers.is_empty())
        {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::Entity,
            });
        }
        for component_id in component_ids {
            let Some(component_observers) = observers.component_observers().get(component_id)
            else {
                continue;
            };
            if !component_observers.global_observers().is_empty() {
                return Err(PersistentComponentMetadataError::Observer {
                    event,
                    scope: PersistentObserverScope::ComponentGlobal,
                });
            }
            if component_observers
                .entity_component_observers()
                .get(&target)
                .is_some_and(|observers| !observers.is_empty())
            {
                return Err(PersistentComponentMetadataError::Observer {
                    event,
                    scope: PersistentObserverScope::EntityComponent,
                });
            }
        }
    }

    Ok(())
}

/// Validates that inserting the selected components cannot run implicit lifecycle work.
pub fn validate_entity_insertion(
    world: &World,
    target: Entity,
    component_ids: &[ComponentId],
) -> Result<(), PersistentComponentMetadataError> {
    const EVENTS: [(PersistentLifecycleEvent, EventKey); 2] = [
        (PersistentLifecycleEvent::Add, ADD),
        (PersistentLifecycleEvent::Insert, INSERT),
    ];

    for component_id in component_ids {
        let info = world
            .components()
            .get_info(*component_id)
            .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
        if info.required_components().iter_ids().next().is_some() {
            return Err(PersistentComponentMetadataError::RequiredComponents);
        }
        for (event, _) in EVENTS {
            if hook_is_registered(info.hooks(), event) {
                return Err(PersistentComponentMetadataError::LifecycleHook(event));
            }
        }
    }

    validate_entity_insertion_observers(world, target, component_ids)
}

fn validate_entity_insertion_observers(
    world: &World,
    target: Entity,
    component_ids: &[ComponentId],
) -> Result<(), PersistentComponentMetadataError> {
    for (event, event_key) in [
        (PersistentLifecycleEvent::Add, ADD),
        (PersistentLifecycleEvent::Insert, INSERT),
    ] {
        let Some(observers) = world.observers().try_get_observers(event_key) else {
            continue;
        };
        if !observers.global_observers().is_empty() {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::EventGlobal,
            });
        }
        if observers
            .entity_observers()
            .get(&target)
            .is_some_and(|observers| !observers.is_empty())
        {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::Entity,
            });
        }
        for component_id in component_ids {
            let Some(component_observers) = observers.component_observers().get(component_id)
            else {
                continue;
            };
            if !component_observers.global_observers().is_empty() {
                return Err(PersistentComponentMetadataError::Observer {
                    event,
                    scope: PersistentObserverScope::ComponentGlobal,
                });
            }
            if component_observers
                .entity_component_observers()
                .get(&target)
                .is_some_and(|observers| !observers.is_empty())
            {
                return Err(PersistentComponentMetadataError::Observer {
                    event,
                    scope: PersistentObserverScope::EntityComponent,
                });
            }
        }
    }

    Ok(())
}

/// Validates exact retirement after a known non-linked relationship owner preflights detach.
pub fn validate_lifecycle_free_relationship_despawn<R>(
    world: &mut World,
    entities: &[Entity],
    relationship_targets: &[Entity],
) -> Result<(), crate::LifecycleFreeDespawnError>
where
    R: Relationship,
    R::RelationshipTarget: RelationshipTarget<Relationship = R>,
{
    crate::transaction::validate_lifecycle_free_relationship_despawn::<R>(
        world,
        entities,
        relationship_targets,
    )
}

fn validate_resource_metadata(
    world: &World,
    resource_id: ComponentId,
) -> Result<(), PersistentComponentMetadataError> {
    let info = world
        .components()
        .get_info(resource_id)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    let required = info.required_components().iter_ids().collect::<Vec<_>>();
    if required.as_slice() != [IS_RESOURCE] {
        return Err(PersistentComponentMetadataError::RequiredComponents);
    }
    if let Some(event) = first_hook(info.hooks()) {
        return Err(PersistentComponentMetadataError::LifecycleHook(event));
    }
    Ok(())
}

fn validate_is_resource_support(world: &World) -> Result<(), PersistentComponentMetadataError> {
    let info = world
        .components()
        .get_info(IS_RESOURCE)
        .ok_or(PersistentComponentMetadataError::ComponentMissing)?;
    if info.required_components().iter_ids().next().is_some() {
        return Err(PersistentComponentMetadataError::RequiredComponents);
    }

    let hooks = info.hooks();
    if hook_is_registered(hooks, PersistentLifecycleEvent::Add) {
        return Err(PersistentComponentMetadataError::LifecycleHook(
            PersistentLifecycleEvent::Add,
        ));
    }
    if hook_is_registered(hooks, PersistentLifecycleEvent::Remove) {
        return Err(PersistentComponentMetadataError::LifecycleHook(
            PersistentLifecycleEvent::Remove,
        ));
    }
    if !hook_is_registered(hooks, PersistentLifecycleEvent::Insert)
        || !hook_is_registered(hooks, PersistentLifecycleEvent::Discard)
        || !hook_is_registered(hooks, PersistentLifecycleEvent::Despawn)
    {
        return Err(PersistentComponentMetadataError::ComponentMissing);
    }

    validate_component_observers(
        world,
        world.component_id::<IsResource>().unwrap_or(IS_RESOURCE),
        None,
    )
}

fn validate_component_observers(
    world: &World,
    component_id: ComponentId,
    target: Option<Entity>,
) -> Result<(), PersistentComponentMetadataError> {
    for (event, event_key) in [
        (PersistentLifecycleEvent::Add, ADD),
        (PersistentLifecycleEvent::Insert, INSERT),
        (PersistentLifecycleEvent::Discard, DISCARD),
        (PersistentLifecycleEvent::Remove, REMOVE),
        (PersistentLifecycleEvent::Despawn, DESPAWN),
    ] {
        let Some(observers) = world.observers().try_get_observers(event_key) else {
            continue;
        };
        if !observers.global_observers().is_empty() {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::EventGlobal,
            });
        }
        let component_observers = observers.component_observers().get(&component_id);
        if component_observers.is_some_and(|observers| !observers.global_observers().is_empty()) {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::ComponentGlobal,
            });
        }
        let Some(target) = target else {
            continue;
        };
        if observers
            .entity_observers()
            .get(&target)
            .is_some_and(|observers| !observers.is_empty())
        {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::Entity,
            });
        }
        if component_observers
            .and_then(|observers| observers.entity_component_observers().get(&target))
            .is_some_and(|observers| !observers.is_empty())
        {
            return Err(PersistentComponentMetadataError::Observer {
                event,
                scope: PersistentObserverScope::EntityComponent,
            });
        }
    }

    Ok(())
}

fn first_hook(hooks: &ComponentHooks) -> Option<PersistentLifecycleEvent> {
    [
        PersistentLifecycleEvent::Add,
        PersistentLifecycleEvent::Insert,
        PersistentLifecycleEvent::Discard,
        PersistentLifecycleEvent::Remove,
        PersistentLifecycleEvent::Despawn,
    ]
    .into_iter()
    .find(|event| hook_is_registered(hooks, *event))
}

fn hook_is_registered(hooks: &ComponentHooks, event: PersistentLifecycleEvent) -> bool {
    let mut probe = hooks.clone();
    match event {
        PersistentLifecycleEvent::Add => probe.try_on_add(noop_hook).is_none(),
        PersistentLifecycleEvent::Insert => probe.try_on_insert(noop_hook).is_none(),
        PersistentLifecycleEvent::Discard => probe.try_on_discard(noop_hook).is_none(),
        PersistentLifecycleEvent::Remove => probe.try_on_remove(noop_hook).is_none(),
        PersistentLifecycleEvent::Despawn => probe.try_on_despawn(noop_hook).is_none(),
    }
}

fn noop_hook(_world: DeferredWorld<'_>, _context: HookContext) {}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::{
        component::Component,
        lifecycle::{Add, Despawn, Discard, Insert, Remove},
        observer::On,
    };

    #[derive(Component)]
    struct Plain;

    #[derive(Component)]
    #[require(Required)]
    struct Requires;

    #[derive(Component, Default)]
    struct Required;

    #[derive(Component)]
    #[component(on_add = intrinsic_add)]
    struct Hooked;

    #[derive(Component)]
    #[relationship(relationship_target = TestChildren)]
    struct TestParent(Entity);

    #[derive(Component)]
    #[relationship_target(relationship = TestParent)]
    struct TestChildren(Vec<Entity>);

    #[derive(Resource)]
    struct PlainResource;

    #[derive(Resource)]
    #[require(Required)]
    struct RequiredResource;

    fn intrinsic_add(_world: DeferredWorld<'_>, _context: HookContext) {}

    #[test]
    fn registration_rejects_required_components_and_intrinsic_hooks() {
        assert_eq!(
            validate_persistent_component_registration::<Plain>(),
            Ok(())
        );
        assert_eq!(
            validate_persistent_component_registration::<Requires>(),
            Err(PersistentComponentMetadataError::RequiredComponents)
        );
        assert_eq!(
            validate_persistent_component_registration::<Hooked>(),
            Err(PersistentComponentMetadataError::LifecycleHook(
                PersistentLifecycleEvent::Add
            ))
        );
    }

    #[test]
    fn apply_probe_observes_dynamic_hooks_and_component_observers() {
        let mut hooked = World::new();
        let component_id = hooked.register_component::<Plain>();
        hooked
            .register_component_hooks::<Plain>()
            .on_remove(noop_hook);
        assert_eq!(
            validate_persistent_component_apply(&hooked, component_id, None),
            Err(PersistentComponentMetadataError::LifecycleHook(
                PersistentLifecycleEvent::Remove
            ))
        );

        let mut observed = World::new();
        observed.add_observer(|_: On<Add, Plain>| {});
        observed.flush();
        let component_id = observed.component_id::<Plain>().unwrap();
        assert_eq!(
            validate_persistent_component_apply(&observed, component_id, None),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Add,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );

        let mut required = World::new();
        required.register_required_components::<Plain, Required>();
        let component_id = required.component_id::<Plain>().unwrap();
        assert_eq!(
            validate_persistent_component_apply(&required, component_id, None),
            Err(PersistentComponentMetadataError::RequiredComponents)
        );
    }

    #[test]
    fn apply_probe_covers_every_lifecycle_event_and_observer_scope() {
        let mut event_global = World::new();
        event_global.register_component::<Plain>();
        event_global.add_observer(|_: On<Add>| {});
        event_global.flush();
        assert_eq!(
            validate_persistent_component_apply(
                &event_global,
                event_global.component_id::<Plain>().unwrap(),
                None,
            ),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Add,
                scope: PersistentObserverScope::EventGlobal,
            })
        );

        let mut component_global = World::new();
        component_global.add_observer(|_: On<Insert, Plain>| {});
        component_global.flush();
        assert_eq!(
            validate_persistent_component_apply(
                &component_global,
                component_global.component_id::<Plain>().unwrap(),
                None,
            ),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Insert,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );

        let mut entity = World::new();
        let target = entity.spawn_empty().id();
        entity.entity_mut(target).observe(|_: On<Discard>| {});
        entity.flush();
        let component_id = entity.register_component::<Plain>();
        assert_eq!(
            validate_persistent_component_apply(&entity, component_id, Some(target)),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Discard,
                scope: PersistentObserverScope::Entity,
            })
        );

        let mut entity_component = World::new();
        let target = entity_component.spawn_empty().id();
        entity_component
            .entity_mut(target)
            .observe(|_: On<Remove, Plain>| {});
        entity_component.flush();
        assert_eq!(
            validate_persistent_component_apply(
                &entity_component,
                entity_component.component_id::<Plain>().unwrap(),
                Some(target),
            ),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Remove,
                scope: PersistentObserverScope::EntityComponent,
            })
        );

        let mut despawn = World::new();
        despawn.add_observer(|_: On<Despawn, Plain>| {});
        despawn.flush();
        assert_eq!(
            validate_persistent_component_apply(
                &despawn,
                despawn.component_id::<Plain>().unwrap(),
                None,
            ),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Despawn,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );
    }

    #[test]
    fn resource_probe_allows_bevy_plumbing_and_rejects_external_lifecycle_work() {
        let mut plain = World::new();
        plain.register_component::<PlainResource>();
        assert_eq!(validate_resource_insertion::<PlainResource>(&plain), Ok(()));

        let mut required = World::new();
        required.register_component::<RequiredResource>();
        assert_eq!(
            validate_resource_insertion::<RequiredResource>(&required),
            Err(PersistentComponentMetadataError::RequiredComponents)
        );

        let mut observed_resource = World::new();
        observed_resource.add_observer(|_: On<Add, PlainResource>| {});
        observed_resource.flush();
        assert_eq!(
            validate_resource_insertion::<PlainResource>(&observed_resource),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Add,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );

        let mut observed_marker = World::new();
        observed_marker.register_component::<PlainResource>();
        observed_marker.add_observer(|_: On<Add, IsResource>| {});
        observed_marker.flush();
        assert_eq!(
            validate_resource_insertion::<PlainResource>(&observed_marker),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Add,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );

        let mut scoped = World::new();
        scoped.insert_resource(PlainResource);
        let resource_id = scoped.component_id::<PlainResource>().unwrap();
        let resource_entity = scoped.resource_entities().get(resource_id).unwrap();
        scoped
            .entity_mut(resource_entity)
            .observe(|_: On<Remove, PlainResource>| {});
        scoped.flush();
        assert_eq!(
            validate_resource_scope::<PlainResource>(&scoped),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Remove,
                scope: PersistentObserverScope::EntityComponent,
            })
        );
    }

    #[test]
    fn relationship_insertion_probe_accepts_intrinsic_hooks_and_rejects_observers() {
        let mut plain = World::new();
        let parent = plain.spawn_empty().id();
        let child = plain.spawn_empty().id();
        assert_eq!(
            validate_non_linked_relationship_insertion::<TestParent>(&mut plain, [(child, parent)],),
            Ok(())
        );

        let mut observed = World::new();
        let parent = observed.spawn_empty().id();
        let child = observed.spawn_empty().id();
        observed.add_observer(|_: On<Add, TestParent>| {});
        observed.flush();
        assert_eq!(
            validate_non_linked_relationship_insertion::<TestParent>(
                &mut observed,
                [(child, parent)],
            ),
            Err(PersistentComponentMetadataError::Observer {
                event: PersistentLifecycleEvent::Add,
                scope: PersistentObserverScope::ComponentGlobal,
            })
        );
    }
}
