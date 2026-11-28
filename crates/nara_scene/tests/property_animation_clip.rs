#![cfg(any(feature = "authoring", feature = "cooked"))]

use std::{borrow::Cow, ptr, time::Duration};

use nara_core::{Color, Vec2};
use nara_reflect::{ComponentFieldPath, ComponentFieldPathSegment, ComponentTypeId};
use nara_scene::animation::{
    Easing, KeyframeValue, PropertyAnimationClip, PropertyAnimationClipError, PropertyKeyframe,
    PropertyMarker, PropertyPlaybackMode, PropertyTrack,
};

#[cfg(any(feature = "authoring", feature = "runtime"))]
const SOURCE: &str = r#"PropertyAnimationClip(
    duration_ns: 30,
    tracks: [
        (
            target_child: Some("0193a100-1111-7000-8000-000000000001"),
            component: "game.Gate",
            path: "lights[2].tint",
            keyframes: [
                (time_ns: 1, value: Color((r: 1.0, g: 0.0, b: 0.0, a: 1.0))),
                (time_ns: 30, value: Color((r: 0.0, g: 1.0, b: 0.0, a: 0.5))),
            ],
        ),
        (
            component: "nara.Transform2d",
            path: "translation",
            keyframes: [(time_ns: 0, value: Vec2((1.0, 2.0)))],
        ),
    ],
    markers: [(time_ns: 25, name: "opened")],
)"#;

#[cfg(feature = "cooked")]
const NPAN_BOOL_CLIP: &[u8] = b"NPAN\x01\x00\x01\x00\x09\0\0\0\0\0\0\0\x01\0\x01\0\
    \x00\x09\x00game.Gate\x04\x00open\x03\x01\0\0\0\
    \x02\0\0\0\0\0\0\0\x00\x01\
    \x09\0\0\0\0\0\0\0\x03\0end";

fn key(time_ns: u64, value: KeyframeValue, easing: Easing) -> PropertyKeyframe {
    PropertyKeyframe {
        time: Duration::from_nanos(time_ns),
        value,
        easing,
    }
}

fn marker(time_ns: u64, name: &str) -> PropertyMarker {
    PropertyMarker {
        time: Duration::from_nanos(time_ns),
        name: name.to_owned(),
    }
}

fn track(path: &str, keyframes: Vec<PropertyKeyframe>) -> PropertyTrack {
    PropertyTrack::new(
        None,
        ComponentTypeId::new("game.Gate"),
        path.parse().unwrap(),
        keyframes,
    )
    .unwrap()
}

#[test]
fn clips_order_targets_and_markers_and_share_storage_on_clone() {
    let front = track("a", vec![key(0, KeyframeValue::Bool(false), Easing::Step)]);
    let mut child = front.clone();
    child.target_child = Some("0193a100-1111-7000-8000-000000000001".parse().unwrap());
    let back = track("b", vec![key(0, KeyframeValue::Bool(true), Easing::Step)]);
    let clip = PropertyAnimationClip::new(
        Duration::from_nanos(30),
        PropertyPlaybackMode::PingPong,
        vec![child.clone(), back.clone(), front.clone()],
        vec![marker(30, "end"), marker(0, "start"), marker(0, "enter")],
    )
    .unwrap();
    assert_eq!(clip.tracks(), &[front, back, child]);
    assert_eq!(
        clip.markers(),
        &[marker(0, "enter"), marker(0, "start"), marker(30, "end")]
    );
    assert_eq!(clip.duration(), Duration::from_nanos(30));
    assert_eq!(clip.mode(), PropertyPlaybackMode::PingPong);

    let clone = clip.clone();
    assert!(ptr::eq(clip.tracks(), clone.tracks()));
    assert!(ptr::eq(clip.markers(), clone.markers()));
    assert!(ptr::eq(
        clip.tracks()[0].keyframes(),
        clone.tracks()[0].keyframes()
    ));
}

#[test]
fn easing_curves_sample_normalized_progress() {
    let cases = [
        (Easing::Step, 0.0),
        (Easing::Linear, 0.25),
        (Easing::QuadIn, 0.0625),
        (Easing::QuadOut, 0.4375),
        (Easing::QuadInOut, 0.125),
        (Easing::CubicIn, 0.015625),
        (Easing::CubicOut, 0.578125),
        (Easing::CubicInOut, 0.0625),
        (Easing::SineIn, 0.07612047),
        (Easing::SineOut, 0.38268343),
        (Easing::SineInOut, 0.14644661),
    ];
    for (curve, expected) in cases {
        assert_eq!(curve.sample(-1.0), 0.0);
        assert_eq!(curve.sample(f32::NAN), 0.0);
        assert_eq!(curve.sample(1.0), 1.0);
        assert_eq!(curve.sample(2.0), 1.0);
        assert!((curve.sample(0.25) - expected).abs() < 1e-6, "{curve:?}");
    }
    assert_eq!(Easing::QuadInOut.sample(0.75), 0.875);
    assert_eq!(Easing::CubicInOut.sample(0.75), 0.9375);
}

