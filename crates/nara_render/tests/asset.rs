#![cfg(feature = "animation")]
#![forbid(unsafe_code)]

use std::{mem::size_of, sync::Arc, time::Duration};

use nara_asset::{
    AssetBuildSource, AssetDomain, AssetImportError, AssetImportSource, AssetMeta,
    AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetProductRef, AssetSourceKind,
    ImportedProductId, RuntimeAssetCodec, SourceAssetBuild, SourceAssetImporter, StableAssetId,
};
use nara_core::ByteLimit;
use nara_render::animation::{
    Clip, Frame, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAMES_PER_CLIP, MAX_TOTAL_FRAMES,
    PlaybackMode, SpriteAnimationCodec, SpriteAnimationDomain, SpriteAnimationSet,
    SpriteAnimationSettings,
};
use serde_json::{Value, json};

const SOURCE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const OTHER_SOURCE_ID: &str = "c4d051e8-73ee-4b16-9d13-8c5fcb8a6a32";
const AUXILIARY_ID: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";
const SOURCE: &[u8] = include_bytes!("fixtures/source/v1.json");
const RUNTIME: &[u8] = include_bytes!("fixtures/runtime/v1.json");

fn reference(source: &str, product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), product).unwrap()
}

fn primary(source: &str) -> AssetProductRef {
    reference(source, ImportedProductId::PRIMARY)
}

fn metadata(source: &str, kind: AssetSourceKind, settings: Value) -> AssetMetaIdentityCandidate {
    let identity = AssetMetaIdentity::new(
        StableAssetId::parse_str(source).unwrap(),
        AssetPath::new("animation/actor.arbitrary-extension").unwrap(),
        kind,
    );
    let encoded = AssetMeta::new(identity, settings).to_json_string().unwrap();
    AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap()
}

fn animation_metadata(source: &str, settings: Value) -> AssetMetaIdentityCandidate {
    metadata(
        source,
        AssetSourceKind::Other(SpriteAnimationDomain::SOURCE_KIND.to_owned()),
        settings,
    )
}

fn prepare_with(
    metadata: &AssetMetaIdentityCandidate,
    bytes: &[u8],
) -> Result<SpriteAnimationSet, AssetImportError<()>> {
    let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, ()> {
        panic!("sprite animation imports must not read any auxiliary source or Image")
    };
    let mut source = AssetImportSource::new(metadata, Arc::from(bytes), &mut reader);
    SpriteAnimationDomain::prepare(&mut source)
}

fn prepare(bytes: &[u8]) -> Result<SpriteAnimationSet, AssetImportError<()>> {
    prepare_with(&animation_metadata(SOURCE_ID, json!({})), bytes)
}

fn source_document(clips: Vec<Value>) -> Value {
    json!({"format_version": 1, "clips": clips})
}

fn frame() -> Value {
    json!({"min": [0.0, 0.0], "size": [0.25, 1.0], "duration_nanos": 100_000_000})
}

fn clip(name: &str, frames: Vec<Value>) -> Value {
    json!({"name": name, "mode": "once", "frames": frames})
}

fn single_clip() -> Value {
    source_document(vec![clip("idle", vec![frame()])])
}

fn runtime_document(mut source: Value, product: AssetProductRef) -> Value {
    source["product"] = serde_json::to_value(product).unwrap();
    source
}

fn assert_both_reject(source: Value) {
    let source_bytes = serde_json::to_vec(&source).unwrap();
    assert!(prepare(&source_bytes).is_err(), "source accepted {source}");
    let runtime = runtime_document(source, primary(SOURCE_ID));
    let runtime_bytes = serde_json::to_vec(&runtime).unwrap();
    assert!(
        SpriteAnimationCodec::decode_runtime(&runtime_bytes).is_err(),
        "Runtime accepted {runtime}"
    );
}

fn import_code(error: AssetImportError<()>) -> String {
    match error {
        AssetImportError::Domain(diagnostic) => diagnostic.code().as_str().to_owned(),
        AssetImportError::Source(()) => panic!("unexpected auxiliary source read"),
    }
}

