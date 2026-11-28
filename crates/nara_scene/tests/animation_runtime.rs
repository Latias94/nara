#![cfg(all(feature = "runtime", feature = "cook"))]

use std::time::{Duration, Instant};

use bevy_ecs::{component::Component, message::Messages};
use nara_app::App;
use nara_asset::{
    AssetAppExt, AssetPlugin, NpakEntryKind, RuntimeAssetMode, RuntimeAssetSource, add_asset,
    source::{MemoryAssetSource, asset_id_path},
};
use nara_core::{AssetId, Vec2};
use nara_reflect::{ComponentRegistryPlugin, ComponentTypeId, Schema};
use nara_scene::{
    RegisterSceneComponent, SceneManager, ScenePlugin,
    animation::{
        AnimationPlayer, AnimationPlugin, Easing, KeyframeValue, PropertyAnimationClip,
        PropertyAnimationEvent, PropertyKeyframe, PropertyMarker, PropertyPlaybackMode,
        PropertyTrack, Tween2d, TweenFinished, TweenPlugin,
    },
    authoring::{SceneDocument, SceneEntity},
    components::AnimationSource,
    cook::cook_flattened_scene,
    spatial::{Transform2d, Visibility},
    switch_scene_sync,
};

const STEP: Duration = Duration::from_millis(10);

#[derive(Component, Schema)]
#[schema(
    crate = "nara_reflect",
    id = "fixture.AnimationTarget",
    version = 1,
    field_capabilities(scene, inspect, edit)
)]
struct AnimationTarget {
    position: Vec2,
    charges: u32,
}

fn keys(from: KeyframeValue, to: KeyframeValue) -> Vec<PropertyKeyframe> {
    vec![
        PropertyKeyframe {
            time: Duration::ZERO,
            value: from,
            easing: Easing::Linear,
        },
        PropertyKeyframe {
            time: STEP * 2,
            value: to,
            easing: Easing::Linear,
        },
    ]
}

#[test]
fn an_authored_clip_runs_on_fixed_time_and_addresses_a_custom_child_field() {
    let scene: AssetId = "0193f100-2222-7000-8000-000000000011".parse().unwrap();
    let asset: AssetId = "0193f100-2222-7000-8000-000000000012".parse().unwrap();
    let mut document = SceneDocument::new(scene.to_string().parse().unwrap());
    let mut root = SceneEntity::new(
        "4f99b200-2a5e-4872-8d3d-000000000001".parse().unwrap(),
        "Animated root",
    );
    let mut child = SceneEntity::new(
        "4f99b200-2a5e-4872-8d3d-000000000002".parse().unwrap(),
        "Animated child",
    );
    child.parent = Some(root.id);
    root.set(&AnimationSource { clip: asset }).unwrap();
    root.set(&Transform2d::default()).unwrap();
    root.set(&Visibility::Visible).unwrap();
    child
        .set(&AnimationTarget {
            position: Vec2::new(0.0, 11.0),
            charges: 0,
        })
        .unwrap();
    let expected = (root.id, child.id);
    document.entities.extend([root, child]);
    let catalog = nara_reflect::ComponentSchemaCatalog::from_components([
        AnimationTarget::type_schema().unwrap(),
    ])
    .unwrap();
    let cooked = cook_flattened_scene(&document, &catalog).unwrap();
    let clip = PropertyAnimationClip::new(
        STEP * 2,
        PropertyPlaybackMode::Once,
        vec![
            PropertyTrack::new(
                Some(expected.1),
                ComponentTypeId::new(AnimationTarget::COMPONENT_ID),
                "position.x".parse().unwrap(),
                keys(KeyframeValue::F32(0.0), KeyframeValue::F32(5.0)),
            )
            .unwrap(),
            PropertyTrack::new(
                Some(expected.1),
                ComponentTypeId::new(AnimationTarget::COMPONENT_ID),
                "charges".parse().unwrap(),
                keys(KeyframeValue::I64(0), KeyframeValue::I64(3)),
            )
            .unwrap(),
            PropertyTrack::new(
                None,
                ComponentTypeId::new(Transform2d::COMPONENT_ID),
                "translation".parse().unwrap(),
                keys(
                    KeyframeValue::Vec2(Vec2::ZERO),
                    KeyframeValue::Vec2(Vec2::splat(8.0)),
                ),
            )
            .unwrap(),
            PropertyTrack::new(
                None,
                ComponentTypeId::new(Visibility::COMPONENT_ID),
                "mode".parse().unwrap(),
                keys(
                    KeyframeValue::String("visible".into()),
                    KeyframeValue::String("hidden".into()),
                ),
            )
            .unwrap(),
        ],
        vec![PropertyMarker {
            time: STEP,
            name: "middle".into(),
        }],
    )
    .unwrap();

    let mut files = MemoryAssetSource::new();
    files.insert(&asset_id_path(scene), cooked.bytes).unwrap();
    files
        .insert(&asset_id_path(asset), clip.encode_cooked().unwrap())
        .unwrap();
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        AssetPlugin,
        ScenePlugin,
        AnimationPlugin,
    ))
    .unwrap();
    app.register_scene_component::<AnimationTarget>().unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Cooked,
        files,
        [
            (scene, NpakEntryKind::Scene),
            (asset, NpakEntryKind::PropertyAnimation),
        ],
    ))
    .unwrap();
    let instance = switch_scene_sync(app.world_mut().unwrap(), scene).unwrap();
    let entities = app
        .world()
        .resource::<SceneManager>()
        .entities(instance)
        .unwrap();
    let (root, child) = (entities[&expected.0], entities[&expected.1]);
    let mut events = app
        .world()
        .resource::<Messages<PropertyAnimationEvent>>()
        .get_cursor_current();
    app.step_fixed(STEP).unwrap();
    assert_eq!(
        app.world().get::<Transform2d>(root).unwrap().translation,
        Vec2::splat(4.0)
    );
    assert_eq!(
        app.world().get::<AnimationTarget>(child).unwrap().position,
        Vec2::new(2.5, 11.0)
    );
    assert!(
        matches!(events.read(app.world().resource::<Messages<PropertyAnimationEvent>>()).next(), Some(PropertyAnimationEvent::Marker { entity, name, .. }) if *entity == root && name == "middle")
    );

    app.step_fixed(STEP).unwrap();
    assert_eq!(
        app.world().get::<Visibility>(root),
        Some(&Visibility::Hidden)
    );
    assert_eq!(
        app.world().get::<AnimationTarget>(child).unwrap().charges,
        3
    );
    assert_eq!(
        app.world().get::<AnimationTarget>(child).unwrap().position,
        Vec2::new(5.0, 11.0)
    );
    let mut player = app
        .world_mut()
        .unwrap()
        .get_mut::<AnimationPlayer>(root)
        .unwrap();
    assert!(!player.is_playing() && player.error().is_none());
    player.resume();
    assert!(!player.is_playing());
    assert!(
        matches!(events.read(app.world().resource::<Messages<PropertyAnimationEvent>>()).next(), Some(PropertyAnimationEvent::Completed { entity, .. }) if *entity == root)
    );
}