#[test]
fn track_eases_the_outgoing_segment_and_clamps_to_exact_keys() {
    let track = track(
        "position",
        vec![
            key(10, KeyframeValue::F32(0.0), Easing::QuadIn),
            key(30, KeyframeValue::F32(8.0), Easing::Step),
            key(40, KeyframeValue::F32(12.0), Easing::Linear),
        ],
    );
    let frames = track.keyframes();
    for (time, index) in [(0, 0), (10, 0), (30, 1), (39, 1), (40, 2), (65535, 2)] {
        let value = track.sample(Duration::from_nanos(time));
        assert!(matches!(value, Cow::Borrowed(value) if ptr::eq(value, &frames[index].value)));
    }
    assert_eq!(
        track.sample(Duration::from_nanos(20)).as_ref(),
        &KeyframeValue::F32(2.0)
    );
}

#[test]
fn interpolation_keeps_finite_extremes_and_borrows_discrete_values() {
    let low = KeyframeValue::F32(-f32::MAX);
    let high = KeyframeValue::F32(f32::MAX);
    assert_eq!(
        low.interpolate(&high, 0.5).as_ref(),
        &KeyframeValue::F32(0.0)
    );
    let vector = KeyframeValue::Vec2(Vec2::new(-1.0, 9.0));
    let end = KeyframeValue::Vec2(Vec2::new(3.0, 1.0));
    assert_eq!(
        vector.interpolate(&end, 0.5).as_ref(),
        &KeyframeValue::Vec2(Vec2::new(1.0, 5.0))
    );
    let clear = KeyframeValue::Color(Color::TRANSPARENT);
    let tint = KeyframeValue::Color(Color::rgba(1.0, 3.0, 5.0, 1.0));
    assert_eq!(
        clear.interpolate(&tint, 0.5).as_ref(),
        &KeyframeValue::Color(Color::rgba(0.5, 1.5, 2.5, 0.5))
    );

    for (start, end) in [
        (KeyframeValue::Bool(false), KeyframeValue::Bool(true)),
        (KeyframeValue::I64(i64::MIN), KeyframeValue::I64(i64::MAX)),
        (
            KeyframeValue::String("closed".into()),
            KeyframeValue::String("open".into()),
        ),
    ] {
        assert!(
            matches!(start.interpolate(&end, 0.5), Cow::Borrowed(value) if ptr::eq(value, &start))
        );
        assert!(
            matches!(start.interpolate(&end, 1.0), Cow::Borrowed(value) if ptr::eq(value, &end))
        );
        let track = track(
            "value",
            vec![
                key(0, start, Easing::QuadOut),
                key(u64::MAX, end, Easing::Linear),
            ],
        );
        let almost_end = track.sample(Duration::from_nanos(u64::MAX - 1));
        assert!(
            matches!(almost_end, Cow::Borrowed(value) if ptr::eq(value, &track.keyframes()[0].value))
        );
    }
}

#[test]
fn tracks_reject_invalid_paths_times_and_values() {
    let make = |keys| {
        PropertyTrack::new(
            None,
            ComponentTypeId::new("game.Gate"),
            "open".parse().unwrap(),
            keys,
        )
    };
    let one = key(1, KeyframeValue::F32(0.0), Easing::Linear);
    assert!(matches!(
        make(vec![]),
        Err(PropertyAnimationClipError::EmptyTrack)
    ));
    assert!(matches!(
        make(vec![one.clone(), one.clone()]),
        Err(PropertyAnimationClipError::KeyframeOrder(1))
    ));
    assert!(matches!(
        make(vec![
            one.clone(),
            key(0, KeyframeValue::F32(2.0), Easing::Linear)
        ]),
        Err(PropertyAnimationClipError::KeyframeOrder(1))
    ));
    assert!(matches!(
        make(vec![
            one.clone(),
            key(2, KeyframeValue::Bool(true), Easing::Linear)
        ]),
        Err(PropertyAnimationClipError::MixedValues(1))
    ));
    for value in [
        KeyframeValue::F32(f32::NAN),
        KeyframeValue::Vec2(Vec2::new(f32::INFINITY, 0.0)),
        KeyframeValue::Color(Color::rgba(0.0, 0.0, 0.0, f32::NEG_INFINITY)),
    ] {
        assert!(matches!(
            make(vec![key(0, value, Easing::Linear)]),
            Err(PropertyAnimationClipError::NonFiniteValue(0))
        ));
    }
    for path in [
        ComponentFieldPath::empty(),
        ComponentFieldPath::new([ComponentFieldPathSegment::Index(0)]),
    ] {
        assert!(matches!(
            PropertyTrack::new(
                None,
                ComponentTypeId::new("game.Gate"),
                path,
                vec![one.clone()]
            ),
            Err(PropertyAnimationClipError::InvalidPath)
        ));
    }
    assert!(matches!(
        PropertyTrack::new(
            None,
            ComponentTypeId::new("?"),
            "open".parse().unwrap(),
            vec![one]
        ),
        Err(PropertyAnimationClipError::ComponentId(_))
    ));
}