#[test]
fn public_import_uses_each_actual_metadata_primary_and_preserves_authored_frame_order() {
    for source in [SOURCE_ID, OTHER_SOURCE_ID] {
        let metadata = animation_metadata(source, json!({}));
        let set = prepare_with(&metadata, SOURCE).unwrap();
        assert_eq!(set.product(), primary(source));
        assert_eq!(set.clips().len(), 2);
        assert_eq!(
            set.clips().map(|(name, _)| name).collect::<Vec<_>>(),
            ["idle", "walk"]
        );
        let idle = set.get("idle").unwrap();
        assert_eq!(idle.mode(), PlaybackMode::Once);
        assert_eq!(idle.duration(), Duration::from_millis(350));
        assert_eq!(
            idle.frames()
                .iter()
                .map(|frame| (frame.region().min.x, frame.duration()))
                .collect::<Vec<_>>(),
            [
                (0.5, Duration::from_millis(100)),
                (0.0, Duration::from_millis(250))
            ]
        );
        assert_eq!(idle.sample(Duration::from_millis(100)).region.min.x, 0.0);
        assert!(idle.sample(Duration::from_millis(350)).finished);
        let walk = set.get("walk").unwrap();
        assert_eq!(walk.mode(), PlaybackMode::Loop);
        assert_eq!(walk.duration(), Duration::from_millis(500));
        assert_eq!(walk.sample(Duration::from_millis(50)).region.min.x, 0.25);
        assert_eq!(walk.sample(Duration::from_millis(500)).region.min.x, 0.0);
    }
}

#[test]
fn public_tracked_build_matches_import_and_visits_only_its_real_complete_product() {
    assert_eq!(SpriteAnimationDomain::SOURCE_KIND, "nara.sprite-animation");
    assert_eq!(
        SpriteAnimationDomain::BUILD_ID,
        "nara.sprite-animation.source-build"
    );
    assert_eq!(SpriteAnimationDomain::BUILD_REVISION, 1);
    let metadata = animation_metadata(OTHER_SOURCE_ID, json!({}));
    let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Option<Arc<[u8]>>, ()> {
        panic!("tracked animation builds must not read auxiliary source or Image content")
    };
    let mut source = AssetBuildSource::new(&metadata, Arc::from(SOURCE), &mut reader);
    let built = SpriteAnimationDomain::build(&mut source).unwrap();
    assert_eq!(built, prepare_with(&metadata, SOURCE).unwrap());
    let mut products = Vec::new();
    SpriteAnimationDomain::visit_products(&built, |product| {
        products.push(product);
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(products, [primary(OTHER_SOURCE_ID)]);
    SpriteAnimationDomain::visit_source_dependencies(&built, |_, _, _| -> Result<(), ()> {
        panic!("animation regions do not own the Sprite's Image dependency")
    })
    .unwrap();
    SpriteAnimationCodec::visit_runtime_dependencies(&built, |_, _| -> Result<(), ()> {
        panic!("processed animation content has no outgoing Product dependency")
    })
    .unwrap();
    let settings = SpriteAnimationDomain::normalize_settings(SpriteAnimationSettings {}).unwrap();
    assert_eq!(serde_json::to_value(settings).unwrap(), json!({}));
}

#[test]
fn product_inventory_propagates_the_original_visitor_failure_without_extra_visits() {
    let set = prepare(SOURCE).unwrap();
    let failure = Box::new(37_u32);
    let original = std::ptr::from_ref(failure.as_ref());
    let mut failure = Some(failure);
    let mut visits = 0;
    let returned = SpriteAnimationDomain::visit_products(&set, |product| {
        visits += 1;
        assert_eq!(product, set.product());
        Err::<(), _>(failure.take().unwrap())
    })
    .unwrap_err();
    assert_eq!(visits, 1);
    assert!(std::ptr::eq(returned.as_ref(), original));
    assert_eq!(*returned, 37);
}

#[test]
fn source_kind_is_exact_and_never_guessed_from_an_extension_or_image() {
    for kind in [
        AssetSourceKind::Image,
        AssetSourceKind::Other("nara.sprite-animation.other".to_owned()),
    ] {
        let metadata = metadata(SOURCE_ID, kind, json!({}));
        assert_eq!(
            import_code(prepare_with(&metadata, SOURCE).unwrap_err()),
            "nara.sprite-animation.source-kind"
        );
    }
}

#[test]
fn settings_accept_only_the_empty_object_through_public_metadata_admission() {
    for settings in [
        json!(null),
        json!([]),
        json!(false),
        json!(0),
        json!(""),
        json!({"fps": 60}),
    ] {
        let metadata = animation_metadata(SOURCE_ID, settings.clone());
        assert_eq!(
            import_code(prepare_with(&metadata, SOURCE).unwrap_err()),
            "nara.sprite-animation.settings",
            "settings were {settings}"
        );
        assert!(serde_json::from_value::<SpriteAnimationSettings>(settings).is_err());
    }
    assert!(prepare(SOURCE).is_ok());
    assert_eq!(
        serde_json::from_str::<SpriteAnimationSettings>("{}").unwrap(),
        SpriteAnimationSettings {}
    );
}

#[test]
fn cooked_fixture_and_source_round_trip_to_the_same_deterministic_runtime_content() {
    assert_eq!(SpriteAnimationCodec::RUNTIME_KIND, "nara_sprite_animation");
    assert_eq!(SpriteAnimationCodec::RUNTIME_VERSION, 1);
    let source = prepare(SOURCE).unwrap();
    let cooked = SpriteAnimationCodec::decode_runtime(RUNTIME).unwrap();
    assert_eq!(cooked, source);
    let encoded = SpriteAnimationCodec::encode_runtime(&source).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).unwrap(),
        serde_json::from_slice::<Value>(RUNTIME).unwrap()
    );
    assert_eq!(
        SpriteAnimationCodec::decode_runtime(&encoded).unwrap(),
        source
    );
    assert_eq!(
        SpriteAnimationCodec::encode_runtime(&cooked).unwrap(),
        encoded
    );
    let mut reordered: Value = serde_json::from_slice(SOURCE).unwrap();
    reordered["clips"].as_array_mut().unwrap().reverse();
    let reordered = prepare(&serde_json::to_vec(&reordered).unwrap()).unwrap();
    assert_eq!(
        SpriteAnimationCodec::encode_runtime(&reordered).unwrap(),
        encoded
    );
}

