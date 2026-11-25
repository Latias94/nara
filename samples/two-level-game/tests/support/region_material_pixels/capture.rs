use std::collections::BTreeSet;

use nara::{
    app::{
        App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    },
    asset::{AssetServer, AssetStates, AssetVersion, Assets, Handle},
    core::Vec2,
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    image::{ImageAsset, image_resource_key},
    material::{Material2dAsset, Material2dBinding},
    render::{ExtractedViews, RenderFrame},
    render_wgpu::{
        WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuPolicyCaptureSet,
        WgpuRenderInput, WgpuStockPolicy, take_captured_policy_batch,
    },
    scene::{SceneEntitySource, fixed_spatial::presented_spatial_snapshot},
    sprite::Sprite,
    sprite_render::batching::{
        ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItems, SpriteBatchKind, SpriteBatches,
        SpriteInstance,
    },
    tilemap::{TileSet, TileSetAssets, TileSetContentReceipt, Tilemap},
    window::{
        Window, WindowResolution,
        backend::{BackendWindowHandles, WindowPresentationRequirement},
    },
};
use two_level_game::{GameSnapshot, region_material, region_tileset, side_room};

use crate::{gpu::EXTENT, support::Game};

#[derive(Default, Resource)]
struct Latest(Option<WgpuRenderInput>);

