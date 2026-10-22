use std::{
    collections::BTreeSet,
    sync::{Arc, Weak},
};

use nara_asset::{
    Asset, AssetEvents, AssetMutationBaseline, AssetRecord, AssetServer, AssetStates, AssetVersion,
    Assets, Handle, ProjectAssetDatabase, SourceHash,
};
use nara_image::{ImageAsset, ImageContentSnapshot};

use super::{DependencyObservation, RetainedImage, TileSetAssetError, TileSetAssets, TileSetEntry};
use crate::{TileSet, TileSetSource};

/// Observation of one exact desired-source claim, not residency or exclusive ownership by itself.
///
/// The project owner captures this only for a source it installed. Copied handles or receipts do
/// not prove that another residency holder has retired. Automatic dependency reconciliation keeps
/// the claim; another desired source or a superseding storage writer invalidates it.
#[derive(Debug, Clone)]
pub struct TileSetOwnedSourceReceipt {
    handle: Handle<TileSet>,
    claim: Weak<()>,
}

impl TileSetOwnedSourceReceipt {
    #[must_use]
    pub const fn handle(&self) -> Handle<TileSet> {
        self.handle
    }
}

/// One destination TileSet and the admitted immutable Image allocation it will consume.
pub struct TileSetPublicationInput {
    handle: Handle<TileSet>,
    record: AssetRecord,
    source: TileSetSource,
    source_hash: SourceHash,
    image_handle: Handle<ImageAsset>,
    image: ImageAsset,
    owned_source: Option<TileSetOwnedSourceReceipt>,
}

impl TileSetPublicationInput {
    /// Shares the same immutable candidate pixels for a retained owner-thread preparation.
    #[doc(hidden)]
    pub fn share_retained(&self) -> Option<Self> {
        Some(Self {
            handle: self.handle,
            record: self.record.clone(),
            source: self.source.clone(),
            source_hash: self.source_hash,
            image_handle: self.image_handle,
            image: self.image.share_retained()?,
            owned_source: self.owned_source.clone(),
        })
    }

    #[must_use]
    pub fn new(
        handle: Handle<TileSet>,
        record: AssetRecord,
        source: TileSetSource,
        source_hash: SourceHash,
        image_handle: Handle<ImageAsset>,
        image: ImageAsset,
    ) -> Self {
        Self {
            handle,
            record,
            source,
            source_hash,
            image_handle,
            image,
            owned_source: None,
        }
    }

    #[must_use]
    pub fn with_owned_source(mut self, receipt: TileSetOwnedSourceReceipt) -> Self {
        self.owned_source = Some(receipt);
        self
    }
}

struct SlotGuard<T: Asset> {
    handle: Handle<T>,
    baseline: AssetMutationBaseline<T>,
    next_version: AssetVersion,
}

impl<T: Asset> SlotGuard<T> {
    fn capture(
        handle: Handle<T>,
        assets: &Assets<T>,
        states: &AssetStates,
    ) -> Result<Self, TileSetAssetError> {
        Ok(Self {
            handle,
            baseline: assets
                .mutation_baseline(handle, states)
                .map_err(TileSetAssetError::State)?,
            next_version: states
                .next_version(handle.id())
                .map_err(TileSetAssetError::State)?,
        })
    }

    fn assert_current(&self, assets: &Assets<T>, states: &AssetStates) {
        self.baseline
            .validate(assets, states)
            .expect("prepared TileSet value/state association changed");
        assert_eq!(
            states
                .next_version(self.handle.id())
                .expect("prepared TileSet version remains available"),
            self.next_version
        );
    }
}

struct PreparedInput {
    input: TileSetPublicationInput,
    guard: SlotGuard<TileSet>,
    claim: Arc<()>,
    retention: usize,
}

enum PreparedRetention {
    Existing(Arc<RetainedImage>),
    New,
}