#[test]
fn complete_runtime_membership_compares_both_source_and_product_for_primary_and_auxiliary() {
    let auxiliary = ImportedProductId::parse_subproduct(AUXILIARY_ID).unwrap();
    for product_id in [ImportedProductId::PRIMARY, auxiliary] {
        let product = reference(SOURCE_ID, product_id);
        let runtime = runtime_document(single_clip(), product);
        let set =
            SpriteAnimationCodec::decode_runtime(&serde_json::to_vec(&runtime).unwrap()).unwrap();
        assert_eq!(set.product(), product);
        assert!(SpriteAnimationCodec::contains_runtime_product(
            &set, product
        ));
        assert!(!SpriteAnimationCodec::contains_runtime_product(
            &set,
            reference(OTHER_SOURCE_ID, product_id)
        ));
        let other = if product_id == ImportedProductId::PRIMARY {
            auxiliary
        } else {
            ImportedProductId::PRIMARY
        };
        assert!(!SpriteAnimationCodec::contains_runtime_product(
            &set,
            reference(SOURCE_ID, other)
        ));
        assert!(SpriteAnimationDomain::contains_product(&set, product_id));
        assert!(!SpriteAnimationDomain::contains_product(&set, other));
        let encoded = SpriteAnimationCodec::encode_runtime(&set).unwrap();
        assert_eq!(SpriteAnimationCodec::decode_runtime(&encoded).unwrap(), set);
    }
}

#[test]
fn unsupported_versions_and_unknown_source_or_runtime_fields_are_rejected() {
    for version in [0, 2] {
        let mut document = single_clip();
        document["format_version"] = json!(version);
        assert_eq!(
            import_code(prepare(&serde_json::to_vec(&document).unwrap()).unwrap_err()),
            "nara.sprite-animation.version"
        );
        let runtime = runtime_document(document, primary(SOURCE_ID));
        let error = SpriteAnimationCodec::decode_runtime(&serde_json::to_vec(&runtime).unwrap())
            .unwrap_err();
        assert_eq!(
            error.diagnostic().code().as_str(),
            "nara.sprite-animation.version"
        );
    }
    let mut root = single_clip();
    root["clock"] = json!("game");
    assert_both_reject(root);
    let mut named_clip = single_clip();
    named_clip["clips"][0]["fps"] = json!(10);
    assert_both_reject(named_clip);
    let mut frame = single_clip();
    frame["clips"][0]["frames"][0]["image"] = json!("atlas.png");
    assert_both_reject(frame);
    let mut source_product = single_clip();
    source_product["product"] = serde_json::to_value(primary(OTHER_SOURCE_ID)).unwrap();
    assert!(prepare(&serde_json::to_vec(&source_product).unwrap()).is_err());
    let mut runtime = runtime_document(single_clip(), primary(SOURCE_ID));
    runtime["settings"] = json!({});
    assert!(SpriteAnimationCodec::decode_runtime(&serde_json::to_vec(&runtime).unwrap()).is_err());
    runtime.as_object_mut().unwrap().remove("settings");
    runtime.as_object_mut().unwrap().remove("product");
    assert!(SpriteAnimationCodec::decode_runtime(&serde_json::to_vec(&runtime).unwrap()).is_err());
}

