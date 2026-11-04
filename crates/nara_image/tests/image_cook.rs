use std::{num::NonZeroU32, sync::Arc};

use nara_asset::{AssetProductRef, ImportedProductId, StableAssetId};
#[cfg(feature = "runtime")]
use nara_asset::{AssetRecord, AssetSourceKind, AssetSourcePath};
use nara_core::ByteLimit;
use nara_image::{
    ImageAsset, ImageColorSpace, ImageCookError, ImageCookLimits, ImageCookSettings,
    ImageCookTarget, ImageExtent, ImageFormat, ImageOversizePolicy, cook_runtime_image,
};
#[cfg(feature = "runtime")]
use nara_image::{ImageBytesImportRequest, ImageImportSettings, ImageImporter};

#[cfg(all(feature = "runtime", feature = "serde"))]
use nara_asset::RuntimeAssetCodec;
#[cfg(all(feature = "runtime", feature = "serde"))]
use nara_image::ImageRuntimeCodec;

const SOURCE: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const AUXILIARY: &str = "37277e38-0824-42ee-a574-24ce57aee588";

#[cfg(feature = "runtime")]
#[test]
fn real_png_cooks_to_distinct_4k_and_2k_runtime_products_with_the_same_identity() {
    let extent = ImageExtent::new(4096, 1);
    let pixels = pattern_pixels(extent);
    let png = rgba_png(extent, &pixels);
    let expected_2k: Vec<_> = pixels
        .chunks_exact(8)
        .flat_map(|pair| pair[4..].iter().copied())
        .collect();

    for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
        let record = AssetRecord::new(
            product(ImportedProductId::PRIMARY).source(),
            AssetSourcePath::new("textures/target-sensitive.png").unwrap(),
            AssetSourceKind::Image,
        );
        let imported = ImageImporter::default()
            .preflight_unpublished_import(ImageBytesImportRequest::new(
                record,
                png.clone(),
                ImageImportSettings::new(color_space),
            ))
            .unwrap()
            .import()
            .unwrap();
        assert_eq!(imported.extent(), extent);
        assert_eq!(imported.pixels(), pixels);
        assert!(imported.source().is_some());

        let large =
            cook_runtime_image(&imported, fit_nearest(), target(4096), exact_limits(extent))
                .unwrap();
        let small = cook_runtime_image(
            &imported,
            fit_nearest(),
            target(2048),
            exact_limits(ImageExtent::new(2048, 1)),
        )
        .unwrap();
        assert_eq!(large.extent(), extent);
        assert_eq!(small.extent(), ImageExtent::new(2048, 1));
        assert_eq!(large.pixels(), pixels);
        assert_eq!(small.pixels(), expected_2k);
        assert_eq!(large.content_hash(), imported.content_hash());
        assert_ne!(large.content_hash(), small.content_hash());
        for cooked in [&large, &small] {
            assert_eq!(cooked.product(), imported.product());
            assert_eq!(cooked.format(), imported.format());
            assert_eq!(cooked.color_space(), color_space);
            assert!(cooked.source().is_none());
            assert!(cooked.share_retained().is_none());
        }

        #[cfg(feature = "serde")]
        {
            let large_bytes = ImageRuntimeCodec::encode_runtime(&large).unwrap();
            let small_bytes = ImageRuntimeCodec::encode_runtime(&small).unwrap();
            assert_ne!(large_bytes, small_bytes);
            assert!(large_bytes.len() > small_bytes.len());
            for (cooked, encoded) in [(&large, &large_bytes), (&small, &small_bytes)] {
                assert_eq!(
                    ImageRuntimeCodec::encode_runtime(cooked).unwrap().as_ref(),
                    encoded.as_ref()
                );
                let decoded = ImageRuntimeCodec::decode_runtime(encoded).unwrap();
                assert_eq!(&decoded, cooked);
                assert_eq!(decoded.product(), imported.product());
                assert_eq!(decoded.content_hash(), cooked.content_hash());
                assert!(decoded.source().is_none());
                assert_eq!(
                    ImageRuntimeCodec::runtime_retained_bytes(cooked).unwrap(),
                    exact_limits(cooked.extent()).max_output_bytes().get()
                );
            }
        }

        assert_eq!(
            cook_runtime_image(
                &imported,
                ImageCookSettings::default(),
                target(2048),
                exact_limits(extent),
            )
            .unwrap_err(),
            ImageCookError::TargetDimensionExceeded {
                extent,
                max_texture_dimension_2d: NonZeroU32::new(2048).unwrap(),
            }
        );
        assert_eq!(imported.extent(), extent);
        assert_eq!(imported.pixels(), pixels);
        assert!(imported.source().is_some());
    }
}

