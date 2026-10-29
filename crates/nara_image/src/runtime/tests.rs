use std::{collections::BTreeSet, sync::Arc};

use image::ImageEncoder;
use nara_asset::{
    AssetDomain, AssetPath, AssetProductRef, AssetRecord, AssetSourceKind, ImportedProductId,
    RuntimeAssetCodec, StableAssetId,
};
use serde_json::{Value, json};

use super::{ImageDomain, ImageRuntimeCodec, codec};
use crate::{
    ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageExtent, ImageFormat,
    ImageImportSettings, ImageImporter,
};

const SOURCE: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const OTHER_SOURCE: &str = "371f8e14-9392-4fb2-85b3-2f0c7fe3ff24";
const AUXILIARY: &str = "37277e38-0824-42ee-a574-24ce57aee588";
const PIXELS: [u8; 4] = [0x13, 0x57, 0x9b, 0xdf];
// A fixed, nonempty binary grammar witness: 0x137 metadata bytes followed by four raw bytes.
const GOLDEN: &[u8] = b"NRI\0\x37\x01\0\0{\"kind\":\"runtime_image\",\"format_version\":1,\"engine_min_version\":\"0.1.0\",\"generator\":{\"name\":\"nara_image\",\"version\":\"0.1.0\"},\"payload\":{\"product\":{\"source\":\"2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f\",\"product\":{\"kind\":\"primary\"}},\"extent\":{\"width\":1,\"height\":1},\"format\":\"rgba8\",\"color_space\":\"srgb\",\"pixel_bytes\":4}}\x13\x57\x9b\xdf";

fn reference(source: &str, product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), product).unwrap()
}

fn primary() -> AssetProductRef {
    reference(SOURCE, ImportedProductId::PRIMARY)
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct(AUXILIARY).unwrap()
}

fn image(product: AssetProductRef, color_space: ImageColorSpace) -> ImageAsset {
    ImageAsset::from_product(
        product,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        color_space,
        PIXELS.to_vec(),
    )
    .unwrap()
}

fn metadata() -> Value {
    serde_json::from_slice(&GOLDEN[8..GOLDEN.len() - PIXELS.len()]).unwrap()
}

fn frame(metadata: &[u8], pixels: &[u8]) -> Vec<u8> {
    let mut bytes = b"NRI\0".to_vec();
    bytes.extend_from_slice(&u32::try_from(metadata.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(metadata);
    bytes.extend_from_slice(pixels);
    bytes
}

fn framed_value(metadata: &Value) -> Vec<u8> {
    frame(&serde_json::to_vec(metadata).unwrap(), &PIXELS)
}

fn rejection_code(bytes: &[u8]) -> String {
    ImageRuntimeCodec::decode_runtime(bytes)
        .unwrap_err()
        .diagnostic()
        .code()
        .as_str()
        .to_owned()
}

#[test]
fn fixed_nonempty_golden_uses_json_metadata_and_raw_rgba() {
    assert_eq!(GOLDEN.len(), 8 + 311 + 4);
    let image = ImageRuntimeCodec::decode_runtime(GOLDEN).unwrap();
    assert_eq!(image.product(), Some(primary()));
    assert_eq!(image.extent(), ImageExtent::new(1, 1));
    assert_eq!(image.format(), ImageFormat::Rgba8);
    assert_eq!(image.color_space(), ImageColorSpace::Srgb);
    assert_eq!(image.pixels(), PIXELS);
    assert!(image.source().is_none());
    assert!(image.share_retained().is_none());
    assert_eq!(
        ImageRuntimeCodec::encode_runtime(&image).unwrap().as_ref(),
        GOLDEN
    );
}

#[test]
fn deterministic_roundtrip_preserves_primary_auxiliary_and_both_color_spaces() {
    for selector in [ImportedProductId::PRIMARY, auxiliary()] {
        for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
            let product = reference(SOURCE, selector);
            let image = image(product, color_space);
            let first = ImageRuntimeCodec::encode_runtime(&image).unwrap();
            let second = ImageRuntimeCodec::encode_runtime(&image).unwrap();
            assert_eq!(first, second);
            let decoded = ImageRuntimeCodec::decode_runtime(&first).unwrap();
            assert_eq!(decoded, image);
            assert_eq!(decoded.product(), Some(product));
            assert_eq!(decoded.content_hash(), image.content_hash());
            assert!(decoded.source().is_none());
            assert!(ImageDomain::contains_product(&decoded, selector));
            assert!(ImageRuntimeCodec::contains_runtime_product(
                &decoded, product
            ));
            assert!(!ImageRuntimeCodec::contains_runtime_product(
                &decoded,
                reference(OTHER_SOURCE, selector),
            ));
            let absent = if selector == ImportedProductId::PRIMARY {
                auxiliary()
            } else {
                ImportedProductId::PRIMARY
            };
            assert!(!ImageDomain::contains_product(&decoded, absent));
            assert!(!ImageRuntimeCodec::contains_runtime_product(
                &decoded,
                reference(SOURCE, absent)
            ));
        }
    }
}

#[test]
fn anonymous_images_cannot_be_encoded_or_claim_product_membership() {
    for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
        let anonymous = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            color_space,
            PIXELS.to_vec(),
        )
        .unwrap();
        assert!(anonymous.product().is_none());
        assert!(anonymous.source().is_none());
        assert_eq!(
            ImageRuntimeCodec::encode_runtime(&anonymous)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "image.runtime-product"
        );
        assert_eq!(
            ImageRuntimeCodec::runtime_retained_bytes(&anonymous)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "image.runtime-product"
        );
        for selector in [ImportedProductId::PRIMARY, auxiliary()] {
            assert!(!ImageDomain::contains_product(&anonymous, selector));
            assert!(!ImageRuntimeCodec::contains_runtime_product(
                &anonymous,
                reference(SOURCE, selector)
            ));
        }
    }
}