#[test]
fn clip_and_frame_records_require_objects_and_their_collections_require_sequences() {
    assert_both_reject(source_document(vec![json!(["idle", "once", [frame()]])]));
    let mut positional_frame = single_clip();
    positional_frame["clips"][0]["frames"][0] = json!([[0.0, 0.0], [0.25, 1.0], 100]);
    assert_both_reject(positional_frame);
    let mut named_frames = single_clip();
    named_frames["clips"][0]["frames"] = json!({"first": frame()});
    assert_both_reject(named_frames);
    let mut named_clips = single_clip();
    named_clips["clips"] = json!({"idle": clip("idle", vec![frame()])});
    assert_both_reject(named_clips);
}

#[test]
fn source_and_runtime_reject_invalid_uv_regions_and_zero_or_overflowing_durations() {
    for (min, size) in [
        (json!([-0.01, 0.0]), json!([0.25, 1.0])),
        (json!([0.0, 0.0]), json!([0.0, 1.0])),
        (json!([0.0, 0.0]), json!([-0.25, 1.0])),
        (json!([0.8, 0.0]), json!([0.25, 1.0])),
        (json!([0.0, 0.5]), json!([0.25, 1.0])),
        (json!([0.0, 0.0]), json!([1e100, 1.0])),
    ] {
        let mut document = single_clip();
        document["clips"][0]["frames"][0]["min"] = min;
        document["clips"][0]["frames"][0]["size"] = size;
        assert_both_reject(document);
    }
    for duration in [json!(0), json!(-1), json!(0.5)] {
        let mut document = single_clip();
        document["clips"][0]["frames"][0]["duration_nanos"] = duration;
        assert_both_reject(document);
    }
    let mut maximum = frame();
    maximum["duration_nanos"] = json!(u64::MAX);
    let document = source_document(vec![clip("idle", vec![maximum.clone()])]);
    let accepted = prepare(&serde_json::to_vec(&document).unwrap()).unwrap();
    assert_eq!(
        accepted.get("idle").unwrap().duration(),
        Duration::from_nanos(u64::MAX)
    );
    let encoded = SpriteAnimationCodec::encode_runtime(&accepted).unwrap();
    assert_eq!(
        SpriteAnimationCodec::decode_runtime(&encoded).unwrap(),
        accepted
    );
    assert_both_reject(source_document(vec![clip("idle", vec![maximum, frame()])]));
}

#[test]
fn source_and_runtime_reject_invalid_names_duplicate_names_modes_and_empty_clips() {
    for name in [
        "".to_owned(),
        "x".repeat(MAX_CLIP_NAME_BYTES + 1),
        "idle walk".to_owned(),
        "静止".to_owned(),
    ] {
        assert_both_reject(source_document(vec![clip(&name, vec![frame()])]));
    }
    assert_both_reject(source_document(vec![
        clip("idle", vec![frame()]),
        clip("idle", vec![frame()]),
    ]));
    assert_both_reject(source_document(vec![clip("idle", vec![])]));
    let mut invalid_mode = single_clip();
    invalid_mode["clips"][0]["mode"] = json!("ping-pong");
    assert_both_reject(invalid_mode);
    let distinct = source_document(vec![
        clip("Idle", vec![frame()]),
        clip("idle", vec![frame()]),
    ]);
    let set = prepare(&serde_json::to_vec(&distinct).unwrap()).unwrap();
    assert!(set.get("Idle").is_some() && set.get("idle").is_some());
}

#[test]
fn every_domain_limit_is_enforced_through_both_public_decoders() {
    let too_many_clips = (0..=MAX_CLIPS)
        .map(|index| clip(&format!("clip-{index}"), vec![frame()]))
        .collect();
    assert_both_reject(source_document(too_many_clips));
    assert_both_reject(source_document(vec![clip(
        "idle",
        vec![frame(); MAX_FRAMES_PER_CLIP + 1],
    )]));
    let too_many_total_frames = (0..=MAX_TOTAL_FRAMES / MAX_FRAMES_PER_CLIP)
        .map(|index| {
            let frames = if index == MAX_TOTAL_FRAMES / MAX_FRAMES_PER_CLIP {
                1
            } else {
                MAX_FRAMES_PER_CLIP
            };
            clip(&format!("clip-{index}"), vec![frame(); frames])
        })
        .collect();
    assert_both_reject(source_document(too_many_total_frames));
}