#[test]
fn clips_reject_duplicate_targets_and_invalid_marker_timelines() {
    let track = track(
        "open",
        vec![key(30, KeyframeValue::Bool(true), Easing::Step)],
    );
    let make = |duration, tracks, markers| {
        PropertyAnimationClip::new(
            Duration::from_nanos(duration),
            PropertyPlaybackMode::Once,
            tracks,
            markers,
        )
    };
    assert!(matches!(
        make(0, vec![], vec![]),
        Err(PropertyAnimationClipError::InvalidDuration)
    ));
    assert!(matches!(
        make(29, vec![track.clone()], vec![]),
        Err(PropertyAnimationClipError::KeyframeOutsideDuration(0))
    ));
    assert!(matches!(
        make(30, vec![track.clone(), track.clone()], vec![]),
        Err(PropertyAnimationClipError::DuplicateTrack)
    ));
    for bad in [marker(0, ""), marker(31, "end")] {
        assert!(matches!(
            make(30, vec![track.clone()], vec![bad]),
            Err(PropertyAnimationClipError::InvalidMarker)
        ));
    }
    assert!(matches!(
        make(30, vec![track], vec![marker(10, "end"), marker(10, "end")]),
        Err(PropertyAnimationClipError::DuplicateMarker)
    ));
}

#[test]
fn keyframe_budget_applies_across_the_clip() {
    const LIMIT: u64 = 65_536;
    let full = track(
        "a",
        (0..LIMIT)
            .map(|time| key(time, KeyframeValue::Bool(false), Easing::Step))
            .collect(),
    );
    let extra = track("b", vec![key(0, KeyframeValue::Bool(false), Easing::Step)]);
    let make = |tracks| {
        PropertyAnimationClip::new(
            Duration::from_nanos(LIMIT),
            PropertyPlaybackMode::Once,
            tracks,
            vec![],
        )
    };
    assert!(matches!(
        make(vec![full.clone(), extra]),
        Err(PropertyAnimationClipError::Limit {
            what: "clip keyframes",
            actual: 65_537,
            maximum: 65_536,
        })
    ));
    let clip = make(vec![full]).unwrap();
    assert_eq!(clip.tracks()[0].keyframes().len() as u64, LIMIT);
    #[cfg(feature = "cooked")]
    assert_eq!(
        PropertyAnimationClip::from_cooked(&clip.encode_cooked().unwrap()).unwrap(),
        clip
    );
}

#[cfg(feature = "cooked")]
#[test]
fn npan_v1_matches_the_wire_and_preserves_all_value_types() {
    let golden = PropertyAnimationClip::new(
        Duration::from_nanos(9),
        PropertyPlaybackMode::Loop,
        vec![track(
            "open",
            vec![key(2, KeyframeValue::Bool(true), Easing::Step)],
        )],
        vec![marker(9, "end")],
    )
    .unwrap();
    assert_eq!(golden.encode_cooked().unwrap(), NPAN_BOOL_CLIP);
    assert_eq!(
        PropertyAnimationClip::from_cooked(NPAN_BOOL_CLIP).unwrap(),
        golden
    );

    let mut tracks = [
        KeyframeValue::Color(Color::rgba(-1.0, 3.0, 5.0, 0.5)),
        KeyframeValue::F32(-0.0),
        KeyframeValue::Vec2(Vec2::new(-1.0, 2.0)),
        KeyframeValue::String("café".into()),
        KeyframeValue::Bool(false),
        KeyframeValue::I64(i64::MIN),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, value)| {
        track(
            &format!(r#"slots[{index}]["a.\"b\""]"#),
            vec![key(0, value, Easing::SineInOut)],
        )
    })
    .collect::<Vec<_>>();
    tracks[1].target_child = Some("0193a100-1111-7000-8000-000000000001".parse().unwrap());
    let duration = Duration::from_nanos(u64::MAX);
    let clip = PropertyAnimationClip::new(
        duration,
        PropertyPlaybackMode::PingPong,
        tracks.clone(),
        vec![],
    )
    .unwrap();
    tracks.reverse();
    let reordered =
        PropertyAnimationClip::new(duration, PropertyPlaybackMode::PingPong, tracks, vec![])
            .unwrap();
    let bytes = clip.encode_cooked().unwrap();
    assert_eq!(reordered.encode_cooked().unwrap(), bytes);
    let decoded = PropertyAnimationClip::from_cooked(&bytes).unwrap();
    assert_eq!(decoded, clip);
    assert_eq!(decoded.encode_cooked().unwrap(), bytes);
}

