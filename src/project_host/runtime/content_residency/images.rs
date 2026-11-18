use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::{
    AssetEvents, AssetMutationBaseline, AssetProductRef, AssetPublicationRevision, AssetRecord,
    AssetRef, AssetServer, AssetSlotRevision, AssetSourceKind, AssetStateRevision, AssetStates,
    Assets, Handle, ImportedProductId, LoadState,
};
use nara_image::ImageAsset;
use nara_scene::advanced::SceneAssetReservations;

use super::super::{HostFault, single_error};
use super::ResidencyInput;
use crate::runtime_content::{RuntimeContentAsset, RuntimeContentLease};

#[cfg(test)]
mod tests;

struct RuntimeImageOwner {
    _acquired: RuntimeContentAsset<ImageAsset>,
    // Escaped pixels retain the wrapper reservation as well as the original decoded output.
    _metadata: Arc<RuntimeContentLease>,
}

#[derive(Clone)]
pub(super) struct OwnedImage {
    pub(super) handle: Handle<ImageAsset>,
    pub(super) product: AssetProductRef,
    pub(super) record: Option<AssetRecord>,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    publication: AssetPublicationRevision,
}

impl OwnedImage {
    pub(super) fn capture(
        handle: Handle<ImageAsset>,
        product: AssetProductRef,
        record: Option<AssetRecord>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Self {
        Self {
            handle,
            product,
            record,
            slot: images.slot_revision(handle),
            state: states.state_revision(handle.id()),
            publication: states.publication_revision(handle.id()),
        }
    }

    fn matches(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        states.state_revision(self.handle.id()) == self.state
            && self.matches_slot(server, images, states)
    }

    fn matches_slot(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        images
            .get(self.handle)
            .is_some_and(|image| image.product() == Some(self.product))
            && binding_matches(self.handle, self.product, self.record.as_ref(), server)
            && images.slot_revision(self.handle) == self.slot
            && states.published_slot_revision(self.handle.id()) == Some(&self.slot)
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_some())
    }
}

pub(super) struct ImagePublication {
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
    record: Option<AssetRecord>,
    image: ImageAsset,
    baseline: AssetMutationBaseline<ImageAsset>,
}

/// Finite CPU publications whose only fallible version advances were checked before Scene commit.
pub(super) struct ImagePublications {
    publish: BTreeMap<AssetProductRef, ImagePublication>,
    retire: Vec<OwnedImage>,
    shared: Vec<OwnedImage>,
}

