use std::sync::Arc;

#[cfg(feature = "project-source")]
use nara_asset::{AssetProductRef, AssetRef, AssetSourceKind, ImportedProductId};
use nara_asset::{AssetServer, AssetStates, Assets};
use nara_render::image::{
    ImageAsset, ImagePublicationError, ImagePublicationInput, ImagePublicationMode,
};
pub(super) use nara_render::image::{
    ImageContentReceipt as OwnedImage, PreparedImagePublications as ImagePublications,
};
use nara_scene::advanced::SceneAssetReservations;

use super::{
    HostFault, ProjectSceneResidency, ResidencyInput, binding_changed, failure, stock::StockChange,
};
use crate::runtime_content::{RuntimeContentAsset, RuntimeContentLease};

struct RuntimeImageOwner {
    _acquired: RuntimeContentAsset<ImageAsset>,
    // Escaped pixels retain the wrapper reservation as well as the original decoded output.
    _metadata: Arc<RuntimeContentLease>,
}

pub(super) fn prepare(
    content: ResidencyInput<'_>,
    change: StockChange<'_>,
    bindings: &mut SceneAssetReservations,
    server: &AssetServer,
    images: &Assets<ImageAsset>,
    states: &AssetStates,
    runtime_lease: Option<&Arc<RuntimeContentLease>>,
) -> Result<ImagePublications, HostFault> {
    let mode = if change.is_additive() {
        ImagePublicationMode::Add
    } else {
        ImagePublicationMode::Replace
    };
    let owned = change
        .current()
        .into_iter()
        .flat_map(ProjectSceneResidency::owned_images);
    if mode == ImagePublicationMode::Replace {
        // Check the incumbent before staging reservations that consume process-local handles.
        for receipt in owned.clone() {
            receipt
                .validate_retirement(server, images, states)
                .map_err(image_failure)?;
        }
    }
    let mut inputs = Vec::with_capacity(content.image_count());
    match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            for (index, content) in snapshot.images().iter().enumerate() {
                let source = content.source();
                let product = AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY)
                    .map_err(|_| binding_changed())?;
                let handle = bindings
                    .reserve::<ImageAsset>(
                        &AssetRef::Product(product),
                        snapshot.asset_resolution(),
                        &AssetSourceKind::Image,
                    )
                    .map_err(|_| binding_changed())?;
                let image = snapshot.share_image_for_consumer(index).ok_or_else(|| {
                    failure(
                        "project.scene.image-snapshot-invalid",
                        "Prepared Scene Image is unavailable",
                    )
                })?;
                inputs.push(ImagePublicationInput::new(
                    handle,
                    product,
                    Some(source.path().clone()),
                    image,
                ));
            }
        }
        ResidencyInput::Processed(content) => {
            for acquired in &content.images {
                if !content.base().same_base(acquired.base()) {
                    return Err(binding_changed());
                }
                let product = acquired.reference();
                let handle = bindings
                    .reserve_product::<ImageAsset>(product)
                    .map_err(|_| binding_changed())?;
                let owner = Arc::new(RuntimeImageOwner {
                    _acquired: acquired.clone(),
                    _metadata: Arc::clone(runtime_lease.ok_or_else(super::resources_missing)?),
                });
                let image = acquired.product().share_with_retention_owner(owner);
                inputs.push(ImagePublicationInput::new(handle, product, None, image));
            }
        }
    }
    ImagePublications::prepare(
        inputs,
        owned,
        mode,
        server,
        bindings.staged_server(),
        images,
        states,
    )
    .map_err(image_failure)
}

pub(super) fn retire<'receipt>(
    owned: impl Iterator<Item = &'receipt OwnedImage> + Clone,
    server: &AssetServer,
    images: &Assets<ImageAsset>,
    states: &AssetStates,
) -> Result<ImagePublications, HostFault> {
    ImagePublications::prepare(
        Vec::new(),
        owned,
        ImagePublicationMode::Replace,
        server,
        server,
        images,
        states,
    )
    .map_err(image_failure)
}

pub(super) fn image_failure(error: ImagePublicationError) -> HostFault {
    match error {
        ImagePublicationError::IdentityMismatch => binding_changed(),
        ImagePublicationError::Superseded => failure(
            "project.scene.image-ownership-changed",
            "Scene Image publication is no longer owned by the current level",
        ),
        ImagePublicationError::VersionExhausted => failure(
            "project.scene.image-version-exhausted",
            "Scene Image publication versions are exhausted",
        ),
    }
}