/// Synchronous reservation of one finite publication/retirement set.
///
/// The exclusive owner borrow prevents another request or reconciliation from spending the
/// checked entry and retained-pixel capacity. Preparation changes no desired sources, diagnostics,
/// states or publications. Dropping this plan releases its candidate Images and reservation.
/// Commit must follow the matching Image publication in the same exclusive World transaction.
#[must_use]
pub struct PreparedTileSetPublications<'owner> {
    owner: &'owner mut TileSetAssets,
    inputs: Vec<PreparedInput>,
    retirements: Vec<SlotGuard<TileSet>>,
    retained: Vec<PreparedRetention>,
}

impl TileSetAssets {
    /// Captures a source claim after the caller's own successful installation.
    #[doc(hidden)]
    pub fn owned_source_receipt(
        &self,
        handle: Handle<TileSet>,
        tilesets: &Assets<TileSet>,
        states: &AssetStates,
    ) -> Option<TileSetOwnedSourceReceipt> {
        let entry = self.entries.get(&handle.id())?;
        entry_is_current(entry, tilesets, states).then(|| TileSetOwnedSourceReceipt {
            handle,
            claim: Arc::downgrade(&entry.source_claim),
        })
    }

    /// Preflights a finite residency transition without publishing any source or Image.
    ///
    /// Handles must already be bound in the same staged server used for Scene decoding. Existing
    /// entries require their caller-owned source claim. Shared entries retain their atlas barrier;
    /// only an explicit, disjoint retirement can remove a source and allow fresh layout admission.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_publications<'owner>(
        &'owner mut self,
        inputs: Vec<TileSetPublicationInput>,
        retirements: &[TileSetOwnedSourceReceipt],
        server: &AssetServer,
        database: &ProjectAssetDatabase,
        tilesets: &Assets<TileSet>,
        states: &AssetStates,
    ) -> Result<PreparedTileSetPublications<'owner>, TileSetAssetError> {
        let mut ids = BTreeSet::new();
        let mut retired = Vec::new();
        for receipt in retirements {
            if !ids.insert(receipt.handle.id()) {
                return Err(TileSetAssetError::InvalidPublicationBatch);
            }
            let entry = self
                .entries
                .get(&receipt.handle.id())
                .ok_or(TileSetAssetError::Superseded)?;
            validate_claim(entry, receipt, tilesets, states)?;
            if !record_matches(receipt.handle, &entry.record, server) {
                return Err(TileSetAssetError::IdentityMismatch);
            }
            retired.push(SlotGuard::capture(receipt.handle, tilesets, states)?);
        }
        let mut entry_count = self.entries.len() - retired.len();
        let mut retention = self.retention();
        let mut prepared = Vec::<PreparedInput>::new();
        let mut retained = Vec::new();
        for input in inputs {
            if !ids.insert(input.handle.id()) {
                return Err(TileSetAssetError::InvalidPublicationBatch);
            }
            input
                .source
                .validate()
                .map_err(|_| TileSetAssetError::InvalidSource)?;
            if input.record.source_kind() != &crate::tileset_asset_source_kind() {
                return Err(TileSetAssetError::InvalidSourceKind);
            }
            if !record_matches(input.handle, &input.record, server)
                || database.record_for_path(input.record.path()) != Some(&input.record)
            {
                return Err(TileSetAssetError::IdentityMismatch);
            }
            let image_record = database
                .resolve_ref(&input.source.image)
                .map_err(|_| TileSetAssetError::DependencyUnavailable)?;
            if image_record.source_kind() != &nara_asset::AssetSourceKind::Image
                || !record_matches(input.image_handle, image_record, server)
                || input.image.source().path() != image_record.path()
                || input.image.source().stable_id() != image_record.stable_id()
            {
                return Err(TileSetAssetError::IdentityMismatch);
            }
            input
                .source
                .validate_image_extent(input.image.extent())
                .map_err(|_| TileSetAssetError::ImageExtentMismatch)?;
            let claim = if let Some(entry) = self.entries.get(&input.handle.id()) {
                validate_claim(
                    entry,
                    input
                        .owned_source
                        .as_ref()
                        .ok_or(TileSetAssetError::Superseded)?,
                    tilesets,
                    states,
                )?;
                if entry
                    .accepted_atlas
                    .is_some_and(|atlas| atlas != input.source.atlas)
                {
                    return Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild);
                }
                if entry.record == input.record
                    && entry.desired == input.source
                    && entry.source_hash == input.source_hash
                {
                    Arc::clone(&entry.source_claim)
                } else {
                    Arc::new(())
                }
            } else {
                if input.owned_source.is_some()
                    || tilesets.get(input.handle).is_some()
                    || states
                        .state(input.handle.id())
                        .is_some_and(|state| state.load_state() != &nara_asset::LoadState::Removed)
                {
                    return Err(TileSetAssetError::Superseded);
                }
                entry_count += 1;
                Arc::new(())
            };
            if entry_count > self.limits.entries.get() {
                return Err(TileSetAssetError::Capacity);
            }
            let retention_index = if let Some(prior) = prepared
                .iter()
                .find(|prior| same_pixels(prior.input.image.pixels(), input.image.pixels()))
            {
                prior.retention
            } else {
                let existing = self
                    .retained
                    .iter()
                    .filter_map(Weak::upgrade)
                    .find(|retained| same_pixels(retained.image.pixels(), input.image.pixels()));
                let reservation = match existing {
                    Some(existing) => PreparedRetention::Existing(existing),
                    None => {
                        retention.images = retention
                            .images
                            .checked_add(1)
                            .ok_or(TileSetAssetError::RetentionCapacity)?;
                        retention.bytes = retention
                            .bytes
                            .checked_add(input.image.pixels().len())
                            .ok_or(TileSetAssetError::RetentionCapacity)?;
                        if retention.images > self.limits.retained_images.get()
                            || retention.bytes > self.limits.retained_bytes.get()
                        {
                            return Err(TileSetAssetError::RetentionCapacity);
                        }
                        PreparedRetention::New
                    }
                };
                retained.push(reservation);
                retained.len() - 1
            };
            let guard = SlotGuard::capture(input.handle, tilesets, states)?;
            prepared.push(PreparedInput {
                input,
                guard,
                claim,
                retention: retention_index,
            });
        }
        Ok(PreparedTileSetPublications {
            owner: self,
            inputs: prepared,
            retirements: retired,
            retained,
        })
    }
}

