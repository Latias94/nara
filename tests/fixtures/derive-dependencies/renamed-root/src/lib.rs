use engine::{PersistentComponent, ecs::Component};

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