#[test]
fn aspect_fitting_and_center_nearest_select_exact_texels_on_both_axes() {
    let cases: &[(ImageExtent, u32, ImageExtent, &[usize])] = &[
        (
            ImageExtent::new(7, 5),
            4,
            ImageExtent::new(4, 2),
            &[7, 9, 11, 13, 21, 23, 25, 27],
        ),
        (
            ImageExtent::new(5, 7),
            4,
            ImageExtent::new(2, 4),
            &[1, 3, 11, 13, 21, 23, 31, 33],
        ),
        (
            ImageExtent::new(5, 3),
            3,
            ImageExtent::new(3, 1),
            &[5, 7, 9],
        ),
        (
            ImageExtent::new(3, 5),
            3,
            ImageExtent::new(1, 3),
            &[1, 7, 13],
        ),
        (
            ImageExtent::new(4, 4),
            2,
            ImageExtent::new(2, 2),
            &[5, 7, 13, 15],
        ),
        (ImageExtent::new(3, 3), 1, ImageExtent::new(1, 1), &[4]),
    ];
    for &(input_extent, cap, output_extent, selected_texels) in cases {
        let image = product_image(input_extent, ImageColorSpace::Srgb);
        let cooked = cook_runtime_image(
            &image,
            fit_nearest(),
            target(cap),
            exact_limits(output_extent),
        )
        .unwrap();
        let expected: Vec<_> = selected_texels
            .iter()
            .flat_map(|&index| image.pixels()[index * 4..index * 4 + 4].iter().copied())
            .collect();

        assert_eq!(cooked.extent(), output_extent);
        assert_eq!(cooked.pixels(), expected);
        assert!(cooked.extent().width <= cap);
        assert!(cooked.extent().height <= cap);
    }
}

#[test]
fn one_texel_axes_stay_nonzero_and_inputs_beyond_runtime_dimensions_can_shrink() {
    let cases = [
        (ImageExtent::new(7, 1), 2, ImageExtent::new(2, 1), 1, 5),
        (ImageExtent::new(1, 7), 2, ImageExtent::new(1, 2), 1, 5),
        (
            ImageExtent::new(16_385, 1),
            1,
            ImageExtent::new(1, 1),
            8192,
            8192,
        ),
        (
            ImageExtent::new(1, 16_385),
            1,
            ImageExtent::new(1, 1),
            8192,
            8192,
        ),
        (
            ImageExtent::new(16_385, 1),
            64,
            ImageExtent::new(64, 1),
            128,
            16_256,
        ),
        (
            ImageExtent::new(1, 16_385),
            64,
            ImageExtent::new(1, 64),
            128,
            16_256,
        ),
    ];
    for (input_extent, cap, output_extent, first, last) in cases {
        let image = product_image(input_extent, ImageColorSpace::Linear);
        let cooked = cook_runtime_image(
            &image,
            fit_nearest(),
            target(cap),
            exact_limits(output_extent),
        )
        .unwrap();
        assert_eq!(cooked.extent(), output_extent);
        assert_eq!(
            &cooked.pixels()[..4],
            &image.pixels()[first * 4..first * 4 + 4]
        );
        let end = cooked.pixels().len();
        assert_eq!(
            &cooked.pixels()[end - 4..],
            &image.pixels()[last * 4..last * 4 + 4]
        );
    }
}

#[test]
fn complete_product_color_alpha_and_pixels_survive_without_upscaling_or_retention() {
    let extent = ImageExtent::new(3, 2);
    let pixels = [
        255, 17, 93, 0, 1, 254, 127, 1, 73, 129, 201, 127, 42, 97, 213, 128, 0, 127, 255, 254, 255,
        255, 255, 255,
    ];
    let auxiliary = ImportedProductId::parse_subproduct(AUXILIARY).unwrap();
    for selector in [ImportedProductId::PRIMARY, auxiliary] {
        for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
            let reference = product(selector);
            let mut image = ImageAsset::from_product(
                reference,
                extent,
                ImageFormat::Rgba8,
                color_space,
                pixels.to_vec(),
            )
            .unwrap();
            let owner = Arc::new(());
            let weak_owner = Arc::downgrade(&owner);
            assert!(image.try_attach_retention_owner(owner));

            for settings in [ImageCookSettings::default(), fit_nearest()] {
                let cooked =
                    cook_runtime_image(&image, settings, target(u32::MAX), exact_limits(extent))
                        .unwrap();
                assert_eq!(cooked.product(), Some(reference));
                assert_eq!(cooked.extent(), extent);
                assert_eq!(cooked.format(), ImageFormat::Rgba8);
                assert_eq!(cooked.color_space(), color_space);
                assert_eq!(cooked.pixels(), pixels);
                assert_eq!(cooked.content_hash(), image.content_hash());
                assert_ne!(cooked.pixels().as_ptr(), image.pixels().as_ptr());
                assert!(cooked.source().is_none());
                assert!(cooked.share_retained().is_none());
                assert_eq!(weak_owner.strong_count(), 1);
            }
            assert!(image.share_retained().is_some());
            drop(image);
            assert!(weak_owner.upgrade().is_none());
        }
    }
}