impl PreparedTileSetPublications<'_> {
    /// Commits the prevalidated transition after its exact Images and staged server are published.
    /// Violating that synchronous commit contract is an invariant fault, not a recoverable result.
    pub fn commit(
        self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        tilesets: &mut Assets<TileSet>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Vec<TileSetOwnedSourceReceipt> {
        let mut captured = Vec::with_capacity(self.inputs.len());
        for prepared in &self.inputs {
            prepared.guard.assert_current(tilesets, states);
            let input = &prepared.input;
            assert!(
                record_matches(input.handle, &input.record, server),
                "prepared TileSet identity changed"
            );
            let image = ImageContentSnapshot::capture(input.image_handle, server, states, images)
                .expect("prepared destination Image must be published before its TileSet");
            assert!(
                same_pixels(image.pixels(), input.image.pixels()),
                "prepared destination Image allocation changed"
            );
            assert_eq!(
                image.source(),
                input.image.source(),
                "prepared destination Image source changed"
            );
            assert_eq!(image.extent(), input.image.extent());
            assert_eq!(image.format(), input.image.format());
            captured.push(image);
        }
        for retirement in &self.retirements {
            retirement.assert_current(tilesets, states);
            let entry = self
                .owner
                .entries
                .get(&retirement.handle.id())
                .expect("borrowed TileSet owner retains its entry");
            assert!(
                record_matches(retirement.handle, &entry.record, server),
                "prepared TileSet retirement identity changed"
            );
        }
        let mut retention_owners = self
            .retained
            .into_iter()
            .map(|reservation| match reservation {
                PreparedRetention::Existing(owner) => Some(owner),
                PreparedRetention::New => None,
            })
            .collect::<Vec<_>>();
        // Explicit, source-scoped unload removes the desired entry, so TaskUpdate cannot revive it.
        for retirement in self.retirements {
            retirement
                .baseline
                .remove(tilesets, states, events)
                .expect("prepared TileSet retirement version remains available");
            self.owner.entries.remove(&retirement.handle.id());
        }
        self.owner
            .retained
            .retain(|retained| retained.strong_count() != 0);
        let mut receipts = Vec::with_capacity(self.inputs.len());
        for (prepared, image) in self.inputs.into_iter().zip(captured) {
            let owner = retention_owners[prepared.retention].get_or_insert_with(|| {
                let owner = Arc::new(RetainedImage {
                    image: image.clone(),
                });
                self.owner.retained.push(Arc::downgrade(&owner));
                owner
            });
            let image = image.with_retention_owner(Arc::clone(owner));
            let TileSetPublicationInput {
                handle,
                record,
                source,
                source_hash,
                image_handle,
                ..
            } = prepared.input;
            let tileset = source
                .clone()
                .into_tileset(image_handle)
                .expect("prepared TileSet source remains valid")
                .with_image_content(image.clone());
            prepared
                .guard
                .baseline
                .commit_loaded(tileset, tilesets, states, events, Some(source_hash), None)
                .expect("prepared TileSet publication version remains available");
            let observation = DependencyObservation {
                record: Some(nara_asset::AssetRecord::new(
                    image.source().stable_id(),
                    image.source().path().clone(),
                    nara_asset::AssetSourceKind::Image,
                )),
                state: Some(states.state_revision(image_handle.id())),
                slot: Some(images.slot_revision(image_handle)),
            };
            receipts.push(TileSetOwnedSourceReceipt {
                handle,
                claim: Arc::downgrade(&prepared.claim),
            });
            self.owner.entries.insert(
                handle.id(),
                TileSetEntry {
                    source_claim: prepared.claim,
                    handle,
                    record,
                    accepted_atlas: Some(source.atlas),
                    desired: source,
                    source_hash,
                    source_active: true,
                    published: Some(image),
                    observation: Some(observation),
                    slot: tilesets.slot_revision(handle),
                    state: states.state_revision(handle.id()),
                    error: None,
                },
            );
        }
        receipts
    }
}

