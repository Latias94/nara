#![cfg(all(feature = "scene-cook", feature = "physics-2d"))]

use std::time::{Duration, Instant};

use nara::{
    asset::{
        NpakEntryKind, RuntimeAssetMode,
        image::SamplerDescriptor,
        source::{MemoryAssetSource, asset_id_path},
    },
    physics2d::{CompoundCollider2d, TilemapColliders2d},
    prelude::*,
    reflect::ComponentSchemaCatalog,
    render::tilemap::TileChunkCoord,
    scene::{
        authoring::{SceneDocument, SceneEntity},
        cook::cook_flattened_scene,
    },
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
fn cooked_tiles_rebuild_only_dirty_render_chunks_and_generate_merged_colliders() {
    let scene_id = "0193f100-2222-7000-8000-000000000031".parse().unwrap();
    let tileset_id = "0193f100-2222-7000-8000-000000000032".parse().unwrap();
    let image_id = "0193f100-2222-7000-8000-000000000033".parse().unwrap();
    let mut document = SceneDocument::new("681a78dc-b6ab-4f3a-8eb6-4742d2ced237".parse().unwrap());
    let mut row = SceneEntity::new(
        "4f99b200-2a5e-4872-8d3d-000000000006".parse().unwrap(),
        "Tiles",
    );
    row.set(&TilemapSource {
        tileset: tileset_id,
        tile_size: Vec2::ONE,
        width: 33,
        height: 1,
        origin_x: 0,
        origin_y: 0,
        layer: -1,
        cells: vec![1; 33].into(),
    })
    .unwrap();
    document.entities.push(row);
    let scene = cook_flattened_scene(&document, &ComponentSchemaCatalog::default()).unwrap();
    let tileset = TileSetAsset {
        image: image_id,
        layout: TileAtlasLayout::grid(Vec2::ONE, 1, 1),
        sampler: SamplerDescriptor::NEAREST_CLAMP,
        tiles: vec![TileDefinition {
            solid: true,
            ..Default::default()
        }],
    };
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![255; 4],
    )
    .unwrap();
    let mut source = MemoryAssetSource::new();
    let mut kinds = Vec::new();
    for (id, kind, bytes) in [
        (scene_id, NpakEntryKind::Scene, scene.bytes),
        (
            tileset_id,
            NpakEntryKind::TileSet,
            tileset.encode_cooked().unwrap(),
        ),
        (
            image_id,
            NpakEntryKind::Image,
            image.encode_cooked().unwrap(),
        ),
    ] {
        kinds.push((id, kind));
        source.insert(&asset_id_path(id), bytes).unwrap();
    }
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless()).unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Cooked,
        source,
        kinds,
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    let scene = switch_scene_sync(world, scene_id).unwrap();
    let entity = *world
        .resource::<SceneManager>()
        .entities(scene)
        .unwrap()
        .values()
        .next()
        .unwrap();
    load_asset_sync::<ImageAsset>(world, image_id).unwrap();
    app.step_fixed(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(
        app.world()
            .get::<TilemapColliders2d>(entity)
            .unwrap()
            .entities()
            .len(),
        2
    );
    app.update_with_delta(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let map = app.world().get::<Tilemap>(entity).unwrap();
    assert_eq!(map.cell_count(), 33);
    let chunks = map.chunk_revisions().collect::<Vec<_>>();
    assert_eq!(chunks.len(), 2);

    let world = app.world();
    let colliders = world.get::<TilemapColliders2d>(entity).unwrap();
    assert_eq!(colliders.entities().len(), 2);
    assert!(colliders.entities().iter().all(|&entity| {
        world
            .get::<CompoundCollider2d>(entity)
            .unwrap()
            .boxes()
            .len()
            == 1
            && world
                .resource::<PhysicsWorld2d>()
                .body_error(entity)
                .is_none()
    }));

    app.world_mut()
        .unwrap()
        .get_mut::<TilemapSource>(entity)
        .unwrap()
        .cells
        .0[32] = 0;
    app.update_with_delta(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let map = app.world().get::<Tilemap>(entity).unwrap();
    assert_eq!(map.cell_count(), 32);
    assert_eq!(map.chunk_revisions().collect::<Vec<_>>(), [chunks[0]]);
    assert_eq!(chunks[0].coord, TileChunkCoord::new(0, 0));
    assert_eq!(
        app.world()
            .get::<TilemapColliders2d>(entity)
            .unwrap()
            .entities()
            .len(),
        1
    );

    app.world_mut()
        .unwrap()
        .resource_mut::<SceneManager>()
        .despawn_scene(scene);
    app.update_with_delta(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert!(app.world().get_entity(entity).is_err());
    assert_eq!(
        app.world()
            .resource::<PhysicsWorld2d>()
            .compound_colliders()
            .count(),
        0
    );
}

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
