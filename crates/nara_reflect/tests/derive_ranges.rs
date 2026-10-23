use nara_ecs::Component;
use nara_reflect::{
    ComponentCodecError, ComponentNumericBound as Bound, ComponentNumericInterval,
    ComponentRegistry, ComponentValue, PersistentComponent, PersistentComponentProvider,
    validate_component_value,
};

type Seconds = f32;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "test.Cooldown",
    version = 1,
    alias = "Cooldown",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    authoring_initial
)]
struct Cooldown {
    #[nara(
        id = "seconds",
        alias = "Seconds",
        initial = 0.5_f32,
        range(min_inclusive = 0.0_f32, max_inclusive = 30.0_f32)
    )]
    seconds: Seconds,
    #[nara(id = "offset", alias = "Offset", initial = -2_i16,
        range(min_exclusive = -10_i16, max_inclusive = 10_i16))]
    offset: i16,
    #[nara(
        id = "count",
        alias = "Count",
        initial = 1_u64,
        range(max_exclusive = 5_u64)
    )]
    count: u64,
}

nara_reflect::persistent_components! { pub mod inventory = [Cooldown]; }
nara_reflect::persistent_components! { mod duplicate_inventory = [Cooldown, Cooldown]; }

mod nested_inventory {
    use super::Cooldown as ImportedCooldown;

    #[allow(non_snake_case, dead_code)]
    fn Ok() {}

    nara_reflect::persistent_components! { pub mod types = [ImportedCooldown]; }
}

#[test]
fn typed_range_literals_and_aliases_reach_shared_validation() {
    let schema = Cooldown::persistent_component_schema();
    let seconds = schema
        .fields
        .iter()
        .find(|field| field.id.as_str() == "seconds")
        .unwrap();
    assert_eq!(
        seconds.shape.numeric_interval(),
        Some(ComponentNumericInterval::F64 {
            min: Some(Bound::Included(
                nara_reflect::ComponentFloat::new(0.0).unwrap()
            )),
            max: Some(Bound::Included(
                nara_reflect::ComponentFloat::new(30.0).unwrap()
            )),
        })
    );
    let mut registry = ComponentRegistry::new();
    inventory::register(&mut registry).unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    for (seconds, offset, count, valid) in [
        (0.0, -9, 0, true),
        (30.0, 10, 4, true),
        (31.0, 0, 1, false),
        (1.0, -10, 1, false),
        (1.0, 0, 5, false),
    ] {
        let value = ComponentValue::map([
            ("seconds", ComponentValue::f64(seconds).unwrap()),
            ("offset", ComponentValue::I64(offset)),
            ("count", ComponentValue::U64(count)),
        ]);
        assert_eq!(validate_component_value(&schema, &value).is_ok(), valid);
        assert_eq!(
            snapshot.preflight_component(&schema.id, &value).is_ok(),
            valid
        );
        if !valid {
            assert!(matches!(
                snapshot.preflight_component(&schema.id, &value),
                Err(ComponentCodecError::Domain(_))
            ));
        }
    }
}

#[test]
fn inventory_callbacks_validate_the_same_explicit_membership() {
    let schemas = inventory::schemas().unwrap();
    let initial = inventory::initial_catalog().unwrap();
    assert_eq!(schemas, initial.components);
    assert_eq!(initial.generation(), 1);
    let mut registry = ComponentRegistry::new();
    inventory::register(&mut registry).unwrap();
    registry.freeze().unwrap();
    assert_eq!(registry.catalog().unwrap().components, schemas);
    assert!(duplicate_inventory::schemas().is_err());
    assert!(duplicate_inventory::initial_catalog().is_err());
    assert_eq!(nested_inventory::types::schemas().unwrap(), schemas);
    assert_eq!(nested_inventory::types::initial_catalog().unwrap(), initial);
    let mut nested = ComponentRegistry::new();
    nested_inventory::types::register(&mut nested).unwrap();
    nested.freeze().unwrap();
    assert_eq!(nested.catalog().unwrap().components, schemas);
}
