use std::collections::BTreeMap;

use nara_core::{AssetId, Color, EntityUuid, Vec2, Vec3, Vec4};
use nara_ecs::Component;
use nara_reflect::{
    AssetReferenceDemand, Bytes, ComponentCodecError, ComponentFieldEditorHint,
    ComponentRegistryError, ComponentSchemaCatalog, ComponentValue, ComponentValueKind,
    ComponentValueShape, FieldPath, PackedF32, PackedI32, PackedU16, PackedU32, Schema,
    validate_component_value,
};

#[derive(Debug, Default, PartialEq)]
struct RuntimeOnly(u32);

type Maybe<T> = Option<T>;

#[derive(Component, Schema, Debug)]
#[schema(id = "game.NestedOptional", version = 1)]
struct NestedOptional {
    scalar: Maybe<Maybe<i32>>,
    items: Vec<Maybe<Maybe<i32>>>,
}

#[test]
fn nested_optional_fields_fail_instead_of_collapsing_present_null() {
    let schema = NestedOptional::type_schema().unwrap();
    assert!(matches!(
        ComponentSchemaCatalog::from_components([schema]),
        Err(ComponentRegistryError::InvalidComponentFieldShape { .. })
    ));
    let mut component = NestedOptional {
        scalar: Some(None),
        items: Vec::new(),
    };
    for field in ["scalar", "items"] {
        let path = FieldPath::from_fields([field]);
        assert!(
            component
                .get_field(&path)
                .unwrap_err()
                .to_string()
                .contains("nested Option"),
            "{field}",
        );
        assert!(
            component
                .set_field(&path, ComponentValue::Null)
                .unwrap_err()
                .to_string()
                .contains("nested Option"),
            "{field}",
        );
    }
    assert_eq!(component.scalar, Some(None));
    assert!(
        component
            .encode_component()
            .unwrap_err()
            .to_string()
            .contains("nested Option")
    );
    let flattened = ComponentValue::map([
        ("scalar", ComponentValue::Null),
        ("items", ComponentValue::List(vec![])),
    ]);
    assert!(NestedOptional::decode_component((&flattened).into()).is_err());
}

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(id = "game.Motion", version = 2, group = "Gameplay", default)]
struct Motion {
    heading: Vec2,
    #[schema(range = -3.0..=10.0, step = 0.5, unit = "px/s", tooltip = "Movement speed")]
    speed: f32,
    #[schema(choices = ["walk", "climb"], default = String::from("walk"))]
    mode: String,
    #[schema(choices("assist", "manual"), default = Some(String::from("assist")))]
    guidance: Option<String>,
    #[schema(color_srgb)]
    tint: Color,
    #[schema(read_only)]
    counter: u32,
    #[schema(skip, default = RuntimeOnly(3))]
    runtime: RuntimeOnly,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            heading: Vec2::new(1.0, 0.0),
            speed: 2.5,
            mode: "walk".to_owned(),
            guidance: Some("assist".to_owned()),
            tint: Color::rgba(2.0, 0.1, 0.2, 0.75),
            counter: 9,
            runtime: RuntimeOnly(3),
        }
    }
}

#[test]
fn derived_schema_advertises_postcard_in_metadata_only_builds() {
    let schema = Motion::type_schema().unwrap();
    assert!(schema.supports_postcard());

    #[cfg(feature = "serde")]
    {
        let catalog = ComponentSchemaCatalog::from_components([schema.clone()]).unwrap();
        let encoded = catalog.to_json_string().unwrap();
        assert!(encoded.contains("\"supports_postcard\": true"));
        let exported = ComponentSchemaCatalog::from_json_bytes(encoded.as_bytes()).unwrap();
        assert!(exported.components()[0].supports_postcard());

        let mut source_only = schema;
        source_only.supports_postcard = false;
        let source_only = ComponentSchemaCatalog::from_components([source_only]).unwrap();
        let legacy = source_only.to_json_string().unwrap();
        assert!(!legacy.contains("\"supports_postcard\""));
        let legacy = ComponentSchemaCatalog::from_json_bytes(legacy.as_bytes()).unwrap();
        assert!(!legacy.components()[0].supports_postcard());
        assert_ne!(legacy.fingerprint(), exported.fingerprint());
        #[cfg(feature = "postcard")]
        assert_eq!(
            legacy.components()[0].postcard_schema_hash(),
            exported.components()[0].postcard_schema_hash()
        );
    }
}

