#![cfg(feature = "animation")]

use std::{sync::Arc, time::Duration};

use nara_app::{App, CoreStage, RuntimeTimeSettings};
use nara_asset::{
    AssetAppExt, AssetError, AssetLoadError, AssetPlugin, Assets, NpakEntryKind, RuntimeAssetMode,
    RuntimeAssetSource, add_asset,
    cooked::{NpakArchive, NpakCompression, NpakInput, encode_npak},
    load_asset_sync, prepare_asset,
    source::{MemoryAssetSource, asset_id_path},
};
use nara_core::{AssetId, Color, Vec2};
use nara_ecs::{Entity, Query, ResMut, Resource, bevy_ecs::message::Messages};
use nara_render::{
    animation::{
        Clip, Frame, MAX_MARKERS_PER_STEP, PlaybackMode, SpriteAnimationAssetError,
        SpriteAnimationClock, SpriteAnimationEvent, SpriteAnimationLoader, SpriteAnimationPlugin,
        SpriteAnimationSet, SpriteAnimator,
    },
    sprite::{Sprite, TextureRegion},
};

const ANIMATION_ID: &str = "0193a100-1111-7000-8000-000000000001";
const SOURCE: &str = r#"(
    clips: [(name: "idle", mode: ping_pong, frames: [
        (min: (0.0, 0.0), size: (0.25, 1.0), duration_nanos: 2, event: Some("start")),
        (min: (0.25, 0.0), size: (0.25, 1.0), duration_nanos: 3),
    ])],
)"#;

fn region(index: usize) -> TextureRegion {
    TextureRegion::new(Vec2::new(index as f32 * 0.25, 0.0), Vec2::new(0.25, 1.0))
}

fn marked_clip(mode: PlaybackMode) -> Clip {
    Clip::new(
        mode,
        [(2, "start"), (3, "step"), (5, "end")]
            .into_iter()
            .enumerate()
            .map(|(index, (duration, marker))| {
                Frame::new(region(index), Duration::from_nanos(duration))
                    .unwrap()
                    .with_event(marker)
                    .unwrap()
            }),
    )
    .unwrap()
}

fn animation_set() -> SpriteAnimationSet {
    SpriteAnimationSet::new([
        ("walk".into(), marked_clip(PlaybackMode::Loop)),
        ("hit".into(), marked_clip(PlaybackMode::Once)),
        ("bounce".into(), marked_clip(PlaybackMode::PingPong)),
    ])
    .unwrap()
}

fn animation_app() -> App {
    let mut app = App::new();
    app.add_plugins((AssetPlugin, SpriteAnimationPlugin))
        .unwrap();
    app
}

