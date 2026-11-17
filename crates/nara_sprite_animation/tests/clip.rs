use std::time::Duration;

use nara_asset::{AssetProductRef, ImportedProductId, StableAssetId};
use nara_core::Vec2;
use nara_sprite::TextureRegion;
use nara_sprite_animation::{
    Clip, ClipError, Frame, FrameSample, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAMES_PER_CLIP,
    MAX_TOTAL_FRAMES, PlaybackMode, SpriteAnimationSet,
};

fn product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("10000000-0000-4000-8000-000000000001").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn region(index: usize) -> TextureRegion {
    TextureRegion::new(Vec2::new(index as f32 * 0.25, 0.0), Vec2::new(0.25, 1.0))
}

fn unequal_clip(mode: PlaybackMode) -> Clip {
    Clip::new(
        mode,
        [
            Frame::new(region(0), Duration::from_nanos(2)).unwrap(),
            Frame::new(region(1), Duration::from_nanos(3)).unwrap(),
            Frame::new(region(2), Duration::from_nanos(5)).unwrap(),
        ],
    )
    .unwrap()
}

fn repeated_clip(frames: usize) -> Clip {
    let frame = Frame::new(TextureRegion::FULL, Duration::from_nanos(1)).unwrap();
    Clip::new(PlaybackMode::Loop, std::iter::repeat_n(frame, frames)).unwrap()
}

#[test]
fn public_accessors_preserve_authored_frames_and_product_identity() {
    let clip = unequal_clip(PlaybackMode::Once);
    assert_eq!(clip.mode(), PlaybackMode::Once);
    assert_eq!(clip.duration(), Duration::from_nanos(10));
    let authored: Vec<_> = clip
        .frames()
        .iter()
        .map(|frame| (frame.region(), frame.duration()))
        .collect();
    assert_eq!(
        authored,
        vec![
            (region(0), Duration::from_nanos(2)),
            (region(1), Duration::from_nanos(3)),
            (region(2), Duration::from_nanos(5)),
        ]
    );

    let set = SpriteAnimationSet::new(product(), [("walk".to_owned(), clip.clone())]).unwrap();
    assert_eq!(set.product(), product());
    assert_eq!(set.get("walk"), Some(&clip));
    assert!(set.get("Walk").is_none());
    assert!(set.get("missing").is_none());
}

#[test]
fn unequal_durations_assign_exact_boundaries_to_the_next_frame() {
    let clip = unequal_clip(PlaybackMode::Once);
    for (elapsed_ns, expected_index) in [(0, 0), (1, 0), (2, 1), (4, 1), (5, 2), (9, 2)] {
        assert_eq!(
            clip.sample(Duration::from_nanos(elapsed_ns)),
            FrameSample {
                index: expected_index,
                region: region(expected_index),
                finished: false,
            },
            "elapsed nanoseconds: {elapsed_ns}"
        );
    }
}

#[test]
fn looping_wraps_at_exact_cycle_ends_and_never_finishes() {
    let clip = unequal_clip(PlaybackMode::Loop);
    for (elapsed_ns, expected_index) in [
        (0, 0),
        (2, 1),
        (5, 2),
        (9, 2),
        (10, 0),
        (12, 1),
        (15, 2),
        (20, 0),
        (25, 2),
    ] {
        assert_eq!(
            clip.sample(Duration::from_nanos(elapsed_ns)),
            FrameSample {
                index: expected_index,
                region: region(expected_index),
                finished: false,
            },
            "elapsed nanoseconds: {elapsed_ns}"
        );
    }
}

#[test]
fn once_holds_the_final_frame_and_finishes_only_at_the_clip_end() {
    let clip = unequal_clip(PlaybackMode::Once);
    assert!(!clip.sample(Duration::from_nanos(9)).finished);
    for elapsed in [
        Duration::from_nanos(10),
        Duration::from_nanos(11),
        Duration::MAX,
    ] {
        assert_eq!(
            clip.sample(elapsed),
            FrameSample {
                index: 2,
                region: region(2),
                finished: true,
            }
        );
    }
}

#[test]
fn looping_reduces_huge_elapsed_time_before_any_narrowing() {
    let clip = unequal_clip(PlaybackMode::Loop);
    for elapsed in [
        Duration::from_nanos(u64::MAX) + Duration::from_nanos(1),
        Duration::MAX,
    ] {
        assert_eq!(
            clip.sample(elapsed),
            FrameSample {
                index: 2,
                region: region(2),
                finished: false,
            }
        );
    }
}

