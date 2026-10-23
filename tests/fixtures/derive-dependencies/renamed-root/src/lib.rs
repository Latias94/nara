use engine::{PersistentComponent, ecs::Component};

#[cfg(test)]
use engine::ecs;

#[cfg(test)]
#[path = "../../facade_contract.rs"]
mod facade_contract;

#[cfg(test)]
#[derive(engine::prelude::Bundle)]
pub struct RootBundle(RenamedRoot);

#[cfg(test)]
#[derive(engine::prelude::SystemParam)]
pub struct RootParams<'w> {
    value: engine::ecs::ResMut<'w, RootResource>,
}

#[cfg(test)]
#[derive(Default, engine::prelude::Resource)]
pub struct RootResource(u32);

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.RenamedRoot",
    version = 1,
    alias = "Renamed root",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct RenamedRoot {
    #[nara(id = "value", alias = "Value", range(min_inclusive = -5_i64, max_exclusive = 10_i64))]
    pub value: i64,
}

engine::persistent_components! { pub mod inventory = [RenamedRoot]; }

#[test]
fn root_prelude_derives_execute_without_a_bevy_dependency() {
    let mut world = ecs::World::new();
    world.init_resource::<RootResource>();
    let entity = world.spawn(RootBundle(RenamedRoot { value: 4 })).id();
    let mut state = ecs::bevy_ecs::system::SystemState::<RootParams>::new(&mut world);
    state.get_mut(&mut world).unwrap().value.0 += 1;
    assert_eq!(world.resource::<RootResource>().0, 1);
    assert_eq!(world.get::<RenamedRoot>(entity).unwrap().value, 4);
}

#[test]
fn root_only_dependency_exposes_the_inventory_and_typed_range_derive() {
    let mut registry = engine::reflect::ComponentRegistry::new();
    inventory::register(&mut registry).unwrap();
    registry.freeze().unwrap();
    let catalog = inventory::initial_catalog().unwrap();
    assert_eq!(registry.catalog().unwrap().components, catalog.components);
    let schema = &catalog.components[0];
    for (value, valid) in [(-5, true), (9, true), (10, false)] {
        assert_eq!(
            engine::reflect::validate_component_value(
                schema,
                &engine::reflect::ComponentValue::map([(
                    "value",
                    engine::reflect::ComponentValue::I64(value)
                ),]),
            )
            .is_ok(),
            valid
        );
    }
}