#[test]
fn anonymous_procedural_images_cannot_become_durable_runtime_products() {
    let extent = ImageExtent::new(1, 1);
    let image = ImageAsset::new(
        extent,
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![13, 47, 201, 0],
    )
    .unwrap();
    for settings in [ImageCookSettings::default(), fit_nearest()] {
        assert_eq!(
            cook_runtime_image(&image, settings, target(1), exact_limits(extent)).unwrap_err(),
            ImageCookError::AnonymousProduct
        );
    }
    assert!(image.product().is_none());
    assert_eq!(image.pixels(), [13, 47, 201, 0]);
}

#[test]
fn exact_output_and_allocation_limits_accept_and_one_byte_less_rejects() {
    let image = product_image(ImageExtent::new(4096, 1), ImageColorSpace::Srgb);
    for cap in [4096, 2048, 2] {
        let output_extent = ImageExtent::new(cap, 1);
        let exact = exact_limits(output_extent);
        let cooked = cook_runtime_image(&image, fit_nearest(), target(cap), exact).unwrap();
        assert_eq!(cooked.extent(), output_extent);

        let output_below = byte_limit(exact.max_output_bytes().get() - 1);
        assert_eq!(
            cook_runtime_image(
                &image,
                fit_nearest(),
                target(cap),
                ImageCookLimits::new(output_below, exact.max_allocation_bytes()),
            )
            .unwrap_err(),
            ImageCookError::OutputBytesExceeded {
                required: exact.max_output_bytes().get(),
                limit: output_below,
            }
        );
        let allocation_below = byte_limit(exact.max_allocation_bytes().get() - 1);
        assert_eq!(
            cook_runtime_image(
                &image,
                fit_nearest(),
                target(cap),
                ImageCookLimits::new(exact.max_output_bytes(), allocation_below),
            )
            .unwrap_err(),
            ImageCookError::AllocationBytesExceeded {
                required: exact.max_allocation_bytes().get(),
                limit: allocation_below,
            }
        );
    }
}

#[test]
fn runtime_format_constraints_are_admitted_even_when_the_device_allows_larger_images() {
    for extent in [ImageExtent::new(16_385, 1), ImageExtent::new(1, 16_385)] {
        let image = product_image(extent, ImageColorSpace::Srgb);
        for settings in [ImageCookSettings::default(), fit_nearest()] {
            assert_eq!(
                cook_runtime_image(
                    &image,
                    settings,
                    target(u32::MAX),
                    ImageCookLimits::new(ByteLimit::ONE, ByteLimit::ONE),
                )
                .unwrap_err(),
                ImageCookError::RuntimeExtentExceeded { extent }
            );
        }
    }
}

fn product(selector: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(SOURCE).unwrap(), selector).unwrap()
}

fn product_image(extent: ImageExtent, color_space: ImageColorSpace) -> ImageAsset {
    ImageAsset::from_product(
        product(ImportedProductId::PRIMARY),
        extent,
        ImageFormat::Rgba8,
        color_space,
        pattern_pixels(extent),
    )
    .unwrap()
}

fn pattern_pixels(extent: ImageExtent) -> Vec<u8> {
    (0..extent.pixel_count())
        .flat_map(|index| {
            [
                index as u8,
                (index >> 8) as u8,
                index.wrapping_mul(17).wrapping_add(3) as u8,
                [0, 1, 127, 255][index as usize % 4],
            ]
        })
        .collect()
}

#[cfg(feature = "runtime")]
fn rgba_png(extent: ImageExtent, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, extent.width, extent.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    drop(writer);
    bytes
}

fn target(cap: u32) -> ImageCookTarget {
    ImageCookTarget::new(NonZeroU32::new(cap).unwrap())
}

fn fit_nearest() -> ImageCookSettings {
    ImageCookSettings {
        oversize: ImageOversizePolicy::FitNearest,
    }
}

fn exact_limits(extent: ImageExtent) -> ImageCookLimits {
    let pixels = usize::try_from(extent.pixel_count()).unwrap() * 4;
    let retained =
        size_of::<ImageAsset>() + size_of::<Box<[u8]>>() + 2 * size_of::<usize>() + pixels;
    ImageCookLimits::new(byte_limit(retained), byte_limit(retained + pixels))
}

fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).unwrap()
}
