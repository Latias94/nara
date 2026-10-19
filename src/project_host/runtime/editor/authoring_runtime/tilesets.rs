use nara_asset::{AssetRef, ProjectAssetDatabase, SourceHash};
use nara_tilemap::{TileSetAssetError, TileSetAssets, TileSetSource, TileSetSourceFileLimits};

use super::*;

pub(in crate::project_host::runtime::editor) enum EditorTileSetSourceUpdate {
    Request(AssetRecord, TileSetSource, SourceHash),
    Reject(AssetRecord),
    Remove(AssetRecord),
}

#[derive(Resource)]
struct EditorTileSetPort;

impl __RuntimeDriverPort for EditorTileSetPort {
    type Input = Vec<EditorTileSetSourceUpdate>;
    type Output = Result<(bool, Vec<AssetRef>), EditorImageRuntimeFailure>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(EditorImageRuntimeFailure::RuntimeUnavailable)
    }
}

pub(super) fn install(app: &mut App) -> Result<(), DiagnosticReport> {
    app.insert_resource(EditorTileSetPort)
        .and_then(|app| app.__register_driver_port::<EditorTileSetPort>(apply_in_world))
        .map_err(|_| runtime_error("project.editor.preview-tileset-port-failed"))?;
    Ok(())
}

impl EditorAuthoringRuntime {
    #[cfg(test)]
    pub(in crate::project_host::runtime::editor) fn test_tile_set(
        &self,
        path: &nara_asset::AssetPath,
    ) -> Option<TileSet> {
        let world = self.runtime.world();
        let handle = world
            .resource::<TileSetAssets>()
            .sources()
            .find(|(_, record, _)| record.path() == path)
            .map(|(handle, _, _)| handle)?;
        world.resource::<Assets<TileSet>>().get(handle).cloned()
    }

    pub(in crate::project_host::runtime::editor) fn tile_set_records(&self) -> &[AssetRecord] {
        &self.tileset_records
    }

    pub(in crate::project_host::runtime::editor) fn reconcile_tile_sets(
        &mut self,
        updates: Vec<EditorTileSetSourceUpdate>,
    ) -> Result<Vec<AssetRef>, EditorImageRuntimeFailure> {
        if let Some(failure) = self.image_revocation_failure {
            return Err(failure);
        }
        if updates.is_empty() && self.runtime.world().resource::<TileSetAssets>().is_empty() {
            return Ok(Vec::new());
        }
        #[cfg(feature = "presentation-2d")]
        let successor = self
            .image_publication_revision
            .checked_add(1)
            .ok_or(EditorImageRuntimeFailure::Publication)?;
        let (changed, demands) = self
            .runtime
            .with_driver_scope(|scope| scope.__apply_port::<EditorTileSetPort>(updates))
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)??;
        #[cfg(feature = "presentation-2d")]
        if changed {
            self.image_publication_revision = successor;
        }
        #[cfg(not(feature = "presentation-2d"))]
        let _ = changed;
        Ok(demands)
    }
}

pub(super) fn publish_initial(
    world: &mut World,
    snapshot: &AuthoringProjectSessionSnapshot,
) -> Result<(), DiagnosticReport> {
    let mut updates = Vec::new();
    for source in snapshot.asset_sources().filter(|source| {
        source.record().source_kind() == &AssetSourceKind::Other(String::from("tileset"))
    }) {
        updates.push(
            match TileSetSource::decode_ron_bytes(
                source.bytes(),
                TileSetSourceFileLimits::default(),
            ) {
                Ok(decoded) => EditorTileSetSourceUpdate::Request(
                    source.record().clone(),
                    decoded,
                    SourceHash::from_bytes(source.bytes()),
                ),
                Err(_) => EditorTileSetSourceUpdate::Reject(source.record().clone()),
            },
        );
    }
    apply_in_world(world, updates)
        .map_err(|_| runtime_error("project.editor.preview-tileset-world-failed"))?
        .map_err(|_| runtime_error("project.editor.preview-tileset-publication-failed"))?;
    Ok(())
}