#[derive(Default)]
pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.region-material-pixels"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.spawn(Window::new(
            "Region Material CPU packet capture",
            WindowResolution::new(EXTENT.width, EXTENT.height),
        ));
        app.init_resource::<Latest>()?;
        app.add_systems(
            CoreStage::Render,
            retain.in_set(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

fn retain(world: &mut World) {
    let Some(batch) = take_captured_policy_batch(world) else {
        return;
    };
    world.resource_mut::<Latest>().0 = batch
        .into_render_input()
        .expect("the original ordinary stock Seal");
}

pub struct Capture {
    pub input: WgpuRenderInput,
    pub publication: Option<(Handle<Material2dAsset>, AssetVersion)>,
    pub binding: Option<Material2dBinding>,
    pub image: Option<(Handle<ImageAsset>, AssetVersion)>,
    pub floor: Option<Floor>,
}

pub struct Floor {
    pub publication: (Handle<TileSet>, AssetVersion),
    pub receipt: TileSetContentReceipt,
    pub tileset: TileSet,
}

pub fn sample(game: &mut Game, expected_sprites: usize, expected_tilemaps: usize) -> Capture {
    game.maintenance();
    game.session
        .observe(|world| {
            assert!(!world.contains_resource::<BackendWindowHandles>());
            assert!(!world.contains_resource::<WindowPresentationRequirement>());
            assert_eq!(
                world.resource::<GameSnapshot>().tick,
                world.resource::<FixedTime>().tick()
            );
            let input = world
                .resource::<Latest>()
                .0
                .clone()
                .expect("the original game packet");
            assert_eq!(
                input.packet().frame_index(),
                world.resource::<RenderFrame>().index
            );
            assert_eq!(
                input
                    .packet()
                    .source_generation()
                    .runtime_generation()
                    .map(|generation| generation.get()),
                game.session.runtime_generation(),
            );
            let stock = input
                .captured::<WgpuStockPolicy>()
                .expect("the original stock policy");
            let batches = stock.sprite_batches().expect("the original Sprite batches");
            assert_eq!(batches, world.resource::<SpriteBatches>());
            let floor = capture_floor(world, batches, expected_tilemaps);
            let server = world.resource::<AssetServer>();
            let states = world.resource::<AssetStates>();
            let materials = world.resource::<Assets<Material2dAsset>>();
            let images = world.resource::<Assets<ImageAsset>>();
            let resident = server
                .get_product_handle::<Material2dAsset>(region_material::material_product())
                .and_then(|handle| materials.get(handle).map(|asset| (handle, asset)));
            let publication = resident.map(|(handle, asset)| {
                assert_eq!(asset.product(), region_material::material_product());
                assert_eq!(
                    asset.generation().declaration(),
                    &region_material::declaration()
                );
                assert_eq!(
                    states.published_slot_revision(handle.id()),
                    Some(&materials.slot_revision(handle)),
                );
                let version = states.state(handle.id()).unwrap().usable_version().unwrap();
                assert!(version.raw() > 0);
                (handle, version)
            });
            let image_handle = server.get_product_handle::<ImageAsset>(side_room::image_product());
            let image = image_handle.and_then(|handle| {
                images.get(handle).map(|asset| {
                    assert_eq!(asset.product(), Some(side_room::image_product()));
                    (
                        handle,
                        states.state(handle.id()).unwrap().usable_version().unwrap(),
                    )
                })
            });
            let key = image_handle.map(image_resource_key);
            let mut binding = None;
            let mut sprites = 0;
            for batch in batches.as_slice() {
                if batch.kind != SpriteBatchKind::Sprite {
                    continue;
                }
                let Some(base) = batch
                    .image
                    .as_ref()
                    .filter(|base| Some(base.identity().key()) == key)
                else {
                    continue;
                };
                let (handle, asset) =
                    resident.expect("a genuine resident Material for every relic draw");
                assert_eq!(
                    base.resource().pixels(),
                    region_material::SOURCE_PIXEL.repeat(4)
                );
                assert_eq!(
                    Some(base.identity().asset_version()),
                    image.map(|(_, version)| version)
                );
                let extension = batch
                    .material_extension
                    .as_ref()
                    .expect("the original relic Material binding");
                assert_eq!(extension.binding.generation(), asset.generation());
                assert_eq!(
                    extension.binding.parameters(),
                    &region_material::parameters()
                );
                assert_eq!(Some(handle), publication.map(|(handle, _)| handle));
                assert_eq!(extension.textures.len(), 1);
                assert_eq!(extension.textures[0].0, 1);
                let palette = server
                    .get_product_handle::<ImageAsset>(region_material::palette_product())
                    .unwrap();
                assert_eq!(
                    extension.binding.textures()[region_material::PALETTE_ID],
                    palette
                );
                assert_eq!(
                    extension.textures[0].1.identity().key(),
                    image_resource_key(palette)
                );
                assert_eq!(
                    &extension.textures[0].1.resource().pixels()[..4],
                    &region_material::PALETTE_PIXEL
                );
                if let Some(previous) = &binding {
                    assert_eq!(previous, &extension.binding);
                } else {
                    binding = Some(extension.binding.clone());
                }
                sprites += batch
                    .instances
                    .iter()
                    .filter(|sprite| sprite.color.a > 0.0)
                    .count();
            }
            assert_eq!(sprites, expected_sprites);
            if expected_sprites == 0 {
                assert!(publication.is_none() && image.is_none());
                assert!(batches.as_slice().iter().all(|batch| {
                    batch.material_extension.as_ref().is_none_or(|extension| {
                        extension.binding.generation().declaration().id()
                            != region_material::MATERIAL_ID
                    })
                }));
            }
            Capture {
                input,
                publication,
                binding,
                image,
                floor,
            }
        })
        .unwrap()
}

fn capture_floor(world: &World, batches: &SpriteBatches, expected_maps: usize) -> Option<Floor> {
    let server = world.resource::<AssetServer>();
    let states = world.resource::<AssetStates>();
    let tilesets = world.resource::<Assets<TileSet>>();
    let owner = world.resource::<TileSetAssets>();
    let handle = server.get_product_handle::<TileSet>(region_tileset::tileset_product());
    if expected_maps == 0 {
        assert!(handle.is_none_or(|handle| tilesets.get(handle).is_none()));
        assert!(owner.is_empty());
        assert!(
            world
                .iter_entities()
                .all(|entity| !entity.contains::<Tilemap>())
        );
        assert!(
            batches
                .as_slice()
                .iter()
                .all(|batch| batch.kind != SpriteBatchKind::Tilemap)
        );
        return None;
    }
    let handle = handle.expect("the original region TileSet Product binding");
    let tileset = tilesets.get(handle).expect("the original resident TileSet");
    let receipt = owner
        .content_receipt(handle, tilesets, states)
        .expect("the original TileSet owner's current declaration receipt");
    assert_eq!(receipt.handle(), handle);
    let version = states.state(handle.id()).unwrap().usable_version().unwrap();
    assert_eq!(version.raw(), 1);
    assert_eq!(
        states.published_slot_revision(handle.id()),
        Some(&tilesets.slot_revision(handle))
    );
    let descriptor = region_tileset::descriptor();
    assert_eq!(tileset.atlas(), Some(descriptor.atlas()));
    assert_eq!(tileset.tile_size(), descriptor.atlas().tile_size);
    assert_eq!(tileset.material().sampler, descriptor.sampler());
    assert_eq!(tileset.material().alpha_mode, descriptor.alpha_mode());
    assert_eq!(tileset.material().tint, descriptor.tint());
    let content = tileset
        .image_content()
        .expect("the original TileSet Image snapshot");
    assert_eq!(content.product(), side_room::image_product());
    assert_eq!(tileset.material().image, Some(content.handle()));
    assert_eq!(content.pixels(), region_material::SOURCE_PIXEL.repeat(4));
    assert!(content.is_current(server, states, world.resource::<Assets<ImageAsset>>()));
    let prepared = content.prepared_snapshot();
    let extracted = world.resource::<ExtractedSprites>();
    let queued = world.resource::<QueuedSpriteItems>();
    let views = world.resource::<ExtractedViews>();
    let spatial = presented_spatial_snapshot(world).unwrap();
    let mut floors = BTreeSet::new();
    let mut extracted_cells = 0;
    for entity in world.iter_entities() {
        let Some(tilemap) = entity.get::<Tilemap>() else {
            continue;
        };
        let source = entity
            .get::<SceneEntitySource>()
            .expect("the original floor Scene identity");
        let (origin, expected) = match source.entity_id.as_str() {
            region_tileset::SIDE_ROOM_FLOOR => (
                region_tileset::SIDE_ROOM_ORIGIN,
                region_tileset::side_room_cells(),
            ),
            region_tileset::LOOKOUT_FLOOR => (
                region_tileset::LOOKOUT_ORIGIN,
                region_tileset::lookout_cells(),
            ),
            _ => panic!("unexpected authored floor identity: {source:?}"),
        };
        let local = spatial.local(entity.id()).unwrap();
        assert_eq!(local.translation, origin);
        assert_eq!(local.rotation, 0.0);
        assert_eq!(local.scale, Vec2::ONE);
        assert!(spatial.parent(entity.id()).unwrap().is_none());
        assert!(!entity.contains::<Sprite>());
        assert_eq!(tilemap.tileset, Some(handle));
        assert_eq!(tilemap.tile_size, region_tileset::TILE_SIZE);
        assert_eq!(tilemap.layer.index, region_tileset::FLOOR_LAYER);
        assert_eq!(tilemap.sort_key, 0);
        assert_eq!(tilemap.y_sort_anchor, None);
        assert_eq!(
            tilemap
                .cells()
                .map(|(coord, cell)| (coord, *cell))
                .collect::<Vec<_>>(),
            expected
        );
        floors.insert(entity.id());
        let matrix = spatial.global(entity.id()).unwrap().matrix();
        let cells = extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.entity == entity.id())
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), tilemap.cell_count());
        for sprite in cells {
            let ExtractedSpriteKind::TilemapCell { coord, tile } = sprite.kind else {
                panic!("a real Tilemap must retain TilemapCell extraction")
            };
            let cell = tilemap
                .get_cell(coord)
                .expect("the original live Tilemap cell");
            let region = tileset
                .normalized_region(tile)
                .expect("a valid original atlas region");
            let image = sprite
                .image_content
                .as_ref()
                .expect("the extracted atlas Image");
            assert_eq!(tile, cell.tile);
            assert_eq!(sprite.color, cell.color);
            assert_eq!(sprite.material.image, Some(content.handle()));
            assert_eq!(sprite.material.sampler, tileset.material().sampler);
            assert_eq!(sprite.material.alpha_mode, tileset.material().alpha_mode);
            assert_eq!(sprite.material.tint, tileset.material().tint);
            assert_eq!(sprite.layer, tilemap.layer.index);
            assert_eq!(sprite.sort_key, tilemap.sort_key);
            assert!(sprite.material_extension.is_none());
            assert_eq!(sprite.texture_region.min, region.min);
            assert_eq!(sprite.texture_region.size, region.size);
            assert_eq!(image.prepared_snapshot(), prepared);
            assert!(image.same_allocation(content));
            let local_center =
                (Vec2::new(coord.x as f32, coord.y as f32) + Vec2::splat(0.5)) * tilemap.tile_size;
            assert_eq!(sprite.world_center, matrix.transform_point2(local_center));
            assert_eq!(
                sprite.world_x_axis,
                matrix.transform_vector2(Vec2::new(tilemap.tile_size.x * 0.5, 0.0))
            );
            assert_eq!(
                sprite.world_y_axis,
                matrix.transform_vector2(Vec2::new(0.0, tilemap.tile_size.y * 0.5))
            );
            extracted_cells += 1;
        }
    }
    assert_eq!(floors.len(), expected_maps);
    assert_eq!(extracted_cells, expected_maps * 8);
    let mut drawn = BTreeSet::new();
    let mut drawn_sources = BTreeSet::new();
    let mut drawn_cells = 0;
    for batch in batches
        .as_slice()
        .iter()
        .filter(|batch| batch.kind == SpriteBatchKind::Tilemap)
    {
        assert_eq!(batch.image.as_ref(), Some(&prepared));
        assert!(batch.material_extension.is_none());
        let view = &views.as_slice()[batch.view_index];
        assert_eq!(view.target, batch.target);
        assert_eq!(view.order, batch.view_order);
        for instance in &batch.instances {
            let mut matches = queued.as_slice().iter().filter(|item| {
                item.kind == batch.kind
                    && item.view_index == batch.view_index
                    && item.view_order == batch.view_order
                    && item.target == batch.target
                    && item.layer == batch.layer
                    && item.sort_key == batch.sort_key
                    && item.material == batch.material
                    && item.image == batch.image
                    && item.instance == *instance
                    && item.material_extension == batch.material_extension
            });
            let item = matches.next().expect("the original queued Tilemap draw");
            assert!(matches.next().is_none());
            assert!(floors.contains(&item.entity));
            assert!(drawn_sources.insert((item.view_index, item.source_order)));
            let sprite = extracted
                .as_slice()
                .iter()
                .find(|sprite| {
                    sprite.entity == item.entity && sprite.source_order == item.source_order
                })
                .expect("the original extracted source of the captured Tilemap instance");
            assert_eq!(item.layer, sprite.layer);
            assert_eq!(item.sort_key, sprite.sort_key);
            assert_eq!(item.world_sort_y, sprite.world_sort_y);
            assert_eq!(
                *instance,
                SpriteInstance {
                    center: view.mapping.world_to_clip(sprite.world_center),
                    x_axis: view.mapping.world_vector_to_clip(sprite.world_x_axis),
                    y_axis: view.mapping.world_vector_to_clip(sprite.world_y_axis),
                    color: sprite.color,
                    uv: sprite.texture_region,
                }
            );
            drawn.insert(item.entity);
            drawn_cells += 1;
        }
    }
    assert_eq!(drawn, floors);
    assert_eq!(drawn_cells, expected_maps * 8);
    Some(Floor {
        publication: (handle, version),
        receipt,
        tileset: tileset.clone(),
    })
}

