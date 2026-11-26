#![cfg(feature = "authoring")]

use bevy_ecs::component::Component;
use nara_core::{Color, Vec2};
use nara_reflect::{ComponentTypeId, ComponentValue, FieldPath, Schema};
use nara_scene::authoring::{SceneDocument, SceneEntity};

#[derive(Component, Schema)]
#[schema(id = "game.FloatDefaults", version = 1, default)]
struct FloatDefaults {
    gain: f32,
    offset: Vec2,
    tint: Color,
}

impl Default for FloatDefaults {
    fn default() -> Self {
        Self {
            gain: 0.1,
            offset: Vec2::new(0.1, 0.2),
            tint: Color::rgba(0.1, 0.2, 0.3, 0.4),
        }
    }
}

#[test]
fn f64_edits_at_the_same_f32_defaults_elide_on_the_first_save() {
    let schemas = [FloatDefaults::type_schema().unwrap()];
    let mut document = SceneDocument::new("01234567-89ab-cdef-0123-456789abcdef".parse().unwrap());
    let mut row = SceneEntity::new(
        "c62cea0c-0539-4cef-93d5-71c8ff7ee454".parse().unwrap(),
        "Float",
    );
    row.set(&FloatDefaults::default()).unwrap();
    let payload = row
        .components
        .get_mut(&ComponentTypeId::new(FloatDefaults::COMPONENT_ID))
        .unwrap()
        .value_mut()
        .unwrap();
    for (path, rounded) in [("gain", 0.1), ("offset.x", 0.1), ("tint.b", 0.3)] {
        let path = path.parse::<FieldPath>().unwrap();
        let draft = ComponentValue::f64(rounded).unwrap();
        assert_ne!(payload.read_path(&path).unwrap(), draft);
        payload.replace_path(&path, draft).unwrap();
    }
    document.entities.push(row);
    let first = document.to_ron_with(&schemas).unwrap();
    assert!(first.contains(r#""game.FloatDefaults": ()"#), "{first}");
    let reloaded = SceneDocument::parse_with(&first, &schemas).unwrap();
    assert_eq!(reloaded.to_ron_with(&schemas).unwrap(), first);
}
