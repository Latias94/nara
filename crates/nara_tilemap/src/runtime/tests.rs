use nara_asset::StableAssetId;
use nara_core::Vec2;

use super::*;

const SOURCE: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const IMAGE_SOURCE: &str = "b73f0f16-09e8-4265-b090-b689b41c197e";
const AUXILIARY: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";

fn reference(source: &str, product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), product).unwrap()
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct(AUXILIARY).unwrap()
}

fn descriptor(atlas: TileAtlasLayout) -> TileSetDescriptor {
    TileSetDescriptor::new(
        reference(SOURCE, ImportedProductId::PRIMARY),
        reference(IMAGE_SOURCE, ImportedProductId::PRIMARY),
        atlas,
    )
    .unwrap()
}

#[test]
fn descriptor_defaults_and_lowering_preserve_actual_atlas_and_material_values() {
    let atlas = TileAtlasLayout::grid(Vec2::new(16.0, 8.0), 4, 2)
        .with_margin(Vec2::new(1.0, 2.0))
        .with_spacing(Vec2::new(2.0, 1.0));
    let original = descriptor(atlas);
    assert_eq!(
        original.product(),
        reference(SOURCE, ImportedProductId::PRIMARY)
    );
    assert_eq!(
        original.image_product(),
        reference(IMAGE_SOURCE, ImportedProductId::PRIMARY)
    );
    assert_eq!(original.atlas(), atlas);
    assert_eq!(original.sampler(), SamplerDescriptor::default());
    assert_eq!(original.alpha_mode(), AlphaMode2d::default());
    assert_eq!(original.tint(), Color::WHITE);

    let tint = Color::rgba(-2.0, 3.0, 0.25, 2.0);
    let customized = original
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(AlphaMode2d::Opaque)
        .with_tint(tint)
        .unwrap();
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let lowered = customized.into_tileset(image).unwrap();
    assert_eq!(lowered.atlas(), Some(atlas));
    assert_eq!(lowered.tile_size(), atlas.tile_size);
    assert_eq!(lowered.material().image, Some(image));
    assert_eq!(lowered.material().sampler, SamplerDescriptor::NEAREST_CLAMP);
    assert_eq!(lowered.material().alpha_mode, AlphaMode2d::Opaque);
    assert_eq!(lowered.material().tint, tint);
    assert!(lowered.image_content().is_none());
}

#[test]
fn descriptor_accepts_primary_and_auxiliary_products_without_implicit_primary_membership() {
    for own in [ImportedProductId::PRIMARY, auxiliary()] {
        for image in [ImportedProductId::PRIMARY, auxiliary()] {
            let product = reference(SOURCE, own);
            let image = reference(IMAGE_SOURCE, image);
            let descriptor =
                TileSetDescriptor::new(product, image, TileAtlasLayout::grid(Vec2::ONE, 1, 1))
                    .unwrap();
            assert_eq!(descriptor.product(), product);
            assert_eq!(descriptor.image_product(), image);
            assert!(TileSetDomain::contains_product(&descriptor, own));
            let other = if own == ImportedProductId::PRIMARY {
                auxiliary()
            } else {
                ImportedProductId::PRIMARY
            };
            assert!(!TileSetDomain::contains_product(&descriptor, other));
        }
    }
}

#[test]
fn descriptor_rejects_invalid_layouts_through_the_actual_atlas_validation() {
    let valid = TileAtlasLayout::grid(Vec2::ONE, 1, 1);
    for atlas in [
        TileAtlasLayout {
            columns: 0,
            ..valid
        },
        TileAtlasLayout { rows: 0, ..valid },
        TileAtlasLayout {
            tile_size: Vec2::ZERO,
            ..valid
        },
        TileAtlasLayout {
            tile_size: Vec2::new(-1.0, 1.0),
            ..valid
        },
        TileAtlasLayout {
            tile_size: Vec2::new(f32::NAN, 1.0),
            ..valid
        },
        TileAtlasLayout {
            tile_size: Vec2::new(1.0, f32::INFINITY),
            ..valid
        },
        TileAtlasLayout {
            margin: Vec2::new(-1.0, 0.0),
            ..valid
        },
        TileAtlasLayout {
            spacing: Vec2::new(0.0, -1.0),
            ..valid
        },
        TileAtlasLayout {
            margin: Vec2::splat(f32::INFINITY),
            ..valid
        },
        TileAtlasLayout {
            spacing: Vec2::splat(f32::NAN),
            ..valid
        },
        TileAtlasLayout {
            tile_size: Vec2::splat(f32::MAX),
            columns: 2,
            ..valid
        },
    ] {
        assert!(!atlas.is_valid());
        assert_eq!(
            TileSetDescriptor::new(
                reference(SOURCE, ImportedProductId::PRIMARY),
                reference(IMAGE_SOURCE, ImportedProductId::PRIMARY),
                atlas,
            ),
            Err(TileSetDescriptorError::InvalidAtlas),
        );
    }
}

