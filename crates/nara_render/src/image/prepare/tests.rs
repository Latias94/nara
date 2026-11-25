use std::sync::Arc;

use nara_asset::{
    AssetEvents, AssetProductRef, AssetSourcePath, ImportDependencyDigest, ImportRecipe,
    ImportedProductId, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};

use super::*;
use crate::image::{ImageImportSettings, ImageSourceMetadata};

fn primary_image_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("20000000-0000-4000-8000-000000000001").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn test_image_source(product: AssetProductRef) -> ImageSourceMetadata {
    let settings = ImageImportSettings::default();
    let recipe = ImportRecipe::new(
        product.source(),
        SourceHash::from_bytes(b"source"),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.image").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
    );
    ImageSourceMetadata::new(
        AssetSourcePath::new("textures/direct-image.png").unwrap(),
        settings,
        recipe,
    )
    .unwrap()
}

#[test]
fn content_hash_changes_when_content_descriptor_changes() {
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![0, 0, 255, 255],
    )
    .unwrap();
    let changed = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![0, 0, 255, 255],
    )
    .unwrap();

    assert_ne!(image_content_hash(&image), image_content_hash(&changed));
}

#[test]
fn code_first_prepared_capture_binds_the_actual_slot_without_a_managed_catalog() {
    let handle = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let image = |pixels: Vec<u8>| {
        ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels,
        )
        .unwrap()
    };
    assert!(PreparedImageSnapshot::capture(handle, &images, &states).is_none());
    images.insert(handle, image(vec![255, 0, 0, 255]));
    assert!(PreparedImageSnapshot::capture(handle, &images, &states).is_none());
    images
        .commit_loaded(
            handle,
            image(vec![255, 0, 0, 255]),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    let first = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
    let same = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
    assert_eq!(first.identity(), same.identity());
    assert_eq!(
        first.resource().pixels().as_ptr(),
        same.resource().pixels().as_ptr()
    );

    images.insert(handle, image(vec![0, 255, 0, 255]));
    let second = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
    assert_eq!(
        first.identity().asset_version(),
        second.identity().asset_version()
    );
    assert_ne!(
        first.identity().content_hash(),
        second.identity().content_hash()
    );
    assert_ne!(
        first.identity().slot_revision(),
        second.identity().slot_revision()
    );
    assert_eq!(first.resource().pixels(), &[255, 0, 0, 255]);
    assert_eq!(second.resource().pixels(), &[0, 255, 0, 255]);

    let mut prepared = PreparedImages::default();
    assert!(prepared.insert(first.clone()).is_none());
    let retained = prepared.get(image_resource_key(handle)).unwrap().clone();
    assert_eq!(prepared.insert(second.clone()), Some(first));
    drop(images);
    drop(prepared);
    assert_eq!(retained.resource().pixels(), &[255, 0, 0, 255]);
    assert_eq!(second.resource().pixels(), &[0, 255, 0, 255]);
}

#[cfg(feature = "image-runtime")]
#[test]
fn code_first_capture_retains_the_original_import_lease_and_external_owner() {
    use image::ImageEncoder;
    use nara_asset::{AssetRecord, AssetServer, AssetSourceKind};

    use crate::image::{ImageBytesImportRequest, ImageImporter};

    let record = AssetRecord::new(
        primary_image_product().source(),
        AssetSourcePath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[24, 120, 220, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let request = ImageBytesImportRequest::new(
        record.clone(),
        png,
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    let importer = ImageImporter::default();
    let host = importer.budget_host();
    let mut server = AssetServer::new();
    let imported_handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut imported_images = Assets::default();
    let mut imported_states = AssetStates::default();
    let mut events = AssetEvents::default();
    importer
        .import_image(
            request,
            imported_handle,
            AssetVersion::ZERO,
            &server,
            &imported_images,
            &imported_states,
        )
        .unwrap()
        .commit_retained(
            &server,
            &mut imported_images,
            &mut imported_states,
            &mut events,
        )
        .unwrap();
    let image = imported_images.remove(imported_handle).unwrap();
    assert_eq!(host.snapshot().active_rgba_bytes(), 4);
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let shared = image.share_with_retention_owner(owner);
    drop(image);
    let pixels = shared.pixels().as_ptr();
    let content_hash = image_content_hash(&shared);
    let handle = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let version = images
        .commit_loaded(handle, shared, &mut states, &mut events, None, None)
        .unwrap();
    let before = host.snapshot();
    let captured = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
    assert_eq!(captured.identity().key(), image_resource_key(handle));
    assert_eq!(captured.identity().asset_version(), version);
    assert_eq!(
        captured.identity().slot_revision(),
        &images.slot_revision(handle)
    );
    assert_eq!(captured.identity().content_hash(), content_hash);
    assert_eq!(captured.resource().pixels().as_ptr(), pixels);
    assert!(captured.resource().has_retention_owner());
    assert_eq!(host.snapshot(), before);
    let retained = captured.clone();
    drop(images);
    drop(captured);
    assert!(weak.upgrade().is_some());
    assert_eq!(host.snapshot(), before);
    assert_eq!(retained.resource().pixels().as_ptr(), pixels);
    drop(retained);
    assert!(weak.upgrade().is_none());
    assert_eq!(host.snapshot().active_bytes(), 0);
    assert_eq!(host.snapshot().released_reservations(), 1);
}

#[test]
fn prepared_image_resource_retains_exact_pixels_across_source_replacement() {
    let product = primary_image_product();
    let mut image = ImageAsset::from_product(
        product,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    let old_bytes = [24, 120, 220, 255];
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    assert!(image.try_attach_retention_owner(Arc::clone(&owner)));

    let old_allocation = image.pixels().as_ptr();
    let prepared = PreparedImageResource::from_image(&image);
    let replacement = ImageAsset::from_import(
        test_image_source(product),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![255, 32, 16, 255],
    )
    .unwrap();
    let replacement_prepared = PreparedImageResource::from_image(&replacement);

    drop(owner);
    drop(image);

    assert!(prepared.has_retention_owner());
    assert!(weak.upgrade().is_some());
    assert_eq!(prepared.pixels(), old_bytes);
    assert_eq!(prepared.pixels().as_ptr(), old_allocation);
    assert_eq!(replacement_prepared.pixels(), [255, 32, 16, 255]);
    assert_ne!(replacement_prepared.pixels().as_ptr(), old_allocation);

    drop(prepared);
    assert!(weak.upgrade().is_none());
}

#[test]
fn prepared_resource_equality_ignores_image_provenance_and_product_id() {
    let product = primary_image_product();
    let extent = ImageExtent::new(1, 1);
    let format = ImageFormat::Rgba8;
    let pixels = vec![24, 120, 220, 255];
    let imported =
        ImageAsset::from_import(test_image_source(product), extent, format, pixels.clone())
            .unwrap();
    let auxiliary = AssetProductRef::new(
        product.source(),
        ImportedProductId::parse_subproduct("30000000-0000-4000-8000-000000000001").unwrap(),
    )
    .unwrap();
    let runtime = ImageAsset::from_product(
        auxiliary,
        extent,
        format,
        ImageColorSpace::Srgb,
        pixels.clone(),
    )
    .unwrap();
    let anonymous = ImageAsset::new(extent, format, ImageColorSpace::Srgb, pixels).unwrap();
    let prepared = PreparedImageResource::from_image(&imported);
    assert_eq!(prepared, PreparedImageResource::from_image(&runtime));
    assert_eq!(prepared, PreparedImageResource::from_image(&anonymous));
}