#[test]
fn exported_defaults_restore_sparse_values_without_overwriting_explicit_null() {
    let schema = Motion::type_schema().unwrap();
    assert_eq!(schema.group(), Some("Gameplay"));
    let catalog = ComponentSchemaCatalog::from_components([schema.clone()]).unwrap();
    let schema = &catalog.components()[0];
    assert_eq!(
        schema
            .fields()
            .iter()
            .map(|field| field.id().as_str())
            .collect::<Vec<_>>(),
        ["heading", "speed", "mode", "guidance", "tint", "counter"]
    );
    assert_eq!(schema.fields()[1].step(), Some(0.5));
    assert_eq!(schema.fields()[1].unit(), Some("px/s"));
    assert_eq!(schema.fields()[1].tooltip(), Some("Movement speed"));
    assert_eq!(
        schema.fields()[4].editor_hint(),
        Some(ComponentFieldEditorHint::LinearRgba)
    );
    assert!(schema.fields()[5].is_read_only());
    assert!(schema.authoring_initial_value().is_none());

    let sparse = ComponentValue::map([("guidance", ComponentValue::Null)]);
    let full = schema.with_defaults(sparse.clone()).unwrap();
    let expected = Motion {
        guidance: None,
        ..Motion::default()
    };
    assert_eq!(full, expected.encode_component().unwrap());
    assert_eq!(
        Motion::decode_component((&sparse).into()).unwrap(),
        expected
    );
    assert_eq!(Motion::decode_component((&full).into()).unwrap(), expected);
    assert!(
        schema
            .with_defaults(ComponentValue::map([("runtime", ComponentValue::U64(17))]))
            .is_err()
    );

    #[cfg(feature = "serde")]
    {
        let remote =
            ComponentSchemaCatalog::from_json_bytes(catalog.to_json_string().unwrap().as_bytes())
                .unwrap();
        assert_eq!(remote.components()[0].with_defaults(sparse).unwrap(), full);
        assert_eq!(remote.components()[0].schema_hash(), schema.schema_hash());
    }
}

#[test]
fn dynamic_edits_enforce_ranges_choices_and_read_only_without_resetting_skipped_state() {
    let mut component = Motion {
        runtime: RuntimeOnly(50),
        ..Motion::default()
    };
    let schema = Motion::type_schema().unwrap();
    let path = |text: &str| text.parse::<FieldPath>().unwrap();
    component
        .set_field(&path("heading.y"), ComponentValue::f64(2.0).unwrap())
        .unwrap();
    assert_eq!(component.heading, Vec2::new(1.0, 2.0));
    assert_eq!(component.runtime, RuntimeOnly(50));
    assert_eq!(
        component.get_field(&path("tint.r")).unwrap().as_f64(),
        Some(2.0)
    );
    assert!(matches!(
        component.set_field(&path("counter"), ComponentValue::U64(10)),
        Err(ComponentCodecError::ReadOnlyField { field }) if field == "counter"
    ));
    assert!(component.get_field(&path("runtime")).is_err());
    assert!(
        component
            .set_field(&path("speed"), ComponentValue::f64(10.5).unwrap())
            .is_err()
    );
    assert_eq!(component.speed, 2.5);

    let illegal_choice = ComponentValue::String("fly".to_owned());
    assert!(!schema.fields()[2].accepts_value(&illegal_choice));
    assert!(
        component
            .set_field(&path("mode"), illegal_choice.clone())
            .is_err()
    );
    assert!(
        component
            .set_field(&path("guidance"), illegal_choice.clone())
            .is_err()
    );
    assert_eq!(component.mode, "walk");
    let invalid = ComponentValue::map([("mode", illegal_choice.clone())]);
    assert!(validate_component_value(&schema, &invalid).is_err());
    assert!(Motion::decode_component((&invalid).into()).is_err());

    let mut bad_default = schema.clone();
    bad_default.fields[2].default_value = Some(illegal_choice);
    assert!(matches!(
        ComponentSchemaCatalog::from_components([bad_default]),
        Err(ComponentRegistryError::InvalidComponentFieldDefault { .. })
    ));
    let mut bad_step = schema.clone();
    bad_step.fields[1].step = Some(nara_reflect::ComponentFloat::new(-0.5).unwrap());
    assert!(matches!(
        ComponentSchemaCatalog::from_components([bad_step]),
        Err(ComponentRegistryError::InvalidFieldStep { .. })
    ));
    let mut bad_choices = schema;
    bad_choices.fields[0].choices = vec!["any".to_owned()];
    assert!(matches!(
        ComponentSchemaCatalog::from_components([bad_choices]),
        Err(ComponentRegistryError::InvalidFieldChoices { .. })
    ));
}

