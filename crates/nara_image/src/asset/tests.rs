use super::*;

use std::collections::HashSet;

#[cfg(feature = "runtime")]
use nara_asset::{AssetEvents, AssetRecord, AssetServer, AssetSourceKind, AssetStates, Assets};
use nara_asset::{ImportDependencyDigest, ImportRecipe, ImporterId, ImporterVersion};

use crate::budget::{ImageImportCharge, ImageImportPixelLease};
use crate::{ImageImportBudgetHost, ImageImportLimits, PreparedImageResource};

fn product(selection: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("20000000-0000-4000-8000-000000000001").unwrap(),
        selection,
    )
    .unwrap()
}

fn source(path: &str, source_bytes: &[u8], color_space: ImageColorSpace) -> ImageSourceMetadata {
    let settings = ImageImportSettings::new(color_space);
    let import_recipe = ImportRecipe::new(
        product(ImportedProductId::PRIMARY).source(),
        SourceHash::from_bytes(source_bytes),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.image").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
    );
    ImageSourceMetadata::new(AssetSourcePath::new(path).unwrap(), settings, import_recipe).unwrap()
}

fn processed(selection: ImportedProductId, color_space: ImageColorSpace) -> ImageAsset {
    ImageAsset::from_product(
        product(selection),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        color_space,
        vec![24, 120, 220, 255],
    )
    .unwrap()
}

#[test]
fn image_source_metadata_rejects_a_mismatched_settings_hash() {
    let metadata = source(
        "textures/player.png",
        b"image-source",
        ImageColorSpace::Srgb,
    );
    let settings = ImageImportSettings::new(ImageColorSpace::Linear);
    let import_recipe = metadata.import_recipe().clone();
    let actual = import_recipe.settings_hash();

    let error =
        ImageSourceMetadata::new(metadata.path().clone(), settings, import_recipe).unwrap_err();

    assert_eq!(
        error,
        ImageSourceMetadataCreateError::SettingsHashMismatch {
            expected: settings.settings_hash(),
            actual,
        }
    );
}

#[test]
fn image_asset_rejects_zero_overflowing_and_mismatched_rgba_storage() {
    let metadata = || {
        source(
            "textures/direct-image.png",
            b"source",
            ImageColorSpace::Srgb,
        )
    };
    for extent in [ImageExtent::new(0, 1), ImageExtent::new(1, 0)] {
        assert_eq!(
            ImageAsset::from_import(metadata(), extent, ImageFormat::Rgba8, Vec::new())
                .unwrap_err(),
            ImageAssetCreateError::ZeroExtent { extent }
        );
    }
    assert_eq!(
        ImageAsset::from_import(
            metadata(),
            ImageExtent::new(u32::MAX, u32::MAX),
            ImageFormat::Rgba8,
            Vec::new(),
        )
        .unwrap_err(),
        ImageAssetCreateError::ByteLengthOverflow {
            extent: ImageExtent::new(u32::MAX, u32::MAX),
        }
    );
    for (pixels, expected, actual) in [(vec![0; 3], 4, 3), (vec![0; 5], 4, 5)] {
        assert_eq!(
            ImageAsset::from_import(
                metadata(),
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                pixels,
            )
            .unwrap_err(),
            ImageAssetCreateError::PixelLengthMismatch {
                extent: ImageExtent::new(1, 1),
                expected,
                actual,
            }
        );
    }
}

#[test]
fn image_asset_reuses_the_admitted_pixel_allocation() {
    let pixels = vec![24, 120, 220, 255];
    let admitted_allocation = pixels.as_ptr();
    let image = ImageAsset::from_import(
        source("textures/player.png", b"allocation", ImageColorSpace::Srgb),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        pixels,
    )
    .unwrap();

    assert_eq!(image.pixels().as_ptr(), admitted_allocation);
}

#[test]
fn shared_image_pixels_retain_their_lifetime_owner_until_runtime_release() {
    let mut image = processed(ImportedProductId::PRIMARY, ImageColorSpace::Srgb);
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);

    assert!(image.share_retained().is_none());
    assert!(image.try_attach_retention_owner(Arc::clone(&owner)));
    assert!(!image.try_attach_retention_owner(Arc::new(())));
    let shared = image.share_retained().unwrap();
    drop(owner);

    assert!(weak.upgrade().is_some());
    assert_eq!(shared.pixels().as_ptr(), image.pixels().as_ptr());
    drop(image);
    assert!(weak.upgrade().is_some());
    drop(shared);
    assert!(weak.upgrade().is_none());
}

