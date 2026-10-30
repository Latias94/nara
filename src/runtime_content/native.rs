//! First-party CPU facts required before native publication or a successful deep Check.

use nara_asset::AssetProductRef;
use nara_image::{ImageAsset, ImageExtent};
use nara_tilemap::TileSetDescriptor;

use super::{RuntimeContentBase, RuntimeContentError};

#[derive(Debug, Clone, Copy)]
pub(super) struct ImageFact {
    pub(super) product: AssetProductRef,
    pub(super) extent: ImageExtent,
}

impl RuntimeContentBase {
    pub(super) fn image_fact(
        &self,
        reference: AssetProductRef,
        image: &ImageAsset,
    ) -> Result<ImageFact, RuntimeContentError> {
        let product = image
            .product()
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        if self.member(reference).map(|member| member.products()) != Some(&[product][..]) {
            return Err(RuntimeContentError::ProductIdentityMismatch);
        }
        Ok(ImageFact {
            product,
            extent: image.extent(),
        })
    }

    pub(super) fn validate_tileset_member(
        &self,
        reference: AssetProductRef,
        descriptor: &TileSetDescriptor,
    ) -> Result<(), RuntimeContentError> {
        let member = self
            .member(reference)
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        if member.products() != &[descriptor.product()] {
            return Err(RuntimeContentError::ProductIdentityMismatch);
        }
        // This independent native contract remains necessary even for a selected custom codec
        // that underreports the descriptor's actual Image in its generic reference inventory.
        self.validate_immediate_dependency(member, descriptor.image_product())
    }
}

pub(super) fn validate_tileset_image(
    descriptor: &TileSetDescriptor,
    extent: Option<ImageExtent>,
) -> Result<(), RuntimeContentError> {
    let extent = extent.ok_or(RuntimeContentError::NativeResourceUnavailable)?;
    descriptor
        .validate_image_extent(extent)
        .map_err(RuntimeContentError::TileSet)
}
