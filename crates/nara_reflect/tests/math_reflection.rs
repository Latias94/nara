use std::any::TypeId;

use nara_core::{Mat3, Vec2, Vec3};
use nara_reflect::{ComponentRegistry, FromReflect, PartialReflect, Reflect};

#[derive(Reflect, Debug, PartialEq)]
struct SpatialConfiguration {
    position: Vec2,
    direction: Vec3,
    transform: Mat3,
}

#[test]
fn public_math_types_share_registration_and_dynamic_reflection() {
    let mut registry = ComponentRegistry::new();
    registry
        .register_reflected_type::<SpatialConfiguration>()
        .expect("public math configuration registers through the ordinary reflection API");
    registry.freeze().expect("reflection registry freezes");
    let types = registry.type_registry().expect("frozen type registry");
    for id in [
        TypeId::of::<SpatialConfiguration>(),
        TypeId::of::<Vec2>(),
        TypeId::of::<Vec3>(),
        TypeId::of::<Mat3>(),
    ] {
        assert!(
            types.get(id).is_some(),
            "configuration dependencies register"
        );
    }

    let configuration = SpatialConfiguration {
        position: Vec2::new(1.5, -2.0),
        direction: Vec3::new(0.0, 1.0, 0.5),
        transform: Mat3::from_translation(Vec2::new(3.0, 4.0)),
    };
    let dynamic = configuration
        .to_dynamic()
        .expect("public math fields support dynamic reflection");
    assert_eq!(
        SpatialConfiguration::from_reflect(dynamic.as_ref()),
        Some(configuration)
    );
}
