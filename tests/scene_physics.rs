#![cfg(all(feature = "scene-cook", feature = "physics-2d"))]

use std::time::{Duration, Instant};

use nara::{
    asset::{
        RuntimeAssetMode,
        source::{MemoryAssetSource, asset_id_path},
    },
    prelude::*,
};

#[cfg(all(
    feature = "asset-cooked",
    feature = "presentation-2d",
    feature = "render-wgpu",
    not(target_family = "wasm"),
))]
#[path = "scene_physics/gpu.rs"]
mod gpu;

#[test]
fn switching_scenes_retires_dynamic_bodies_and_keeps_new_roots_physical() {
    let first: AssetId = "0193f100-2222-7000-8000-000000000031".parse().unwrap();
    let second: AssetId = "0193f100-2222-7000-8000-000000000032".parse().unwrap();
    let mut source = MemoryAssetSource::new();
    for (scene, x) in [(first, 0.0), (second, 4.0)] {
        let text = format!(
            r#"Scene(version: 1, id: "681a78dc-b6ab-4f3a-8eb6-4742d2ced237",
            entities: [(id: "4f99b200-2a5e-4872-8d3d-000000000001", name: "Falling orb",
                components: {{"nara.Transform2d": (translation: ({x:?}, 10.0))}})])"#,
        );
        source
            .insert(&asset_id_path(scene), text.as_bytes())
            .unwrap();
    }
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless()).unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory(
        RuntimeAssetMode::Source,
        source,
    ))
    .unwrap();
    let initial = switch_scene_sync(app.world_mut().unwrap(), first).unwrap();
    let old_body = add_body(&mut app, initial);
    app.step_fixed(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_physics(&app, old_body);

    for id in [second, first, second, first] {
        let previous = app
            .world()
            .resource::<SceneManager>()
            .main_instance()
            .unwrap();
        let previous = *app
            .world()
            .resource::<SceneManager>()
            .entities(previous)
            .unwrap()
            .values()
            .next()
            .unwrap();
        let next = app
            .world_mut()
            .unwrap()
            .resource_mut::<SceneManager>()
            .switch_scene(id);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update_with_delta(Duration::ZERO).unwrap();
            let state = app.world().resource::<SceneManager>().state(next).unwrap();
            if state == &SceneLoadState::Ready {
                break;
            }
            assert!(!matches!(state, SceneLoadState::Failed(_)), "{state:?}");
            assert!(Instant::now() < deadline, "scene did not load");
            std::thread::yield_now();
        }
        let body = add_body(&mut app, next);
        app.step_fixed(FixedTime::DEFAULT_TIMESTEP).unwrap();
        assert!(app.world().get_entity(previous).is_err());
        assert_physics(&app, body);
    }
    assert_eq!(app.world().resource::<Assets<SceneAsset>>().len(), 2);
}

fn add_body(app: &mut App, instance: SceneInstanceId) -> Entity {
    let world = app.world_mut().unwrap();
    let body = *world
        .resource::<SceneManager>()
        .entities(instance)
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(world.get::<SceneMember>(body), Some(&SceneMember(instance)));
    assert!(world.get::<ChildOf>(body).is_none());
    world
        .entity_mut(body)
        .insert((RigidBody2d::Dynamic, Collider2d::Circle { radius: 0.5 }));
    body
}

fn assert_physics(app: &App, body: Entity) {
    let world = app.world();
    let physics = world.resource::<PhysicsWorld2d>();
    assert_eq!(physics.body_error(body), None);
    assert_eq!(
        physics
            .colliders()
            .map(|shape| shape.entity)
            .collect::<Vec<_>>(),
        [body]
    );
    assert!(world.get::<Transform2d>(body).unwrap().translation.y < 10.0);
}