#[test]
fn descriptor_rejects_every_non_finite_tint_channel_and_revalidates_at_lowering() {
    let valid = descriptor(TileAtlasLayout::grid(Vec2::ONE, 1, 1));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for channel in 0..4 {
            let mut channels = [1.0; 4];
            channels[channel] = invalid;
            let tint = Color::rgba(channels[0], channels[1], channels[2], channels[3]);
            assert_eq!(
                valid.with_tint(tint),
                Err(TileSetDescriptorError::InvalidTint)
            );
            let mut corrupted = valid;
            corrupted.tint = tint;
            assert_eq!(
                corrupted.validate(),
                Err(TileSetDescriptorError::InvalidTint)
            );
            assert_eq!(
                corrupted.validate_image_extent(ImageExtent::new(1, 1)),
                Err(TileSetDescriptorError::InvalidTint),
            );
            assert_eq!(
                corrupted.into_tileset(nara_asset::allocate_anonymous_handle().unwrap()),
                Err(TileSetDescriptorError::InvalidTint),
            );
        }
    }
}

fn assert_extent_agreement(atlas: TileAtlasLayout, extent: ImageExtent, expected: bool) {
    let descriptor = descriptor(atlas);
    assert_eq!(descriptor.validate_image_extent(extent).is_ok(), expected);
    #[cfg(feature = "serde")]
    {
        let source = crate::TileSetSource::new(
            nara_asset::AssetRef::product(descriptor.image_product()),
            atlas,
        );
        assert_eq!(source.validate_image_extent(extent).is_ok(), expected);
    }
}

#[test]
fn exact_image_extent_includes_both_axes_margins_spacing_and_fractional_tiles() {
    let atlas = TileAtlasLayout::grid(Vec2::new(16.0, 8.0), 4, 2)
        .with_margin(Vec2::new(1.0, 2.0))
        .with_spacing(Vec2::new(2.0, 1.0));
    assert_extent_agreement(atlas, ImageExtent::new(72, 21), true);
    for extent in [
        ImageExtent::new(71, 21),
        ImageExtent::new(72, 20),
        ImageExtent::new(0, 21),
        ImageExtent::new(72, 0),
    ] {
        assert_extent_agreement(atlas, extent, false);
    }
    let fractional = TileAtlasLayout::grid(Vec2::new(0.5, 1.5), 3, 2)
        .with_margin(Vec2::new(0.25, 0.5))
        .with_spacing(Vec2::new(0.0, 1.0));
    assert_extent_agreement(fractional, ImageExtent::new(2, 5), true);
}

#[test]
fn exact_image_extent_refuses_rounded_counts_and_non_integer_sizes() {
    let atlas = TileAtlasLayout::grid(Vec2::ONE, 16_777_217, 1);
    assert_eq!(atlas.atlas_size().x, 16_777_216.0);
    assert_extent_agreement(atlas, ImageExtent::new(16_777_216, 1), false);
    assert_extent_agreement(atlas, ImageExtent::new(16_777_217, 1), false);
    assert_extent_agreement(
        TileAtlasLayout::grid(Vec2::ONE, 16_777_216, 1),
        ImageExtent::new(16_777_216, 1),
        true,
    );
    assert_extent_agreement(
        TileAtlasLayout::grid(Vec2::new(1.5, 1.0), 1, 1),
        ImageExtent::new(1, 1),
        false,
    );
}

