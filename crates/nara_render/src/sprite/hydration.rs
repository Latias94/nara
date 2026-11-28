use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{Asset, AssetError, AssetLoadSet, AssetServer, Assets, Handle};
use nara_core::AssetId;
use nara_ecs::{
    Commands, Component, DetectChangesMut, Entity, EntityCommands, EntityWorldMut, Mut, Or, Query,
    Res, ResMut, With, World,
    bevy_ecs::error::{BevyError, warn},
    schedule::{IntoScheduleConfigs, SystemSet},
};
use nara_scene::{
    SceneUpdateSet,
    components::{SpriteSource, Transform2d},
};

use crate::{
    image::{IMAGE_PLUGIN_ID, ImageAsset},
    material::{Material2dBinding, SpriteMaterialBindingAsset, register_sprite_material_loader},
    sprite::Sprite,
};

/// Hydrates authored sprites in `TaskUpdate`, including in apps without a renderer.
///
/// The plugin seeds missing sprites from `SpriteSource`. Fields it manages follow the source
/// while their current values match the last applied values. Texture regions, sampling, anchors,
/// and sort settings belong to the game. Pending or failed asset replacements keep the last
/// resident value. Removing the source detaches its managed assets and any untouched sprite
/// created by this plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteSourceHydrationPlugin;

/// Applies authored sprite values after asset publication and scene switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct SpriteSourceHydrationSet;

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
        app.add_systems(
            CoreStage::TaskUpdate,
            hydrate_sprite_sources
                .in_set(SpriteSourceHydrationSet)
                .after(AssetLoadSet::Publish)
                .after(SceneUpdateSet),
        )?;
        Ok(())
    }
}

#[derive(Component)]
struct HydratedSprite {
    applied: Sprite,
    created_sprite: bool,
    image_request: Option<AssetRequest<ImageAsset>>,
    material_request: Option<AssetRequest<SpriteMaterialBindingAsset>>,
    binding: Option<(Handle<SpriteMaterialBindingAsset>, Material2dBinding)>,
}

struct AssetRequest<T> {
    id: AssetId,
    handle: Option<Handle<T>>,
    source_generation: u64,
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
) {
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
            &mut commands,
        )
        .map(|(handle, _)| handle);
        let material = ready_asset(
            &mut hydrated.material_request,
            source.material,
            hydrated.binding.as_ref().map(|(handle, _)| *handle),
            &mut server,
            &materials,
            &mut commands,
        );

        if let Some(sprite) = sprite {
            hydrated.apply_sprite(sprite, source, image);
        } else {
            hydrated.created_sprite = true;
            hydrated.applied = authored_sprite(source);
            hydrated.applied.material.image = image;
            commands.try_insert_if_new((hydrated.applied.clone(), Transform2d::default()));
        }
        hydrated.apply_binding(binding, material, &mut commands);
        if let Some(new) = new {
            commands.try_insert_if_new(new);
        }
    }
}

fn ready_asset<'a, T: Asset>(
    request: &mut Option<AssetRequest<T>>,
    id: Option<AssetId>,
    previous: Option<Handle<T>>,
    server: &mut AssetServer,
    assets: &'a Assets<T>,
    commands: &mut EntityCommands<'_>,
) -> Option<(Handle<T>, &'a T)> {
    let Some(id) = id else {
        *request = None;
        return None;
    };
    let source_generation = server.runtime_source_generation();
    let mut cached = request.as_mut().filter(|request| request.id == id);
    if let Some(cached) = &mut cached
        && let Some(handle) = cached.handle
        && let Some(asset) = assets.get(handle)
    {
        cached.source_generation = source_generation;
        return Some((handle, asset));
    }
    let handle = if cached.is_none_or(|request| request.source_generation != source_generation) {
        // Keep rejected requests until the reference or mount changes.
        let handle = match server.load_id(id) {
            Ok(handle) => Some(handle),
            Err(AssetError::MissingRuntimeSource) => None,
            Err(error) => {
                let entity = commands.id();
                commands.commands().queue_handled(
                    move |_: &mut World| -> Result<(), BevyError> {
                        Err(BevyError::warning(format!(
                            "sprite source on {entity} could not request asset '{id}': {error}"
                        )))
                    },
                    warn,
                );
                None
            }
        };
        *request = Some(AssetRequest {
            id,
            handle,
            source_generation,
        });
        handle
    } else {
        None
    };

    handle
        .into_iter()
        .chain(previous)
        .find_map(|handle| assets.get(handle).map(|asset| (handle, asset)))
}

