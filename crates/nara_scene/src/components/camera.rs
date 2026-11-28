use bevy_ecs::component::Component;
use nara_core::Color;
use nara_reflect::Schema;

use crate::spatial::Transform2d;

/// A scene camera projected into the primary window by the renderer.
#[derive(Component, Schema, Debug, Clone, Copy, PartialEq)]
#[require(Transform2d)]
#[schema(
    crate = "nara_reflect",
    id = "nara.Camera2dSource",
    version = 1,
    default,
    alias = "Camera 2D",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Camera2dSource {
    /// Vertical size of the orthographic projection in camera units.
    #[schema(alias = "Viewport Height", range(min_exclusive = 0.0))]
    pub viewport_height: f32,
    /// Optional linear RGBA clear for the target.
    #[schema(alias = "Clear Color")]
    pub clear_color: Option<Color>,
    #[schema(alias = "Order")]
    pub order: i32,
    /// Controls whether the camera contributes a view.
    #[schema(alias = "Active")]
    pub active: bool,
}

impl Default for Camera2dSource {
    fn default() -> Self {
        Self {
            viewport_height: 720.0,
            clear_color: None,
            order: 0,
            active: true,
        }
    }
}

#[cfg(all(test, feature = "authoring"))]
mod tests {
    use super::*;
    use crate::authoring::SceneDocument;

    const SCENE: &str = r#"
Scene(
    version: 1,
    id: "681a78dc-b6ab-4f3a-8eb6-4742d2ced237",
    entities: [
        (
            id: "4f99b200-2a5e-4872-8d3d-000000000001",
            name: "Default Camera",
            components: {"nara.Camera2dSource": ()},
        ),
        (
            id: "4f99b200-2a5e-4872-8d3d-000000000002",
            name: "Alternate Camera",
            components: {
                "nara.Camera2dSource": (
                    viewport_height: 180.0,
                    clear_color: (0.25, 0.5, 0.75, 1.0),
                    order: -2,
                    active: false,
                ),
            },
        ),
    ],
)"#;

    #[test]
    fn camera_defaults_and_linear_clear_round_trip_through_ron() {
        let document = SceneDocument::parse(SCENE).unwrap();
        assert_eq!(
            document.entities[0].typed::<Camera2dSource>().unwrap(),
            Some(Camera2dSource::default())
        );
        assert_eq!(
            document.entities[1].typed::<Camera2dSource>().unwrap(),
            Some(Camera2dSource {
                viewport_height: 180.0,
                clear_color: Some(Color::rgba(0.25, 0.5, 0.75, 1.0)),
                order: -2,
                active: false,
            })
        );
        let compact = document.to_ron().unwrap();
        assert!(compact.contains(r#""nara.Camera2dSource": ()"#));
        assert_eq!(SceneDocument::parse(&compact).unwrap(), document);
        assert!(SceneDocument::parse(&SCENE.replace("180.0", "0.0")).is_err());
    }

    #[cfg(feature = "cook")]
    #[test]
    fn cooked_cameras_use_builtin_native_registration_and_require_a_transform() {
        use bevy_ecs::world::World;
        use nara_reflect::ComponentSchemaCatalog;

        use crate::{
            cook::cook_flattened_scene,
            cooked::{CookedRegistry, CookedScene},
        };

        let document = SceneDocument::parse(SCENE).unwrap();
        let schemas = ComponentSchemaCatalog::from_components([]).unwrap();
        let output = cook_flattened_scene(&document, &schemas).unwrap();
        let mut registry = CookedRegistry::new();
        registry.register_builtins().unwrap();
        let scene = CookedScene::parse(&output.bytes).unwrap();
        let mut world = World::new();
        let cameras = scene.prepare(&registry).unwrap().spawn(&mut world, || ());
        for row in &document.entities {
            let entity = cameras[&row.id];
            assert_eq!(
                world.get::<Camera2dSource>(entity).copied(),
                row.typed().unwrap()
            );
            assert_eq!(
                world.get::<Transform2d>(entity),
                Some(&Transform2d::IDENTITY)
            );
        }
    }
}
