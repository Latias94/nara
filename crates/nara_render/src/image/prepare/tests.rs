use nara_asset::AssetEvents;

use super::*;

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
