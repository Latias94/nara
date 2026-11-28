use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{Asset, AssetError, AssetServer, Assets, Handle};
use nara_core::AssetId;
use nara_ecs::{
    Commands, Component, DetectChangesMut, Entity, EntityCommands, EntityWorldMut, Mut, Or, Query,
    Res, ResMut, With, bevy_ecs::error::BevyError,
};
use nara_scene::components::SpriteSource;

use crate::{
    image::{IMAGE_PLUGIN_ID, ImageAsset},
    material::{Material2dBinding, SpriteMaterialBindingAsset, register_sprite_material_loader},
    sprite::Sprite,
};

/// Hydrates authored sprites in `PreUpdate`, including in apps without a renderer.
///
/// The plugin seeds missing sprites from `SpriteSource`. Fields it manages follow the source
/// while their current values match the last applied values. Texture regions, sampling, anchors,
/// and sort settings belong to the game. Pending or failed asset replacements keep the last
/// resident value. Removing the source detaches its managed assets and any untouched sprite
/// created by this plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteSourceHydrationPlugin;

pub const SPRITE_SOURCE_HYDRATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.sprite-source-hydration");
pub const SPRITE_SOURCE_HYDRATION_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SPRITE_SOURCE_HYDRATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[IMAGE_PLUGIN_ID]);

impl Plugin for SpriteSourceHydrationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SPRITE_SOURCE_HYDRATION_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        register_sprite_material_loader(app)?;
        app.add_systems(CoreStage::PreUpdate, hydrate_sprite_sources)?;
        Ok(())
    }
}

#[derive(Component)]
struct HydratedSprite {
    applied: Sprite,
    created_sprite: bool,
    image_request: Option<(AssetId, Handle<ImageAsset>)>,
    material_request: Option<(AssetId, Handle<SpriteMaterialBindingAsset>)>,
    binding: Option<(Handle<SpriteMaterialBindingAsset>, Material2dBinding)>,
}

impl HydratedSprite {
    fn new(source: &SpriteSource) -> Self {
        Self {
            applied: authored_sprite(source),
            created_sprite: false,
            image_request: None,
            material_request: None,
            binding: None,
        }
    }

    fn apply_sprite(
        &mut self,
        mut sprite: Mut<'_, Sprite>,
        source: &SpriteSource,
        image: Option<Handle<ImageAsset>>,
    ) {
        if sprite.size == self.applied.size && sprite.size != source.size {
            sprite.size = source.size;
            self.applied.size = source.size;
        }
        if sprite.layer == self.applied.layer && sprite.layer != source.layer {
            sprite.layer = source.layer;
            self.applied.layer = source.layer;
        }
        if sprite.material.tint == self.applied.material.tint && sprite.material.tint != source.tint
        {
            sprite.material.tint = source.tint;
            self.applied.material.tint = source.tint;
        }
        if sprite.material.image == self.applied.material.image && sprite.material.image != image {
            sprite.material.image = image;
            self.applied.material.image = image;
        }
    }

    fn apply_binding(
        &mut self,
        current: Option<Mut<'_, Material2dBinding>>,
        ready: Option<(
            Handle<SpriteMaterialBindingAsset>,
            &SpriteMaterialBindingAsset,
        )>,
        commands: &mut EntityCommands<'_>,
    ) {
        if current.as_deref() != self.binding.as_ref().map(|(_, binding)| binding) {
            return;
        }

        if let Some((handle, SpriteMaterialBindingAsset(binding))) = ready {
            if self
                .binding
                .as_ref()
                .is_some_and(|(previous, applied)| *previous == handle && applied == binding)
            {
                return;
            }
            if let Some(mut current) = current {
                current.set_if_neq(binding.clone());
            } else {
                commands.try_insert_if_new(binding.clone());
            }
            self.binding = Some((handle, binding.clone()));
        } else if let Some((_, binding)) = self.binding.take() {
            remove_if_unchanged(commands, binding);
        }
    }
}

