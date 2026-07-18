//! Internal workspace contracts coupled to the selected Bevy ECS version.

use bevy_ecs::{
    component::{Component, ComponentId},
    entity::Entity,
    lifecycle::{ADD, ComponentHooks, DESPAWN, DISCARD, HookContext, INSERT, REMOVE},
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
}