fn cooked_source(entries: &[NpakInput<'_>]) -> RuntimeAssetSource {
    let archive = NpakArchive::from_bytes(encode_npak(entries).unwrap()).unwrap();
    RuntimeAssetSource::from_npak_archive(Arc::new(archive))
}

#[test]
fn cooked_sprite_animations_are_sorted_and_reject_malformed_records() {
    let a = ("a".to_owned(), marked_clip(PlaybackMode::PingPong));
    let z = ("z".to_owned(), marked_clip(PlaybackMode::Once));
    let first = SpriteAnimationSet::new([z.clone(), a.clone()]).unwrap();
    let second = SpriteAnimationSet::new([a, z]).unwrap();
    let bytes = first.encode_cooked().unwrap();
    assert_eq!(&bytes[..13], b"NSAN\x01\0\x02\0\x01a\x02\x03\0");
    assert_eq!(bytes, second.encode_cooked().unwrap());
    assert_eq!(SpriteAnimationSet::from_cooked(&bytes).unwrap(), first);

    for end in 0..bytes.len() {
        assert!(SpriteAnimationSet::from_cooked(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(SpriteAnimationSet::from_cooked(&trailing).is_err());

    let mut unsupported = bytes.clone();
    unsupported[4] = 2;
    assert!(matches!(
        SpriteAnimationSet::from_cooked(&unsupported),
        Err(SpriteAnimationAssetError::UnsupportedVersion(2))
    ));
    for (offset, byte) in [
        (6, 255),  // clip count
        (8, 255),  // name length
        (9, b'z'), // name order
        (10, 255), // mode
        (12, 2),   // frame count
        (29, 0),   // duration
        (37, 255), // marker length
        (38, 255), // marker UTF-8
    ] {
        let mut malformed = bytes.clone();
        malformed[offset] = byte;
        assert!(SpriteAnimationSet::from_cooked(&malformed).is_err());
    }
}

#[test]
fn source_loading_follows_the_animation_authoring_feature() {
    let id = ANIMATION_ID.parse().unwrap();
    let mut files = MemoryAssetSource::new();
    files.insert(&asset_id_path(id), SOURCE.as_bytes()).unwrap();
    let source = RuntimeAssetSource::from_memory(RuntimeAssetMode::Source, files);
    let prepared = prepare_asset(&source, id, &SpriteAnimationLoader);

    #[cfg(feature = "animation-authoring")]
    {
        use nara_render::animation::MAX_SPRITE_ANIMATION_SOURCE_BYTES;

        let animation = SpriteAnimationSet::from_ron(SOURCE).unwrap();
        let prepared = prepared.unwrap();
        assert_eq!(prepared.value(), &animation);
        assert!(prepared.dependencies().is_empty());
        let idle = animation.get("idle").unwrap();
        assert_eq!(idle.mode(), PlaybackMode::PingPong);
        assert_eq!(idle.frames()[0].event(), Some("start"));
        assert_eq!(idle.frames()[1].event(), None);
        let cooked = animation.encode_cooked().unwrap();
        assert_eq!(SpriteAnimationSet::from_cooked(&cooked).unwrap(), animation);

        let oversized = " ".repeat(MAX_SPRITE_ANIMATION_SOURCE_BYTES + 1);
        assert!(matches!(
            SpriteAnimationSet::from_ron(&oversized),
            Err(SpriteAnimationAssetError::SourceTooLarge)
        ));
        assert!(SpriteAnimationSet::from_ron(&SOURCE.replace("nanos: 3", "nanos: 0")).is_err());
    }
    #[cfg(not(feature = "animation-authoring"))]
    {
        use nara_render::animation::SpriteAnimationLoaderError;

        let Err(AssetLoadError::Decode { source, .. }) = prepared else {
            panic!("expected a source loading error");
        };
        assert!(matches!(
            source.downcast_ref::<SpriteAnimationLoaderError>(),
            Some(SpriteAnimationLoaderError::SourceDisabled)
        ));
    }
}

#[test]
fn plugin_loads_sprite_animations_from_their_npak_kind() {
    let animation = animation_set();
    let bytes = animation.encode_cooked().unwrap();
    let id: AssetId = ANIMATION_ID.parse().unwrap();
    let raw: AssetId = "0193a100-1111-7000-8000-000000000002".parse().unwrap();
    let mut app = animation_app();
    app.set_asset_source(cooked_source(&[
        NpakInput::new(
            id,
            NpakEntryKind::SpriteAnimation,
            &bytes,
            NpakCompression::Lz4,
        ),
        NpakInput::new(raw, NpakEntryKind::Raw, &bytes, NpakCompression::None),
    ]))
    .unwrap();

    let world = app.world_mut().unwrap();
    let handle = load_asset_sync::<SpriteAnimationSet>(world, id).unwrap();
    assert_eq!(NpakEntryKind::SpriteAnimation as u64, 9);
    assert_eq!(
        world.resource::<Assets<SpriteAnimationSet>>().get(handle),
        Some(&animation)
    );
    assert!(matches!(
        load_asset_sync::<SpriteAnimationSet>(world, raw),
        Err(AssetLoadError::Request(AssetError::WrongRuntimeKind {
            expected: NpakEntryKind::SpriteAnimation,
            actual: NpakEntryKind::Raw,
            ..
        }))
    ));
}

fn spawn_animator(app: &mut App, clip: &str, clock: SpriteAnimationClock) -> Entity {
    let world = app.world_mut().unwrap();
    let handle = add_asset(world, animation_set()).unwrap();
    world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            SpriteAnimator::new(handle, clip).with_clock(clock),
        ))
        .id()
}

fn take_events(app: &mut App) -> Vec<SpriteAnimationEvent> {
    app.world_mut()
        .unwrap()
        .resource_mut::<Messages<SpriteAnimationEvent>>()
        .drain()
        .collect()
}

fn marker(entity: Entity, clip: &str, marker: &str) -> SpriteAnimationEvent {
    SpriteAnimationEvent::FrameMarker {
        entity,
        clip: clip.into(),
        marker: marker.into(),
    }
}

#[derive(Resource, Default)]
struct ExtractedRegion(Option<TextureRegion>);

#[test]
fn virtual_playback_updates_before_extract_and_completes_once() {
    let mut app = animation_app();
    let settings = RuntimeTimeSettings::default().with_time_scale(0.5).unwrap();
    app.insert_resource(settings)
        .unwrap()
        .init_resource::<ExtractedRegion>()
        .unwrap()
        .add_systems(
            CoreStage::Extract,
            |sprites: Query<&Sprite>, mut region: ResMut<ExtractedRegion>| {
                region.0 = sprites.single().unwrap().texture_region;
            },
        )
        .unwrap();
    let entity = spawn_animator(&mut app, "hit", SpriteAnimationClock::Virtual);
    {
        let world = app.world_mut().unwrap();
        let mut animator = world.get_mut::<SpriteAnimator>(entity).unwrap();
        animator.set_speed(2.0).unwrap();
        assert!(animator.set_speed(f32::NAN).is_err());
        assert_eq!(animator.speed(), 2.0);
    }
    app.update_with_delta(Duration::from_nanos(2)).unwrap();
    assert_eq!(app.world().resource::<ExtractedRegion>().0, Some(region(1)));
    assert_eq!(
        take_events(&mut app),
        [
            marker(entity, "hit", "start"),
            marker(entity, "hit", "step")
        ]
    );

    app.world_mut()
        .unwrap()
        .get_mut::<SpriteAnimator>(entity)
        .unwrap()
        .pause();
    app.update_with_delta(Duration::from_nanos(20)).unwrap();
    let animator = app.world().get::<SpriteAnimator>(entity).unwrap();
    assert_eq!(animator.elapsed(), Duration::from_nanos(2));
    assert!(take_events(&mut app).is_empty());

    app.world_mut()
        .unwrap()
        .get_mut::<SpriteAnimator>(entity)
        .unwrap()
        .resume();
    app.update_with_delta(Duration::from_nanos(12)).unwrap();
    assert_eq!(app.world().resource::<ExtractedRegion>().0, Some(region(2)));
    let animator = app.world().get::<SpriteAnimator>(entity).unwrap();
    assert_eq!(animator.elapsed(), Duration::from_nanos(10));
    assert!(!animator.is_playing());
    assert_eq!(
        take_events(&mut app),
        [
            marker(entity, "hit", "end"),
            SpriteAnimationEvent::Completed {
                entity,
                clip: "hit".into()
            },
        ]
    );
    app.update_with_delta(Duration::from_nanos(12)).unwrap();
    assert!(take_events(&mut app).is_empty());

    app.world_mut()
        .unwrap()
        .get_mut::<SpriteAnimator>(entity)
        .unwrap()
        .play("walk");
    app.update_with_delta(Duration::ZERO).unwrap();
    assert_eq!(app.world().resource::<ExtractedRegion>().0, Some(region(0)));
    assert_eq!(take_events(&mut app), [marker(entity, "walk", "start")]);
}

#[test]
fn fixed_playback_reports_crossed_loops_and_bounds_large_steps() {
    let mut app = animation_app();
    let entity = spawn_animator(&mut app, "walk", SpriteAnimationClock::Fixed);
    app.update_with_delta(Duration::from_nanos(7)).unwrap();
    assert_eq!(
        app.world().get::<Sprite>(entity).unwrap().texture_region,
        None
    );

    app.world_mut()
        .unwrap()
        .get_mut::<SpriteAnimator>(entity)
        .unwrap()
        .set_speed(0.5)
        .unwrap();
    app.step_fixed(Duration::from_nanos(3)).unwrap();
    assert_eq!(take_events(&mut app), [marker(entity, "walk", "start")]);
    app.step_fixed(Duration::from_nanos(47)).unwrap();
    let animator = app.world().get::<SpriteAnimator>(entity).unwrap();
    assert_eq!(animator.elapsed(), Duration::from_nanos(25));
    assert_eq!(
        take_events(&mut app),
        [
            "step", "end", "start", "step", "end", "start", "step", "end"
        ]
        .map(|name| marker(entity, "walk", name))
    );

    {
        let world = app.world_mut().unwrap();
        let mut animator = world.get_mut::<SpriteAnimator>(entity).unwrap();
        animator.play("bounce");
        animator.set_speed(1.0).unwrap();
    }
    app.step_fixed(Duration::from_nanos(13)).unwrap();
    assert_eq!(
        take_events(&mut app),
        ["start", "step", "end", "step", "start"].map(|name| marker(entity, "bounce", name))
    );
    app.world_mut()
        .unwrap()
        .get_mut::<SpriteAnimator>(entity)
        .unwrap()
        .restart();
    app.step_fixed(Duration::from_nanos(10_007)).unwrap();
    assert_eq!(take_events(&mut app).len(), MAX_MARKERS_PER_STEP);
    assert_eq!(
        app.world().get::<Sprite>(entity).unwrap().texture_region,
        Some(region(1))
    );
}

#[cfg(feature = "animation-scene")]
#[test]
fn scene_source_applies_clip_changes_and_removes_its_generated_animator() {
    use nara_scene::components::SpriteAnimatorSource;

    let id = ANIMATION_ID.parse().unwrap();
    let bytes = animation_set().encode_cooked().unwrap();
    let mut app = animation_app();
    let source = SpriteAnimatorSource {
        set: id,
        clip: "walk".into(),
    };
    let generated = app.world_mut().unwrap().spawn(source.clone()).id();
    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(app.world().get::<SpriteAnimator>(generated).is_none());

    app.set_asset_source(cooked_source(&[NpakInput::new(
        id,
        NpakEntryKind::SpriteAnimation,
        &bytes,
        NpakCompression::None,
    )]))
    .unwrap();
    let world = app.world_mut().unwrap();
    let handle = load_asset_sync::<SpriteAnimationSet>(world, id).unwrap();
    let manual = world
        .spawn((source, SpriteAnimator::new(handle, "hit")))
        .id();

    app.update_with_delta(Duration::ZERO).unwrap();
    for entity in [generated, manual] {
        let animator = app.world().get::<SpriteAnimator>(entity).unwrap();
        assert_eq!(animator.clip(), "walk");
    }

    let world = app.world_mut().unwrap();
    world
        .get_mut::<SpriteAnimatorSource>(generated)
        .unwrap()
        .clip = "hit".into();
    world.get_mut::<SpriteAnimator>(manual).unwrap().play("hit");
    app.update_with_delta(Duration::ZERO).unwrap();
    for entity in [generated, manual] {
        let animator = app.world().get::<SpriteAnimator>(entity).unwrap();
        assert_eq!(animator.clip(), "hit");
        app.world_mut()
            .unwrap()
            .entity_mut(entity)
            .remove::<SpriteAnimatorSource>();
    }
    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(app.world().get::<SpriteAnimator>(generated).is_none());
    assert!(app.world().get::<SpriteAnimator>(manual).is_some());
}