#[test]
fn exact_image_extent_retains_subnormal_terms_lost_by_f32_and_f64_summation() {
    let tiny = f32::from_bits(1);
    let margin = TileAtlasLayout::grid(Vec2::ONE, 1, 1).with_margin(Vec2::new(tiny, 0.0));
    assert_eq!(margin.atlas_size().x, 1.0);
    assert_eq!(1.0 + 2.0 * f64::from(tiny), 1.0);
    assert_extent_agreement(margin, ImageExtent::new(1, 1), false);

    let spacing = TileAtlasLayout::grid(Vec2::ONE, 2, 1).with_spacing(Vec2::new(tiny, 0.0));
    assert_eq!(spacing.atlas_size().x, 2.0);
    assert_extent_agreement(spacing, ImageExtent::new(2, 1), false);

    let tile = TileAtlasLayout::grid(Vec2::new(tiny, 1.0), 1, 1).with_margin(Vec2::new(0.5, 0.0));
    assert_eq!(tile.atlas_size().x, 1.0);
    assert_extent_agreement(tile, ImageExtent::new(1, 1), false);
}

#[cfg(feature = "serde")]
mod codec_tests {
    use std::mem::{needs_drop, size_of};

    use nara_asset::RuntimeAssetCodec;
    use nara_material::{AddressMode, FilterMode};
    use serde_json::{Value, json};

    use super::*;

    const GOLDEN: &[u8] = concat!(
        r#"{"kind":"runtime_tileset","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara_tilemap","version":"0.1.0"},"payload":{"product":{"source":"2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f","product":{"kind":"primary"}},"image":{"source":"b73f0f16-09e8-4265-b090-b689b41c197e","product":{"kind":"subproduct","id":"011a8cc0-343a-4b95-8adb-3aa6d81977ed"}},"#,
        r#""atlas":{"tile_size":[16.0,8.0],"columns":4,"rows":2,"margin":[1.0,2.0],"spacing":[2.0,1.0]},"sampler":{"min_filter":"nearest","mag_filter":"linear","mipmap_filter":"nearest","address_mode_u":"repeat","address_mode_v":"mirror_repeat"},"alpha_mode":"opaque","tint":{"r":0.25,"g":0.5,"b":0.75,"a":1.0}}}"#,
    )
    .as_bytes();

    fn golden_descriptor() -> TileSetDescriptor {
        TileSetDescriptor::new(
            reference(SOURCE, ImportedProductId::PRIMARY),
            reference(IMAGE_SOURCE, auxiliary()),
            TileAtlasLayout::grid(Vec2::new(16.0, 8.0), 4, 2)
                .with_margin(Vec2::new(1.0, 2.0))
                .with_spacing(Vec2::new(2.0, 1.0)),
        )
        .unwrap()
        .with_sampler(SamplerDescriptor {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::MirrorRepeat,
        })
        .with_alpha_mode(AlphaMode2d::Opaque)
        .with_tint(Color::rgba(0.25, 0.5, 0.75, 1.0))
        .unwrap()
    }

    fn golden_value() -> Value {
        serde_json::from_slice(GOLDEN).unwrap()
    }

    fn rejection_code(bytes: &[u8]) -> String {
        TileSetRuntimeCodec::decode_runtime(bytes)
            .unwrap_err()
            .diagnostic()
            .code()
            .as_str()
            .to_owned()
    }

    fn assert_metadata_rejected(value: &Value) {
        assert_eq!(
            rejection_code(&serde_json::to_vec(value).unwrap()),
            "tileset.runtime-metadata",
        );
    }

    #[test]
    fn nonempty_golden_round_trips_exact_product_atlas_and_material_content() {
        let expected = golden_descriptor();
        let decoded = TileSetRuntimeCodec::decode_runtime(GOLDEN).unwrap();
        assert_eq!(decoded, expected);
        decoded
            .validate_image_extent(ImageExtent::new(72, 21))
            .unwrap();
        assert_eq!(
            TileSetRuntimeCodec::encode_runtime(&expected)
                .unwrap()
                .as_ref(),
            GOLDEN
        );
        assert_eq!(
            TileSetRuntimeCodec::encode_runtime(&decoded)
                .unwrap()
                .as_ref(),
            GOLDEN
        );
    }