#[test]
fn prepared_image_resource_retains_exact_pixels_across_source_replacement() {
    let mut image = processed(ImportedProductId::PRIMARY, ImageColorSpace::Srgb);
    let old_bytes = [24, 120, 220, 255];
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    assert!(image.try_attach_retention_owner(Arc::clone(&owner)));

    let old_allocation = image.pixels().as_ptr();
    let prepared = PreparedImageResource::from_image(&image);
    let replacement = ImageAsset::from_import(
        source(
            "textures/direct-image.png",
            b"source",
            ImageColorSpace::Srgb,
        ),
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

#[cfg(feature = "serde")]
#[test]
fn shared_image_pixels_keep_the_canonical_byte_content_wire_shape() {
    let image = ImageAsset::from_import(
        source("textures/player.png", b"source", ImageColorSpace::Srgb),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![24, 120, 220, 255],
    )
    .unwrap();

    let encoded = serde_json::to_value(&image).unwrap();
    assert_eq!(
        encoded.get("pixels"),
        Some(&serde_json::json!([24, 120, 220, 255])),
    );

    let decoded = serde_json::from_value::<ImageAsset>(encoded).unwrap();
    assert_eq!(decoded, image);
    assert_eq!(decoded.pixels(), &[24, 120, 220, 255]);
    assert!(!std::ptr::eq(decoded.pixels(), image.pixels()));
}

#[cfg(feature = "serde")]
#[test]
fn image_asset_deserialization_reuses_the_constructor_validation() {
    let image = ImageAsset::from_import(
        source(
            "textures/direct-image.png",
            b"source",
            ImageColorSpace::Srgb,
        ),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    let valid = serde_json::to_value(image).unwrap();

    let mut zero = valid.clone();
    zero["extent"]["width"] = serde_json::json!(0);
    assert!(serde_json::from_value::<ImageAsset>(zero).is_err());

    let mut zero = valid.clone();
    zero["extent"]["height"] = serde_json::json!(0);
    assert!(serde_json::from_value::<ImageAsset>(zero).is_err());

    let mut overflow = valid.clone();
    overflow["extent"]["width"] = serde_json::json!(u32::MAX);
    overflow["extent"]["height"] = serde_json::json!(u32::MAX);
    assert!(serde_json::from_value::<ImageAsset>(overflow).is_err());

    let mut undersized = valid.clone();
    undersized["pixels"] = serde_json::json!([24, 120, 220]);
    assert!(serde_json::from_value::<ImageAsset>(undersized).is_err());

    let mut mismatched_recipe = valid.clone();
    mismatched_recipe["source"]["settings"]["color_space"] = serde_json::json!("linear");
    assert!(serde_json::from_value::<ImageAsset>(mismatched_recipe).is_err());

    let mut oversized = valid;
    oversized["pixels"] = serde_json::json!([24, 120, 220, 255, 0]);
    assert!(serde_json::from_value::<ImageAsset>(oversized).is_err());
}

#[test]
fn content_hash_covers_pixels_and_descriptor_without_import_or_product_identity() {
    let extent = ImageExtent::new(1, 1);
    let format = ImageFormat::Rgba8;
    let pixels = vec![24, 120, 220, 255];
    let first = ImageAsset::from_import(
        source("textures/first.png", b"first source", ImageColorSpace::Srgb),
        extent,
        format,
        pixels.clone(),
    )
    .unwrap();
    let other_source = ImageAsset::from_import(
        source(
            "textures/second.png",
            b"second source",
            ImageColorSpace::Srgb,
        ),
        extent,
        format,
        pixels.clone(),
    )
    .unwrap();
    let anonymous = ImageAsset::new(extent, format, ImageColorSpace::Srgb, pixels.clone()).unwrap();
    let auxiliary =
        ImportedProductId::parse_subproduct("30000000-0000-4000-8000-000000000001").unwrap();
    let runtime = processed(auxiliary, ImageColorSpace::Srgb);
    let other_product = ImageAsset::from_product(
        AssetProductRef::new(
            StableAssetId::parse_str("20000000-0000-4000-8000-000000000002").unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap(),
        extent,
        format,
        ImageColorSpace::Srgb,
        pixels.clone(),
    )
    .unwrap();

    assert!(first.source().is_some());
    assert_eq!(first.product(), Some(product(ImportedProductId::PRIMARY)));
    assert!(anonymous.product().is_none());
    assert!(anonymous.source().is_none());
    assert_eq!(runtime.product(), Some(product(auxiliary)));
    assert!(runtime.source().is_none());
    assert_ne!(first.product(), runtime.product());
    assert_ne!(first.product(), other_product.product());
    assert_ne!(first.source(), other_source.source());
    assert_ne!(
        first.source().unwrap().import_recipe().digest(),
        other_source.source().unwrap().import_recipe().digest()
    );
    for same in [&anonymous, &other_source, &runtime, &other_product] {
        assert_eq!(first.content_hash(), same.content_hash());
    }
    let mut hashes = HashSet::new();
    hashes.insert(first.content_hash());
    hashes.insert(anonymous.content_hash());
    hashes.insert(runtime.content_hash());
    assert_eq!(hashes.len(), 1);
    assert_eq!(first.content_hash().as_bytes().len(), 32);
    assert_eq!(
        PreparedImageResource::from_image(&first),
        PreparedImageResource::from_image(&runtime)
    );
    assert_eq!(
        PreparedImageResource::from_image(&first),
        PreparedImageResource::from_image(&anonymous)
    );

    let changed_pixels = ImageAsset::new(
        extent,
        format,
        ImageColorSpace::Srgb,
        vec![24, 120, 221, 255],
    )
    .unwrap();
    let changed_color = processed(auxiliary, ImageColorSpace::Linear);
    assert_ne!(first.content_hash(), changed_pixels.content_hash());
    assert_ne!(first.content_hash(), changed_color.content_hash());

    let row = ImageAsset::new(
        ImageExtent::new(2, 1),
        format,
        ImageColorSpace::Srgb,
        pixels.repeat(2),
    )
    .unwrap();
    let column = ImageAsset::new(
        ImageExtent::new(1, 2),
        format,
        ImageColorSpace::Srgb,
        pixels.repeat(2),
    )
    .unwrap();
    assert_ne!(row.content_hash(), column.content_hash());
    assert_ne!(first.content_hash(), row.content_hash());
}

#[test]
fn constructors_validate_exact_pixels_with_or_without_product_binding() {
    for binding in [None, Some(product(ImportedProductId::PRIMARY))] {
        let make = |extent, pixels| match binding {
            Some(product) => ImageAsset::from_product(
                product,
                extent,
                ImageFormat::Rgba8,
                ImageColorSpace::Linear,
                pixels,
            ),
            None => ImageAsset::new(extent, ImageFormat::Rgba8, ImageColorSpace::Linear, pixels),
        };
        assert!(matches!(
            make(ImageExtent::new(0, 1), vec![]),
            Err(ImageAssetCreateError::ZeroExtent { .. })
        ));
        assert!(matches!(
            make(ImageExtent::new(u32::MAX, u32::MAX), vec![]),
            Err(ImageAssetCreateError::ByteLengthOverflow { .. })
        ));
        assert!(matches!(
            make(ImageExtent::new(1, 1), vec![0; 3]),
            Err(ImageAssetCreateError::PixelLengthMismatch { .. })
        ));
        let image = make(ImageExtent::new(1, 1), vec![0; 4]).unwrap();
        assert_eq!(image.product(), binding);
        assert!(image.source().is_none());
        assert_eq!(image.extent(), ImageExtent::new(1, 1));
        assert_eq!(image.format(), ImageFormat::Rgba8);
        assert_eq!(image.color_space(), ImageColorSpace::Linear);
        assert_eq!(image.pixels(), &[0; 4]);
    }
}

#[test]
fn shared_pixels_keep_all_lifetime_owners_without_changing_product_binding() {
    for mut original in [
        ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![24, 120, 220, 255],
        )
        .unwrap(),
        processed(ImportedProductId::PRIMARY, ImageColorSpace::Srgb),
    ] {
        let first_owner = Arc::new(());
        let first_weak = Arc::downgrade(&first_owner);
        let second_owner = Arc::new(());
        let second_weak = Arc::downgrade(&second_owner);
        let third_owner = Arc::new(());
        let third_weak = Arc::downgrade(&third_owner);
        assert!(original.share_retained().is_none());
        assert!(original.try_attach_retention_owner(first_owner));
        let shared = original.share_with_retention_owner(second_owner);
        let shared_again = shared.share_with_retention_owner(third_owner);
        assert!(shared.source().is_none());
        assert!(shared_again.source().is_none());
        assert_eq!(original.product(), shared.product());
        assert_eq!(original.product(), shared_again.product());
        assert_eq!(original.content_hash(), shared.content_hash());
        assert!(original.same_allocation(&shared));
        assert!(shared.same_allocation(&shared_again));
        let prepared = PreparedImageResource::from_image(&shared_again);
        assert_eq!(prepared.content_hash(), original.content_hash());
        let clone = prepared.clone();
        let last = shared_again.share_retained().unwrap();
        assert_eq!(original.product(), last.product());
        assert!(last.source().is_none());
        assert!(original.same_allocation(&last));
        drop(original);
        drop(shared);
        drop(shared_again);
        drop(last);
        drop(prepared);
        for owner in [&first_weak, &second_weak, &third_weak] {
            assert!(owner.upgrade().is_some());
        }
        assert_eq!(clone.pixels(), &[24, 120, 220, 255]);
        drop(clone);
        for owner in [&first_weak, &second_weak, &third_weak] {
            assert!(owner.upgrade().is_none());
        }
    }
}

#[test]
fn shared_owner_preserves_an_existing_import_budget_lease() {
    let host = ImageImportBudgetHost::new(ImageImportLimits::default()).unwrap();
    let reservation = host.reserve(ImageImportCharge::retained_pixels(4)).unwrap();
    let (lease, overlap) = ImageImportPixelLease::from_publication(reservation, 4).unwrap();
    drop(overlap);
    let mut original = processed(ImportedProductId::PRIMARY, ImageColorSpace::Linear);
    assert!(original.try_attach_import_pixel_lease(lease));
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let shared = original.share_with_retention_owner(owner);
    let prepared = PreparedImageResource::from_image(&shared);
    assert_eq!(host.snapshot().active_rgba_bytes(), 4);
    drop(original);
    drop(shared);
    assert_eq!(host.snapshot().active_rgba_bytes(), 4);
    assert!(weak.upgrade().is_some());
    drop(prepared);
    assert_eq!(host.snapshot().active_bytes(), 0);
    assert!(weak.upgrade().is_none());
}

#[cfg(feature = "runtime")]
#[test]
fn source_reload_requires_matching_real_provenance_before_reusing_settings() {
    let record = AssetRecord::new(
        product(ImportedProductId::PRIMARY).source(),
        AssetSourcePath::new("textures/source.png").unwrap(),
        AssetSourceKind::Image,
    );
    for path in [
        None,
        Some("textures/other.png"),
        Some(record.path().as_str()),
    ] {
        let image = match path {
            None => processed(ImportedProductId::PRIMARY, ImageColorSpace::Linear),
            Some(path) => ImageAsset::from_import(
                source(path, b"real source", ImageColorSpace::Linear),
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                vec![24, 120, 220, 255],
            )
            .unwrap(),
        };
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let version = images
            .commit_loaded(
                handle,
                image,
                &mut states,
                &mut AssetEvents::default(),
                None,
                None,
            )
            .unwrap();
        let result = crate::import::ImagePublicationSnapshot::capture_with_settings(
            &record, handle, version, &images, &states,
        );
        if path == Some(record.path().as_str()) {
            assert_eq!(
                result.unwrap().1,
                Some(ImageImportSettings::new(ImageColorSpace::Linear))
            );
        } else {
            assert_eq!(
                result.unwrap_err(),
                crate::ImagePublicationFailureKind::TargetMismatch
            );
        }
        assert_eq!(images.get(handle).unwrap().pixels(), &[24, 120, 220, 255]);
        assert_eq!(states.version(handle.id()), Some(version));
    }
}

#[cfg(feature = "serde")]
#[test]
fn anonymous_serde_omits_product_and_source_and_recomputes_content() {
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    let wire = serde_json::to_value(&image).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({
            "extent": {"width": 1, "height": 1},
            "format": "rgba8",
            "color_space": "linear",
            "pixels": [24, 120, 220, 255],
        })
    );
    let decoded: ImageAsset = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded, image);
    assert!(decoded.product().is_none());
    assert!(decoded.source().is_none());
    assert_eq!(decoded.content_hash(), image.content_hash());

    let mut changed = wire.clone();
    changed["pixels"][0] = serde_json::json!(25);
    let changed: ImageAsset = serde_json::from_value(changed).unwrap();
    assert!(changed.product().is_none());
    assert!(changed.source().is_none());
    assert_ne!(changed.content_hash(), image.content_hash());

    let mut invalid = wire;
    invalid["pixels"] = serde_json::json!([24, 120, 220]);
    assert!(serde_json::from_value::<ImageAsset>(invalid).is_err());
}

#[cfg(feature = "serde")]
#[test]
fn source_less_serde_preserves_complete_product_and_recomputes_changed_content() {
    let auxiliary =
        ImportedProductId::parse_subproduct("30000000-0000-4000-8000-000000000001").unwrap();
    let image = processed(auxiliary, ImageColorSpace::Linear);
    let wire = serde_json::to_value(&image).unwrap();
    let fields = wire.as_object().unwrap();
    assert_eq!(fields.len(), 5);
    for field in ["product", "extent", "format", "color_space", "pixels"] {
        assert!(fields.contains_key(field));
    }
    assert!(!fields.contains_key("source"));
    assert!(!fields.contains_key("content_hash"));
    let decoded: ImageAsset = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded, image);
    assert_eq!(decoded.product(), Some(product(auxiliary)));
    assert_eq!(decoded.content_hash(), image.content_hash());
    assert!(decoded.source().is_none());

    let mut changed = wire.clone();
    changed["pixels"][0] = serde_json::json!(25);
    let changed: ImageAsset = serde_json::from_value(changed).unwrap();
    assert_ne!(changed.content_hash(), image.content_hash());
    assert_eq!(changed.product(), image.product());

    let mut changed = wire.clone();
    changed["color_space"] = serde_json::json!("srgb");
    let changed: ImageAsset = serde_json::from_value(changed).unwrap();
    assert_ne!(changed.content_hash(), image.content_hash());

    let mut null_source = wire;
    null_source["source"] = serde_json::Value::Null;
    assert_eq!(
        serde_json::from_value::<ImageAsset>(null_source).unwrap(),
        image
    );
}