#[test]
fn a_single_frame_supports_the_entire_admitted_duration_range() {
    let duration = Duration::from_nanos(u64::MAX);
    let frame = Frame::new(TextureRegion::FULL, duration).unwrap();
    for mode in [PlaybackMode::Loop, PlaybackMode::Once] {
        let clip = Clip::new(mode, [frame]).unwrap();
        assert_eq!(clip.duration(), duration);
        assert_eq!(clip.sample(Duration::ZERO).index, 0);
        assert!(!clip.sample(duration - Duration::from_nanos(1)).finished);
        assert_eq!(
            clip.sample(duration),
            FrameSample {
                index: 0,
                region: TextureRegion::FULL,
                finished: mode == PlaybackMode::Once,
            }
        );
        assert_eq!(
            clip.sample(Duration::MAX),
            FrameSample {
                index: 0,
                region: TextureRegion::FULL,
                finished: mode == PlaybackMode::Once,
            }
        );
    }
}

#[test]
fn construction_rejects_zero_and_overflowing_durations() {
    assert_eq!(
        Frame::new(TextureRegion::FULL, Duration::ZERO),
        Err(ClipError::ZeroFrameDuration)
    );
    for duration in [
        Duration::from_nanos(u64::MAX) + Duration::from_nanos(1),
        Duration::MAX,
    ] {
        assert_eq!(
            Frame::new(TextureRegion::FULL, duration),
            Err(ClipError::DurationOverflow)
        );
    }
    assert_eq!(
        Clip::new(
            PlaybackMode::Loop,
            [
                Frame::new(TextureRegion::FULL, Duration::from_nanos(u64::MAX)).unwrap(),
                Frame::new(TextureRegion::FULL, Duration::from_nanos(1)).unwrap(),
            ]
        ),
        Err(ClipError::DurationOverflow)
    );

    let exact_max = Clip::new(
        PlaybackMode::Once,
        [
            Frame::new(region(0), Duration::from_nanos(u64::MAX - 1)).unwrap(),
            Frame::new(region(1), Duration::from_nanos(1)).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(exact_max.duration(), Duration::from_nanos(u64::MAX));
    assert_eq!(
        exact_max.sample(Duration::from_nanos(u64::MAX - 2)).index,
        0
    );
    assert_eq!(
        exact_max.sample(Duration::from_nanos(u64::MAX - 1)).index,
        1
    );
    assert!(
        !exact_max
            .sample(Duration::from_nanos(u64::MAX - 1))
            .finished
    );
    assert!(exact_max.sample(Duration::from_nanos(u64::MAX)).finished);
}

#[test]
fn regions_must_be_finite_positive_and_within_the_normalized_image() {
    let duration = Duration::from_nanos(1);
    for valid in [
        TextureRegion::FULL,
        TextureRegion::new(Vec2::splat(0.5), Vec2::splat(0.5)),
        TextureRegion::new(Vec2::ZERO, Vec2::splat(f32::MIN_POSITIVE)),
    ] {
        assert_eq!(Frame::new(valid, duration).unwrap().region(), valid);
    }

    for invalid in [
        TextureRegion::new(Vec2::new(f32::NAN, 0.0), Vec2::ONE),
        TextureRegion::new(Vec2::new(0.0, f32::INFINITY), Vec2::ONE),
        TextureRegion::new(Vec2::ZERO, Vec2::new(f32::NEG_INFINITY, 1.0)),
        TextureRegion::new(Vec2::ZERO, Vec2::new(1.0, f32::NAN)),
        TextureRegion::new(Vec2::ZERO, Vec2::new(0.0, 1.0)),
        TextureRegion::new(Vec2::ZERO, Vec2::new(1.0, 0.0)),
        TextureRegion::new(Vec2::ZERO, Vec2::new(-0.25, 1.0)),
        TextureRegion::new(Vec2::ZERO, Vec2::new(1.0, -0.25)),
        TextureRegion::new(Vec2::new(-0.25, 0.0), Vec2::splat(0.5)),
        TextureRegion::new(Vec2::new(0.0, -0.25), Vec2::splat(0.5)),
        TextureRegion::new(Vec2::new(1.0, 0.0), Vec2::splat(0.25)),
        TextureRegion::new(Vec2::new(0.0, 1.0), Vec2::splat(0.25)),
        TextureRegion::new(Vec2::splat(0.75), Vec2::splat(0.5)),
        TextureRegion::new(Vec2::ZERO, Vec2::splat(f32::MAX)),
    ] {
        assert_eq!(
            Frame::new(invalid, duration),
            Err(ClipError::InvalidRegion),
            "region: {invalid:?}"
        );
    }
}

#[test]
fn clips_reject_empty_input_and_stop_at_the_frame_limit() {
    assert_eq!(Clip::new(PlaybackMode::Loop, []), Err(ClipError::EmptyClip));
    assert_eq!(
        repeated_clip(MAX_FRAMES_PER_CLIP).frames().len(),
        MAX_FRAMES_PER_CLIP
    );
    let frame = Frame::new(TextureRegion::FULL, Duration::from_nanos(1)).unwrap();
    assert_eq!(
        Clip::new(PlaybackMode::Loop, std::iter::repeat(frame)),
        Err(ClipError::TooManyFrames)
    );
}

#[test]
fn names_accept_exact_ascii_bounds_and_reject_duplicate_or_invalid_input() {
    let clip = repeated_clip(1);
    let max_name = "x".repeat(MAX_CLIP_NAME_BYTES);
    let set = SpriteAnimationSet::new(
        product(),
        [
            ("walk_2.left-fast".to_owned(), clip.clone()),
            ("Idle".to_owned(), clip.clone()),
            ("idle".to_owned(), clip.clone()),
            (max_name.clone(), clip.clone()),
        ],
    )
    .unwrap();
    assert!(set.get(&max_name).is_some());
    assert!(set.get("Idle").is_some());
    assert!(set.get("idle").is_some());

    for invalid in [
        String::new(),
        "x".repeat(MAX_CLIP_NAME_BYTES + 1),
        "walk left".to_owned(),
        "walk/left".to_owned(),
        "walk\\left".to_owned(),
        "walk:left".to_owned(),
        "walk\n".to_owned(),
        "walk\0".to_owned(),
        "caf\u{e9}".to_owned(),
    ] {
        assert_eq!(
            SpriteAnimationSet::new(product(), [(invalid.clone(), clip.clone())]),
            Err(ClipError::InvalidClipName),
            "name: {invalid:?}"
        );
    }
    assert_eq!(
        SpriteAnimationSet::new(
            product(),
            [("walk".to_owned(), clip.clone()), ("walk".to_owned(), clip)]
        ),
        Err(ClipError::DuplicateClipName("walk".to_owned()))
    );
}

#[test]
fn clip_iteration_borrows_in_name_order_with_an_exact_remaining_length() {
    let set = SpriteAnimationSet::new(
        product(),
        [
            ("walk".to_owned(), repeated_clip(3)),
            ("idle".to_owned(), repeated_clip(1)),
            ("jump".to_owned(), repeated_clip(2)),
        ],
    )
    .unwrap();
    let mut clips = set.clips();
    assert_eq!(clips.len(), 3);
    for (remaining, name, frame_count) in [(2, "idle", 1), (1, "jump", 2), (0, "walk", 3)] {
        let (actual_name, clip) = clips.next().unwrap();
        assert_eq!(actual_name, name);
        assert_eq!(clip.frames().len(), frame_count);
        assert!(std::ptr::eq(clip, set.get(name).unwrap()));
        assert_eq!(clips.len(), remaining);
    }
    assert!(clips.next().is_none());

    let empty = SpriteAnimationSet::new(product(), []).unwrap();
    assert_eq!(empty.clips().len(), 0);
    assert!(empty.get("idle").is_none());
}

#[test]
fn sets_admit_the_named_clip_limit_and_reject_the_next_entry() {
    let set = SpriteAnimationSet::new(
        product(),
        (0..MAX_CLIPS).map(|index| (format!("clip-{index}"), repeated_clip(1))),
    )
    .unwrap();
    assert_eq!(set.clips().len(), MAX_CLIPS);
    assert_eq!(
        SpriteAnimationSet::new(
            product(),
            (0..).map(|index| (format!("clip-{index}"), repeated_clip(1)))
        ),
        Err(ClipError::TooManyClips)
    );
}

#[test]
fn aggregate_frame_limit_applies_across_individually_valid_clips() {
    let full_clips = MAX_TOTAL_FRAMES / MAX_FRAMES_PER_CLIP;
    let set = SpriteAnimationSet::new(
        product(),
        (0..full_clips).map(|index| (format!("clip-{index}"), repeated_clip(MAX_FRAMES_PER_CLIP))),
    )
    .unwrap();
    assert_eq!(
        set.clips()
            .map(|(_, clip)| clip.frames().len())
            .sum::<usize>(),
        MAX_TOTAL_FRAMES
    );
    assert_eq!(
        SpriteAnimationSet::new(
            product(),
            (0..full_clips)
                .map(|index| (format!("clip-{index}"), repeated_clip(MAX_FRAMES_PER_CLIP)))
                .chain(std::iter::once(("overflow".to_owned(), repeated_clip(1))))
        ),
        Err(ClipError::TooManyTotalFrames)
    );
}