#[test]
fn png_import_cooks_to_identical_pixels_without_source_or_import_provenance() {
    let pixels = [0, 127, 128, 255, 34, 68, 102, 136];
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&pixels, 2, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
        let record = AssetRecord::new(
            primary().source(),
            AssetPath::new("textures/runtime-import.png").unwrap(),
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
        assert!(imported.source().is_some());
        assert_eq!(imported.product(), Some(primary()));
        assert_eq!(
            ImageRuntimeCodec::runtime_retained_bytes(&imported)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "image.runtime-retention"
        );
        let encoded = ImageRuntimeCodec::encode_runtime(&imported).unwrap();
        let header_length = u32::from_le_bytes(encoded[4..8].try_into().unwrap()) as usize;
        let header: Value = serde_json::from_slice(&encoded[8..8 + header_length]).unwrap();
        let keys: BTreeSet<_> = header["payload"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            BTreeSet::from(["color_space", "extent", "format", "pixel_bytes", "product"])
        );
        let decoded = ImageRuntimeCodec::decode_runtime(&encoded).unwrap();
        assert_eq!(decoded.product(), imported.product());
        assert_eq!(decoded.extent(), imported.extent());
        assert_eq!(decoded.format(), imported.format());
        assert_eq!(decoded.color_space(), color_space);
        assert_eq!(decoded.pixels(), pixels);
        assert_eq!(decoded.content_hash(), imported.content_hash());
        assert!(decoded.source().is_none());
        assert!(decoded.share_retained().is_none());
        assert!(ImageRuntimeCodec::runtime_retained_bytes(&decoded).is_ok());
    }
}

#[test]
fn changed_pixels_and_color_space_recompute_the_content_hash() {
    let original = ImageRuntimeCodec::decode_runtime(GOLDEN).unwrap();
    let mut changed = GOLDEN.to_vec();
    *changed.last_mut().unwrap() ^= 0x40;
    let decoded = ImageRuntimeCodec::decode_runtime(&changed).unwrap();
    assert_ne!(original.content_hash(), decoded.content_hash());
    let expected = ImageAsset::from_product(
        primary(),
        original.extent(),
        original.format(),
        original.color_space(),
        vec![PIXELS[0], PIXELS[1], PIXELS[2], PIXELS[3] ^ 0x40],
    )
    .unwrap();
    assert_eq!(decoded.content_hash(), expected.content_hash());

    let mut header = metadata();
    header["payload"]["color_space"] = json!("linear");
    let linear = ImageRuntimeCodec::decode_runtime(&framed_value(&header)).unwrap();
    assert_eq!(original.pixels(), linear.pixels());
    assert_ne!(original.content_hash(), linear.content_hash());
}

