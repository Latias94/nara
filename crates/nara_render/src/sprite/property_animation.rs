use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_ecs::{Entity, World};
use nara_scene::animation::{
    KeyframeValue, PropertyAnimationError, PropertyAppliers, PropertyWrite, SPRITE_COMPONENT_ID,
};

use super::Sprite;

const ID: PluginId = PluginId::new("nara.sprite-property-animation");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime);

/// Registers typed `nara.Sprite` properties for the headless animation player and tweens.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpritePropertyAnimationPlugin;

impl Plugin for SpritePropertyAnimationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        use nara_scene::animation::ApplyProperty;
        let write: ApplyProperty = |world: &mut World, entity: Entity, path, value| {
            let Some(field @ ("size" | "tint")) = path.top_level_field() else {
                return Ok(PropertyWrite::Unhandled);
            };
            let Some(mut sprite) = world.get_mut::<Sprite>(entity) else {
                if world
                    .get::<nara_scene::components::SpriteSource>(entity)
                    .is_some()
                {
                    return Ok(PropertyWrite::Waiting);
                }
                return Err(PropertyAnimationError::MissingComponent(
                    SPRITE_COMPONENT_ID,
                ));
            };
            match (field, value) {
                ("size", KeyframeValue::Vec2(dimensions)) => {
                    if sprite.size != *dimensions {
                        sprite.size = *dimensions;
                    }
                }
                ("tint", KeyframeValue::Color(color)) => {
                    if sprite.material.tint != *color {
                        sprite.material.tint = *color;
                    }
                }
                _ => return Err(PropertyAnimationError::WrongValue(path.clone())),
            }
            Ok(PropertyWrite::Applied)
        };
        app.init_resource::<PropertyAppliers>()?;
        app.world_mut()?
            .resource_mut::<PropertyAppliers>()
            .register(SPRITE_COMPONENT_ID, write);
        Ok(())
    }
}
