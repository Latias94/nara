use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, Weak},
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{
    AssetError, AssetEvents, AssetId, AssetRecord, AssetServer, AssetSlotRevision, AssetSourceKind,
    AssetStateError, AssetStateRevision, AssetStates, AssetTaskUpdateSet, Assets, Handle,
    LoadState, ProjectAssetDatabase, SourceHash,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::{Resource, schedule::IntoScheduleConfigs};
use nara_image::{ImageAsset, ImageContentError, ImageContentSnapshot};

use crate::{TileAtlasLayout, TileSet, TileSetSource};

mod access;
mod publication;

pub use access::TileSetContent;
pub use publication::{
    PreparedTileSetPublications, TileSetOwnedSourceReceipt, TileSetPublicationInput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSetAssetLimits {
    entries: ItemLimit,
    retained_images: ItemLimit,
    retained_bytes: ByteLimit,
}

impl TileSetAssetLimits {
    #[must_use]
    pub const fn new(
        entries: ItemLimit,
        retained_images: ItemLimit,
        retained_bytes: ByteLimit,
    ) -> Self {
        Self {
            entries,
            retained_images,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn entries(self) -> ItemLimit {
        self.entries
    }

    #[must_use]
    pub const fn retained_images(self) -> ItemLimit {
        self.retained_images
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

impl Default for TileSetAssetLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(256).expect("default TileSet entry limit is non-zero"),
            ItemLimit::new(512).expect("default TileSet retained image limit is non-zero"),
            ByteLimit::new(128 * 1024 * 1024).expect("default TileSet byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSetRetentionSnapshot {
    pub images: usize,
    pub bytes: usize,
}

#[derive(Debug)]
struct RetainedImage {
    image: ImageContentSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyObservation {
    record: Option<AssetRecord>,
    state: Option<AssetStateRevision>,
    slot: Option<AssetSlotRevision>,
}

#[derive(Debug)]
struct TileSetEntry {
    source_claim: Arc<()>,
    handle: Handle<TileSet>,
    record: AssetRecord,
    desired: TileSetSource,
    source_hash: SourceHash,
    source_active: bool,
    accepted_atlas: Option<TileAtlasLayout>,
    published: Option<ImageContentSnapshot>,
    observation: Option<DependencyObservation>,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    error: Option<TileSetAssetError>,
}

/// Concrete reconciliation owner for decoded TileSet sources and exact Image dependencies.
#[derive(Debug, Resource)]
pub struct TileSetAssets {
    limits: TileSetAssetLimits,
    entries: BTreeMap<AssetId, TileSetEntry>,
    retained: Vec<Weak<RetainedImage>>,
}

impl Default for TileSetAssets {
    fn default() -> Self {
        Self::with_limits(TileSetAssetLimits::default())
    }
}

impl TileSetAssets {
    #[must_use]
    pub fn with_limits(limits: TileSetAssetLimits) -> Self {
        Self {
            limits,
            entries: BTreeMap::new(),
            retained: Vec::new(),
        }
    }

    #[must_use]
    pub const fn limits(&self) -> TileSetAssetLimits {
        self.limits
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn error(&self, handle: Handle<TileSet>) -> Option<&TileSetAssetError> {
        self.entries
            .get(&handle.id())
            .and_then(|entry| entry.error.as_ref())
    }

    #[must_use]
    pub fn desired(&self, handle: Handle<TileSet>) -> Option<&TileSetSource> {
        self.entries.get(&handle.id()).map(|entry| &entry.desired)
    }

    /// Enumerates admitted source ownership for concrete Host refresh and dependency demand.
    pub fn sources(&self) -> impl Iterator<Item = (Handle<TileSet>, &AssetRecord, &TileSetSource)> {
        self.entries
            .values()
            .map(|entry| (entry.handle, &entry.record, &entry.desired))
    }

    #[must_use]
    pub fn retention(&self) -> TileSetRetentionSnapshot {
        self.retained.iter().filter_map(Weak::upgrade).fold(
            TileSetRetentionSnapshot {
                images: 0,
                bytes: 0,
            },
            |mut snapshot, retained| {
                snapshot.images += 1;
                snapshot.bytes += retained.image.pixels().len();
                snapshot
            },
        )
    }

    /// Replaces the latest desired source without giving up an eligible published dependency.
    pub fn request(
        &mut self,
        record: AssetRecord,
        source: TileSetSource,
        source_hash: SourceHash,
        server: &mut AssetServer,
        tilesets: &Assets<TileSet>,
        states: &mut AssetStates,
    ) -> Result<Handle<TileSet>, TileSetAssetError> {
        source
            .validate()
            .map_err(|_| TileSetAssetError::InvalidSource)?;
        if record.source_kind() != &AssetSourceKind::Other(String::from("tileset")) {
            return Err(TileSetAssetError::InvalidSourceKind);
        }
        let existing = self
            .entries
            .values()
            .find(|entry| entry.record.path() == record.path())
            .map(|entry| entry.handle);
        if existing.is_none_or(|handle| !self.entries.contains_key(&handle.id()))
            && self.entries.len() >= self.limits.entries.get()
        {
            return Err(TileSetAssetError::Capacity);
        }
        let handle = server
            .reserve_record::<TileSet>(&record)
            .map_err(TileSetAssetError::Asset)?;
        let published = self
            .entries
            .get(&handle.id())
            .filter(|entry| {
                entry.slot == tilesets.slot_revision(handle)
                    && entry.state == states.state_revision(handle.id())
            })
            .and_then(|entry| entry.published.clone());
        let accepted_atlas = self
            .entries
            .get(&handle.id())
            .and_then(|entry| entry.accepted_atlas);
        if self.entries.get(&handle.id()).is_some_and(|entry| {
            entry.record == record
                && entry.source_hash == source_hash
                && entry.desired == source
                && entry.source_active
                && entry.slot == tilesets.slot_revision(handle)
                && entry.state == states.state_revision(handle.id())
                && entry.error != Some(TileSetAssetError::Superseded)
                && entry.error != Some(TileSetAssetError::RetentionCapacity)
        }) {
            if let Some(entry) = self.entries.get_mut(&handle.id()) {
                if matches!(
                    entry.error,
                    Some(
                        TileSetAssetError::DependencyUnavailable
                            | TileSetAssetError::ImageExtentMismatch
                    )
                ) {
                    entry.observation = None;
                    entry.error = None;
                }
            }
            return Ok(handle);
        }
        let source_claim = self
            .entries
            .get(&handle.id())
            .filter(|entry| {
                entry.record == record
                    && entry.desired == source
                    && entry.source_hash == source_hash
                    && entry.slot == tilesets.slot_revision(handle)
                    && entry.state == states.state_revision(handle.id())
                    && entry.error != Some(TileSetAssetError::Superseded)
            })
            .map_or_else(|| Arc::new(()), |entry| Arc::clone(&entry.source_claim));
        states.set_loading(handle.id());
        self.entries.insert(
            handle.id(),
            TileSetEntry {
                source_claim,
                handle,
                record,
                desired: source,
                source_hash,
                source_active: true,
                accepted_atlas,
                published,
                observation: None,
                slot: tilesets.slot_revision(handle),
                state: states.state_revision(handle.id()),
                error: None,
            },
        );
        Ok(handle)
    }

    /// Explicitly rechecks a capacity or dependency failure against current authority.
    pub fn retry(&mut self, handle: Handle<TileSet>) -> bool {
        let Some(entry) = self.entries.get_mut(&handle.id()) else {
            return false;
        };
        entry.observation = None;
        true
    }

    /// Stops a rejected source's pending dependency publication while retaining eligible last-good.
    pub fn reject_source(
        &mut self,
        handle: Handle<TileSet>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<bool, TileSetAssetError> {
        let Some(entry) = self.entries.get_mut(&handle.id()) else {
            return Ok(false);
        };
        if entry.error == Some(TileSetAssetError::Superseded)
            || entry.slot != tilesets.slot_revision(handle)
            || entry.state != states.state_revision(handle.id())
        {
            return Err(TileSetAssetError::Superseded);
        }
        if !entry.source_active {
            return Ok(true);
        }
        entry.source_active = false;
        states.set_loading(handle.id());
        let message = TileSetAssetError::InvalidSource.to_string();
        if entry.published.is_some() {
            tilesets.record_reload_failure(handle, states, events, message)
        } else {
            tilesets.record_load_failure(handle, states, events, message)
        }
        .map_err(TileSetAssetError::State)?;
        entry.error = Some(TileSetAssetError::InvalidSource);
        refresh_entry(entry, tilesets, states);
        Ok(true)
    }

    pub fn remove(
        &mut self,
        handle: Handle<TileSet>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<bool, TileSetAssetError> {
        let Some(entry) = self.entries.get(&handle.id()) else {
            return Ok(false);
        };
        if entry.error == Some(TileSetAssetError::Superseded)
            || entry.slot != tilesets.slot_revision(handle)
            || entry.state != states.state_revision(handle.id())
        {
            return Err(TileSetAssetError::Superseded);
        }
        let result = revoke(handle, tilesets, states, events);
        if result.is_err() {
            states.invalidate_publication(handle.id());
            tilesets.remove(handle);
        }
        self.entries.remove(&handle.id());
        result?;
        Ok(true)
    }

    /// Revokes a missing source without discarding the layout contract required by its consumers.
    pub fn revoke_source(
        &mut self,
        handle: Handle<TileSet>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<bool, TileSetAssetError> {
        let Some(entry) = self.entries.get_mut(&handle.id()) else {
            return Ok(false);
        };
        if entry.error == Some(TileSetAssetError::Superseded)
            || entry.slot != tilesets.slot_revision(handle)
            || entry.state != states.state_revision(handle.id())
        {
            return Err(TileSetAssetError::Superseded);
        }
        if !entry.source_active && entry.published.is_none() {
            return Ok(true);
        }
        let result = revoke(handle, tilesets, states, events);
        if result.is_err() {
            states.invalidate_publication(handle.id());
            tilesets.remove(handle);
        }
        entry.source_active = false;
        entry.published = None;
        entry.error = Some(TileSetAssetError::IdentityMismatch);
        refresh_entry(entry, tilesets, states);
        result?;
        Ok(true)
    }

    /// Observes dependency identity and exact slot changes without repeating source I/O or decoding.
    pub fn reconcile(
        &mut self,
        server: &mut AssetServer,
        database: &ProjectAssetDatabase,
        images: &Assets<ImageAsset>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) {
        let ids = self.entries.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(mut entry) = self.entries.remove(&id) else {
                continue;
            };
            self.reconcile_entry(
                &mut entry, server, database, images, tilesets, states, events,
            );
            self.entries.insert(id, entry);
        }
    }

    fn retain_image(
        &mut self,
        image: ImageContentSnapshot,
    ) -> Result<ImageContentSnapshot, TileSetAssetError> {
        self.retained
            .retain(|retained| retained.strong_count() != 0);
        if let Some(retained) = self
            .retained
            .iter()
            .filter_map(Weak::upgrade)
            .find(|retained| retained.image.same_allocation(&image))
        {
            return Ok(image.with_retention_owner(retained));
        }
        let retained = self.retention();
        if retained.images >= self.limits.retained_images.get()
            || image.pixels().len()
                > self
                    .limits
                    .retained_bytes
                    .get()
                    .saturating_sub(retained.bytes)
        {
            return Err(TileSetAssetError::RetentionCapacity);
        }
        let owner = Arc::new(RetainedImage {
            image: image.clone(),
        });
        self.retained.push(Arc::downgrade(&owner));
        Ok(image.with_retention_owner(owner))
    }

    #[allow(clippy::too_many_arguments)]
    fn reconcile_entry(
        &mut self,
        entry: &mut TileSetEntry,
        server: &mut AssetServer,
        database: &ProjectAssetDatabase,
        images: &Assets<ImageAsset>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) {
        if entry.error == Some(TileSetAssetError::Superseded) {
            return;
        }
        if entry.slot != tilesets.slot_revision(entry.handle)
            || entry.state != states.state_revision(entry.handle.id())
        {
            entry.published = None;
            entry.error = Some(TileSetAssetError::Superseded);
            refresh_entry(entry, tilesets, states);
            return;
        }
        let catalog_valid = database.record_for_path(entry.record.path()) == Some(&entry.record)
            && server.get_handle::<TileSet>(entry.handle.id()) == Some(entry.handle)
            && server.path(entry.handle.id()) == Some(entry.record.path().as_str())
            && server.stable_id(entry.handle.id()) == Some(entry.record.stable_id());
        let publication_revoked = entry.published.as_ref().is_some_and(|image| {
            !image.is_authorized(server, states, images)
                || image.source().is_none_or(|source| {
                    database
                        .record_for_path(source.path())
                        .is_none_or(|record| {
                            record.source_kind() != &AssetSourceKind::Image
                                || record.stable_id() != source.stable_id()
                        })
                })
        });
        if (!catalog_valid && entry.published.is_some()) || publication_revoked {
            if let Err(error) = revoke(entry.handle, tilesets, states, events) {
                states.invalidate_publication(entry.handle.id());
                tilesets.remove(entry.handle);
                entry.error = Some(error);
                entry.published = None;
                refresh_entry(entry, tilesets, states);
                return;
            }
            entry.published = None;
            refresh_entry(entry, tilesets, states);
        }
        if !catalog_valid {
            entry.error = Some(TileSetAssetError::IdentityMismatch);
            return;
        }
        if !entry.source_active {
            return;
        }
        let record = database.resolve_ref(&entry.desired.image).ok();
        let image_handle = record
            .as_ref()
            .filter(|record| record.source_kind() == &AssetSourceKind::Image)
            .and_then(|record| server.reserve_record::<ImageAsset>(record).ok());
        let observation = DependencyObservation {
            record: record.cloned(),
            state: image_handle.map(|handle| states.state_revision(handle.id())),
            slot: image_handle.map(|handle| images.slot_revision(handle)),
        };
        if entry.observation.as_ref() == Some(&observation) && !publication_revoked {
            return;
        }
        entry.observation = Some(observation);
        let result = (|| {
            if entry
                .accepted_atlas
                .is_some_and(|atlas| atlas != entry.desired.atlas)
            {
                return Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild);
            }
            let handle = image_handle.ok_or(TileSetAssetError::DependencyUnavailable)?;
            // Published content remains eligible while a later import is pending or failed.
            // Snapshot admission owns identity, publication and exact storage-slot checks.
            let image = match ImageContentSnapshot::capture(handle, server, states, images) {
                Ok(image) => image,
                Err(ImageContentError::Unavailable)
                    if states.state(handle.id()).is_some_and(|state| {
                        matches!(state.load_state(), LoadState::Loading)
                            && state.usable_version().is_none()
                    }) =>
                {
                    return Ok(None);
                }
                Err(_) => return Err(TileSetAssetError::DependencyUnavailable),
            };
            let record = record.ok_or(TileSetAssetError::DependencyUnavailable)?;
            if record.source_kind() != &AssetSourceKind::Image
                || image.source().is_none_or(|source| {
                    source.path() != record.path() || source.stable_id() != record.stable_id()
                })
            {
                return Err(TileSetAssetError::IdentityMismatch);
            }
            entry
                .desired
                .validate_image_extent(image.extent())
                .map_err(|_| TileSetAssetError::ImageExtentMismatch)?;
            let tileset = entry
                .desired
                .clone()
                .into_tileset(handle)
                .map_err(|_| TileSetAssetError::InvalidSource)?;
            states
                .next_version(entry.handle.id())
                .map_err(TileSetAssetError::State)?;
            let image = self.retain_image(image)?;
            let tileset = tileset.with_image_content(image.clone());
            tilesets
                .commit_loaded(
                    entry.handle,
                    tileset,
                    states,
                    events,
                    Some(entry.source_hash),
                    None,
                )
                .map_err(TileSetAssetError::State)?;
            Ok(Some(image))
        })();
        match result {
            Ok(Some(image)) => {
                entry.accepted_atlas = Some(entry.desired.atlas);
                entry.published = Some(image);
                entry.error = None;
            }
            Ok(None) => {}
            Err(error) => {
                states.set_loading(entry.handle.id());
                let failure = if entry.published.is_some() {
                    tilesets.record_reload_failure(entry.handle, states, events, error.to_string())
                } else {
                    tilesets.record_load_failure(entry.handle, states, events, error.to_string())
                };
                entry.error = Some(failure.err().map(TileSetAssetError::State).unwrap_or(error));
            }
        }
        refresh_entry(entry, tilesets, states);
    }
}

fn refresh_entry(entry: &mut TileSetEntry, tilesets: &Assets<TileSet>, states: &AssetStates) {
    entry.slot = tilesets.slot_revision(entry.handle);
    entry.state = states.state_revision(entry.handle.id());
}

fn revoke(
    handle: Handle<TileSet>,
    tilesets: &mut Assets<TileSet>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
) -> Result<(), TileSetAssetError> {
    tilesets
        .remove_with_state(handle, states, events)
        .map(|_| ())
        .map_err(TileSetAssetError::State)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileSetAssetError {
    InvalidSourceKind,
    InvalidSource,
    Capacity,
    RetentionCapacity,
    DependencyUnavailable,
    ImageExtentMismatch,
    AtlasLayoutChangeRequiresRebuild,
    IdentityMismatch,
    Superseded,
    InvalidPublicationBatch,
    Asset(AssetError),
    State(AssetStateError),
}

impl fmt::Display for TileSetAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset(error) => error.fmt(formatter),
            Self::State(error) => error.fmt(formatter),
            _ => formatter.write_str(match self {
                Self::InvalidSourceKind => "TileSet source kind is invalid",
                Self::InvalidSource => "TileSet source is invalid",
                Self::Capacity => "TileSet source capacity is exhausted",
                Self::RetentionCapacity => "TileSet retained Image capacity is exhausted",
                Self::DependencyUnavailable => "TileSet Image dependency is unavailable",
                Self::ImageExtentMismatch => {
                    "TileSet atlas does not exactly match its Image extent"
                }
                Self::AtlasLayoutChangeRequiresRebuild => {
                    "TileSet atlas layout changes require rebuilding its consumers"
                }
                Self::IdentityMismatch => "TileSet source identity is no longer admitted",
                Self::Superseded => "TileSet publication was superseded by another writer",
                Self::InvalidPublicationBatch => {
                    "TileSet publication inputs overlap or repeat an identity"
                }
                Self::Asset(_) | Self::State(_) => unreachable!(),
            }),
        }
    }
}

impl Error for TileSetAssetError {}

#[derive(Debug, Default, Clone, Copy)]
pub struct TileSetAssetPlugin;

pub const TILESET_ASSET_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.tilemap.assets");
pub const TILESET_ASSET_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(TILESET_ASSET_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(&[nara_asset::ASSET_PLUGIN_ID]);

impl Plugin for TileSetAssetPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &TILESET_ASSET_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<TileSetAssets>()?;
        app.init_resource::<Assets<TileSet>>()?;
        app.init_resource::<Assets<ImageAsset>>()?;
        app.add_systems(
            CoreStage::TaskUpdate,
            reconcile_tilesets.after(AssetTaskUpdateSet::ApplyResults),
        )?;
        Ok(())
    }
}

fn reconcile_tilesets(mut content: TileSetContent) {
    content.reconcile();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetPath, AssetRef, ImportArtifactKey,
        ImportArtifactRecord, ImportDependencyDigest, ImportProfile, ImportedAssetType, ImporterId,
        ImporterVersion, StableAssetId,
    };
    use nara_core::Vec2;
    use nara_image::{ImageExtent, ImageFormat, ImageImportSettings, ImageSourceMetadata};

    use crate::{TileAtlasLayout, TileIndex};

    mod access;
    mod publication;
    mod source_admission;

    struct Fixture {
        server: AssetServer,
        database: ProjectAssetDatabase,
        images: Assets<ImageAsset>,
        tilesets: Assets<TileSet>,
        states: AssetStates,
        events: AssetEvents,
        owner: TileSetAssets,
        image_a: AssetRecord,
        image_b: AssetRecord,
        tileset: AssetRecord,
    }

    impl Fixture {
        fn new(limits: TileSetAssetLimits) -> Self {
            let image_a = record(
                "10000000-0000-4000-8000-000000000001",
                "a.png",
                AssetSourceKind::Image,
            );
            let image_b = record(
                "10000000-0000-4000-8000-000000000002",
                "b.png",
                AssetSourceKind::Image,
            );
            let tileset = record(
                "10000000-0000-4000-8000-000000000003",
                "terrain.tileset.ron",
                AssetSourceKind::Other(String::from("tileset")),
            );
            let mut fixture = Self {
                server: AssetServer::new(),
                database: ProjectAssetDatabase::default(),
                images: Assets::default(),
                tilesets: Assets::default(),
                states: AssetStates::default(),
                events: AssetEvents::default(),
                owner: TileSetAssets::with_limits(limits),
                image_a,
                image_b,
                tileset,
            };
            fixture.database.insert(fixture.image_a.clone()).unwrap();
            fixture.database.insert(fixture.tileset.clone()).unwrap();
            fixture.publish_image(fixture.image_a.clone(), 2, 1, 10);
            fixture
        }

        fn publish_image(
            &mut self,
            record: AssetRecord,
            width: u32,
            height: u32,
            byte: u8,
        ) -> Handle<ImageAsset> {
            let handle = self.server.reserve_record::<ImageAsset>(&record).unwrap();
            let image = image(&record, width, height, byte);
            self.images
                .commit_loaded(
                    handle,
                    image,
                    &mut self.states,
                    &mut self.events,
                    None,
                    None,
                )
                .unwrap();
            handle
        }

        fn request(&mut self, image: &str) -> Handle<TileSet> {
            self.owner
                .request(
                    self.tileset.clone(),
                    source(image),
                    SourceHash::from_bytes(image.as_bytes()),
                    &mut self.server,
                    &self.tilesets,
                    &mut self.states,
                )
                .unwrap()
        }

        fn reconcile(&mut self) {
            self.owner.reconcile(
                &mut self.server,
                &self.database,
                &self.images,
                &mut self.tilesets,
                &mut self.states,
                &mut self.events,
            );
        }
    }

    fn record(id: &str, path: &str, kind: AssetSourceKind) -> AssetRecord {
        AssetRecord::new(
            StableAssetId::parse_str(id).unwrap(),
            AssetPath::new(path).unwrap(),
            kind,
        )
    }

    fn source(path: &str) -> TileSetSource {
        TileSetSource::new(
            AssetRef::path(path).unwrap(),
            TileAtlasLayout::grid(Vec2::new(1.0, 1.0), 2, 1),
        )
    }

    fn image(record: &AssetRecord, width: u32, height: u32, byte: u8) -> ImageAsset {
        let pixels = vec![byte; width as usize * height as usize * 4];
        let settings = ImageImportSettings::default();
        let key = ImportArtifactKey::new(
            record.stable_id(),
            SourceHash::from_bytes(&pixels),
            ImportDependencyDigest::empty(),
            ImporterId::new("test.image").unwrap(),
            ImporterVersion::new(1),
            settings.settings_hash(),
            ImportProfile::default(),
            ImportedAssetType::new("image").unwrap(),
            ArtifactLabel::default(),
            ArtifactFormatVersion::new(1),
        );
        ImageAsset::from_import(
            ImageSourceMetadata::new(
                record.path().clone(),
                settings,
                ImportArtifactRecord::new(key).unwrap(),
            )
            .unwrap(),
            ImageExtent::new(width, height),
            ImageFormat::Rgba8,
            pixels,
        )
        .unwrap()
    }

    #[test]
    fn compatible_image_replacement_publishes_exact_new_content_and_retains_old_pixels() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let old = fixture
            .tilesets
            .get(tileset)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        let image = fixture.publish_image(fixture.image_a.clone(), 2, 1, 20);
        fixture.reconcile();
        let successor = fixture.tilesets.get(tileset).unwrap();
        let current = successor.image_content().unwrap();
        assert_eq!(old.pixels(), &[10; 8]);
        assert_eq!(current.pixels(), &[20; 8]);
        assert_eq!(current.handle(), image);
        assert!(old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
        assert!(!old.is_current(&fixture.server, &fixture.states, &fixture.images));
        assert!(current.is_current(&fixture.server, &fixture.states, &fixture.images));
        assert_eq!(
            successor.normalized_region(TileIndex::new(1)).unwrap().min,
            Vec2::new(0.5, 0.0)
        );
    }

    #[test]
    fn new_and_retargeted_tilesets_use_published_images_during_reload_or_failure() {
        for retarget in [false, true] {
            for reload_failed in [false, true] {
                let mut fixture = Fixture::new(TileSetAssetLimits::default());
                let dependency = if retarget {
                    let tileset = fixture.request("a.png");
                    fixture.reconcile();
                    assert!(fixture.tilesets.get(tileset).is_some());
                    fixture.database.insert(fixture.image_b.clone()).unwrap();
                    fixture.publish_image(fixture.image_b.clone(), 2, 1, 20)
                } else {
                    fixture
                        .server
                        .reserve_record::<ImageAsset>(&fixture.image_a)
                        .unwrap()
                };
                let published = ImageContentSnapshot::capture(
                    dependency,
                    &fixture.server,
                    &fixture.states,
                    &fixture.images,
                )
                .unwrap();
                fixture.states.set_loading(dependency.id());
                if reload_failed {
                    fixture
                        .images
                        .record_reload_failure(
                            dependency,
                            &mut fixture.states,
                            &mut fixture.events,
                            "successor decode failed",
                        )
                        .unwrap();
                }
                assert!(published.is_current(&fixture.server, &fixture.states, &fixture.images));

                let tileset = fixture.request(if retarget { "b.png" } else { "a.png" });
                fixture.reconcile();
                assert_eq!(fixture.owner.error(tileset), None);
                let actual = fixture
                    .tilesets
                    .get(tileset)
                    .expect("an authorized published dependency must admit its TileSet")
                    .image_content()
                    .unwrap();
                assert!(actual.same_allocation(&published));
                assert_eq!(actual.version(), published.version());
                assert_eq!(actual.pixels(), published.pixels());

                fixture
                    .images
                    .remove_with_state(dependency, &mut fixture.states, &mut fixture.events)
                    .unwrap();
                fixture.reconcile();
                assert!(fixture.tilesets.get(tileset).is_none());
                assert!(!published.is_authorized(
                    &fixture.server,
                    &fixture.states,
                    &fixture.images,
                ));
            }
        }
    }

    #[test]
    fn unpublished_dependencies_wait_or_fail_and_recover_after_publication() {
        for load_failed in [false, true] {
            let mut fixture = Fixture::new(TileSetAssetLimits::default());
            let dependency = fixture
                .server
                .reserve_record::<ImageAsset>(&fixture.image_a)
                .unwrap();
            fixture
                .images
                .remove_with_state(dependency, &mut fixture.states, &mut fixture.events)
                .unwrap();
            fixture.states.set_loading(dependency.id());
            if load_failed {
                fixture
                    .images
                    .record_load_failure(
                        dependency,
                        &mut fixture.states,
                        &mut fixture.events,
                        "initial decode failed",
                    )
                    .unwrap();
            }
            let tileset = fixture.request("a.png");
            fixture.reconcile();
            assert!(fixture.tilesets.get(tileset).is_none());
            assert_eq!(
                fixture.owner.error(tileset),
                load_failed.then_some(&TileSetAssetError::DependencyUnavailable),
            );

            fixture.publish_image(fixture.image_a.clone(), 2, 1, 30);
            fixture.reconcile();
            assert_eq!(fixture.owner.error(tileset), None);
            assert_eq!(
                fixture
                    .tilesets
                    .get(tileset)
                    .unwrap()
                    .image_content()
                    .unwrap()
                    .pixels(),
                &[30; 8],
            );
        }
    }

    #[test]
    fn loading_cannot_hide_invalid_published_identity_or_storage() {
        for wrong_identity in [false, true] {
            let mut fixture = Fixture::new(TileSetAssetLimits::default());
            let dependency = fixture
                .server
                .reserve_record::<ImageAsset>(&fixture.image_a)
                .unwrap();
            if wrong_identity {
                let invalid = image(&fixture.image_b, 2, 1, 20);
                fixture
                    .images
                    .commit_loaded(
                        dependency,
                        invalid,
                        &mut fixture.states,
                        &mut fixture.events,
                        None,
                        None,
                    )
                    .unwrap();
            } else {
                let removed = fixture.images.remove(dependency).unwrap();
                fixture.images.insert(dependency, removed);
            }
            fixture.states.set_loading(dependency.id());
            let tileset = fixture.request("a.png");
            fixture.reconcile();
            assert!(fixture.tilesets.get(tileset).is_none());
            assert_eq!(
                fixture.owner.error(tileset),
                Some(&TileSetAssetError::DependencyUnavailable),
            );
        }
    }

    #[test]
    fn incompatible_image_extent_keeps_coherent_last_good_until_a_valid_successor() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let old_version = fixture.states.version(tileset.id());
        fixture.publish_image(fixture.image_a.clone(), 4, 1, 30);
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::ImageExtentMismatch)
        );
        let old = fixture
            .tilesets
            .get(tileset)
            .unwrap()
            .image_content()
            .unwrap();
        assert_eq!(old.extent(), ImageExtent::new(2, 1));
        assert_eq!(old.pixels(), &[10; 8]);
        assert_eq!(
            fixture.states.state(tileset.id()).unwrap().usable_version(),
            old_version
        );
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 40);
        fixture.reconcile();
        assert_eq!(fixture.owner.error(tileset), None);
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[40; 8]
        );
    }

    #[test]
    fn unresolved_desired_dependency_does_not_churn_and_recovers_after_catalog_admission() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        assert_eq!(fixture.request("b.png"), tileset);
        fixture.reconcile();
        let failed = fixture.states.state_revision(tileset.id());
        let events = fixture.events.len();
        for _ in 0..3 {
            fixture.reconcile();
        }
        assert_eq!(fixture.states.state_revision(tileset.id()), failed);
        assert_eq!(fixture.events.len(), events);
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[10; 8]
        );
        fixture.database.insert(fixture.image_b.clone()).unwrap();
        fixture.publish_image(fixture.image_b.clone(), 2, 1, 50);
        fixture.reconcile();
        assert_eq!(fixture.owner.error(tileset), None);
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .source()
                .unwrap()
                .path(),
            fixture.image_b.path()
        );
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[50; 8]
        );
    }

    #[test]
    fn published_dependency_removal_revokes_while_latest_desired_dependency_is_missing() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let old = fixture
            .tilesets
            .get(tileset)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        fixture.request("b.png");
        fixture.reconcile();
        let image_a = fixture
            .server
            .reserve_record::<ImageAsset>(&fixture.image_a)
            .unwrap();
        fixture
            .images
            .remove_with_state(image_a, &mut fixture.states, &mut fixture.events)
            .unwrap();
        fixture.reconcile();
        assert!(fixture.tilesets.get(tileset).is_none());
        assert!(
            fixture
                .states
                .state(tileset.id())
                .unwrap()
                .usable_version()
                .is_none()
        );
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 60);
        fixture.reconcile();
        assert!(fixture.tilesets.get(tileset).is_none());
        assert!(!old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
        fixture.database.insert(fixture.image_b.clone()).unwrap();
        fixture.publish_image(fixture.image_b.clone(), 2, 1, 70);
        fixture.reconcile();
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[70; 8]
        );
    }

    #[test]
    fn retained_image_budget_charges_shared_payload_once_and_outlives_removed_tilesets() {
        let limits = TileSetAssetLimits::new(
            ItemLimit::new(2).unwrap(),
            ItemLimit::ONE,
            ByteLimit::new(8).unwrap(),
        );
        let mut fixture = Fixture::new(limits);
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let retained = fixture
            .tilesets
            .get(tileset)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        let second_record = record(
            "10000000-0000-4000-8000-000000000004",
            "second.tileset.ron",
            AssetSourceKind::Other(String::from("tileset")),
        );
        fixture.database.insert(second_record.clone()).unwrap();
        let second = fixture
            .owner
            .request(
                second_record,
                source("a.png"),
                SourceHash::from_bytes(b"second"),
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        fixture.reconcile();
        assert!(fixture.tilesets.get(second).is_some());
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 1,
                bytes: 8
            }
        );
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 80);
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::RetentionCapacity)
        );
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[10; 8]
        );
        fixture
            .owner
            .remove(
                tileset,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events,
            )
            .unwrap();
        fixture
            .owner
            .remove(
                second,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events,
            )
            .unwrap();
        assert_eq!(fixture.owner.retention().bytes, 8);
        drop(retained);
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 0,
                bytes: 0
            }
        );
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[80; 8]
        );
    }

    #[test]
    fn a_direct_tileset_write_stops_the_reconciler_without_destroying_another_writers_value() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let replacement = TileSet::new(Vec2::new(9.0, 7.0));
        fixture.tilesets.insert(tileset, replacement.clone());
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::Superseded)
        );
        let slot = fixture.tilesets.slot_revision(tileset);
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 90);
        fixture.reconcile();
        assert_eq!(fixture.tilesets.get(tileset), Some(&replacement));
        assert_eq!(fixture.tilesets.slot_revision(tileset), slot);
    }

    #[test]
    fn invalid_direct_source_is_rejected_before_claiming_a_handle_or_changing_state() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let mut invalid = source("a.png");
        invalid.atlas.columns = 0;
        let baseline = fixture.server.clone();
        let states = fixture.states.len();
        assert_eq!(
            fixture.owner.request(
                fixture.tileset.clone(),
                invalid,
                SourceHash::from_bytes(b"invalid"),
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states
            ),
            Err(TileSetAssetError::InvalidSource)
        );
        assert_eq!(fixture.server, baseline);
        assert_eq!(fixture.states.len(), states);
        assert!(fixture.owner.is_empty());
    }

    #[test]
    fn rejected_source_blocks_late_desired_dependency_but_keeps_published_revocation_active() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        fixture.request("b.png");
        fixture.reconcile();
        fixture
            .owner
            .reject_source(
                tileset,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events,
            )
            .unwrap();
        fixture.database.insert(fixture.image_b.clone()).unwrap();
        fixture.publish_image(fixture.image_b.clone(), 2, 1, 80);
        fixture.reconcile();
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[10; 8]
        );
        let image_a = fixture
            .server
            .reserve_record::<ImageAsset>(&fixture.image_a)
            .unwrap();
        fixture
            .images
            .remove_with_state(image_a, &mut fixture.states, &mut fixture.events)
            .unwrap();
        fixture.reconcile();
        assert!(fixture.tilesets.get(tileset).is_none());
        fixture.request("b.png");
        fixture.reconcile();
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[80; 8]
        );
    }

    #[test]
    fn published_dependency_catalog_revocation_is_observed_while_desired_dependency_is_missing() {
        for mismatched_identity in [false, true] {
            let mut fixture = Fixture::new(TileSetAssetLimits::default());
            let tileset = fixture.request("a.png");
            fixture.reconcile();
            fixture.request("b.png");
            fixture.reconcile();
            fixture.database = ProjectAssetDatabase::default();
            fixture.database.insert(fixture.tileset.clone()).unwrap();
            if mismatched_identity {
                fixture
                    .database
                    .insert(record(
                        "10000000-0000-4000-8000-000000000099",
                        "a.png",
                        AssetSourceKind::Image,
                    ))
                    .unwrap();
            }
            fixture.reconcile();
            assert!(fixture.tilesets.get(tileset).is_none());
            assert!(
                fixture
                    .states
                    .state(tileset.id())
                    .unwrap()
                    .usable_version()
                    .is_none()
            );
        }
    }

    #[test]
    fn appearance_edits_keep_exact_image_content_and_image_edits_clear_it() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let original = fixture.tilesets.get(tileset).unwrap().clone();
        let changed = original
            .clone()
            .with_tint(nara_core::Color::rgb(0.5, 0.75, 1.0))
            .with_sampler(nara_material::SamplerDescriptor::NEAREST_CLAMP)
            .with_alpha_mode(nara_material::AlphaMode2d::Opaque);
        assert_eq!(changed.image_content(), original.image_content());
        let changed = changed.with_image(
            original.image_content().unwrap().handle(),
            original.atlas().unwrap(),
        );
        assert!(changed.image_content().is_none());
    }

    #[test]
    fn published_atlas_layout_requires_rebuild_and_cannot_be_changed_through_dependency_recreation()
    {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let altered = TileSetSource::new(
            AssetRef::path("a.png").unwrap(),
            TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
        );
        fixture
            .owner
            .request(
                fixture.tileset.clone(),
                altered,
                SourceHash::from_bytes(b"layout-change"),
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
        );
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .atlas()
                .unwrap()
                .columns,
            2
        );
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[10; 8]
        );
        let image = fixture
            .server
            .reserve_record::<ImageAsset>(&fixture.image_a)
            .unwrap();
        fixture
            .images
            .remove_with_state(image, &mut fixture.states, &mut fixture.events)
            .unwrap();
        fixture.reconcile();
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 60);
        fixture.reconcile();
        assert!(fixture.tilesets.get(tileset).is_none());
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
        );
        fixture.request("a.png");
        fixture.reconcile();
        assert_eq!(fixture.owner.error(tileset), None);
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[60; 8]
        );
    }

    #[test]
    fn source_revocation_preserves_the_published_layout_until_consumers_are_rebuilt() {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        fixture
            .owner
            .revoke_source(
                tileset,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events,
            )
            .unwrap();
        assert!(fixture.tilesets.get(tileset).is_none());

        let changed_layout = TileSetSource::new(
            AssetRef::path("a.png").unwrap(),
            TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
        );
        let restored_handle = fixture
            .owner
            .request(
                fixture.tileset.clone(),
                changed_layout,
                SourceHash::from_bytes(b"restored-with-changed-layout"),
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        fixture.reconcile();
        assert_eq!(restored_handle, tileset);
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
        );
        assert!(fixture.tilesets.get(tileset).is_none());

        assert_eq!(fixture.request("a.png"), tileset);
        fixture.reconcile();
        let restored = fixture.tilesets.get(tileset).unwrap();
        assert_eq!(fixture.owner.error(tileset), None);
        assert_eq!(restored.atlas().unwrap().columns, 2);
        assert_eq!(restored.image_content().unwrap().pixels(), &[10; 8]);
    }

    #[test]
    fn requesting_an_identical_source_retries_capacity_after_a_retained_generation_is_released() {
        let limits = TileSetAssetLimits::new(
            ItemLimit::ONE,
            ItemLimit::new(2).unwrap(),
            ByteLimit::new(16).unwrap(),
        );
        let mut fixture = Fixture::new(limits);
        let tileset = fixture.request("a.png");
        fixture.reconcile();
        let held_first = fixture
            .tilesets
            .get(tileset)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();

        fixture.publish_image(fixture.image_a.clone(), 2, 1, 20);
        fixture.reconcile();
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 2,
                bytes: 16
            }
        );
        fixture.publish_image(fixture.image_a.clone(), 2, 1, 30);
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(tileset),
            Some(&TileSetAssetError::RetentionCapacity)
        );
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[20; 8]
        );

        drop(held_first);
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 1,
                bytes: 8
            }
        );
        assert_eq!(fixture.request("a.png"), tileset);
        fixture.reconcile();
        assert_eq!(fixture.owner.error(tileset), None);
        assert_eq!(
            fixture
                .tilesets
                .get(tileset)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[30; 8]
        );
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 1,
                bytes: 8
            }
        );
    }
}
