use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{
    Asset, AssetError, AssetLoadSet, AssetServer, AssetStates, Assets, Handle, LoadState,
};
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
    material::{
        AlphaMode2d, Material2dBinding, SpriteMaterialBindingAsset, register_sprite_material_loader,
    },
    sprite::{NineSlice, Sprite, SpriteScaleMode},
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
        let authored = authored_sprite(source);
        merge_source_field(
            sprite.reborrow().map_unchanged(|sprite| &mut sprite.size),
            &mut self.applied.size,
            authored.size,
        );
        merge_source_field(
            sprite.reborrow().map_unchanged(|sprite| &mut sprite.layer),
            &mut self.applied.layer,
            authored.layer,
        );
        merge_source_field(
            sprite.reborrow().map_unchanged(|sprite| &mut sprite.flip_x),
            &mut self.applied.flip_x,
            authored.flip_x,
        );
        merge_source_field(
            sprite.reborrow().map_unchanged(|sprite| &mut sprite.flip_y),
            &mut self.applied.flip_y,
            authored.flip_y,
        );
        merge_source_field(
            sprite
                .reborrow()
                .map_unchanged(|sprite| &mut sprite.scale_mode),
            &mut self.applied.scale_mode,
            authored.scale_mode,
        );
        let mut material = sprite.map_unchanged(|sprite| &mut sprite.material);
        merge_source_field(
            material
                .reborrow()
                .map_unchanged(|material| &mut material.tint),
            &mut self.applied.material.tint,
            authored.material.tint,
        );
        merge_source_field(
            material
                .reborrow()
                .map_unchanged(|material| &mut material.alpha_mode),
            &mut self.applied.material.alpha_mode,
            authored.material.alpha_mode,
        );
        merge_source_field(
            material.map_unchanged(|material| &mut material.image),
            &mut self.applied.material.image,
            image,
        );
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

fn merge_source_field<T: Copy + PartialEq>(mut current: Mut<'_, T>, applied: &mut T, next: T) {
    if *current == *applied && current.set_if_neq(next) {
        *applied = next;
    }
}

fn authored_sprite(source: &SpriteSource) -> Sprite {
    let scale_mode = source.slice.map_or(SpriteScaleMode::Stretch, |slice| {
        SpriteScaleMode::Sliced(NineSlice::new(slice.x, slice.y, slice.z, slice.w))
    });
    let alpha_mode = if source.additive {
        AlphaMode2d::Additive
    } else {
        AlphaMode2d::Blend
    };
    Sprite::from_color(source.size, source.tint)
        .with_layer(source.layer)
        .with_flip(source.flip_x, source.flip_y)
        .with_scale_mode(scale_mode)
        .with_alpha_mode(alpha_mode)
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
    states: Res<AssetStates>,
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
            &states,
            &images,
            &mut commands,
        )
        .map(|(handle, _)| handle);
        let material = ready_asset(
            &mut hydrated.material_request,
            source.material,
            hydrated.binding.as_ref().map(|(handle, _)| *handle),
            &mut server,
            &states,
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
    states: &AssetStates,
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
    let handle = if cached.is_none_or(|request| {
        request.source_generation != source_generation
            || request.handle.is_some_and(|handle| {
                states
                    .state(handle.id())
                    .is_some_and(|state| state.load_state() == &LoadState::Removed)
            })
    }) {
        // Keep rejected requests until the reference or mount changes. Removed assets can reload.
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
        AssetAppExt, AssetEvents, AssetLoadError, AssetPlugin, AssetProductRef, ImportedProductId,
        NpakEntryKind, RuntimeAssetBackend, RuntimeAssetMode, RuntimeAssetSource, load_asset_sync,
        source::{MemoryAssetSource, asset_id_path},
    };
    use nara_core::Vec4;
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
    fn sprite_source_applies_visuals_without_clobbering_runtime_overrides() {
        let mut app = App::new();
        app.add_plugins((AssetPlugin, ImagePlugin, SpriteSourceHydrationPlugin))
            .unwrap();
        let source = SpriteSource {
            flip_x: true,
            slice: Some(Vec4::new(1.0, 2.0, -3.0, f32::NAN)),
            additive: true,
            ..SpriteSource::default()
        };
        let world = app.world_mut().unwrap();
        let managed = world.spawn(source.clone()).id();
        let overridden = world.spawn(source).id();
        app.update_with_delta(Duration::ZERO).unwrap();

        let sprite = app.world().get::<Sprite>(managed).unwrap();
        assert!(sprite.flip_x);
        assert!(!sprite.flip_y);
        assert_eq!(sprite.material.alpha_mode, AlphaMode2d::Additive);
        assert_eq!(
            sprite.scale_mode,
            SpriteScaleMode::Sliced(NineSlice::new(1.0, 2.0, 0.0, 0.0))
        );

        let world = app.world_mut().unwrap();
        let override_mode = SpriteScaleMode::Sliced(NineSlice::new(4.0, 3.0, 2.0, 1.0));
        let mut sprite = world.get_mut::<Sprite>(overridden).unwrap();
        sprite.flip_x = false;
        sprite.scale_mode = override_mode;
        sprite.material.alpha_mode = AlphaMode2d::Opaque;
        world.entity_mut(managed).insert(SpriteSource {
            flip_y: true,
            ..SpriteSource::default()
        });
        world.entity_mut(overridden).insert(SpriteSource {
            flip_x: true,
            flip_y: true,
            ..SpriteSource::default()
        });
        app.update_with_delta(Duration::ZERO).unwrap();

        let sprite = app.world().get::<Sprite>(managed).unwrap();
        assert!(!sprite.flip_x);
        assert!(sprite.flip_y);
        assert_eq!(sprite.scale_mode, SpriteScaleMode::Stretch);
        assert_eq!(sprite.material.alpha_mode, AlphaMode2d::Blend);
        let sprite = app.world().get::<Sprite>(overridden).unwrap();
        assert!(!sprite.flip_x);
        assert!(sprite.flip_y);
        assert_eq!(sprite.scale_mode, override_mode);
        assert_eq!(sprite.material.alpha_mode, AlphaMode2d::Opaque);

        let world = app.world_mut().unwrap();
        world.entity_mut(managed).remove::<SpriteSource>();
        world.entity_mut(overridden).remove::<SpriteSource>();
        app.update_with_delta(Duration::ZERO).unwrap();
        assert!(app.world().get::<Sprite>(managed).is_none());
        assert!(app.world().get::<Sprite>(overridden).is_some());
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

        app.world_mut()
            .unwrap()
            .resource_scope(|world, mut images: Mut<Assets<ImageAsset>>| {
                world.resource_scope(|world, mut states: Mut<AssetStates>| {
                    images
                        .remove_with_state(
                            handle,
                            &mut states,
                            &mut world.resource_mut::<AssetEvents>(),
                        )
                        .unwrap();
                });
            });
        let deadline = Instant::now() + Duration::from_secs(5);
        while app
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
        {
            assert!(
                Instant::now() < deadline,
                "the retired image did not reload"
            );
            app.update_with_delta(Duration::ZERO).unwrap();
            std::thread::yield_now();
        }
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().material.image,
            Some(handle)
        );
    }
}
