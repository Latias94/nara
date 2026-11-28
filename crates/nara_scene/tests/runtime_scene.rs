#![cfg(all(feature = "runtime", feature = "cook"))]

use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy_ecs::{
    hierarchy::ChildOf,
    prelude::{Component, Messages},
};
use nara_app::App;
use nara_asset::{
    AssetAppExt, AssetLoadContext, AssetLoader, AssetPlugin, AssetStates, Assets, RuntimeAssetMode,
    RuntimeAssetSource,
    cooked::{NpakArchive, NpakCompression, NpakEntryKind, NpakInput, encode_npak},
    load_asset_sync,
    source::{MemoryAssetSource, asset_id_path},
};
use nara_core::AssetId;
use nara_reflect::{ComponentRegistryPlugin, ComponentSchemaCatalog, Schema};
use nara_scene::{
    Name, RegisterSceneComponent, SceneAsset, SceneEvent, SceneInstanceId, SceneLoadState,
    SceneManager, SceneMember, ScenePlugin,
    authoring::{SceneDocument, SceneEntity, SpriteSource},
    cook::cook_scene,
    switch_scene_sync,
};

const FIRST: &str = "0193f100-2222-7000-8000-000000000011";
const SECOND: &str = "0193f100-2222-7000-8000-000000000012";
const IMAGE: &str = "0193f100-2222-7000-8000-000000000013";
const MISSING: &str = "0193f100-2222-7000-8000-000000000014";
const PREFAB: &str = "0193f100-2222-7000-8000-000000000015";

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(crate = "nara_reflect", id = "fixture.Sector", version = 1)]
struct Sector {
    number: u32,
}

struct ImageBytes;

impl AssetLoader for ImageBytes {
    type Asset = Vec<u8>;
    type Error = Infallible;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Image)
    }

    fn load(&self, bytes: &[u8], _: &mut AssetLoadContext<'_>) -> Result<Vec<u8>, Infallible> {
        Ok(bytes.to_vec())
    }
}

fn app(source: RuntimeAssetSource) -> App {
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, AssetPlugin, ScenePlugin))
        .unwrap();
    app.register_scene_component::<Sector>().unwrap();
    app.set_asset_source(source).unwrap();
    app.register_asset_loader(ImageBytes).unwrap();
    app
}

fn schemas() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog::from_components([Sector::type_schema().unwrap()]).unwrap()
}

fn document(number: u32, image: Option<AssetId>) -> SceneDocument {
    let mut scene = SceneDocument::new("681a78dc-b6ab-4f3a-8eb6-4742d2ced237".parse().unwrap());
    let root = "4f99b200-2a5e-4872-8d3d-000000000001".parse().unwrap();
    let mut sector = SceneEntity::new(root, "Sector");
    sector.set(&Sector { number }).unwrap();
    sector
        .set(&SpriteSource {
            image,
            ..Default::default()
        })
        .unwrap();
    let mut child = SceneEntity::new(
        "4f99b200-2a5e-4872-8d3d-000000000002".parse().unwrap(),
        "Child",
    );
    child.parent = Some(root);
    scene.entities.extend([sector, child]);
    scene
}

fn pack(include_image: bool) -> RuntimeAssetSource {
    let first = cook_scene(&document(1, None), &schemas(), |_| {
        Err::<SceneDocument, _>("unexpected prefab")
    })
    .unwrap()
    .bytes;
    let second = cook_scene(
        &document(2, Some(IMAGE.parse().unwrap())),
        &schemas(),
        |_| Err::<SceneDocument, _>("unexpected prefab"),
    )
    .unwrap()
    .bytes;
    let mut entries = vec![
        NpakInput::new(
            FIRST.parse().unwrap(),
            NpakEntryKind::Scene,
            &first,
            NpakCompression::None,
        ),
        NpakInput::new(
            SECOND.parse().unwrap(),
            NpakEntryKind::Scene,
            &second,
            NpakCompression::None,
        ),
    ];
    if include_image {
        entries.push(NpakInput::new(
            IMAGE.parse().unwrap(),
            NpakEntryKind::Image,
            b"pixel",
            NpakCompression::None,
        ));
    }
    let archive = NpakArchive::from_bytes(encode_npak(&entries).unwrap()).unwrap();
    RuntimeAssetSource::from_npak_archive(Arc::new(archive))
}

fn settle(app: &mut App, instance: SceneInstanceId) -> SceneLoadState {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update_with_delta(Duration::ZERO).unwrap();
        let state = app
            .world()
            .resource::<SceneManager>()
            .state(instance)
            .unwrap();
        if matches!(state, SceneLoadState::Ready | SceneLoadState::Failed(_)) {
            return state.clone();
        }
        assert!(Instant::now() < deadline, "scene did not settle");
        std::thread::yield_now();
    }
}