#[test]
fn schema_hash_distinguishes_postcard_field_order_and_defaults() {
    let schema = Motion::type_schema().unwrap();
    assert_eq!(
        schema.schema_hash(),
        Motion::type_schema().unwrap().schema_hash()
    );
    let mut reordered = schema.clone();
    reordered.fields.swap(0, 1);
    assert_ne!(schema.schema_hash(), reordered.schema_hash());
    let mut changed_default = schema.clone();
    changed_default.fields[1].default_value = Some(ComponentValue::f64(1.0).unwrap());
    assert_ne!(schema.schema_hash(), changed_default.schema_hash());
}

type AssetMap = Option<BTreeMap<String, AssetId>>;

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(id = "game.Bulk", version = 1)]
struct Bulk {
    source: AssetId,
    entity: EntityUuid,
    cells: PackedU16,
    unsigned: PackedU32,
    signed: PackedI32,
    heights: PackedF32,
    data: Bytes,
    axes: Vec3,
    plane: Vec4,
    images: AssetMap,
}

fn bulk() -> Bulk {
    let source: AssetId = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f".parse().unwrap();
    Bulk {
        source,
        entity: "a3655815-bbfd-456d-a911-93c231c17091".parse().unwrap(),
        cells: vec![1, u16::MAX].into(),
        unsigned: vec![0, u32::MAX].into(),
        signed: vec![i32::MIN, i32::MAX].into(),
        heights: vec![f32::MIN, -0.0, f32::MAX].into(),
        data: vec![0, 255].into(),
        axes: Vec3::new(1.0, 2.0, 3.0),
        plane: Vec4::new(4.0, 5.0, 6.0, 7.0),
        images: Some(BTreeMap::from([("other.png".to_owned(), source)])),
    }
}