pub fn assert_released(
    game: &Game,
    material: Handle<Material2dAsset>,
    image: Handle<ImageAsset>,
    tileset: &TileSetContentReceipt,
) {
    game.session
        .observe(|world| {
            assert!(
                world
                    .resource::<Assets<Material2dAsset>>()
                    .get(material)
                    .is_none()
            );
            assert!(world.resource::<Assets<ImageAsset>>().get(image).is_none());
            let tile_assets = world.resource::<Assets<TileSet>>();
            let owner = world.resource::<TileSetAssets>();
            assert!(tile_assets.get(tileset.handle()).is_none());
            assert!(owner.is_empty());
            assert!(
                owner
                    .content_receipt(
                        tileset.handle(),
                        tile_assets,
                        world.resource::<AssetStates>()
                    )
                    .is_none()
            );
            let server = world.resource::<AssetServer>();
            assert_eq!(
                server.product(tileset.handle().id()),
                Some(region_tileset::tileset_product())
            );
            assert_eq!(
                server.get_product_handle::<TileSet>(region_tileset::tileset_product()),
                Some(tileset.handle())
            );
            for id in [material.id(), image.id(), tileset.handle().id()] {
                assert!(
                    world
                        .resource::<AssetStates>()
                        .state(id)
                        .unwrap()
                        .usable_version()
                        .is_none()
                );
            }
        })
        .unwrap();
}