#[derive(Clone, Copy)]
pub(super) enum ImageChange<'a> {
    Replace(&'a [OwnedImage]),
    Add(&'a [OwnedImage]),
}

impl ImagePublications {
    pub(super) fn empty() -> Self {
        Self {
            publish: BTreeMap::new(),
            retire: Vec::new(),
            shared: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.retire.is_empty() && self.shared.is_empty()
    }
    pub(super) fn prepare(
        content: ResidencyInput<'_>,
        change: ImageChange<'_>,
        bindings: &mut SceneAssetReservations,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
        runtime_lease: Option<&Arc<RuntimeContentLease>>,
    ) -> Result<Self, HostFault> {
        let (owned, additive) = match change {
            ImageChange::Replace(owned) => (owned, false),
            ImageChange::Add(shared) => (shared, true),
        };
        if !additive
            && owned
                .iter()
                .any(|image| !image.matches(server, images, states))
        {
            return Err(superseded());
        }
        let owned_by_id = owned
            .iter()
            .map(|image| (image.handle.id(), image))
            .collect::<BTreeMap<_, _>>();
        let mut publish = BTreeMap::new();
        let mut next_ids = BTreeSet::new();
        let mut shared = Vec::new();
        let mut insert = |handle: Handle<ImageAsset>,
                          product: AssetProductRef,
                          record: Option<AssetRecord>,
                          image: ImageAsset| {
            if image.product() != Some(product)
                || publish.contains_key(&product)
                || !next_ids.insert(handle.id())
            {
                return Err(identity_conflict());
            }
            let original = owned_by_id
                .get(&handle.id())
                .filter(|image| image.product == product && image.record == record);
            if additive && let Some(original) = original {
                if !original.matches(server, images, states)
                    || images.get(handle).is_none_or(|resident| {
                        resident.content_hash() != image.content_hash()
                            || resident.source() != image.source()
                    })
                {
                    return Err(superseded());
                }
                shared.push((*original).clone());
                return Ok(());
            }
            if original.is_none()
                && (images.get(handle).is_some()
                    || states.state(handle.id()).is_some_and(|state| {
                        !matches!(state.load_state(), nara_asset::LoadState::Removed)
                    }))
            {
                return Err(superseded());
            }
            states
                .next_version(handle.id())
                .map_err(|_| version_exhausted())?;
            let baseline = images
                .prepare_mutation(handle, states)
                .map_err(|_| superseded())?;
            publish.insert(
                product,
                ImagePublication {
                    handle,
                    product,
                    record,
                    image,
                    baseline,
                },
            );
            Ok(())
        };
        match content {
            ResidencyInput::Source(snapshot) => {
                for (index, content) in snapshot.images().iter().enumerate() {
                    let source = content.source();
                    let record = AssetRecord::new(
                        source.stable_id(),
                        source.path().clone(),
                        AssetSourceKind::Image,
                    );
                    let product =
                        AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                            .map_err(|_| identity_conflict())?;
                    let handle = bindings
                        .reserve::<ImageAsset>(
                            &AssetRef::Product(product),
                            snapshot.asset_resolution(),
                            &AssetSourceKind::Image,
                        )
                        .map_err(|_| identity_conflict())?;
                    let image = snapshot.share_image_for_consumer(index).ok_or_else(|| {
                        HostFault::new(single_error(
                            "project.scene.image-snapshot-invalid",
                            "Prepared Scene Image is unavailable",
                        ))
                    })?;
                    insert(handle, product, Some(record), image)?;
                }
            }
            ResidencyInput::Processed(content) => {
                for acquired in &content.images {
                    if !content.base().same_base(acquired.base()) {
                        return Err(identity_conflict());
                    }
                    let product = acquired.product().product().ok_or_else(identity_conflict)?;
                    if product != acquired.reference() {
                        return Err(identity_conflict());
                    }
                    let handle = bindings
                        .reserve_product::<ImageAsset>(product)
                        .map_err(|_| identity_conflict())?;
                    let owner = Arc::new(RuntimeImageOwner {
                        _acquired: acquired.clone(),
                        _metadata: Arc::clone(runtime_lease.ok_or_else(super::resources_missing)?),
                    });
                    let image = acquired.product().share_with_retention_owner(owner);
                    insert(handle, product, None, image)?;
                }
            }
        }
        let mut retire = Vec::new();
        if !additive {
            for image in owned
                .iter()
                .filter(|image| !next_ids.contains(&image.handle.id()))
            {
                states
                    .next_version(image.handle.id())
                    .map_err(|_| version_exhausted())?;
                retire.push(image.clone());
            }
        }
        Ok(Self {
            publish,
            retire,
            shared,
        })
    }

    pub(super) fn retire(
        owned: &[OwnedImage],
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, HostFault> {
        let mut retire = Vec::with_capacity(owned.len());
        for image in owned {
            let id = image.handle.id();
            let state = states.state_revision(id);
            if !image.matches_slot(server, images, states)
                || states.publication_revision(id) != image.publication
                || (image.state != state
                    && !states.state(id).is_some_and(|status| {
                        matches!(status.load_state(), LoadState::Failed { .. })
                    }))
            {
                return Err(superseded());
            }
            images
                .prepare_mutation(image.handle, states)
                .map_err(|_| superseded())?;
            states.next_version(id).map_err(|_| version_exhausted())?;
            let mut receipt = image.clone();
            receipt.state = state;
            retire.push(receipt);
        }
        Ok(Self {
            retire,
            ..Self::empty()
        })
    }

    pub(super) fn image_for_product(
        &self,
        product: AssetProductRef,
    ) -> Option<(Handle<ImageAsset>, ImageAsset)> {
        let item = self.publish.get(&product)?;
        Some((item.handle, item.image.share_retained()?))
    }

    pub(super) fn validate(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(), HostFault> {
        for item in self.publish.values() {
            item.baseline
                .validate(images, states)
                .map_err(|_| superseded())?;
        }
        for image in self.retire.iter().chain(&self.shared) {
            if !image.matches(server, images, states) {
                return Err(superseded());
            }
        }
        Ok(())
    }

    pub(super) fn commit(
        self,
        server: &AssetServer,
        images: &mut Assets<ImageAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Vec<OwnedImage> {
        // The Scene transaction cannot run lifecycle hooks or replace these engine resources.
        // Validate the complete captured baseline before the first publication.
        for item in self.publish.values() {
            item.baseline
                .validate(images, states)
                .expect("Scene Image publication retained its admitted value/state association");
            assert!(binding_matches(
                item.handle,
                item.product,
                item.record.as_ref(),
                server
            ));
        }
        for image in self.retire.iter().chain(&self.shared) {
            assert!(image.matches(server, images, states));
        }
        let mut receipts = Vec::with_capacity(self.publish.len());
        for item in self.publish.into_values() {
            let source_hash = item.image.source().map(|source| source.source_hash());
            let import_recipe_digest = item
                .image
                .source()
                .map(|source| source.import_recipe().digest());
            item.baseline
                .commit_loaded(
                    item.image,
                    images,
                    states,
                    events,
                    source_hash,
                    import_recipe_digest,
                )
                .expect("Scene Image publication version was preflighted");
            receipts.push(OwnedImage::capture(
                item.handle,
                item.product,
                item.record,
                images,
                states,
            ));
        }
        for image in self.retire {
            assert!(
                images
                    .remove_with_state(image.handle, states, events)
                    .expect("Scene Image retirement version was preflighted")
                    .is_some()
            );
        }
        receipts
    }
}

fn binding_matches(
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
    record: Option<&AssetRecord>,
    server: &AssetServer,
) -> bool {
    server.get_handle::<ImageAsset>(handle.id()) == Some(handle)
        && server.product(handle.id()) == Some(product)
        && record.is_none_or(|record| {
            product.source() == record.stable_id()
                && product.product() == ImportedProductId::PRIMARY
                && server.path(handle.id()) == Some(record.path())
        })
}

fn superseded() -> HostFault {
    HostFault::new(single_error(
        "project.scene.image-ownership-changed",
        "Scene Image publication is no longer owned by the current level",
    ))
}

fn identity_conflict() -> HostFault {
    HostFault::new(single_error(
        "project.scene.asset-identity-conflict",
        "Prepared Scene asset identity conflicts with the runtime",
    ))
}

fn version_exhausted() -> HostFault {
    HostFault::new(single_error(
        "project.scene.image-version-exhausted",
        "Scene Image publication versions are exhausted",
    ))
}