fn remove_if_unchanged<T: Component + PartialEq>(commands: &mut EntityCommands<'_>, applied: T) {
    commands.queue_silenced(move |mut entity: EntityWorldMut| {
        if entity.get::<T>() == Some(&applied) {
            entity.remove::<T>();
        }
    });
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use nara_asset::{
        AssetAppExt, AssetLoadError, AssetPlugin, AssetProductRef, ImportedProductId,
        NpakEntryKind, RuntimeAssetBackend, RuntimeAssetMode, RuntimeAssetSource, load_asset_sync,
        source::{MemoryAssetSource, asset_id_path},
    };
    use nara_ecs::bevy_ecs::entity_disabling::Disabled;

    use super::*;
    use crate::image::{ImageColorSpace, ImageExtent, ImageFormat, ImagePlugin};

    struct UnknownImageKind(Arc<AtomicUsize>);

    impl RuntimeAssetBackend for UnknownImageKind {
        fn mode(&self) -> RuntimeAssetMode {
            RuntimeAssetMode::Cooked
        }

        fn read_asset(
            &self,
            _: AssetId,
            _: Option<NpakEntryKind>,
        ) -> Result<Vec<u8>, AssetLoadError> {
            panic!("a rejected request must not read asset bytes");
        }

        fn validate_kind(&self, id: AssetId, _: NpakEntryKind) -> Result<(), AssetError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(AssetError::UnknownRuntimeKind(id))
        }
    }

    #[test]
    fn sprite_image_recovers_from_request_and_decode_failures_after_remounts() {
        let mut app = App::new();
        app.add_plugins((AssetPlugin, ImagePlugin, SpriteSourceHydrationPlugin))
            .unwrap();
        let kind_checks = Arc::new(AtomicUsize::new(0));
        app.set_asset_source(RuntimeAssetSource::new(UnknownImageKind(
            kind_checks.clone(),
        )))
        .unwrap();
        let id = "0193a100-1111-7000-8000-000000000001".parse().unwrap();
        let entity = app
            .world_mut()
            .unwrap()
            .spawn(SpriteSource {
                image: Some(id),
                ..Default::default()
            })
            .id();
        let unaffected = app.world_mut().unwrap().spawn(SpriteSource::default()).id();

        for _ in 0..3 {
            app.update_with_delta(Duration::ZERO).unwrap();
        }
        assert_eq!(kind_checks.load(Ordering::Relaxed), 1);
        assert!(app.world().get::<Sprite>(unaffected).is_some());
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().material.image,
            None
        );

        let mut files = MemoryAssetSource::new();
        let path = asset_id_path(id);
        files.insert(&path, b"broken image").unwrap();
        app.world_mut().unwrap().entity_mut(entity).insert(Disabled);
        app.set_asset_source(RuntimeAssetSource::from_memory(
            RuntimeAssetMode::Cooked,
            files,
        ))
        .unwrap();
        app.update_with_delta(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .entity_mut(entity)
            .remove::<Disabled>();
        app.update_with_delta(Duration::ZERO).unwrap();
        let reference = AssetProductRef::new(id.into(), ImportedProductId::PRIMARY).unwrap();
        let handle = app
            .world()
            .resource::<AssetServer>()
            .get_product_handle::<ImageAsset>(reference)
            .unwrap();
        assert!(load_asset_sync::<ImageAsset>(app.world_mut().unwrap(), id).is_err());

        let image = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            vec![42, 0, 0, 255],
        )
        .unwrap();
        let mut files = MemoryAssetSource::new();
        files.insert(&path, image.encode_cooked().unwrap()).unwrap();
        app.set_asset_source(RuntimeAssetSource::from_memory(
            RuntimeAssetMode::Cooked,
            files,
        ))
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<Sprite>(entity).unwrap().material.image != Some(handle) {
            assert!(
                Instant::now() < deadline,
                "the remounted image did not hydrate"
            );
            app.update_with_delta(Duration::ZERO).unwrap();
            std::thread::yield_now();
        }
        assert_eq!(
            app.world().resource::<Assets<ImageAsset>>().get(handle),
            Some(&image)
        );
    }
}