    #[test]
    fn complete_membership_uses_own_source_and_product_without_claiming_the_image() {
        for own in [ImportedProductId::PRIMARY, auxiliary()] {
            for image in [ImportedProductId::PRIMARY, auxiliary()] {
                let descriptor = TileSetDescriptor::new(
                    reference(SOURCE, own),
                    reference(IMAGE_SOURCE, image),
                    TileAtlasLayout::grid(Vec2::ONE, 1, 1),
                )
                .unwrap();
                let bytes = TileSetRuntimeCodec::encode_runtime(&descriptor).unwrap();
                let decoded = TileSetRuntimeCodec::decode_runtime(&bytes).unwrap();
                assert_eq!(decoded, descriptor);
                assert!(TileSetRuntimeCodec::contains_runtime_product(
                    &decoded,
                    reference(SOURCE, own)
                ));
                assert!(!TileSetRuntimeCodec::contains_runtime_product(
                    &decoded,
                    reference(IMAGE_SOURCE, own)
                ));
                assert!(!TileSetRuntimeCodec::contains_runtime_product(
                    &decoded,
                    decoded.image_product()
                ));
                let other = if own == ImportedProductId::PRIMARY {
                    auxiliary()
                } else {
                    ImportedProductId::PRIMARY
                };
                assert!(!TileSetRuntimeCodec::contains_runtime_product(
                    &decoded,
                    reference(SOURCE, other)
                ));
            }
        }
    }

    #[test]
    fn required_product_inventory_visits_exactly_the_image_once_and_preserves_callback_failure() {
        let descriptor = golden_descriptor();
        let mut seen = None;
        TileSetRuntimeCodec::visit_required_runtime_products(&descriptor, |reference| {
            assert!(seen.is_none());
            seen = Some(reference);
            Ok::<(), ()>(())
        })
        .unwrap();
        assert_eq!(seen, Some(descriptor.image_product()));

        let failure = String::from("concrete caller failure");
        let mut calls = 0;
        let result =
            TileSetRuntimeCodec::visit_required_runtime_products(&descriptor, |reference| {
                calls += 1;
                assert_eq!(reference, descriptor.image_product());
                Err(&failure)
            });
        assert_eq!(calls, 1);
        assert!(std::ptr::eq(result.unwrap_err(), &failure));
    }

    #[test]
    fn core_contract_rejects_wrong_kind_version_and_engine_requirements() {
        for (key, value, expected) in [
            ("kind", json!("tileset"), "tileset.runtime-kind"),
            ("format_version", json!(2), "tileset.runtime-version"),
            ("format_version", json!(0), "tileset.runtime-metadata"),
            (
                "engine_min_version",
                json!("9999.0.0"),
                "tileset.runtime-engine-version",
            ),
            (
                "engine_min_version",
                json!("not-semver"),
                "tileset.runtime-metadata",
            ),
        ] {
            let mut value_to_decode = golden_value();
            value_to_decode[key] = value;
            assert_eq!(
                rejection_code(&serde_json::to_vec(&value_to_decode).unwrap()),
                expected,
                "{key}"
            );
        }
    }

    #[test]
    fn all_records_reject_unknown_and_missing_fields() {
        for pointer in [
            "",
            "/generator",
            "/payload",
            "/payload/product",
            "/payload/product/product",
            "/payload/image",
            "/payload/image/product",
            "/payload/atlas",
            "/payload/sampler",
            "/payload/tint",
        ] {
            let baseline = golden_value();
            let record = baseline.pointer(pointer).unwrap().as_object().unwrap();
            let mut unknown = baseline.clone();
            unknown
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".to_owned(), Value::Null);
            assert_metadata_rejected(&unknown);
            for key in record.keys() {
                let mut missing = baseline.clone();
                missing
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert_metadata_rejected(&missing);
            }
        }
        for field in [
            "source_path",
            "source_hash",
            "artifact",
            "cache",
            "image_handle",
            "content_hash",
        ] {
            let mut value = golden_value();
            value["payload"][field] = Value::Null;
            assert_metadata_rejected(&value);
        }
    }

    #[test]
    fn duplicate_record_fields_cannot_be_masked_by_the_last_value() {
        let original = std::str::from_utf8(GOLDEN).unwrap();
        for (single, duplicate) in [
            (
                "\"kind\":\"runtime_tileset\"",
                "\"kind\":\"tileset\",\"kind\":\"runtime_tileset\"",
            ),
            (
                "\"name\":\"nara_tilemap\"",
                "\"name\":\"other\",\"name\":\"nara_tilemap\"",
            ),
            (
                "\"kind\":\"primary\"",
                "\"kind\":\"primary\",\"kind\":\"primary\"",
            ),
            ("\"columns\":4", "\"columns\":0,\"columns\":4"),
            (
                "\"min_filter\":\"nearest\"",
                "\"min_filter\":\"linear\",\"min_filter\":\"nearest\"",
            ),
            ("\"r\":0.25", "\"r\":0.0,\"r\":0.25"),
        ] {
            let bytes = original.replacen(single, duplicate, 1);
            assert_ne!(bytes, original);
            assert_eq!(rejection_code(bytes.as_bytes()), "tileset.runtime-metadata");
        }
    }

