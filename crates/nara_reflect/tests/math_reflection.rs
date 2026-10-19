use std::any::TypeId;

use nara_core::{Mat3, Vec2, Vec3};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentFieldPath, ComponentNumericDomain, ComponentRegistry, ComponentRegistryError,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, FromReflect, PartialReflect,
    PersistentComponent, PersistentComponentProvider, Reflect, validate_component_value,
};

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
    let encoded = expected.__encode_persistent_component().unwrap();
    assert_eq!(
        encoded.field("position").unwrap(),
        &ComponentValue::map([
            ("x", ComponentValue::f64(1.5).unwrap()),
            ("y", ComponentValue::f64(-2.0).unwrap()),
        ])
    );
    validate_component_value(&schema, &encoded).unwrap();
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<MotionPath>()
        .unwrap();
    registry.freeze().unwrap();
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    registry
        .preflight_component(&ComponentTypeId::new("math.MotionPath"), &encoded)
        .unwrap()
        .unwrap()
        .apply(&mut world, entity)
        .unwrap();
    assert_eq!(world.get::<MotionPath>(entity), Some(&expected));
}

#[test]
fn persistent_vec2_and_vector_items_reject_missing_extra_and_out_of_domain_members() {
    let schema = MotionPath::persistent_component_schema();
    let original = motion_path().__encode_persistent_component().unwrap();
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
            assert!(MotionPath::__decode_persistent_component(&candidate).is_err());
        }
    }
}

#[test]
fn adopting_closed_vectors_requires_both_an_owner_successor_and_explicit_migration() {
    let id = ComponentTypeId::new("math.MotionPath");
    let mut old_schema = MotionPath::persistent_component_schema();
    for field in &mut old_schema.fields {
        match field.id().as_str() {
            "position" => field.shape = ComponentValueShape::opaque(ComponentValueKind::Map),
            "waypoints" => {
                field.shape =
                    ComponentValueShape::list(ComponentValueShape::opaque(ComponentValueKind::Map))
            }
            _ => {}
        }
    }
    let previous = ComponentSchemaCatalog {
        components: vec![old_schema],
        ..Default::default()
    };
    let mut current = ComponentSchemaCatalog::successor_of(&previous).unwrap();
    current.components = vec![MotionPath::persistent_component_schema()];
    let owner = ComponentSchemaOwnerId::new("math.motion");
    assert!(matches!(
        ComponentRegistry::from_owner_catalog_candidate(owner, current.clone(), Some(previous.clone())),
        Err(ComponentRegistryError::ComponentSchemaChangedWithoutVersionBump { component_id }) if component_id == id
    ));
    current.components[0].version = ComponentSchemaVersion::new(2).unwrap();
    let mut registry =
        ComponentRegistry::from_owner_catalog_candidate(owner, current, Some(previous)).unwrap();
    registry
        .register_native_component_with_codec::<MotionPath, _, _>(
            &id,
            MotionPath::__decode_persistent_component,
            MotionPath::__encode_persistent_component,
        )
        .unwrap();
    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::MissingComponentMigrationChain { .. })
    ));
    registry
        .register_component_migration(
            &id,
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).unwrap(),
            Ok,
        )
        .unwrap();
    registry.freeze().unwrap();
    let original = motion_path().__encode_persistent_component().unwrap();
    let migrated = registry
        .migrate_component_value(&id, ComponentSchemaVersion::ONE, &original)
        .unwrap();
    assert_eq!(migrated.version, ComponentSchemaVersion::new(2).unwrap());
    assert_eq!(migrated.value, original);
}
