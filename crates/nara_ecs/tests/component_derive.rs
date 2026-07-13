use nara_ecs::{
    Component, World,
    component::{Immutable, StorageType},
    entity::EntityCloner,
};

#[derive(Component)]
struct GenericComponent<T>(T);

#[derive(Component)]
struct GenericWithWhere<T>(T)
where
    T: Clone;

#[derive(Component)]
struct ConstGenericComponent<T, const N: usize>([T; N]);

#[derive(Component)]
#[component(storage = "SparseSet", immutable)]
struct SparseImmutableComponent;

#[derive(Clone, Component)]
#[component(clone_behavior = Ignore)]
struct IgnoredCloneComponent(u32);

fn assert_component<T: Component>() {}

fn assert_immutable_component<T: Component<Mutability = Immutable>>() {}

#[test]
fn derive_preserves_generic_bounds_and_component_attributes() {
    assert_component::<GenericComponent<String>>();
    assert_component::<GenericWithWhere<String>>();
    assert_component::<ConstGenericComponent<String, 4>>();
    assert_immutable_component::<SparseImmutableComponent>();
    assert_eq!(
        SparseImmutableComponent::STORAGE_TYPE,
        StorageType::SparseSet
    );
}

#[test]
fn derive_preserves_clone_behavior() {
    let mut world = World::new();
    let source = world.spawn(IgnoredCloneComponent(7)).id();
    let target = world.spawn_empty().id();

    EntityCloner::build_opt_out(&mut world).clone_entity(source, target);

    assert_eq!(world.get::<IgnoredCloneComponent>(source).unwrap().0, 7);
    assert!(world.get::<IgnoredCloneComponent>(target).is_none());
}
