use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{AssetServer, AssetSlotRevision, AssetStates, Assets, Handle, LoadState};
use nara_core::AssetId;
use nara_ecs::{
    Commands, Component, DetectChanges, Entity, Or, Query, Ref, Res, ResMut, With, World,
    bevy_ecs::error::{BevyError, warn},
    schedule::IntoScheduleConfigs,
};
use nara_scene::{ResolveTilemapAssets, SCENE_PLUGIN_ID, TilemapAsset, components::TilemapSource};

use crate::image::{IMAGE_PLUGIN_ID, ImageAsset};

use super::{TileCell, TileCoord, TileIndex, TileLayer, TileSet, TileSetAsset, Tilemap};

const ID: PluginId = PluginId::new("nara.tilemap-source-hydration");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[SCENE_PLUGIN_ID, IMAGE_PLUGIN_ID]);

/// Builds a renderable [`Tilemap`] from each authored [`TilemapSource`].
/// A failed or pending replacement keeps the previous map until its assets are ready.
/// Removing the source removes its generated map.
#[derive(Debug, Default, Clone, Copy)]
pub struct TilemapSourceHydrationPlugin;

impl Plugin for TilemapSourceHydrationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::TaskUpdate,
            hydrate_tilemaps.after(ResolveTilemapAssets),
        )?;
        Ok(())
    }
}

#[derive(Component, Default)]
struct HydratedTilemap {
    image: Option<ImageRequest>,
    applied: Option<(Handle<TileSetAsset>, AssetSlotRevision)>,
    grid_changed: bool,
}

struct ImageRequest {
    id: AssetId,
    handle: Option<Handle<ImageAsset>>,
    source_generation: u64,
}

type Tilemaps<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<Ref<'static, TilemapSource>>,
        Option<&'static TilemapAsset>,
        Option<&'static mut Tilemap>,
        Option<&'static mut HydratedTilemap>,
    ),
    Or<(With<TilemapSource>, With<HydratedTilemap>)>,
>;

fn hydrate_tilemaps(
    mut commands: Commands,
    mut server: ResMut<AssetServer>,
    states: Res<AssetStates>,
    images: Res<Assets<ImageAsset>>,
    tilesets: Res<Assets<TileSetAsset>>,
    mut maps: Tilemaps,
) {
    for (entity, source, asset, map, mut tracked) in &mut maps {
        let Some(source) = source else {
            if tracked
                .as_ref()
                .is_some_and(|tracked| tracked.applied.is_some())
            {
                commands.entity(entity).try_remove::<Tilemap>();
            }
            commands.entity(entity).try_remove::<HydratedTilemap>();
            continue;
        };

        let mut new = None;
        let tracked = tracked
            .as_deref_mut()
            .unwrap_or_else(|| new.insert(HydratedTilemap::default()));
        tracked.grid_changed |= source.is_changed();
        if let Some((handle, revision)) = asset.and_then(TilemapAsset::ready)
            && let Some(tileset) = tilesets.get(handle)
            && let Some(image) = request_image(
                &mut tracked.image,
                tileset.image,
                &mut server,
                &states,
                &images,
                &mut commands,
                entity,
            )
        {
            let unchanged = tracked
                .applied
                .as_ref()
                .is_some_and(|(id, old)| *id == handle && old == revision);
            if !unchanged || tracked.grid_changed || map.is_none() {
                let visual =
                    TileSet::from_image(image, tileset.layout).with_sampler(tileset.sampler);
                if let Some(mut map) = map {
                    sync_grid(&mut map, &source);
                    map.set_authored_tileset(visual);
                } else {
                    let mut map = Tilemap::new(source.tile_size);
                    sync_grid(&mut map, &source);
                    map.set_authored_tileset(visual);
                    commands.entity(entity).try_insert(map);
                }
                tracked.applied = Some((handle, revision.clone()));
                tracked.grid_changed = false;
            }
        }
        if let Some(new) = new {
            commands.entity(entity).try_insert(new);
        }
    }
}

fn request_image(
    request: &mut Option<ImageRequest>,
    id: AssetId,
    server: &mut AssetServer,
    states: &AssetStates,
    images: &Assets<ImageAsset>,
    commands: &mut Commands,
    entity: Entity,
) -> Option<Handle<ImageAsset>> {
    let source_generation = server.runtime_source_generation();
    if let Some(cached) = request.as_mut().filter(|request| request.id == id) {
        if let Some(handle) = cached.handle.filter(|&handle| images.get(handle).is_some()) {
            cached.source_generation = source_generation;
            return Some(handle);
        }
        let removed = cached.handle.is_some_and(|handle| {
            states
                .state(handle.id())
                .is_some_and(|state| state.load_state() == &LoadState::Removed)
        });
        if cached.source_generation == source_generation && !removed {
            return None;
        }
    }
    let handle = match server.load_id(id) {
        Ok(handle) => Some(handle),
        Err(error) => {
            commands.queue_handled(
                move |_: &mut World| -> Result<(), BevyError> {
                    Err(BevyError::warning(format!(
                        "tilemap source on {entity} could not request image '{id}': {error}"
                    )))
                },
                warn,
            );
            None
        }
    };
    *request = Some(ImageRequest {
        id,
        handle,
        source_generation,
    });
    handle.filter(|&handle| images.get(handle).is_some())
}

fn sync_grid(map: &mut Tilemap, source: &TilemapSource) {
    map.tile_size = source.tile_size;
    map.layer = TileLayer::new(source.layer);
    let obsolete: Vec<_> = map
        .cells()
        .filter_map(|(coord, _)| {
            let x = i64::from(coord.x) - i64::from(source.origin_x);
            let y = i64::from(coord.y) - i64::from(source.origin_y);
            let occupied = (0..i64::from(source.width)).contains(&x)
                && (0..i64::from(source.height)).contains(&y)
                && source
                    .cells
                    .0
                    .get(y as usize * source.width as usize + x as usize)
                    .is_some_and(|&cell| cell != 0);
            (!occupied).then_some(coord)
        })
        .collect();
    for coord in obsolete {
        map.remove_cell(coord);
    }
    for (index, &value) in source.cells.0.iter().enumerate() {
        if let Some(tile) = value.checked_sub(1)
            && let Some((x, y)) = source.cell_coordinates(index)
        {
            let coord = TileCoord::new(x, y);
            let cell = TileCell::new(TileIndex::new(u32::from(tile)));
            if map.get_cell(coord) != Some(&cell) {
                map.set_cell(coord, cell);
            }
        }
    }
}