#[cfg(feature = "cooked")]
#[test]
fn npan_rejects_truncation_unknown_tags_and_invalid_extents() {
    for len in 0..NPAN_BOOL_CLIP.len() {
        assert!(
            PropertyAnimationClip::from_cooked(&NPAN_BOOL_CLIP[..len]).is_err(),
            "length {len}"
        );
    }
    let mut trailing = NPAN_BOOL_CLIP.to_vec();
    trailing.push(0);
    assert!(PropertyAnimationClip::from_cooked(&trailing).is_err());
    let mutations = [
        (0, b'X'),
        (6, 3),
        (7, 1),
        (8, 1),
        (17, 5),
        (19, 33),
        (20, 2),
        (23, 255),
        (34, b'.'),
        (38, 6),
        (41, 2),
        (51, 11),
        (52, 2),
        (53, 10),
    ];
    for (offset, value) in mutations {
        let mut broken = NPAN_BOOL_CLIP.to_vec();
        broken[offset] = value;
        assert!(
            PropertyAnimationClip::from_cooked(&broken).is_err(),
            "offset {offset}"
        );
    }
    let mut wrong_version = NPAN_BOOL_CLIP.to_vec();
    wrong_version[4] = 2;
    assert!(matches!(
        PropertyAnimationClip::from_cooked(&wrong_version),
        Err(PropertyAnimationClipError::UnsupportedVersion(2))
    ));

    let mut noncanonical_path = NPAN_BOOL_CLIP.to_vec();
    noncanonical_path.splice(32..38, *b"\x08\x00[\"open\"]");
    assert!(matches!(
        PropertyAnimationClip::from_cooked(&noncanonical_path),
        Err(PropertyAnimationClipError::InvalidCooked(
            "noncanonical field path"
        ))
    ));

    let mut nonfinite = NPAN_BOOL_CLIP.to_vec();
    nonfinite[38] = 0;
    nonfinite.splice(52..53, f32::NAN.to_le_bytes());
    assert!(matches!(
        PropertyAnimationClip::from_cooked(&nonfinite),
        Err(PropertyAnimationClipError::NonFiniteValue(0))
    ));
}

#[cfg(feature = "authoring")]
#[test]
fn source_uses_nanoseconds_defaults_and_canonical_field_paths() {
    use nara_scene::animation::MAX_PROPERTY_ANIMATION_SOURCE_BYTES;

    let mut padded = SOURCE.to_owned();
    padded.push_str(&" ".repeat(MAX_PROPERTY_ANIMATION_SOURCE_BYTES - padded.len()));
    let clip = PropertyAnimationClip::from_ron(&padded).unwrap();
    padded.push(' ');
    assert!(matches!(
        PropertyAnimationClip::from_ron(&padded),
        Err(PropertyAnimationClipError::SourceTooLarge)
    ));
    assert_eq!(clip.mode(), PropertyPlaybackMode::Once);
    assert_eq!(clip.duration(), Duration::from_nanos(30));
    assert_eq!(clip.markers(), &[marker(25, "opened")]);
    assert_eq!(clip.tracks()[0].component.as_str(), "nara.Transform2d");
    assert_eq!(
        clip.tracks()[0].sample(Duration::ZERO).as_ref(),
        &KeyframeValue::Vec2(Vec2::new(1.0, 2.0))
    );
    let child = &clip.tracks()[1];
    assert_eq!(child.path.to_string(), "lights[2].tint");
    assert_eq!(child.keyframes()[0].easing, Easing::Linear);
    assert_eq!(child.keyframes()[0].time, Duration::from_nanos(1));
    #[cfg(feature = "cooked")]
    assert_eq!(
        PropertyAnimationClip::from_cooked(&clip.encode_cooked().unwrap()).unwrap(),
        clip
    );
}