    #[test]
    fn positional_arrays_cannot_impersonate_records_while_atlas_vec2_tuples_remain_valid() {
        let value = golden_value();
        for (pointer, positional) in [
            (
                "",
                json!([
                    "runtime_tileset",
                    1,
                    "0.1.0",
                    value["generator"],
                    value["payload"]
                ]),
            ),
            ("/generator", json!(["nara_tilemap", "0.1.0"])),
            (
                "/payload",
                json!([
                    value["payload"]["product"],
                    value["payload"]["image"],
                    value["payload"]["atlas"],
                    value["payload"]["sampler"],
                    "opaque",
                    value["payload"]["tint"]
                ]),
            ),
            ("/payload/product", json!([SOURCE, {"kind": "primary"}])),
            ("/payload/product/product", json!(["primary"])),
            (
                "/payload/image",
                json!([IMAGE_SOURCE, {"kind": "subproduct", "id": AUXILIARY}]),
            ),
            ("/payload/image/product", json!(["subproduct", AUXILIARY])),
            (
                "/payload/atlas",
                json!([[16.0, 8.0], 4, 2, [1.0, 2.0], [2.0, 1.0]]),
            ),
            (
                "/payload/sampler",
                json!(["nearest", "linear", "nearest", "repeat", "mirror_repeat"]),
            ),
            ("/payload/tint", json!([0.25, 0.5, 0.75, 1.0])),
        ] {
            let mut invalid = value.clone();
            *invalid.pointer_mut(pointer).unwrap() = positional;
            assert_metadata_rejected(&invalid);
        }
    }

    #[test]
    fn explicit_product_identities_are_required_and_canonical_on_both_sides() {
        for pointer in ["/payload/product", "/payload/image"] {
            for invalid in [
                Value::Null,
                json!(SOURCE),
                json!({"source": SOURCE}),
                json!({"source": "00000000-0000-0000-0000-000000000000", "product": {"kind": "primary"}}),
                json!({"source": SOURCE.to_uppercase(), "product": {"kind": "primary"}}),
                json!({"source": SOURCE, "product": {"kind": "primary", "id": AUXILIARY}}),
                json!({"source": SOURCE, "product": {"kind": "subproduct", "id": "00000000-0000-0000-0000-000000000000"}}),
                json!({"source": SOURCE, "product": {"kind": "subproduct", "id": AUXILIARY.to_uppercase()}}),
            ] {
                let mut value = golden_value();
                *value.pointer_mut(pointer).unwrap() = invalid;
                assert_metadata_rejected(&value);
            }
        }
    }

    #[test]
    fn decode_reuses_atlas_validation_and_rejects_invalid_material_values() {
        for (pointer, invalid) in [
            ("/payload/atlas/columns", json!(0)),
            ("/payload/atlas/rows", json!(0)),
            ("/payload/atlas/tile_size", json!([0.0, 8.0])),
            ("/payload/atlas/margin", json!([-1.0, 2.0])),
            ("/payload/atlas/spacing", json!([2.0, -1.0])),
        ] {
            let mut value = golden_value();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert_eq!(
                rejection_code(&serde_json::to_vec(&value).unwrap()),
                "tileset.runtime-atlas"
            );
        }
        for (pointer, invalid) in [
            ("/payload/atlas/tile_size", json!([1.0])),
            ("/payload/atlas/tile_size", json!([1.0, 2.0, 3.0])),
            ("/payload/atlas/columns", json!(-1)),
            ("/payload/atlas/rows", json!(1.5)),
            ("/payload/sampler/min_filter", json!("cubic")),
            ("/payload/sampler/address_mode_v", json!("border")),
            ("/payload/alpha_mode", json!("mask")),
            ("/payload/tint/r", Value::Null),
            ("/payload/tint/g", json!("NaN")),
        ] {
            let mut value = golden_value();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert_metadata_rejected(&value);
        }
        for pointer in [
            "/payload/tint/r",
            "/payload/tint/g",
            "/payload/tint/b",
            "/payload/tint/a",
        ] {
            let mut value = golden_value();
            *value.pointer_mut(pointer).unwrap() = json!(1e39);
            assert!(
                TileSetRuntimeCodec::decode_runtime(&serde_json::to_vec(&value).unwrap()).is_err()
            );
        }
    }

