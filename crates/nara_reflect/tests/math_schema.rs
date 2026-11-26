use nara_core::Vec2;
use nara_ecs::Component;
use nara_reflect::{
    ComponentFieldPath, ComponentNumericDomain, ComponentValue, ComponentValueKind,
    ComponentValueShape, PersistentComponent, PersistentComponentProvider,
    validate_component_value,
};

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "math.MotionPath",
    version = 1,
    alias = "Motion path",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct MotionPath {
    #[nara(id = "position", alias = "Position")]
    position: Vec2,
    #[nara(id = "speed", alias = "Speed")]
    speed: f32,
    #[nara(id = "waypoints", alias = "Waypoints")]
    waypoints: Vec<Vec2>,
}

fn motion_path() -> MotionPath {
    MotionPath {
        position: Vec2::new(1.5, -2.0),
        speed: 3.0,
        waypoints: vec![Vec2::ZERO, Vec2::new(-4.0, 5.0)],
    }
}

#[test]
fn persistent_vec2_declares_one_closed_numeric_shape_and_keeps_xy_wire_encoding() {
    let schema = MotionPath::persistent_component_schema();
    let axis = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32);
    let vector =
        ComponentValueShape::record([("x".to_owned(), axis.clone()), ("y".to_owned(), axis)]);
    let shape = |id| {
        schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == id)
            .unwrap()
            .shape()
    };
    assert_eq!(shape("position"), &vector);
    assert_eq!(shape("waypoints"), &ComponentValueShape::list(vector));

    let expected = motion_path();
    let encoded = expected.encode_persistent_component().unwrap();
    assert_eq!(
        encoded.field("position").unwrap(),
        &ComponentValue::map([
            ("x", ComponentValue::f64(1.5).unwrap()),
            ("y", ComponentValue::f64(-2.0).unwrap()),
        ])
    );
    validate_component_value(&schema, &encoded).unwrap();
    assert_eq!(
        MotionPath::decode_persistent_component((&encoded).into()).unwrap(),
        expected
    );
}

#[test]
fn persistent_vec2_and_vector_items_reject_missing_extra_and_out_of_domain_members() {
    let schema = MotionPath::persistent_component_schema();
    let original = motion_path().encode_persistent_component().unwrap();
    for invalid in [
        ComponentValue::map([("x", ComponentValue::f64(1.0).unwrap())]),
        ComponentValue::map([
            ("x", ComponentValue::f64(1.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
            ("z", ComponentValue::f64(3.0).unwrap()),
        ]),
        ComponentValue::map([
            ("x", ComponentValue::f64(f64::MAX).unwrap()),
            ("y", ComponentValue::f64(0.0).unwrap()),
        ]),
        ComponentValue::map([
            ("x", ComponentValue::I64(1)),
            ("y", ComponentValue::f64(0.0).unwrap()),
        ]),
    ] {
        for field in ["position", "waypoints"] {
            let mut candidate = original.clone();
            let replacement = if field == "waypoints" {
                ComponentValue::List(vec![invalid.clone()])
            } else {
                invalid.clone()
            };
            candidate
                .replace_path(&ComponentFieldPath::from_fields([field]), replacement)
                .unwrap();
            assert!(validate_component_value(&schema, &candidate).is_err());
            assert!(MotionPath::decode_persistent_component((&candidate).into()).is_err());
        }
    }
}