#[test]
fn truncated_prefix_wrong_magic_and_invalid_header_lengths_are_rejected() {
    for end in 0..8 {
        assert_eq!(rejection_code(&GOLDEN[..end]), "image.runtime-framing");
    }
    let mut bytes = GOLDEN.to_vec();
    bytes[0] = b'X';
    assert_eq!(rejection_code(&bytes), "image.runtime-framing");
    for length in [0, (codec::MAX_METADATA_BYTES + 1) as u32, u32::MAX] {
        let mut bytes = GOLDEN.to_vec();
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        assert_eq!(rejection_code(&bytes), "image.runtime-metadata");
    }
    assert_eq!(rejection_code(&GOLDEN[..9]), "image.runtime-framing");
    for length in [310_u32, 312] {
        let mut bytes = GOLDEN.to_vec();
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        assert!(ImageRuntimeCodec::decode_runtime(&bytes).is_err());
    }
}

#[test]
fn metadata_byte_limit_accepts_the_exact_boundary_and_rejects_one_more() {
    let mut header = GOLDEN[8..GOLDEN.len() - 4].to_vec();
    header.resize(codec::MAX_METADATA_BYTES, b' ');
    assert!(ImageRuntimeCodec::decode_runtime(&frame(&header, &PIXELS)).is_ok());
    header.push(b' ');
    assert_eq!(
        rejection_code(&frame(&header, &PIXELS)),
        "image.runtime-metadata"
    );
}

#[test]
fn pixels_require_exact_declared_and_extent_lengths_without_any_trailing_bytes() {
    for count in [0, 1, 3, 5] {
        let header = &GOLDEN[8..GOLDEN.len() - 4];
        assert_eq!(
            rejection_code(&frame(header, &vec![0; count])),
            "image.runtime-pixel-length",
        );
    }
    for count in [0, 3, 5, u64::MAX] {
        let mut header = metadata();
        header["payload"]["pixel_bytes"] = json!(count);
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-pixel-length"
        );
    }
}

#[test]
fn dimensions_and_aggregate_pixel_count_are_checked_before_pixel_length() {
    for (width, height) in [
        (0, 1),
        (1, 0),
        (16_385, 1),
        (1, 16_385),
        (u32::MAX, u32::MAX),
        (8192, 4096),
    ] {
        let mut header = metadata();
        header["payload"]["extent"] = json!({"width": width, "height": height});
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-dimensions"
        );
    }
    for extent in [ImageExtent::new(16_384, 1), ImageExtent::new(1, 16_384)] {
        let image = ImageAsset::from_product(
            primary(),
            extent,
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            vec![17; 16_384 * 4],
        )
        .unwrap();
        let encoded = ImageRuntimeCodec::encode_runtime(&image).unwrap();
        assert_eq!(ImageRuntimeCodec::decode_runtime(&encoded).unwrap(), image);
    }
    let oversized = ImageAsset::from_product(
        primary(),
        ImageExtent::new(16_385, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![17; 16_385 * 4],
    )
    .unwrap();
    assert_eq!(
        ImageRuntimeCodec::encode_runtime(&oversized)
            .unwrap_err()
            .diagnostic()
            .code()
            .as_str(),
        "image.runtime-dimensions"
    );
    assert_eq!(
        ImageRuntimeCodec::runtime_retained_bytes(&oversized)
            .unwrap_err()
            .diagnostic()
            .code()
            .as_str(),
        "image.runtime-dimensions"
    );
}

#[test]
fn core_contract_rejects_wrong_kind_version_and_newer_or_invalid_engine_requirements() {
    for (key, value, code) in [
        ("kind", json!("image"), "image.runtime-kind"),
        ("format_version", json!(2), "image.runtime-version"),
        ("format_version", json!(0), "image.runtime-metadata"),
        (
            "engine_min_version",
            json!("9999.0.0"),
            "image.runtime-engine-version",
        ),
        (
            "engine_min_version",
            json!("not-semver"),
            "image.runtime-metadata",
        ),
    ] {
        let mut header = metadata();
        header[key] = value;
        assert_eq!(rejection_code(&framed_value(&header)), code, "{key}");
    }
    let mut header = metadata();
    header["engine_min_version"] = json!("0.1.0+other.build");
    assert!(ImageRuntimeCodec::decode_runtime(&framed_value(&header)).is_ok());
}

#[test]
fn missing_unknown_and_source_import_cache_or_hash_metadata_fields_are_rejected() {
    for field in [
        "source",
        "import",
        "artifact",
        "cache",
        "content_hash",
        "pixels",
    ] {
        let mut header = metadata();
        header["payload"][field] = Value::Null;
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-metadata",
            "{field}"
        );
    }
    for pointer in [
        "",
        "/generator",
        "/payload/extent",
        "/payload/product",
        "/payload/product/product",
    ] {
        let mut header = metadata();
        header
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), Value::Null);
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-metadata",
            "{pointer}"
        );
    }
    for field in ["product", "extent", "format", "color_space", "pixel_bytes"] {
        let mut header = metadata();
        header["payload"].as_object_mut().unwrap().remove(field);
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-metadata",
            "{field}"
        );
    }
}