#[cfg(feature = "authoring")]
#[test]
fn source_rejects_unknown_fields_invalid_persistent_targets_and_excess_tracks() {
    for mutated in [
        SOURCE.replace("duration_ns: 30,", "duration_ns: 30, extra: 0,"),
        SOURCE.replace(
            "0193a100-1111-7000-8000-000000000001",
            "00000000-0000-0000-0000-000000000000",
        ),
        SOURCE.replace("lights[2].tint", "lights[2]..tint"),
        SOURCE.replace("time_ns: 30", "time_ns: 1"),
        SOURCE.replace("g: 1.0", "g: inf"),
        format!("{SOURCE} ()"),
    ] {
        assert!(
            PropertyAnimationClip::from_ron(&mutated).is_err(),
            "{mutated}"
        );
    }
    let tracks =
        r#"(component: "game.Gate", path: "open", keyframes: [(time_ns: 0, value: Bool(true))]),"#
            .repeat(1025);
    let excessive = format!("(duration_ns: 1, tracks: [{tracks}])");
    assert!(matches!(
        PropertyAnimationClip::from_ron(&excessive),
        Err(PropertyAnimationClipError::Ron(_))
    ));
}

#[cfg(feature = "runtime")]
#[test]
fn loader_reads_npak_tag_ten_and_rejects_wrong_kind_before_decode() {
    use nara_asset::{
        AssetLoadError, NpakEntryKind, RuntimeAssetMode, RuntimeAssetSource,
        cooked::{NpakArchive, NpakCompression, NpakInput, encode_npak},
        prepare_asset,
    };
    use nara_core::{AssetId, vfs::MemoryAssetSource};
    use nara_scene::animation::PropertyAnimationLoader;

    let id: AssetId = "0193a100-1111-7000-8000-000000000001".parse().unwrap();
    let kind = NpakEntryKind::PropertyAnimation;
    assert_eq!(kind as u64, 10);
    let bytes = encode_npak(&[NpakInput::new(
        id,
        kind,
        NPAN_BOOL_CLIP,
        NpakCompression::Lz4,
    )])
    .unwrap();
    let archive =
        RuntimeAssetSource::from_npak_archive(NpakArchive::from_bytes(bytes).unwrap().into());
    let prepared = prepare_asset(&archive, id, &PropertyAnimationLoader).unwrap();
    assert_eq!(prepared.value().encode_cooked().unwrap(), NPAN_BOOL_CLIP);
    assert!(prepared.dependencies().is_empty());

    let wrong = RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Cooked,
        MemoryAssetSource::new(),
        [(id, NpakEntryKind::Raw)],
    );
    assert!(matches!(
        prepare_asset(&wrong, id, &PropertyAnimationLoader),
        Err(AssetLoadError::WrongKind {
            expected: NpakEntryKind::PropertyAnimation,
            actual: NpakEntryKind::Raw,
            ..
        })
    ));
}

#[cfg(feature = "runtime")]
#[test]
fn loader_gates_ron_on_authoring() {
    use nara_asset::{RuntimeAssetMode, RuntimeAssetSource, prepare_asset, source::asset_id_path};
    use nara_core::{AssetId, vfs::MemoryAssetSource};
    use nara_scene::animation::PropertyAnimationLoader;

    let id: AssetId = "0193a100-1111-7000-8000-000000000001".parse().unwrap();
    let mut files = MemoryAssetSource::new();
    files
        .insert(&asset_id_path(id), SOURCE.as_bytes().to_vec())
        .unwrap();
    let source = RuntimeAssetSource::from_memory(RuntimeAssetMode::Source, files);
    let result = prepare_asset(&source, id, &PropertyAnimationLoader);
    #[cfg(feature = "authoring")]
    assert_eq!(
        result.unwrap().value(),
        &PropertyAnimationClip::from_ron(SOURCE).unwrap()
    );
    #[cfg(not(feature = "authoring"))]
    {
        use nara_asset::AssetLoadError;
        use nara_scene::animation::PropertyAnimationLoaderError;

        assert!(
            matches!(result, Err(AssetLoadError::Decode { source, .. }) if matches!(
                source.downcast_ref::<PropertyAnimationLoaderError>(),
                Some(PropertyAnimationLoaderError::SourceDisabled),
            ))
        );
    }
}