    #[test]
    fn encoding_and_costing_revalidate_descriptor_semantics() {
        let mut invalid = golden_descriptor();
        invalid.atlas.columns = 0;
        assert_eq!(
            TileSetRuntimeCodec::encode_runtime(&invalid)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "tileset.runtime-atlas"
        );
        assert!(TileSetRuntimeCodec::runtime_retained_bytes(&invalid).is_err());
        let mut invalid = golden_descriptor();
        invalid.tint.a = f32::NAN;
        assert_eq!(
            TileSetRuntimeCodec::encode_runtime(&invalid)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "tileset.runtime-tint"
        );
        assert!(TileSetRuntimeCodec::runtime_retained_bytes(&invalid).is_err());
    }

    #[test]
    fn encoded_input_and_temporary_shape_are_bounded_before_payload_decode() {
        let limit = TileSetRuntimeCodec::runtime_limits().encoded_bytes().get();
        let mut padded = GOLDEN.to_vec();
        padded.resize(limit, b' ');
        assert!(TileSetRuntimeCodec::decode_runtime(&padded).is_ok());
        padded.push(b' ');
        assert_eq!(rejection_code(&padded), "tileset.runtime-metadata");

        let mut trailing = GOLDEN.to_vec();
        trailing.extend_from_slice(b"{}");
        assert_eq!(rejection_code(&trailing), "tileset.runtime-metadata");
        let nested = format!("{}0{}", "[".repeat(9), "]".repeat(9));
        assert_eq!(
            rejection_code(nested.as_bytes()),
            "tileset.runtime-metadata"
        );
        assert_metadata_rejected(&json!([0, 0, 0, 0, 0, 0, 0, 0, 0]));
        let mut oversized_string = golden_value();
        oversized_string["generator"]["name"] = json!("x".repeat(257));
        assert_metadata_rejected(&oversized_string);
        let leaf = json!({"a": 0, "b": 0, "c": 0, "d": 0, "e": 0, "f": 0, "g": 0, "h": 0});
        assert_metadata_rejected(
            &json!({"a": leaf, "b": leaf, "c": leaf, "d": leaf, "e": leaf, "f": leaf, "g": leaf, "h": leaf}),
        );
        let text = "x".repeat(256);
        assert_metadata_rejected(
            &json!({"a": text, "b": text, "c": text, "d": text, "e": text, "f": text, "g": text, "h": text}),
        );
    }

    #[test]
    fn runtime_promises_cover_fixed_retention_bounded_work_and_encoded_output() {
        assert_eq!(TileSetRuntimeCodec::RUNTIME_KIND, "runtime_tileset");
        assert_eq!(TileSetRuntimeCodec::RUNTIME_VERSION, 1);
        let limits = TileSetRuntimeCodec::runtime_limits();
        let inline = size_of::<TileSetDescriptor>();
        assert!(!needs_drop::<TileSetDescriptor>());
        assert_eq!(limits.encoded_bytes().get(), 4 * 1024);
        assert_eq!(
            limits.work_bytes().get(),
            8 * 4096 + 256 * 128 + 1024 * 8 + inline
        );
        assert_eq!(limits.retained_bytes().get(), inline);
        for descriptor in [
            descriptor(TileAtlasLayout::grid(Vec2::ONE, 1, 1)),
            golden_descriptor(),
            descriptor(TileAtlasLayout::grid(Vec2::splat(0.5), u32::MAX, u32::MAX)),
        ] {
            let encoded = TileSetRuntimeCodec::encode_runtime(&descriptor).unwrap();
            let decoded = TileSetRuntimeCodec::decode_runtime(&encoded).unwrap();
            assert!(encoded.len() <= limits.encoded_bytes().get());
            assert_eq!(
                TileSetRuntimeCodec::runtime_retained_bytes(&descriptor).unwrap(),
                inline
            );
            assert_eq!(
                TileSetRuntimeCodec::runtime_retained_bytes(&decoded).unwrap(),
                inline
            );
        }
    }
}