#[test]
fn delayed_tween_completes_once_and_can_restart() {
    let mut app = App::new();
    app.add_plugins(TweenPlugin).unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn((
            Transform2d::default(),
            Tween2d::new(STEP * 2, Easing::QuadIn)
                .with_delay(STEP * 2)
                .with_translation(Vec2::ZERO, Vec2::splat(8.0)),
        ))
        .id();
    let mut events = app
        .world()
        .resource::<Messages<TweenFinished>>()
        .get_cursor_current();
    for expected in [0.0, 0.0, 2.0, 8.0] {
        app.step_fixed(STEP).unwrap();
        assert_eq!(
            app.world().get::<Transform2d>(entity).unwrap().translation,
            Vec2::splat(expected)
        );
    }
    assert_eq!(
        events
            .read(app.world().resource::<Messages<TweenFinished>>())
            .copied()
            .collect::<Vec<_>>(),
        [TweenFinished { entity }]
    );
    app.step_fixed(STEP).unwrap();
    assert_eq!(
        events
            .read(app.world().resource::<Messages<TweenFinished>>())
            .count(),
        0
    );
    app.world_mut()
        .unwrap()
        .get_mut::<Tween2d>(entity)
        .unwrap()
        .restart();
    app.step_fixed(STEP).unwrap();
    app.step_fixed(STEP).unwrap();
    assert_eq!(
        app.world().get::<Transform2d>(entity).unwrap().translation,
        Vec2::ZERO
    );
}

#[test]
#[ignore = "reports warm headless animation throughput"]
fn benchmark_500_animated_transforms() {
    let clip = PropertyAnimationClip::new(
        STEP * 2,
        PropertyPlaybackMode::PingPong,
        vec![
            PropertyTrack::new(
                None,
                ComponentTypeId::new(Transform2d::COMPONENT_ID),
                "translation.x".parse().unwrap(),
                keys(KeyframeValue::F32(0.0), KeyframeValue::F32(10.0)),
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let mut app = App::new();
    app.add_plugins((AssetPlugin, AnimationPlugin)).unwrap();
    let world = app.world_mut().unwrap();
    let clip = add_asset(world, clip).unwrap();
    world
        .spawn_batch((0..500).map(|_| (Transform2d::default(), AnimationPlayer::new(clip))))
        .for_each(drop);
    app.step_fixed(STEP).unwrap();
    let start = Instant::now();
    for _ in 0..240 {
        app.step_fixed(STEP).unwrap();
    }
    let average = start.elapsed() / 240;
    println!(
        "500 animated transforms averaged {:.1} µs per fixed tick",
        average.as_secs_f64() * 1e6
    );
    assert!(average < Duration::from_secs_f64(1.0 / 120.0));
}
