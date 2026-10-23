use nara_asset::{AssetRef, SourceHash};
#[cfg(test)]
use nara_tilemap::TileSet;
use nara_tilemap::{
    TileSetAssetError, TileSetAssets, TileSetContent, TileSetSource, TileSetSourceFileLimits,
};

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
        let outcome = self
            .runtime
            .with_driver_scope(|scope| scope.__apply_port::<EditorTileSetPort>(updates))
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?;
        let (changed, demands) = outcome
            .output
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)??;
        #[cfg(feature = "presentation-2d")]
        if changed {
            self.image_publication_revision = successor;
        }
        #[cfg(not(feature = "presentation-2d"))]
        let _ = changed;
        // The caller still needs these committed demands; Runtime health is observed by its drive.
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
    let mut access = nara_ecs::bevy_ecs::system::SystemState::<TileSetContent>::new(world);
    let mut content = match access.get_mut(world) {
        Ok(content) => content,
        Err(_) => {
            return Err((
                RuntimeWorldAccessError::MissingResource {
                    type_name: std::any::type_name::<TileSetContent>(),
                },
                updates,
            ));
        }
    };
    let result = (|| {
        let before = content.source_revisions().collect::<Vec<_>>();
        for update in updates {
            let result = match update {
                EditorTileSetSourceUpdate::Request(record, source, hash) => {
                    content.request(record, source, hash).map(|_| ())
                }
                EditorTileSetSourceUpdate::Reject(record) => content.reject_source(&record),
                EditorTileSetSourceUpdate::Remove(record) => {
                    content.revoke_source(&record).map(|_| ())
                }
            };
            result.map_err(|_| EditorImageRuntimeFailure::Publication)?;
        }
        content.reconcile();
        let after = content.source_revisions().collect::<Vec<_>>();
        let demands = content
            .sources()
            .filter(|(handle, _, _)| {
                !matches!(
                    content.error(*handle),
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
    })();
    Ok(result)
}