#[test]
fn maximum_names_clips_frames_and_total_storage_fit_preflight_and_retained_promises() {
    let clips = (0..MAX_CLIPS)
        .map(|index| {
            let frames = match index {
                0..15 => MAX_FRAMES_PER_CLIP,
                15 => 240,
                _ => 1,
            };
            let name = format!("{index:02}{}", "x".repeat(MAX_CLIP_NAME_BYTES - 2));
            clip(&name, vec![frame(); frames])
        })
        .collect();
    let document = source_document(clips);
    let source = prepare(&serde_json::to_vec(&document).unwrap()).unwrap();
    assert_eq!(source.clips().len(), MAX_CLIPS);
    assert_eq!(
        source
            .clips()
            .map(|(_, clip)| clip.frames().len())
            .sum::<usize>(),
        MAX_TOTAL_FRAMES
    );
    let encoded = SpriteAnimationCodec::encode_runtime(&source).unwrap();
    let decoded = SpriteAnimationCodec::decode_runtime(&encoded).unwrap();
    assert_eq!(decoded, source);
    let retained = SpriteAnimationCodec::runtime_retained_bytes(&decoded).unwrap();
    let minimum_owned_storage = size_of::<SpriteAnimationSet>()
        + MAX_CLIPS * (size_of::<(Box<str>, Clip)>() + MAX_CLIP_NAME_BYTES)
        + MAX_TOTAL_FRAMES * size_of::<Frame>();
    assert!(retained >= minimum_owned_storage);
    assert!(
        retained
            <= SpriteAnimationCodec::runtime_limits()
                .retained_bytes()
                .get()
    );
    assert!(retained <= SpriteAnimationDomain::limits().retained_bytes().get());
    assert!(encoded.len() <= SpriteAnimationCodec::runtime_limits().encoded_bytes().get());
    let small = prepare(SOURCE).unwrap();
    assert!(SpriteAnimationCodec::runtime_retained_bytes(&small).unwrap() < retained);
}

#[test]
fn encoded_byte_bounds_include_whitespace_without_retaining_input_storage() {
    let limit = SpriteAnimationDomain::limits().source_bytes().get();
    assert_eq!(limit, 4 * 1024 * 1024);
    assert_eq!(
        limit,
        SpriteAnimationCodec::runtime_limits().encoded_bytes().get()
    );
    let mut padded_source = SOURCE.to_vec();
    padded_source.resize(limit, b' ');
    let exact = prepare(&padded_source).unwrap();
    assert_eq!(exact, prepare(SOURCE).unwrap());
    padded_source.push(b' ');
    assert_eq!(
        import_code(prepare(&padded_source).unwrap_err()),
        "nara.sprite-animation.bounds"
    );
    let mut padded_runtime = RUNTIME.to_vec();
    padded_runtime.resize(limit, b' ');
    let runtime = SpriteAnimationCodec::decode_runtime(&padded_runtime).unwrap();
    assert_eq!(runtime, exact);
    assert_eq!(
        SpriteAnimationCodec::runtime_retained_bytes(&runtime).unwrap(),
        SpriteAnimationCodec::runtime_retained_bytes(&exact).unwrap()
    );
    padded_runtime.push(b' ');
    let error = SpriteAnimationCodec::decode_runtime(&padded_runtime).unwrap_err();
    assert_eq!(
        error.diagnostic().code().as_str(),
        "nara.sprite-animation.bounds"
    );
}

#[test]
fn empty_sets_remain_real_products_and_retain_no_clip_storage() {
    let empty = source_document(vec![]);
    let source = prepare(&serde_json::to_vec(&empty).unwrap()).unwrap();
    assert_eq!(source.clips().len(), 0);
    assert!(source.get("idle").is_none());
    let encoded = SpriteAnimationCodec::encode_runtime(&source).unwrap();
    assert_eq!(
        SpriteAnimationCodec::decode_runtime(&encoded).unwrap(),
        source
    );
    assert_eq!(
        SpriteAnimationCodec::runtime_retained_bytes(&source).unwrap(),
        size_of::<SpriteAnimationSet>()
    );
    let mut products = Vec::new();
    SpriteAnimationDomain::visit_products(&source, |product| {
        products.push(product);
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(products, [primary(SOURCE_ID)]);
}