#[test]
fn duplicate_fields_are_rejected_before_their_last_value_can_mask_them() {
    let header = std::str::from_utf8(&GOLDEN[8..GOLDEN.len() - 4]).unwrap();
    for (single, duplicate) in [
        (
            "\"kind\":\"runtime_image\"",
            "\"kind\":\"image\",\"kind\":\"runtime_image\"",
        ),
        ("\"pixel_bytes\":4", "\"pixel_bytes\":8,\"pixel_bytes\":4"),
        ("\"width\":1", "\"width\":0,\"width\":1"),
        (
            "\"kind\":\"primary\"",
            "\"kind\":\"primary\",\"kind\":\"primary\"",
        ),
    ] {
        let duplicate = header.replacen(single, duplicate, 1);
        assert_ne!(duplicate, header);
        assert_eq!(
            rejection_code(&frame(duplicate.as_bytes(), &PIXELS)),
            "image.runtime-metadata"
        );
    }
}

#[test]
fn positional_arrays_cannot_impersonate_any_metadata_record() {
    for (pointer, array) in [
        (
            "",
            json!([
                "runtime_image",
                1,
                "0.1.0",
                metadata()["generator"],
                metadata()["payload"]
            ]),
        ),
        ("/generator", json!(["nara_image", "0.1.0"])),
        (
            "/payload",
            json!([metadata()["payload"]["product"], {"width": 1, "height": 1}, "rgba8", "srgb", 4]),
        ),
        ("/payload/extent", json!([1, 1])),
        ("/payload/product", json!([SOURCE, {"kind": "primary"}])),
        ("/payload/product/product", json!(["primary"])),
    ] {
        let mut header = metadata();
        *header.pointer_mut(pointer).unwrap() = array;
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-metadata",
            "{pointer}"
        );
    }
}

#[test]
fn invalid_identities_format_color_and_numeric_types_are_rejected() {
    for (pointer, value) in [
        (
            "/payload/product/source",
            json!("00000000-0000-0000-0000-000000000000"),
        ),
        ("/payload/product/source", json!(SOURCE.to_uppercase())),
        (
            "/payload/product/product",
            json!({"kind": "subproduct", "id": "00000000-0000-0000-0000-000000000000"}),
        ),
        ("/payload/format", json!("bgra8")),
        ("/payload/color_space", json!("display_p3")),
        ("/payload/extent/width", json!(-1)),
        ("/payload/extent/width", json!(1.5)),
        ("/payload/pixel_bytes", json!("4")),
    ] {
        let mut header = metadata();
        *header.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            rejection_code(&framed_value(&header)),
            "image.runtime-metadata",
            "{pointer}"
        );
    }
}

#[test]
fn metadata_shape_is_bounded_before_semantic_decode() {
    let nested = format!("{}0{}", "{\"nested\":".repeat(10), "}".repeat(10));
    assert_eq!(
        rejection_code(&frame(nested.as_bytes(), &PIXELS)),
        "image.runtime-metadata"
    );
    let mut header = metadata();
    header["generator"]["name"] = json!("x".repeat(257));
    assert_eq!(
        rejection_code(&framed_value(&header)),
        "image.runtime-metadata"
    );
    let mut json_with_trailing_value = GOLDEN[8..GOLDEN.len() - 4].to_vec();
    json_with_trailing_value.extend_from_slice(b"{}");
    assert_eq!(
        rejection_code(&frame(&json_with_trailing_value, &PIXELS)),
        "image.runtime-metadata"
    );
}

#[test]
fn codec_promises_cover_binary_output_copying_and_retained_product_without_import_charge() {
    assert_eq!(ImageRuntimeCodec::RUNTIME_KIND, "runtime_image");
    assert_eq!(ImageRuntimeCodec::RUNTIME_VERSION, 1);
    let limits = ImageRuntimeCodec::runtime_limits();
    assert_eq!(
        limits.encoded_bytes().get(),
        64 * 1024 * 1024 + 16 * 1024 + 8
    );
    assert_eq!(limits.work_bytes().get(), 128 * 1024 * 1024 + 128 * 1024);
    assert_eq!(limits.retained_bytes().get(), 65 * 1024 * 1024);
    assert!(limits.encoded_bytes().get() < 128 * 1024 * 1024);
    assert!(limits.work_bytes().get() >= 2 * limits.encoded_bytes().get());

    // Prove coverage at the maximum pixel length without allocating a 64 MiB test fixture.
    let decoded = ImageRuntimeCodec::decode_runtime(GOLDEN).unwrap();
    let overhead = ImageRuntimeCodec::runtime_retained_bytes(&decoded).unwrap() - PIXELS.len();
    assert!(overhead >= size_of::<ImageAsset>());
    assert!(codec::MAX_RGBA_BYTES.checked_add(overhead).unwrap() <= limits.retained_bytes().get());
}

