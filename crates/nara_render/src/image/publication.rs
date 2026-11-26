use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{
    AssetEvents, AssetMutationBaseline, AssetProductRef, AssetPublicationRevision, AssetServer,
    AssetSlotRevision, AssetSourcePath, AssetStateRevision, AssetStates, Assets, Handle,
    ImportedProductId, LoadState,
};

use super::{ImageAsset, content::catalog_matches};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageBinding {
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
    source_path: Option<AssetSourcePath>,
}

impl ImageBinding {
    fn matches(&self, server: &AssetServer) -> bool {
        catalog_matches(self.handle, self.product, server)
            && self.source_path.as_ref().is_none_or(|path| {
                self.product.product() == ImportedProductId::PRIMARY
                    && server.path(self.handle.id()) == Some(path)
            })
    }
}

/// One authorized Image value and its destination in the staged asset catalog.
pub struct ImagePublicationInput {
    binding: ImageBinding,
    image: ImageAsset,
}

impl ImagePublicationInput {
    /// A source input supplies its canonical catalog path. A cooked input needs only its Product.
    #[must_use]
    pub fn new(
        handle: Handle<ImageAsset>,
        product: AssetProductRef,
        source_path: Option<AssetSourcePath>,
        image: ImageAsset,
    ) -> Self {
        Self {
            binding: ImageBinding {
                handle,
                product,
                source_path,
            },
            image,
        }
    }
}

/// Evidence of the exact Image publication installed by one successful transaction.
#[derive(Debug, Clone)]
pub struct ImageContentReceipt {
    binding: ImageBinding,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    publication: AssetPublicationRevision,
}

impl ImageContentReceipt {
    fn capture(binding: ImageBinding, images: &Assets<ImageAsset>, states: &AssetStates) -> Self {
        let handle = binding.handle;
        Self {
            binding,
            slot: images.slot_revision(handle),
            state: states.state_revision(handle.id()),
            publication: states.publication_revision(handle.id()),
        }
    }

    #[must_use]
    pub const fn handle(&self) -> Handle<ImageAsset> {
        self.binding.handle
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.binding.product
    }

    fn matches(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        states.state_revision(self.handle().id()) == self.state
            && self.matches_slot(server, images, states)
    }

    fn matches_slot(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        let handle = self.handle();
        states.published_slot_revision(handle.id()) == Some(&self.slot)
            && images.slot_revision(handle) == self.slot
            && images
                .get(handle)
                .is_some_and(|image| image.product() == Some(self.product()))
            && self.binding.matches(server)
            && states
                .state(handle.id())
                .is_some_and(|state| state.usable_version().is_some())
    }

    /// Checks that the receipt still owns the live value and publication. A failed reload can
    /// leave the original successful publication eligible for replacement or retirement.
    pub fn validate_retirement(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(), ImagePublicationError> {
        let id = self.handle().id();
        let owned = self.matches_slot(server, images, states)
            && states.publication_revision(id) == self.publication
            && (states.state_revision(id) == self.state
                || states
                    .state(id)
                    .is_some_and(|state| matches!(state.load_state(), LoadState::Failed { .. })));
        if owned {
            Ok(())
        } else {
            Err(ImagePublicationError::Superseded)
        }
    }
}

/// Determines how a transition treats the supplied receipts for existing Images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePublicationMode {
    /// Republish matching destinations and retire the other supplied receipts.
    Replace,
    /// Reuse exact matching values and leave all other existing publications alone.
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImagePublicationError {
    #[error("image publication has a conflicting destination or product identity")]
    IdentityMismatch,
    #[error("image publication no longer owns its asset binding or value/state association")]
    Superseded,
    #[error("image publication version is exhausted")]
    VersionExhausted,
}

struct PreparedImagePublication {
    input: ImagePublicationInput,
    baseline: AssetMutationBaseline<ImageAsset>,
}

/// Prepared value/state cuts for a finite Image publication, reuse and retirement batch.
#[must_use]
pub struct PreparedImagePublications {
    publish: BTreeMap<AssetProductRef, PreparedImagePublication>,
    retire: Vec<ImageContentReceipt>,
    shared: Vec<ImageContentReceipt>,
}

impl PreparedImagePublications {
    pub fn empty() -> Self {
        Self {
            publish: BTreeMap::new(),
            retire: Vec::new(),
            shared: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.retire.is_empty() && self.shared.is_empty()
    }

    /// Preflights the batch without changing any asset value, state or event.
    ///
    /// Existing receipts refer to the live catalog. Destinations must already be bound in the
    /// staged catalog. The two catalogs can be the same for a transaction without new bindings.
    pub fn prepare<'receipt>(
        inputs: Vec<ImagePublicationInput>,
        owned: impl Iterator<Item = &'receipt ImageContentReceipt> + Clone,
        mode: ImagePublicationMode,
        live_server: &AssetServer,
        staged_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, ImagePublicationError> {
        let mut prepared = Self::empty();
        let mut owned_by_id = BTreeMap::new();
        for receipt in owned.clone() {
            if owned_by_id.insert(receipt.handle().id(), receipt).is_some() {
                return Err(ImagePublicationError::IdentityMismatch);
            }
            if mode == ImagePublicationMode::Replace {
                receipt.validate_retirement(live_server, images, states)?;
            }
        }
        let mut next_ids = BTreeSet::new();
        for input in inputs {
            let binding = &input.binding;
            let id = binding.handle.id();
            if input.image.product() != Some(binding.product)
                || input.image.source().map(|source| source.path()) != binding.source_path.as_ref()
                || !binding.matches(staged_server)
                || !next_ids.insert(id)
            {
                return Err(ImagePublicationError::IdentityMismatch);
            }
            let original = owned_by_id
                .get(&id)
                .copied()
                .filter(|receipt| receipt.product() == binding.product);
            if mode == ImagePublicationMode::Add
                && let Some(receipt) = original
            {
                if receipt.binding != *binding
                    || !receipt.matches(live_server, images, states)
                    || images.get(binding.handle).is_none_or(|resident| {
                        resident.content_hash() != input.image.content_hash()
                            || resident.source() != input.image.source()
                    })
                {
                    return Err(ImagePublicationError::Superseded);
                }
                prepared.shared.push(receipt.clone());
                continue;
            }
            if original.is_none()
                && (images.get(binding.handle).is_some()
                    || states
                        .state(id)
                        .is_some_and(|state| !matches!(state.load_state(), LoadState::Removed)))
            {
                return Err(ImagePublicationError::Superseded);
            }
            let baseline = prepare_mutation(binding.handle, images, states)?;
            prepared.publish.insert(
                binding.product,
                PreparedImagePublication { input, baseline },
            );
        }
        if mode == ImagePublicationMode::Replace {
            for receipt in owned.filter(|receipt| !next_ids.contains(&receipt.handle().id())) {
                if !receipt.binding.matches(staged_server) {
                    return Err(ImagePublicationError::Superseded);
                }
                prepare_mutation(receipt.handle(), images, states)?;
                prepared.retire.push(receipt.clone());
            }
        }
        Ok(prepared)
    }

    /// Returns the Products used by the batch, including any shared publications.
    #[must_use]
    pub fn additive_products(&self) -> Vec<AssetProductRef> {
        self.publish
            .keys()
            .copied()
            .chain(self.shared.iter().map(ImageContentReceipt::product))
            .collect()
    }

    pub fn retiring_products(&self) -> impl Iterator<Item = AssetProductRef> + '_ {
        self.retire.iter().map(ImageContentReceipt::product)
    }