fn apply_in_world(
    world: &mut World,
    updates: Vec<EditorTileSetSourceUpdate>,
) -> Result<
    Result<(bool, Vec<AssetRef>), EditorImageRuntimeFailure>,
    (RuntimeWorldAccessError, Vec<EditorTileSetSourceUpdate>),
> {
    let result = with_assets(
        world,
        |owner, server, database, images, tilesets, states, events| {
            let before = owner
                .sources()
                .map(|(handle, _, _)| {
                    (
                        handle,
                        tilesets.slot_revision(handle),
                        states.state_revision(handle.id()),
                    )
                })
                .collect::<Vec<_>>();
            for update in updates {
                let result = match update {
                    EditorTileSetSourceUpdate::Request(record, source, hash) => owner
                        .request(record, source, hash, server, tilesets, states)
                        .map(|_| ()),
                    EditorTileSetSourceUpdate::Reject(record) => {
                        let handle = owner
                            .sources()
                            .find(|(_, existing, _)| *existing == &record)
                            .map(|(handle, _, _)| handle);
                        if let Some(handle) = handle {
                            owner
                                .reject_source(handle, tilesets, states, events)
                                .map(|_| ())
                        } else {
                            server
                                .reserve_record::<TileSet>(&record)
                                .map_err(TileSetAssetError::Asset)
                                .and_then(|handle| {
                                    states.set_loading(handle.id());
                                    tilesets
                                        .record_load_failure(
                                            handle,
                                            states,
                                            events,
                                            TileSetAssetError::InvalidSource.to_string(),
                                        )
                                        .map(|_| ())
                                        .map_err(TileSetAssetError::State)
                                })
                        }
                    }
                    EditorTileSetSourceUpdate::Remove(record) => {
                        let handle = owner
                            .sources()
                            .find(|(_, existing, _)| *existing == &record)
                            .map(|(handle, _, _)| handle);
                        if let Some(handle) = handle {
                            owner
                                .revoke_source(handle, tilesets, states, events)
                                .map(|_| ())
                        } else {
                            Ok(())
                        }
                    }
                };
                result.map_err(|_| EditorImageRuntimeFailure::Publication)?;
            }
            owner.reconcile(server, database, images, tilesets, states, events);
            let after = owner
                .sources()
                .map(|(handle, _, _)| {
                    (
                        handle,
                        tilesets.slot_revision(handle),
                        states.state_revision(handle.id()),
                    )
                })
                .collect::<Vec<_>>();
            let demands = owner
                .sources()
                .filter(|(handle, _, _)| {
                    !matches!(
                        owner.error(*handle),
                        Some(
                            TileSetAssetError::InvalidSource
                                | TileSetAssetError::Superseded
                                | TileSetAssetError::AtlasLayoutChangeRequiresRebuild
                                | TileSetAssetError::IdentityMismatch
                        )
                    )
                })
                .map(|(_, _, source)| source.image.clone())
                .collect();
            Ok((before != after, demands))
        },
    );
    Ok(result)
}

fn with_assets<ResultValue>(
    world: &mut World,
    apply: impl FnOnce(
        &mut TileSetAssets,
        &mut AssetServer,
        &ProjectAssetDatabase,
        &Assets<ImageAsset>,
        &mut Assets<TileSet>,
        &mut AssetStates,
        &mut AssetEvents,
    ) -> ResultValue,
) -> ResultValue {
    world.resource_scope(|world, mut owner: Mut<'_, TileSetAssets>| {
        world.resource_scope(|world, mut server: Mut<'_, AssetServer>| {
            world.resource_scope(|world, database: Mut<'_, ProjectAssetDatabase>| {
                world.resource_scope(|world, images: Mut<'_, Assets<ImageAsset>>| {
                    world.resource_scope(|world, mut tilesets: Mut<'_, Assets<TileSet>>| {
                        world.resource_scope(|world, mut states: Mut<'_, AssetStates>| {
                            let mut events = world.resource_mut::<AssetEvents>();
                            apply(
                                &mut owner,
                                &mut server,
                                &database,
                                &images,
                                &mut tilesets,
                                &mut states,
                                &mut events,
                            )
                        })
                    })
                })
            })
        })
    })
}
