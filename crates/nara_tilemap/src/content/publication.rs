use std::{
    collections::BTreeSet,
    sync::{Arc, Weak},
};

use nara_asset::{
    Asset, AssetEvents, AssetMutationBaseline, AssetRecord, AssetServer, AssetStates, AssetVersion,
    Assets, Handle, ProjectAssetDatabase, SourceHash,
};
use nara_image::{ImageAsset, ImageContentSnapshot};

use super::{
    DependencyObservation, RetainedImage, TileSetAssetError, TileSetAssets, TileSetDeclaration,
    TileSetEntry,
};
use crate::{TileSet, TileSetDescriptor, TileSetSource};

/// Observation of one exact content declaration claim, not residency or exclusive ownership itself.
///
/// The content owner captures this only for a declaration it installed. Copied handles or receipts do
/// not prove that another residency holder has retired. Automatic dependency reconciliation keeps
/// the claim; another declaration or a superseding storage writer invalidates it.
#[derive(Debug, Clone)]
pub struct TileSetContentReceipt {
    handle: Handle<TileSet>,
    claim: Weak<()>,
}

impl TileSetContentReceipt {
    #[must_use]
    pub const fn handle(&self) -> Handle<TileSet> {
        self.handle
    }
}

/// One destination TileSet and the admitted immutable Image allocation it will consume.
pub struct TileSetPublicationInput {
    handle: Handle<TileSet>,
    declaration: TileSetDeclaration,
    image_handle: Handle<ImageAsset>,
    image: ImageAsset,
    receipt: Option<TileSetContentReceipt>,
}

impl TileSetPublicationInput {
    /// Shares the same immutable candidate pixels for a retained owner-thread preparation.
    pub fn share_retained(&self) -> Option<Self> {
        Some(Self {
            handle: self.handle,
            declaration: self.declaration.clone(),
            image_handle: self.image_handle,
            image: self.image.share_retained()?,
            receipt: self.receipt.clone(),
        })
    }

    #[must_use]
    pub fn from_source(
        handle: Handle<TileSet>,
        record: AssetRecord,
        source: TileSetSource,
        source_hash: SourceHash,
        image_handle: Handle<ImageAsset>,
        image: ImageAsset,
    ) -> Self {
        Self {
            handle,
            declaration: TileSetDeclaration::Source {
                record,
                source,
                source_hash,
            },
            image_handle,
            image,
            receipt: None,
        }
    }

    /// Supplies processed content and its actual Image without source records or import hashes.
    #[must_use]
    pub fn from_product(
        handle: Handle<TileSet>,
        descriptor: TileSetDescriptor,
        image_handle: Handle<ImageAsset>,
        image: ImageAsset,
    ) -> Self {
        Self {
            handle,
            declaration: TileSetDeclaration::Product(descriptor),
            image_handle,
            image,
            receipt: None,
        }
    }

    #[must_use]
    pub fn with_receipt(mut self, receipt: TileSetContentReceipt) -> Self {
        self.receipt = Some(receipt);
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
                .prepare_mutation(handle, states)
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
    image_record: Option<AssetRecord>,
    retention: usize,
}

enum PreparedRetention {
    Existing(Arc<RetainedImage>),
    New,
}

/// Synchronous reservation of one finite publication/retirement set.
///
/// The exclusive owner borrow prevents another request or reconciliation from spending the
/// checked entry and retained-pixel capacity. Preparation changes no declarations, diagnostics,
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
    /// Captures a declaration claim after the caller's own successful installation.
    pub fn content_receipt(
        &self,
        handle: Handle<TileSet>,
        tilesets: &Assets<TileSet>,
        states: &AssetStates,
    ) -> Option<TileSetContentReceipt> {
        let entry = self.entries.get(&handle.id())?;
        entry_is_current(entry, tilesets, states).then(|| TileSetContentReceipt {
            handle,
            claim: Arc::downgrade(&entry.claim),
        })
    }

