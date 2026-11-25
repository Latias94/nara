use std::{
    collections::BTreeSet,
    sync::{Arc, Weak},
};

use crate::image::{ImageAsset, ImageContentSnapshot};
use nara_asset::{
    Asset, AssetEvents, AssetMutationBaseline, AssetPathScope, AssetRecord, AssetServer,
    AssetStateRevision, AssetStates, AssetVersion, Assets, Handle, ProjectAssetDatabase,
    SourceHash,
};
use nara_core::ByteLimit;

use super::{
    DependencyObservation, RetainedImage, TileSetAssetError, TileSetAssets, TileSetDeclaration,
    TileSetEntry, same_pixels,
};
use crate::tilemap::{TileSet, TileSetDescriptor, TileSetSource};

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
/// Constructors select publication; an explicit shared receipt selects reuse without publication.
pub struct TileSetPublicationInput {
    handle: Handle<TileSet>,
    declaration: TileSetDeclaration,
    image_handle: Handle<ImageAsset>,
    image: ImageAsset,
    receipt: Option<PublicationReceipt>,
}

#[derive(Clone)]
enum PublicationReceipt {
    Replace(TileSetContentReceipt),
    Shared(TileSetContentReceipt),
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

    /// Binds the source dependency before retaining an input. The caller admits this input's
    /// canonical path allowance; preparation also checks the receiving owner's actual limit.
    #[allow(clippy::too_many_arguments)]
    pub fn from_source(
        handle: Handle<TileSet>,
        record: AssetRecord,
        source: TileSetSource,
        source_hash: SourceHash,
        image_handle: Handle<ImageAsset>,
        image: ImageAsset,
        scope: &AssetPathScope,
        source_reference_bytes: ByteLimit,
    ) -> Result<Self, TileSetAssetError> {
        Ok(Self {
            handle,
            declaration: TileSetDeclaration::from_source(
                record,
                source,
                source_hash,
                scope,
                source_reference_bytes,
            )?,
            image_handle,
            image,
            receipt: None,
        })
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

    /// Replaces an existing declaration under its exact claim, publishing a new TileSet version.
    #[must_use]
    pub fn with_receipt(mut self, receipt: TileSetContentReceipt) -> Self {
        self.receipt = Some(PublicationReceipt::Replace(receipt));
        self
    }

    /// Reuses the exact live declaration and Image publication under the original claim.
    ///
    /// The complete declaration and retained Image allocation must match the original publication.
    /// Preparation reserves no new version or pixel capacity. Commit revalidates the original
    /// value/state and Image baselines, returns this receipt, and leaves the publication unchanged.
    #[must_use]
    pub fn with_shared_receipt(mut self, receipt: TileSetContentReceipt) -> Self {
        self.receipt = Some(PublicationReceipt::Shared(receipt));
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

struct PreparedPublication {
    input: TileSetPublicationInput,
    guard: SlotGuard<TileSet>,
    claim: Arc<()>,
    image_record: Option<AssetRecord>,
    retention: usize,
}

struct PreparedShared {
    receipt: TileSetContentReceipt,
    baseline: AssetMutationBaseline<TileSet>,
    image: ImageContentSnapshot,
    image_state: AssetStateRevision,
}

enum PreparedInput {
    Publish(PreparedPublication),
    Shared(PreparedShared),
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
        entry
            .owned_state_revision(tilesets, states)
            .map(|_| TileSetContentReceipt {
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
            input
                .declaration
                .validate_source_reference_limit(self.limits.source_reference_bytes())?;
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
                let receipt = match input.receipt.as_ref() {
                    Some(
                        PublicationReceipt::Replace(receipt) | PublicationReceipt::Shared(receipt),
                    ) => receipt,
                    None => return Err(TileSetAssetError::Superseded),
                };
                validate_claim(entry, receipt, tilesets, states)?;
                if entry
                    .accepted_atlas
                    .is_some_and(|atlas| atlas != input.declaration.atlas())
                {
                    return Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild);
                }
                if matches!(input.receipt.as_ref(), Some(PublicationReceipt::Shared(_))) {
                    let image = entry
                        .published
                        .as_ref()
                        .ok_or(TileSetAssetError::DependencyUnavailable)?;
                    if !entry.active
                        || entry.error.is_some()
                        || !entry.declaration.same_declaration(&input.declaration)
                        || !states.state(input.handle.id()).is_some_and(|state| {
                            state.load_state() == &nara_asset::LoadState::Loaded
                                && state.usable_version().is_some()
                                && state.source_hash() == entry.declaration.source_hash()
                        })
                        || states.published_slot_revision(input.handle.id()) != Some(&entry.slot)
                        || tilesets.get(input.handle).and_then(TileSet::image_content)
                            != Some(image)
                        || states
                            .state(image.handle().id())
                            .and_then(|state| state.usable_version())
                            != Some(image.version())
                        || states.published_slot_revision(image.handle().id())
                            != Some(image.slot_revision())
                        || server.get_handle::<ImageAsset>(image.handle().id())
                            != Some(image.handle())
                        || server.product(image.handle().id()) != Some(image.product())
                        || !image_matches_input(image, &input)
                    {
                        return Err(TileSetAssetError::Superseded);
                    }
                    prepared.push(PreparedInput::Shared(PreparedShared {
                        receipt: receipt.clone(),
                        baseline: tilesets
                            .prepare_mutation(input.handle, states)
                            .map_err(TileSetAssetError::State)?,
                        image: image.clone(),
                        image_state: states.state_revision(image.handle().id()),
                    }));
                    continue;
                }
                if entry.declaration.same_declaration(&input.declaration) {
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
                .filter_map(|prior| match prior {
                    PreparedInput::Publish(prior) => Some(prior),
                    PreparedInput::Shared(_) => None,
                })
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
            prepared.push(PreparedInput::Publish(PreparedPublication {
                input,
                guard,
                claim,
                image_record,
                retention: retention_index,
            }));
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
            let prepared = match prepared {
                PreparedInput::Publish(prepared) => prepared,
                PreparedInput::Shared(shared) => {
                    shared
                        .baseline
                        .validate(tilesets, states)
                        .expect("prepared shared TileSet value/state association changed");
                    let entry = self
                        .owner
                        .entries
                        .get(&shared.receipt.handle.id())
                        .expect("borrowed TileSet owner retains its shared entry");
                    assert!(
                        entry
                            .declaration
                            .binding_matches(shared.receipt.handle, server),
                        "prepared shared TileSet identity changed"
                    );
                    assert_eq!(
                        states.state_revision(shared.image.handle().id()),
                        shared.image_state,
                        "prepared shared Image state changed"
                    );
                    let image = ImageContentSnapshot::capture(
                        shared.image.handle(),
                        server,
                        states,
                        images,
                    )
                    .expect("prepared shared Image must remain published");
                    assert_eq!(
                        image, shared.image,
                        "prepared shared Image publication changed"
                    );
                    captured.push(image);
                    continue;
                }
            };
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
            let prepared = match prepared {
                PreparedInput::Publish(prepared) => prepared,
                PreparedInput::Shared(shared) => {
                    receipts.push(shared.receipt);
                    continue;
                }
            };
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
        || entry.owned_state_revision(tilesets, states).is_none()
    {
        return Err(TileSetAssetError::Superseded);
    }
    Ok(())
}

fn image_matches_input(image: &ImageContentSnapshot, input: &TileSetPublicationInput) -> bool {
    image.handle() == input.image_handle
        && Some(image.product()) == input.image.product()
        && image.source() == input.image.source()
        && image.extent() == input.image.extent()
        && image.format() == input.image.format()
        && image.color_space() == input.image.color_space()
        && same_pixels(image.pixels(), input.image.pixels())
}
