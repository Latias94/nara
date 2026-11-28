use bevy_ecs::{
    change_detection::{DetectChanges, Ref},
    error::{BevyError, warn},
    prelude::{Commands, Component, Entity, Or, Query, Res, ResMut, SystemSet, With, World},
};
use nara_asset::{
    AssetError, AssetServer, AssetSlotRevision, AssetStates, Assets, Handle, LoadState,
    tilemap::TileSetAsset,
};
use nara_core::AssetId;

use crate::components::TilemapSource;

/// Resolves tile sources after scene activation and asset publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct ResolveTilemapAssets;

/// The tileset request and validation status shared by presentation and physics.
#[derive(Component)]
pub struct TilemapAsset {
    id: AssetId,
    source_generation: u64,
    handle: Option<Handle<TileSetAsset>>,
    revision: Option<AssetSlotRevision>,
    valid: bool,
}

impl TilemapAsset {
    /// The resident tileset if the grid and its tile indices have passed validation.
    #[must_use]
    pub fn ready(&self) -> Option<(Handle<TileSetAsset>, &AssetSlotRevision)> {
        if !self.valid {
            return None;
        }
        Some((self.handle?, self.revision.as_ref()?))
    }
}

type TilemapSources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<Ref<'static, TilemapSource>>,
        Option<&'static mut TilemapAsset>,
    ),
    Or<(With<TilemapSource>, With<TilemapAsset>)>,
>;

pub(super) fn resolve_tilemap_assets(
    mut commands: Commands,
    mut server: ResMut<AssetServer>,
    assets: Res<Assets<TileSetAsset>>,
    states: Res<AssetStates>,
    mut sources: TilemapSources,
) {
    let source_generation = server.runtime_source_generation();
    for (entity, source, mut request) in &mut sources {
        let Some(source) = source else {
            commands.entity(entity).try_remove::<TilemapAsset>();
            continue;
        };
        let cached = request.as_deref();
        let reusable = cached.is_some_and(|request| {
            request.id == source.tileset
                && (request
                    .handle
                    .is_some_and(|handle| assets.get(handle).is_some())
                    || request.source_generation == source_generation
                        && !request.handle.is_some_and(|handle| {
                            states
                                .state(handle.id())
                                .is_some_and(|state| state.load_state() == &LoadState::Removed)
                        }))
        });
        let handle = if reusable {
            cached.and_then(|request| request.handle)
        } else {
            match server.load_id(source.tileset) {
                Ok(handle) => Some(handle),
                Err(AssetError::MissingRuntimeSource) => None,
                Err(error) => {
                    report(&mut commands, entity, error.to_string());
                    None
                }
            }
        };
        let resident =
            handle.and_then(|handle| Some((assets.get(handle)?, assets.slot_revision(handle))));
        if reusable
            && !source.is_changed()
            && cached.is_some_and(|request| {
                request.revision.as_ref() == resident.as_ref().map(|(_, revision)| revision)
                    && request.source_generation == source_generation
            })
        {
            continue;
        }
        let (revision, valid) = resident.map_or((None, false), |(tileset, revision)| {
            let valid = source
                .validate()
                .and_then(|()| source.validate_tile_indices(tileset.tiles.len()));
            if let Err(error) = &valid {
                report(&mut commands, entity, error.to_string());
            }
            (Some(revision), valid.is_ok())
        });
        let next = TilemapAsset {
            id: source.tileset,
            source_generation,
            handle,
            revision,
            valid,
        };
        if let Some(request) = &mut request {
            **request = next;
        } else {
            commands.entity(entity).try_insert(next);
        }
    }
}

fn report(commands: &mut Commands, entity: Entity, error: String) {
    commands.queue_handled(
        move |_: &mut World| -> Result<(), BevyError> {
            Err(BevyError::warning(format!(
                "tilemap source on {entity}: {error}"
            )))
        },
        warn,
    );
}