    /// Preflights a finite residency transition without publishing any TileSet or Image.
    ///
    /// Handles must already be bound in the same staged server used for Scene decoding. Existing
    /// entries require their caller-owned declaration claim. Shared entries retain their atlas
    /// barrier; only an explicit, disjoint retirement can allow fresh layout admission.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_publications<'owner>(
        &'owner mut self,
        inputs: Vec<TileSetPublicationInput>,
        retirements: &[TileSetContentReceipt],
        server: &AssetServer,
        database: Option<&ProjectAssetDatabase>,
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
            if !entry.declaration.binding_matches(receipt.handle, server) {
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
            input.declaration.validate()?;
            if !input
                .declaration
                .is_admitted(input.handle, server, database)
            {
                return Err(TileSetAssetError::IdentityMismatch);
            }
            input.declaration.validate_image_identity(
                input.image_handle,
                input
                    .image
                    .product()
                    .ok_or(TileSetAssetError::IdentityMismatch)?,
                input.image.source(),
                server,
                database,
            )?;
            input
                .declaration
                .validate_image_extent(input.image.extent())?;
            let claim = if let Some(entry) = self.entries.get(&input.handle.id()) {
                validate_claim(
                    entry,
                    input
                        .receipt
                        .as_ref()
                        .ok_or(TileSetAssetError::Superseded)?,
                    tilesets,
                    states,
                )?;
                if entry
                    .accepted_atlas
                    .is_some_and(|atlas| atlas != input.declaration.atlas())
                {
                    return Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild);
                }
                if entry.declaration == input.declaration {
                    Arc::clone(&entry.claim)
                } else {
                    Arc::new(())
                }
            } else {
                if input.receipt.is_some()
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
            let image_record = input.declaration.image_record(database).cloned();
            prepared.push(PreparedInput {
                input,
                guard,
                claim,
                image_record,
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
    ) -> Vec<TileSetContentReceipt> {
        let mut captured = Vec::with_capacity(self.inputs.len());
        for prepared in &self.inputs {
            prepared.guard.assert_current(tilesets, states);
            let input = &prepared.input;
            assert!(
                input.declaration.binding_matches(input.handle, server),
                "prepared TileSet identity changed"
            );
            let image = ImageContentSnapshot::capture(input.image_handle, server, states, images)
                .expect("prepared destination Image must be published before its TileSet");
            assert!(
                same_pixels(image.pixels(), input.image.pixels()),
                "prepared destination Image allocation changed"
            );
            assert_eq!(
                Some(image.product()),
                input.image.product(),
                "prepared destination Image product changed"
            );
            assert_eq!(
                image.source(),
                input.image.source(),
                "prepared destination Image source changed"
            );
            assert_eq!(image.extent(), input.image.extent());
            assert_eq!(image.format(), input.image.format());
            assert_eq!(image.color_space(), input.image.color_space());
            input
                .declaration
                .validate_image_extent(image.extent())
                .expect("prepared TileSet descriptor still matches its Image");
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
                entry.declaration.binding_matches(retirement.handle, server),
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
        // Explicit retirement removes the declaration, so TaskUpdate cannot revive it.
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
                declaration,
                image_handle,
                ..
            } = prepared.input;
            let tileset = declaration
                .clone()
                .into_tileset(image_handle)
                .expect("prepared TileSet declaration remains valid")
                .with_image_content(image.clone());
            prepared
                .guard
                .baseline
                .commit_loaded(
                    tileset,
                    tilesets,
                    states,
                    events,
                    declaration.source_hash(),
                    None,
                )
                .expect("prepared TileSet publication version remains available");
            let observation = DependencyObservation::capture(
                prepared.image_record,
                Some(image_handle),
                images,
                states,
            );
            receipts.push(TileSetContentReceipt {
                handle,
                claim: Arc::downgrade(&prepared.claim),
            });
            self.owner.entries.insert(
                handle.id(),
                TileSetEntry {
                    claim: prepared.claim,
                    handle,
                    accepted_atlas: Some(declaration.atlas()),
                    declaration,
                    active: true,
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
    receipt: &TileSetContentReceipt,
    tilesets: &Assets<TileSet>,
    states: &AssetStates,
) -> Result<(), TileSetAssetError> {
    if receipt.handle != entry.handle
        || !Weak::ptr_eq(&receipt.claim, &Arc::downgrade(&entry.claim))
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

fn same_pixels(left: &[u8], right: &[u8]) -> bool {
    // ImageAsset admits nonempty, immutable, fixed-length RGBA allocations; a live slice identity
    // therefore proves the same pixel allocation, rather than equal content in another allocation.
    std::ptr::eq(left, right)
}