fn validate_claim(
    entry: &TileSetEntry,
    receipt: &TileSetOwnedSourceReceipt,
    tilesets: &Assets<TileSet>,
    states: &AssetStates,
) -> Result<(), TileSetAssetError> {
    if receipt.handle != entry.handle
        || !Weak::ptr_eq(&receipt.claim, &Arc::downgrade(&entry.source_claim))
        || !entry_is_current(entry, tilesets, states)
    {
        return Err(TileSetAssetError::Superseded);
    }
    Ok(())
}

fn entry_is_current(
    entry: &TileSetEntry,
    tilesets: &Assets<TileSet>,
    states: &AssetStates,
) -> bool {
    entry.error != Some(TileSetAssetError::Superseded)
        && entry.slot == tilesets.slot_revision(entry.handle)
        && entry.state == states.state_revision(entry.handle.id())
}

fn record_matches<T: 'static>(
    handle: Handle<T>,
    record: &AssetRecord,
    server: &AssetServer,
) -> bool {
    server.get_handle::<T>(handle.id()) == Some(handle)
        && server.path(handle.id()) == Some(record.path().as_str())
        && server.stable_id(handle.id()) == Some(record.stable_id())
}

fn same_pixels(left: &[u8], right: &[u8]) -> bool {
    // ImageAsset admits nonempty, immutable, fixed-length RGBA allocations; a live slice identity
    // therefore proves the same pixel allocation, rather than equal content in another allocation.
    std::ptr::eq(left, right)
}
