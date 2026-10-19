use nara_asset::{
    AssetEvents, AssetRecord, AssetServer, AssetSlotRevision, AssetSourceKind, AssetStateRevision,
    AssetStates, Assets, Handle, ProjectAssetDatabase, SourceHash,
};
use nara_ecs::{Res, ResMut, system::SystemParam};
use nara_image::ImageAsset;

use super::{TileSetAssetError, TileSetAssets};
use crate::{TileSet, TileSetSource};

/// Concrete access to TileSet source requests and Image-dependent publication.
///
/// The Host supplies already admitted source records and decoded source data. This
/// parameter performs no filesystem discovery or Image decoding. Install
/// [`crate::TileSetAssetPlugin`] and [`nara_asset::AssetPlugin`] before using it.
#[derive(SystemParam)]
pub struct TileSetContent<'w> {
    owner: ResMut<'w, TileSetAssets>,
    server: ResMut<'w, AssetServer>,
    database: Res<'w, ProjectAssetDatabase>,
    images: Res<'w, Assets<ImageAsset>>,
    tilesets: ResMut<'w, Assets<TileSet>>,
    states: ResMut<'w, AssetStates>,
    events: ResMut<'w, AssetEvents>,
}

impl TileSetContent<'_> {
    /// Replaces the desired source while preserving an eligible last-good publication.
    pub fn request(
        &mut self,
        record: AssetRecord,
        source: TileSetSource,
        source_hash: SourceHash,
    ) -> Result<Handle<TileSet>, TileSetAssetError> {
        self.owner.request(
            record,
            source,
            source_hash,
            &mut self.server,
            &self.tilesets,
            &mut self.states,
        )
    }

    /// Records a Host-rejected source without allowing its pending dependency to publish.
    ///
    /// An already published source retains its eligible last-good value. An initial
    /// rejection reserves its admitted identity and publishes a load diagnostic.
    pub fn reject_source(&mut self, record: &AssetRecord) -> Result<(), TileSetAssetError> {
        validate_source_kind(record)?;
        let handle = self.source_handle(record);
        if let Some(handle) = handle {
            self.owner.reject_source(
                handle,
                &mut self.tilesets,
                &mut self.states,
                &mut self.events,
            )?;
        } else {
            let handle = self
                .server
                .reserve_record::<TileSet>(record)
                .map_err(TileSetAssetError::Asset)?;
            self.states.set_loading(handle.id());
            self.tilesets
                .record_load_failure(
                    handle,
                    &mut self.states,
                    &mut self.events,
                    TileSetAssetError::InvalidSource.to_string(),
                )
                .map_err(TileSetAssetError::State)?;
        }
        Ok(())
    }

    /// Revokes a Host-observed missing source while retaining its admitted layout contract.
    pub fn revoke_source(&mut self, record: &AssetRecord) -> Result<bool, TileSetAssetError> {
        validate_source_kind(record)?;
        let Some(handle) = self.source_handle(record) else {
            return Ok(false);
        };
        self.owner.revoke_source(
            handle,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        )
    }

    /// Reconciles current catalog and Image publications without performing source I/O.
    pub fn reconcile(&mut self) {
        self.owner.reconcile(
            &mut self.server,
            &self.database,
            &self.images,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        );
    }

    /// Returns the current published value, including an eligible last-good value.
    #[must_use]
    pub fn get(&self, handle: Handle<TileSet>) -> Option<&TileSet> {
        self.tilesets.get(handle)
    }

    /// Returns the latest reconciliation diagnostic independently from published content.
    #[must_use]
    pub fn error(&self, handle: Handle<TileSet>) -> Option<&TileSetAssetError> {
        self.owner.error(handle)
    }

    /// Enumerates desired sources for Host-owned dependency demand.
    pub fn sources(&self) -> impl Iterator<Item = (Handle<TileSet>, &AssetRecord, &TileSetSource)> {
        self.owner.sources()
    }

    /// Observes storage and load-state changes for admitted sources without exposing mutation.
    pub fn source_revisions(
        &self,
    ) -> impl Iterator<Item = (Handle<TileSet>, AssetSlotRevision, AssetStateRevision)> + '_ {
        self.owner.sources().map(|(handle, _, _)| {
            (
                handle,
                self.tilesets.slot_revision(handle),
                self.states.state_revision(handle.id()),
            )
        })
    }

    fn source_handle(&self, record: &AssetRecord) -> Option<Handle<TileSet>> {
        self.owner
            .sources()
            .find(|(_, existing, _)| *existing == record)
            .map(|(handle, _, _)| handle)
    }
}

fn validate_source_kind(record: &AssetRecord) -> Result<(), TileSetAssetError> {
    if record.source_kind() == &AssetSourceKind::Other(String::from("tileset")) {
        Ok(())
    } else {
        Err(TileSetAssetError::InvalidSourceKind)
    }
}