#[test]
fn stable_references_dense_fields_and_paths_share_the_same_value_tree() {
    let mut component = bulk();
    let schema = Bulk::type_schema().unwrap();
    ComponentSchemaCatalog::from_components([schema.clone()]).unwrap();
    assert_eq!(
        schema.fields()[0].shape(),
        &ComponentValueShape::StableAssetRef {
            demand: AssetReferenceDemand::Immediate
        }
    );
    let locator =
        nara_reflect::encode_asset_reference_value(&nara_asset::AssetRef::path("a.png").unwrap())
            .unwrap();
    assert!(!schema.fields()[0].accepts_value(&locator));
    assert_eq!(schema.fields()[1].shape(), &ComponentValueShape::EntityRef);
    assert_eq!(
        schema.fields()[9].default_value(),
        Some(&ComponentValue::Null)
    );
    let encoded = component.encode_component().unwrap();
    assert!(
        matches!(encoded.get("source"), Some(ComponentValue::AssetReference(id)) if *id == component.source)
    );
    assert_eq!(
        encoded.get("cells").unwrap().kind(),
        ComponentValueKind::PackedU16
    );
    assert_eq!(
        Bulk::decode_component((&encoded).into()).unwrap(),
        component
    );

    let cell = FieldPath::parse_for(schema.id(), "game.Bulk.cells[1]").unwrap();
    assert_eq!(
        component.get_field(&cell).unwrap(),
        ComponentValue::U64(u16::MAX.into())
    );
    assert!(
        component
            .set_field(&cell, ComponentValue::U64(u64::MAX))
            .is_err()
    );
    component.set_field(&cell, ComponentValue::U64(8)).unwrap();
    assert_eq!(component.cells.as_ref(), [1, 8]);
    assert!(component.set_field(&cell, ComponentValue::I64(-1)).is_err());
    let asset_map_path = r#"images["other.png"]"#.parse::<FieldPath>().unwrap();
    assert_eq!(
        component.get_field(&asset_map_path).unwrap(),
        ComponentValue::AssetReference(component.source)
    );
    assert_eq!(asset_map_path.to_string(), r#"images["other.png"]"#);

    let escaped = FieldPath::from_fields(["images", "dot.[]\\\"\n😀\u{7}"]);
    assert_eq!(escaped.to_string().parse::<FieldPath>().unwrap(), escaped);
    for malformed in [
        "",
        "cells[-1]",
        "cells[]",
        "cells[4294967296]",
        "images.",
        "images..field",
        "[0]",
        "images[\"unterminated]",
    ] {
        assert!(malformed.parse::<FieldPath>().is_err(), "{malformed}");
    }
}

#[cfg(feature = "serde")]
#[test]
fn packed_and_uuid_values_round_trip_as_tagged_catalog_values() {
    let encoded = bulk().encode_component().unwrap();
    let json = serde_json::to_string(&encoded).unwrap();
    assert_eq!(
        serde_json::from_str::<ComponentValue>(&json).unwrap(),
        encoded
    );
    assert_eq!(
        ron::from_str::<ComponentValue>(&ron::to_string(&encoded).unwrap()).unwrap(),
        encoded
    );
    for malformed in [
        r#"{"type":"asset_ref","value":"00000000-0000-0000-0000-000000000000"}"#,
        r#"{"type":"packed_u16","value":[-1]}"#,
        r#"{"type":"packed_u16","value":[65536]}"#,
        r#"{"type":"packed_i32","value":[1.0]}"#,
        r#"{"type":"packed_f32","value":[3.5e38]}"#,
        r#"{"type":"bytes","value":[256]}"#,
    ] {
        assert!(
            serde_json::from_str::<ComponentValue>(malformed).is_err(),
            "{malformed}"
        );
    }
    assert!(serde_json::to_string(&ComponentValue::PackedF32(vec![f32::NAN])).is_err());
}

#[cfg(feature = "postcard")]
#[test]
fn postcard_reads_typed_components_and_rejects_bad_payloads() {
    let component = bulk();
    let encoded = component.encode_postcard().unwrap();
    assert_eq!(&encoded[..16], component.source.as_uuid().as_bytes());
    assert_eq!(&encoded[16..32], component.entity.as_uuid().as_bytes());
    assert_eq!(&encoded[32..37], &[4, 1, 0, 255, 255]);
    assert_eq!(Bulk::decode_postcard(&encoded).unwrap(), component);
    for prefix in 0..encoded.len() {
        assert!(
            Bulk::decode_postcard(&encoded[..prefix]).is_err(),
            "prefix {prefix}"
        );
    }
    let mut nil = encoded.clone();
    nil[..16].fill(0);
    assert!(Bulk::decode_postcard(&nil).is_err());
    let mut odd_packed_length = encoded.clone();
    odd_packed_length[32] = 3;
    assert!(Bulk::decode_postcard(&odd_packed_length).is_err());
    let mut trailing = encoded;
    trailing.push(0);
    assert!(Bulk::decode_postcard(&trailing).is_err());
    let bad_float = Bulk {
        heights: vec![f32::INFINITY].into(),
        ..component
    };
    assert!(bad_float.encode_postcard().is_err());

    let motion = Motion {
        runtime: RuntimeOnly(55),
        ..Motion::default()
    };
    let bytes = motion.encode_postcard().unwrap();
    assert_eq!(Motion::decode_postcard(&bytes).unwrap(), Motion::default());
    let mut bad_range = bytes.clone();
    bad_range[8..12].copy_from_slice(&15.0_f32.to_le_bytes());
    assert!(Motion::decode_postcard(&bad_range).is_err());
    let mut bad_choice = bytes;
    bad_choice[13..17].copy_from_slice(b"fly!");
    assert!(Motion::decode_postcard(&bad_choice).is_err());
}

#[cfg(feature = "postcard")]
#[test]
fn exported_schema_writes_the_same_postcard_as_the_native_codec() {
    use nara_asset::AssetRef;
    use nara_reflect::PostcardReference;

    #[derive(Component, Schema, Debug, PartialEq)]
    #[schema(id = "game.PostcardWidths", version = 1)]
    struct Widths {
        int8: i8,
        uint8: u8,
        int16: i16,
        uint16: u16,
        int32: i32,
        uint32: u32,
        int64: i64,
        uint64: u64,
        float32: f32,
        float64: f64,
        stable: Option<AssetId>,
        legacy: BTreeMap<String, Vec<Option<AssetRef>>>,
    }

    let id = bulk().source;
    let expected = Widths {
        int8: -127,
        uint8: 255,
        int16: -18_000,
        uint16: 42_000,
        int32: -112_345,
        uint32: 3_012_345_678,
        int64: -4_012_345_678,
        uint64: u64::MAX,
        float32: 0.1,
        float64: -0.01,
        stable: Some(id),
        legacy: BTreeMap::from([(
            "parts".to_owned(),
            vec![None, Some(AssetRef::StableId(id.into()))],
        )]),
    };
    let schema = Widths::type_schema().unwrap();
    let mut dynamic_refs = Vec::new();
    let mut native_refs = Vec::new();
    let encoded = schema
        .encode_postcard_value_with_references(&expected.encode_component().unwrap(), |reference| {
            dynamic_refs.push(reference)
        })
        .unwrap();
    expected
        .visit_postcard_references(&mut |reference| native_refs.push(reference))
        .unwrap();
    assert_eq!(dynamic_refs, [PostcardReference::Asset(id); 2]);
    assert_eq!(dynamic_refs, native_refs);
    assert_eq!(encoded, expected.encode_postcard().unwrap());
    assert_eq!(Widths::decode_postcard(&encoded).unwrap(), expected);

    let grounded = Widths {
        legacy: BTreeMap::from([(
            "parts".to_owned(),
            vec![Some(AssetRef::path("tile.png").unwrap())],
        )]),
        ..expected
    };
    assert!(
        schema
            .encode_postcard_value(&grounded.encode_component().unwrap())
            .is_err()
    );
    assert!(grounded.visit_postcard_references(&mut |_| {}).is_err());

    let schema = Motion::type_schema().unwrap();
    let sparse = ComponentValue::map([("guidance", ComponentValue::Null)]);
    let expected = Motion {
        guidance: None,
        ..Motion::default()
    };
    let encoded = schema.encode_postcard_value(&sparse).unwrap();
    assert_eq!(encoded, expected.encode_postcard().unwrap());
    let template = schema
        .clone()
        .with_authoring_initial_value(expected.encode_component().unwrap());
    assert_ne!(schema.schema_hash(), template.schema_hash());
    assert_eq!(
        schema.postcard_schema_hash(),
        template.postcard_schema_hash()
    );

    let bulk = bulk();
    assert_eq!(
        Bulk::type_schema()
            .unwrap()
            .encode_postcard_value(&bulk.encode_component().unwrap())
            .unwrap(),
        bulk.encode_postcard().unwrap()
    );
}

#[cfg(feature = "postcard")]
#[test]
fn native_postcard_map_rejects_unordered_or_duplicate_keys() {
    #[derive(Component, Schema)]
    #[schema(id = "game.PostcardMap", version = 1)]
    struct MapComponent {
        values: BTreeMap<String, u8>,
    }

    let sorted = [2, 1, b'a', 9, 1, b'b', 43];
    assert_eq!(
        MapComponent::decode_postcard(&sorted).unwrap().values.len(),
        2
    );
    for invalid in [[2, 1, b'b', 9, 1, b'a', 43], [2, 1, b'a', 9, 1, b'a', 43]] {
        assert!(MapComponent::decode_postcard(&invalid).is_err());
    }
}

#[derive(Component, Schema)]
#[schema(id = "game.Marker", version = 1)]
struct Marker;

#[test]
fn unit_components_use_an_empty_record_and_no_postcard_bytes() {
    let wire = Marker.encode_component().unwrap();
    assert_eq!(wire, ComponentValue::Map(BTreeMap::new()));
    assert!(Marker::decode_component((&wire).into()).is_ok());
    #[cfg(feature = "postcard")]
    {
        assert!(Marker.encode_postcard().unwrap().is_empty());
        assert!(Marker::decode_postcard(&[]).is_ok());
        assert!(Marker::decode_postcard(&[0]).is_err());
    }
}
