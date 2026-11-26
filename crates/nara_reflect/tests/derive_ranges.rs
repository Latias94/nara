use nara_ecs::Component;
use nara_reflect::{
    ComponentNumericBound as Bound, ComponentNumericInterval, ComponentSchemaCatalog,
    ComponentValue, PersistentComponent, PersistentComponentProvider, validate_component_value,
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
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let schema = &catalog.components()[0];
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
        assert_eq!(validate_component_value(schema, &value).is_ok(), valid);
    }
}

#[test]
fn initial_values_use_the_declared_scalar_types() {
    let schema = Cooldown::persistent_component_schema();
    let initial = schema.authoring_initial_value().unwrap();
    let value = Cooldown::decode_persistent_component(initial.into()).unwrap();
    assert_eq!(value.seconds, 0.5);
    assert_eq!(value.offset, -2);
    assert_eq!(value.count, 1);
}