#[cfg(feature = "serde")]
#[test]
fn serde_rejects_forged_source_product_color_and_cached_hash() {
    let image = ImageAsset::from_import(
        source("textures/source.png", b"source", ImageColorSpace::Srgb),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    let wire = serde_json::to_value(&image).unwrap();
    assert_eq!(wire.as_object().unwrap().len(), 6);
    let source_fields = wire["source"].as_object().unwrap();
    assert_eq!(source_fields.len(), 3);
    for field in ["path", "settings", "import_recipe"] {
        assert!(source_fields.contains_key(field));
    }
    assert_eq!(source_fields["import_recipe"].as_object().unwrap().len(), 6);
    assert_eq!(
        serde_json::from_value::<ImageAsset>(wire.clone()).unwrap(),
        image
    );

    let mut wrong_source = wire.clone();
    wrong_source["product"]["source"] = serde_json::json!("20000000-0000-4000-8000-000000000002");
    assert!(serde_json::from_value::<ImageAsset>(wrong_source).is_err());

    let mut nil = wire.clone();
    nil["product"]["source"] = serde_json::json!("00000000-0000-0000-0000-000000000000");
    assert!(serde_json::from_value::<ImageAsset>(nil).is_err());

    let mut auxiliary = wire.clone();
    auxiliary["product"]["product"] = serde_json::json!({
        "kind": "subproduct", "id": "30000000-0000-4000-8000-000000000001"
    });
    assert!(serde_json::from_value::<ImageAsset>(auxiliary).is_err());

    let mut color = wire.clone();
    color["color_space"] = serde_json::json!("linear");
    assert!(serde_json::from_value::<ImageAsset>(color).is_err());

    for field in [
        "content_hash",
        "source_hash",
        "import_recipe_digest",
        "retention",
        "unexpected",
    ] {
        let mut unknown = wire.clone();
        unknown[field] = serde_json::json!(vec![0_u8; 32]);
        assert!(
            serde_json::from_value::<ImageAsset>(unknown).is_err(),
            "{field}"
        );
    }
    let mut unknown_extent = wire;
    unknown_extent["extent"]["depth"] = serde_json::json!(1);
    assert!(serde_json::from_value::<ImageAsset>(unknown_extent).is_err());
}