    #[must_use]
    pub fn handle_for_product(&self, product: AssetProductRef) -> Option<Handle<ImageAsset>> {
        self.publish
            .get(&product)
            .map(|item| item.input.binding.handle)
            .or_else(|| {
                self.shared_receipt(product)
                    .map(ImageContentReceipt::handle)
            })
    }

    /// Returns the exact planned pixels for consumers that keep the Image's retention owner.
    #[must_use]
    pub fn retained_image_for_product(
        &self,
        product: AssetProductRef,
        images: &Assets<ImageAsset>,
    ) -> Option<(Handle<ImageAsset>, ImageAsset)> {
        if let Some(item) = self.publish.get(&product) {
            return Some((
                item.input.binding.handle,
                item.input.image.share_retained()?,
            ));
        }
        let receipt = self.shared_receipt(product)?;
        let handle = receipt.handle();
        if images.slot_revision(handle) != receipt.slot {
            return None;
        }
        Some((handle, images.get(handle)?.share_retained()?))
    }

    fn shared_receipt(&self, product: AssetProductRef) -> Option<&ImageContentReceipt> {
        self.shared
            .iter()
            .find(|receipt| receipt.product() == product)
    }

    /// Rechecks store baselines and existing receipts before publishing the staged catalog.
    pub fn validate(
        &self,
        live_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(), ImagePublicationError> {
        for item in self.publish.values() {
            item.baseline
                .validate(images, states)
                .map_err(|_| ImagePublicationError::Superseded)?;
        }
        for receipt in &self.retire {
            receipt.validate_retirement(live_server, images, states)?;
            prepare_mutation(receipt.handle(), images, states)?;
        }
        if self
            .shared
            .iter()
            .any(|receipt| !receipt.matches(live_server, images, states))
        {
            return Err(ImagePublicationError::Superseded);
        }
        Ok(())
    }

    /// Validates the complete batch against the installed catalog before its first mutation.
    /// Returns receipts only for new publications. Shared Images keep their original receipts.
    pub fn commit(
        self,
        server: &AssetServer,
        images: &mut Assets<ImageAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<Vec<ImageContentReceipt>, ImagePublicationError> {
        self.validate(server, images, states)?;
        if self
            .publish
            .values()
            .any(|item| !item.input.binding.matches(server))
        {
            return Err(ImagePublicationError::Superseded);
        }
        let mut receipts = Vec::with_capacity(self.publish.len());
        for item in self.publish.into_values() {
            let ImagePublicationInput { binding, image } = item.input;
            let source_hash = image.source().map(|source| source.source_hash());
            let recipe = image.source().map(|source| source.import_recipe().digest());
            item.baseline
                .commit_loaded(image, images, states, events, source_hash, recipe)
                .expect("preflighted Image publication remains valid in this disjoint batch");
            receipts.push(ImageContentReceipt::capture(binding, images, states));
        }
        for receipt in self.retire {
            images
                .remove_with_state(receipt.handle(), states, events)
                .expect("preflighted Image retirement remains valid in this disjoint batch");
        }
        Ok(receipts)
    }
}

fn prepare_mutation(
    handle: Handle<ImageAsset>,
    images: &Assets<ImageAsset>,
    states: &AssetStates,
) -> Result<AssetMutationBaseline<ImageAsset>, ImagePublicationError> {
    states
        .next_version(handle.id())
        .map_err(|_| ImagePublicationError::VersionExhausted)?;
    images
        .prepare_mutation(handle, states)
        .map_err(|_| ImagePublicationError::Superseded)
}