fn authored_sprite(source: &SpriteSource) -> Sprite {
    Sprite::from_color(source.size, source.tint).with_layer(source.layer)
}

type SourceSprites<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static SpriteSource>,
        Option<&'static mut Sprite>,
        Option<&'static mut Material2dBinding>,
        Option<&'static mut HydratedSprite>,
    ),
    Or<(With<SpriteSource>, With<HydratedSprite>)>,
>;

fn hydrate_sprite_sources(
    mut commands: Commands,
    mut sources: SourceSprites,
    mut server: ResMut<AssetServer>,
    images: Res<Assets<ImageAsset>>,
    materials: Res<Assets<SpriteMaterialBindingAsset>>,
) -> Result<(), BevyError> {
    for (entity, source, sprite, binding, mut tracked) in &mut sources {
        let mut commands = commands.entity(entity);
        let Some(source) = source else {
            if let Some(mut tracked) = tracked {
                let overridden_binding =
                    binding.as_deref() != tracked.binding.as_ref().map(|(_, value)| value);
                tracked.apply_binding(binding, None, &mut commands);
                if let Some(mut sprite) = sprite {
                    if tracked.created_sprite && !overridden_binding && *sprite == tracked.applied {
                        remove_if_unchanged(&mut commands, tracked.applied.clone());
                    } else if sprite.material.image.is_some()
                        && sprite.material.image == tracked.applied.material.image
                    {
                        sprite.material.image = None;
                    }
                }
                commands.try_remove::<HydratedSprite>();
            }
            continue;
        };

        let mut new = None;
        let hydrated = match tracked.as_deref_mut() {
            Some(tracked) => tracked,
            None => new.insert(HydratedSprite::new(source)),
        };
        let image = ready_asset(
            &mut hydrated.image_request,
            source.image,
            hydrated.applied.material.image,
            &mut server,
            &images,
        )
        .map_err(BevyError::error)?
        .map(|(handle, _)| handle);
        let material = ready_asset(
            &mut hydrated.material_request,
            source.material,
            hydrated.binding.as_ref().map(|(handle, _)| *handle),
            &mut server,
            &materials,
        )
        .map_err(BevyError::error)?;

        if let Some(sprite) = sprite {
            hydrated.apply_sprite(sprite, source, image);
        } else {
            hydrated.created_sprite = true;
            hydrated.applied = authored_sprite(source);
            hydrated.applied.material.image = image;
            commands.try_insert_if_new(hydrated.applied.clone());
        }
        hydrated.apply_binding(binding, material, &mut commands);
        if let Some(new) = new {
            commands.try_insert_if_new(new);
        }
    }
    Ok(())
}

fn ready_asset<'a, T: Asset>(
    request: &mut Option<(AssetId, Handle<T>)>,
    id: Option<AssetId>,
    previous: Option<Handle<T>>,
    server: &mut ResMut<'_, AssetServer>,
    assets: &'a Assets<T>,
) -> Result<Option<(Handle<T>, &'a T)>, AssetError> {
    let Some(id) = id else {
        *request = None;
        return Ok(None);
    };
    if request.is_none_or(|(requested, _)| requested != id) {
        *request = match server.load_id(id) {
            Ok(handle) => Some((id, handle)),
            Err(AssetError::MissingRuntimeSource) => None,
            Err(error) => return Err(error),
        };
    }

    Ok(request
        .map(|(_, handle)| handle)
        .into_iter()
        .chain(previous)
        .find_map(|handle| assets.get(handle).map(|asset| (handle, asset))))
}

fn remove_if_unchanged<T: Component + PartialEq>(commands: &mut EntityCommands<'_>, applied: T) {
    commands.queue_silenced(move |mut entity: EntityWorldMut| {
        if entity.get::<T>() == Some(&applied) {
            entity.remove::<T>();
        }
    });
}