#[test]
fn decoded_retained_cost_counts_inline_value_fixed_pixels_and_shared_allocation_overhead() {
    for extent in [ImageExtent::new(1, 1), ImageExtent::new(32, 16)] {
        let pixel_bytes = usize::try_from(extent.pixel_count() * 4).unwrap();
        let mut pixels = Vec::with_capacity(pixel_bytes + 4096);
        pixels.resize(pixel_bytes, 17);
        let original = ImageAsset::from_product(
            primary(),
            extent,
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels,
        )
        .unwrap();
        let encoded = ImageRuntimeCodec::encode_runtime(&original).unwrap();
        let decoded = ImageRuntimeCodec::decode_runtime(&encoded).unwrap();
        let expected =
            size_of::<ImageAsset>() + pixel_bytes + size_of::<Box<[u8]>>() + 2 * size_of::<usize>();

        assert_eq!(
            ImageRuntimeCodec::runtime_retained_bytes(&original).unwrap(),
            expected
        );
        let shared_pixels = decoded.shared_pixels();
        let owners_before = Arc::strong_count(&shared_pixels);
        for _ in 0..2 {
            let actual = ImageRuntimeCodec::runtime_retained_bytes(&decoded).unwrap();
            assert_eq!(actual, expected);
            assert!(actual > pixel_bytes);
            assert!(actual >= size_of::<ImageAsset>());
            assert!(actual < ImageRuntimeCodec::runtime_limits().retained_bytes().get());
            assert_eq!(Arc::strong_count(&shared_pixels), owners_before);
        }
        assert!(decoded.source().is_none());
        assert!(decoded.share_retained().is_none());
        assert_eq!(decoded, original);
    }
}

#[test]
fn runtime_retained_cost_refuses_attached_external_owners_without_changing_custody() {
    let mut decoded = ImageRuntimeCodec::decode_runtime(GOLDEN).unwrap();
    let owner = Arc::new(vec![0_u8; 1024]);
    assert!(decoded.try_attach_retention_owner(Arc::clone(&owner)));
    let owners_before = Arc::strong_count(&owner);
    let content_hash = decoded.content_hash();

    assert_eq!(
        ImageRuntimeCodec::runtime_retained_bytes(&decoded)
            .unwrap_err()
            .diagnostic()
            .code()
            .as_str(),
        "image.runtime-retention"
    );
    assert_eq!(Arc::strong_count(&owner), owners_before);
    assert_eq!(decoded.content_hash(), content_hash);
    assert!(decoded.source().is_none());
    assert!(decoded.share_retained().is_some());
}

#[test]
fn runtime_retained_cost_refuses_import_leases_without_releasing_or_borrowing_their_charge() {
    use crate::budget::{ImageImportCharge, ImageImportPixelLease};
    use crate::{ImageImportBudgetHost, ImageImportLimits};

    let host = ImageImportBudgetHost::new(ImageImportLimits::default()).unwrap();
    let reservation = host
        .reserve(ImageImportCharge::retained_pixels(PIXELS.len()))
        .unwrap();
    let (lease, overlap) =
        ImageImportPixelLease::from_publication(reservation, PIXELS.len()).unwrap();
    drop(overlap);
    let mut decoded = ImageRuntimeCodec::decode_runtime(GOLDEN).unwrap();
    assert!(decoded.try_attach_import_pixel_lease(lease));
    let before = host.snapshot();

    assert_eq!(
        ImageRuntimeCodec::runtime_retained_bytes(&decoded)
            .unwrap_err()
            .diagnostic()
            .code()
            .as_str(),
        "image.runtime-retention"
    );
    assert_eq!(host.snapshot(), before);
    assert_eq!(host.snapshot().active_rgba_bytes(), PIXELS.len());
    drop(decoded);
    assert_eq!(host.snapshot().active_bytes(), 0);
}