#[test]
fn switch_commits_after_preload_and_keeps_original_roots_and_independent_scenes() {
    let mut app = app(pack(true));
    let original = switch_scene_sync(app.world_mut().unwrap(), FIRST.parse().unwrap()).unwrap();
    let (independent, next) = {
        let mut manager = app.world_mut().unwrap().resource_mut::<SceneManager>();
        (
            manager.load_scene(FIRST.parse().unwrap()),
            manager.switch_scene(SECOND.parse().unwrap()),
        )
    };
    let before = app
        .world()
        .resource::<SceneManager>()
        .entities(original)
        .unwrap()
        .clone();
    let mut cursor = app
        .world()
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_dependency_preload = false;
    loop {
        app.update_with_delta(Duration::ZERO).unwrap();
        let world = app.world();
        let manager = world.resource::<SceneManager>();
        match manager.state(next).unwrap() {
            SceneLoadState::LoadingScene | SceneLoadState::LoadingDependencies { .. } => {
                saw_dependency_preload |= matches!(
                    manager.state(next),
                    Some(SceneLoadState::LoadingDependencies { total: 1, .. })
                );
                assert_eq!(manager.main_instance(), Some(original));
                assert!(
                    before
                        .values()
                        .all(|&entity| world.get_entity(entity).is_ok())
                );
            }
            SceneLoadState::Ready => break,
            state => panic!("unexpected state {state:?}"),
        }
        assert!(Instant::now() < deadline, "scene did not settle");
        std::thread::yield_now();
    }
    assert!(saw_dependency_preload);
    let world = app.world();
    let manager = world.resource::<SceneManager>();
    assert_eq!(manager.main_instance(), Some(next));
    assert_eq!(manager.state(independent), Some(&SceneLoadState::Ready));
    assert!(
        before
            .values()
            .all(|&entity| world.get_entity(entity).is_err())
    );
    let entities = manager.entities(next).unwrap();
    let root = *entities.values().next().unwrap();
    let child = *entities.values().nth(1).unwrap();
    assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 2 }));
    assert!(world.get::<ChildOf>(root).is_none());
    assert_eq!(world.get::<ChildOf>(child).unwrap().parent(), root);
    assert!(
        entities
            .values()
            .all(|&e| world.get::<SceneMember>(e) == Some(&SceneMember(next)))
    );
    let events = cursor
        .read(world.resource::<Messages<SceneEvent>>())
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        [
            SceneEvent::Unloaded {
                instance: original,
                scene_id: FIRST.parse().unwrap()
            },
            SceneEvent::Ready {
                instance: next,
                scene_id: SECOND.parse().unwrap()
            },
        ]
    );

    app.world_mut()
        .unwrap()
        .resource_mut::<SceneManager>()
        .despawn_scene(next);
    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(app.world().get_entity(root).is_err());
    assert_eq!(app.world().resource::<Assets<Vec<u8>>>().len(), 1);
}

#[test]
fn missing_scene_or_dependency_keeps_the_main_instance() {
    let mut app = app(pack(false));
    let main = switch_scene_sync(app.world_mut().unwrap(), FIRST.parse().unwrap()).unwrap();
    let originals = app
        .world()
        .resource::<SceneManager>()
        .entities(main)
        .unwrap()
        .clone();
    for id in [MISSING, SECOND] {
        let pending = app
            .world_mut()
            .unwrap()
            .resource_mut::<SceneManager>()
            .switch_scene(id.parse().unwrap());
        assert!(matches!(
            settle(&mut app, pending),
            SceneLoadState::Failed(_)
        ));
        assert_eq!(
            app.world().resource::<SceneManager>().main_instance(),
            Some(main)
        );
        assert!(
            originals
                .values()
                .all(|&entity| app.world().get::<Name>(entity).is_some())
        );
    }
}

#[test]
fn source_scene_bakes_prefabs_to_identical_runtime_bytes() {
    let source = format!(
        r#"Scene(version: 1, id: "681a78dc-b6ab-4f3a-8eb6-4742d2ced237",
        assets: {{"prop": (id: "{PREFAB}", hint: "prop.prefab.ron")}},
        entities: [(id: "4f99b200-2a5e-4872-8d3d-000000000001", name: "Instance", prefab: "@prop")])"#
    );
    let prefab = r#"Prefab(version: 1, id: "681a78dc-b6ab-4f3a-8eb6-4742d2ced238",
        entities: [(id: "4f99b200-2a5e-4872-8d3d-000000000002", name: "Prefab root",
        components: {"fixture.Sector": (number: 3)})])"#;
    let expected = cook_scene(&SceneDocument::parse(&source).unwrap(), &schemas(), |_| {
        SceneDocument::parse_with(prefab, schemas().components())
    })
    .unwrap();
    let mut memory = MemoryAssetSource::new();
    memory
        .insert(&asset_id_path(FIRST.parse().unwrap()), source.as_bytes())
        .unwrap();
    memory
        .insert(&asset_id_path(PREFAB.parse().unwrap()), prefab.as_bytes())
        .unwrap();
    let mut app = app(RuntimeAssetSource::from_memory(
        RuntimeAssetMode::Source,
        memory,
    ));
    let first = FIRST.parse().unwrap();
    let instance = switch_scene_sync(app.world_mut().unwrap(), first).unwrap();
    let handle = load_asset_sync::<SceneAsset>(app.world_mut().unwrap(), first).unwrap();
    let world = app.world();
    let scene = world.resource::<Assets<SceneAsset>>().get(handle).unwrap();
    assert_eq!(scene.bytes(), expected.bytes);
    assert_eq!(
        world
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .dependencies(),
        &[PREFAB.parse::<AssetId>().unwrap()]
    );
    let manager = world.resource::<SceneManager>();
    let entity = *manager.entities(instance).unwrap().values().next().unwrap();
    assert_eq!(world.get::<Sector>(entity), Some(&Sector { number: 3 }));
}
