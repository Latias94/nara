use nara_asset::AssetEvents;

use super::*;
use crate::budget::{ImageImportCharge, ImageImportPixelLease};
use crate::{ImageImportBudgetHost, ImageImportLimits};

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

#[test]
fn code_first_capture_retains_the_original_import_lease_and_external_owner() {
    let host = ImageImportBudgetHost::new(ImageImportLimits::default()).unwrap();
    let reservation = host.reserve(ImageImportCharge::retained_pixels(4)).unwrap();
    let (lease, overlap) = ImageImportPixelLease::from_publication(reservation, 4).unwrap();
    drop(overlap);
    let mut image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    assert!(image.try_attach_import_pixel_lease(lease));
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let shared = image.share_with_retention_owner(owner);
    drop(image);
    let pixels = shared.pixels().as_ptr();
    let content_hash = image_content_hash(&shared);
    let handle = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
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
