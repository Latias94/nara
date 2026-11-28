#![cfg(all(feature = "runtime", feature = "cook"))]

use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use bevy_ecs::{
    hierarchy::ChildOf,
    lifecycle::HookContext,
    prelude::{Component, Messages, ResMut, Resource, World},
    schedule::IntoScheduleConfigs,
    world::DeferredWorld,
};
use nara_app::{App, CoreStage};
use nara_asset::{
    AssetAppExt, AssetEvents, AssetLoadContext, AssetLoadSet, AssetLoader, AssetPlugin,
    AssetStates, Assets, RuntimeAssetMode, RuntimeAssetSource,
    cooked::{NpakArchive, NpakCompression, NpakEntryKind, NpakInput, encode_npak},
    load_asset_sync,
    source::{MemoryAssetSource, asset_id_path},
};
use nara_core::AssetId;
use nara_reflect::{ComponentRegistryPlugin, ComponentSchemaCatalog, Schema};
use nara_scene::{
    Name, RegisterSceneComponent, SceneAsset, SceneError, SceneEvent, SceneInstanceId,
    SceneLoadState, SceneManager, SceneMember, ScenePlugin, SceneRegistry, SceneUpdateSet,
    authoring::{SceneDocument, SceneEntity, SpriteSource},
    cook::{cook_flattened_scene, cook_scene},
    cooked::CookedSceneError,
    switch_scene_asset_sync, switch_scene_sync,
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
fn synchronous_switch_rejection_preserves_an_in_flight_switch() {
    #[derive(Component, Schema)]
    #[schema(crate = "nara_reflect", id = "fixture.Unregistered", version = 1)]
    struct Unregistered;

    let mut invalid = document(3, None);
    invalid.entities[0].set(&Unregistered).unwrap();
    let schemas = ComponentSchemaCatalog::from_components([
        Sector::type_schema().unwrap(),
        Unregistered::type_schema().unwrap(),
    ])
    .unwrap();
    let invalid =
        SceneAsset::from_cooked(cook_flattened_scene(&invalid, &schemas).unwrap().bytes).unwrap();

    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    let main = switch_scene_sync(world, FIRST.parse().unwrap()).unwrap();
    let second = SECOND.parse().unwrap();
    load_asset_sync::<SceneAsset>(world, second).unwrap();
    let next = world.resource_mut::<SceneManager>().switch_scene(second);
    app.update_with_delta(Duration::ZERO).unwrap();

    let world = app.world_mut().unwrap();
    let manager = world.resource::<SceneManager>();
    let loading = SceneLoadState::LoadingDependencies {
        loaded: 0,
        total: 1,
    };
    assert_eq!(manager.state(next), Some(&loading));
    let root = *manager.entities(main).unwrap().values().next().unwrap();
    let mut events = world
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();
    assert!(matches!(
        switch_scene_asset_sync(world, second, &invalid),
        Err(SceneError::Cooked(CookedSceneError::MissingSchema { .. }))
    ));
    let manager = world.resource::<SceneManager>();
    assert_eq!(manager.main_instance(), Some(main));
    assert_eq!(manager.state(next), Some(&loading));
    assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 1 }));
    assert!(
        events
            .read(world.resource::<Messages<SceneEvent>>())
            .next()
            .is_none()
    );

    assert_eq!(settle(&mut app, next), SceneLoadState::Ready);
    assert_eq!(
        app.world().resource::<SceneManager>().main_instance(),
        Some(next)
    );
}

#[test]
fn wrong_sprite_asset_kind_keeps_the_main_instance() {
    struct MaterialStub;

    impl AssetLoader for MaterialStub {
        type Asset = ();
        type Error = Infallible;

        fn cooked_kind(&self) -> Option<NpakEntryKind> {
            Some(NpakEntryKind::Material)
        }

        fn load(&self, _: &[u8], _: &mut AssetLoadContext<'_>) -> Result<(), Infallible> {
            Ok(())
        }
    }

    let first = FIRST.parse().unwrap();
    let second = SECOND.parse().unwrap();
    for (mode, id, field, expected, actual) in [
        (
            RuntimeAssetMode::Cooked,
            "0193f100-2222-7000-8000-000000000016".parse().unwrap(),
            "image",
            NpakEntryKind::Image,
            NpakEntryKind::Material,
        ),
        (
            RuntimeAssetMode::Source,
            IMAGE.parse().unwrap(),
            "material",
            NpakEntryKind::Material,
            NpakEntryKind::Image,
        ),
    ] {
        let mut invalid = document(2, None);
        let entity = invalid.entities[0].id;
        invalid.entities[0]
            .set(&SpriteSource {
                image: (expected == NpakEntryKind::Image).then_some(id),
                material: (expected == NpakEntryKind::Material).then_some(id),
                ..Default::default()
            })
            .unwrap();

        let mut memory = MemoryAssetSource::new();
        let schemas = schemas();
        for (scene_id, document) in [(first, document(1, None)), (second, invalid)] {
            let bytes = match mode {
                RuntimeAssetMode::Cooked => {
                    cook_flattened_scene(&document, &schemas).unwrap().bytes
                }
                RuntimeAssetMode::Source => document
                    .to_ron_with(schemas.components())
                    .unwrap()
                    .into_bytes(),
            };
            memory.insert(&asset_id_path(scene_id), bytes).unwrap();
        }
        memory.insert(&asset_id_path(id), b"asset").unwrap();
        let mut app = app(RuntimeAssetSource::from_memory_with_kinds(
            mode,
            memory,
            [
                (first, NpakEntryKind::Scene),
                (second, NpakEntryKind::Scene),
                (id, actual),
            ],
        ));
        app.register_asset_loader(MaterialStub).unwrap();

        let world = app.world_mut().unwrap();
        let main = switch_scene_sync(world, first).unwrap();
        let root = world.resource::<SceneManager>().entities(main).unwrap()[&entity];
        let next = world.resource_mut::<SceneManager>().switch_scene(second);
        let message = SceneError::SpriteAssetKindMismatch {
            scene_id: second,
            entity,
            field,
            id,
            expected,
            actual,
        }
        .to_string();
        assert_eq!(
            settle(&mut app, next),
            SceneLoadState::Failed(message.clone())
        );

        let world = app.world_mut().unwrap();
        assert_eq!(
            switch_scene_sync(world, second).unwrap_err().to_string(),
            message
        );
        assert_eq!(world.resource::<SceneManager>().main_instance(), Some(main));
        assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 1 }));
    }
}

#[test]
fn audio_clip_kind_is_checked_before_promotion() {
    // The native audio crate depends on scene, so register a fixture under its stable Schema ID.
    #[derive(Component, Schema)]
    #[schema(crate = "nara_reflect", id = "nara.AudioClipSource", version = 1)]
    struct AuthoredClip {
        clip: AssetId,
    }

    let first = FIRST.parse().unwrap();
    let second = SECOND.parse().unwrap();
    let clip = IMAGE.parse().unwrap();
    let mut target = document(2, None);
    let entity = target.entities[0].id;
    target.entities[0].set(&AuthoredClip { clip }).unwrap();
    let schemas = ComponentSchemaCatalog::from_components([
        Sector::type_schema().unwrap(),
        AuthoredClip::type_schema().unwrap(),
    ])
    .unwrap();
    let initial = cook_flattened_scene(&document(1, None), &schemas).unwrap();
    let target = cook_flattened_scene(&target, &schemas).unwrap();
    let make_source = |kind| {
        let mut memory = MemoryAssetSource::new();
        memory
            .insert(&asset_id_path(first), initial.bytes.as_slice())
            .unwrap();
        memory
            .insert(&asset_id_path(second), target.bytes.as_slice())
            .unwrap();
        memory.insert(&asset_id_path(clip), b"asset").unwrap();
        RuntimeAssetSource::from_memory_with_kinds(
            RuntimeAssetMode::Cooked,
            memory,
            [
                (first, NpakEntryKind::Scene),
                (second, NpakEntryKind::Scene),
                (clip, kind),
            ],
        )
    };
    let live = make_source(NpakEntryKind::Image);
    let candidate = make_source(NpakEntryKind::Audio);
    let asset = SceneAsset::from_cooked(target.bytes).unwrap();
    let mut app = app(live.clone());
    app.register_scene_component::<AuthoredClip>().unwrap();
    let world = app.world_mut().unwrap();
    let main = switch_scene_sync(world, first).unwrap();
    let root = world.resource::<SceneManager>().entities(main).unwrap()[&entity];
    let registry = world.resource::<SceneRegistry>();
    let decoded_clip = registry
        .validate_asset(second, &asset, &candidate, |prepared| {
            let row = prepared.rows().next().unwrap();
            assert_eq!(row.id(), entity);
            Ok(row.get::<AuthoredClip>().unwrap().clip)
        })
        .unwrap();
    assert_eq!(decoded_clip, clip);
    assert_eq!(world.resource::<RuntimeAssetSource>(), &live);

    let mismatch = SceneError::AudioClipAssetKindMismatch {
        scene_id: second,
        entity,
        id: clip,
        actual: NpakEntryKind::Image,
    }
    .to_string();
    assert_eq!(
        registry
            .validate_asset(second, &asset, &live, |_| -> Result<(), String> {
                panic!("called application validation for an invalid clip")
            })
            .unwrap_err()
            .to_string(),
        mismatch
    );
    assert_eq!(
        switch_scene_asset_sync(world, second, &asset)
            .unwrap_err()
            .to_string(),
        mismatch
    );
    let next = world
        .resource_mut::<SceneManager>()
        .switch_scene_validated(second, |_| panic!("validated an invalid clip"));
    assert_eq!(settle(&mut app, next), SceneLoadState::Failed(mismatch));
    let world = app.world();
    assert_eq!(world.resource::<SceneManager>().main_instance(), Some(main));
    assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 1 }));
}

#[test]
fn application_validation_can_reject_a_ready_candidate() {
    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    let first = FIRST.parse().unwrap();
    let second = SECOND.parse().unwrap();
    let main = switch_scene_sync(world, first).unwrap();
    let root = *world
        .resource::<SceneManager>()
        .entities(main)
        .unwrap()
        .values()
        .next()
        .unwrap();
    let next = world
        .resource_mut::<SceneManager>()
        .switch_scene_validated(second, |prepared| {
            let mut rows = prepared.rows();
            let root = rows.next().unwrap();
            let child = rows.next().unwrap();
            assert_eq!(root.parent(), None);
            assert_eq!(root.get::<Sector>(), Some(&Sector { number: 2 }));
            assert_eq!(child.parent(), Some(root.id()));
            assert!(child.get::<Sector>().is_none());
            Err("missing arena role".into())
        });
    let message = SceneError::ValidationFailed {
        scene_id: second,
        message: "missing arena role".into(),
    }
    .to_string();
    let mut events = world
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();
    assert_eq!(
        settle(&mut app, next),
        SceneLoadState::Failed(message.clone())
    );
    let world = app.world();
    let manager = world.resource::<SceneManager>();
    assert_eq!(manager.main_instance(), Some(main));
    assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 1 }));
    assert_eq!(
        events
            .read(world.resource::<Messages<SceneEvent>>())
            .cloned()
            .collect::<Vec<_>>(),
        [SceneEvent::Failed {
            instance: next,
            scene_id: second,
            message
        }]
    );

    let accepted = app
        .world_mut()
        .unwrap()
        .resource_mut::<SceneManager>()
        .switch_scene_validated(second, |_| Ok(()));
    assert_eq!(settle(&mut app, accepted), SceneLoadState::Ready);
    assert_eq!(
        app.world().resource::<SceneManager>().main_instance(),
        Some(accepted)
    );
}

#[test]
fn a_hook_can_cancel_a_pending_switch_in_the_same_tick() {
    #[derive(Resource)]
    struct CancelSwitch(SceneInstanceId);

    fn cancel_switch(mut world: DeferredWorld<'_>, _: HookContext) {
        if let Some(cancelled) = world.get_resource::<CancelSwitch>().map(|cancel| cancel.0) {
            world
                .resource_mut::<SceneManager>()
                .despawn_scene(cancelled);
        }
    }

    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    world
        .register_component_hooks::<Sector>()
        .on_add(cancel_switch);
    let first = FIRST.parse().unwrap();
    let main = switch_scene_sync(world, first).unwrap();
    let mut manager = world.resource_mut::<SceneManager>();
    let root = *manager.entities(main).unwrap().values().next().unwrap();
    let independent = manager.load_scene(first);
    let cancelled = manager.switch_scene(first);
    world.insert_resource(CancelSwitch(cancelled));
    let mut events = world
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();

    app.update_with_delta(Duration::ZERO).unwrap();
    let world = app.world();
    let manager = world.resource::<SceneManager>();
    assert_eq!(manager.state(independent), Some(&SceneLoadState::Ready));
    assert_eq!(manager.main_instance(), Some(main));
    assert!(manager.entities(cancelled).is_none());
    assert_eq!(world.get::<Sector>(root), Some(&Sector { number: 1 }));
    assert_eq!(
        events
            .read(world.resource::<Messages<SceneEvent>>())
            .cloned()
            .collect::<Vec<_>>(),
        [SceneEvent::Ready {
            instance: independent,
            scene_id: first
        }]
    );

    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(
        app.world()
            .resource::<SceneManager>()
            .state(cancelled)
            .is_none()
    );
}

#[test]
fn switches_requested_from_component_hooks_survive_promotion() {
    #[derive(Default, Resource)]
    struct NextSwitch(Option<SceneInstanceId>);

    fn request_next(mut world: DeferredWorld<'_>, _: HookContext) {
        if world
            .get_resource::<NextSwitch>()
            .is_some_and(|next| next.0.is_none())
        {
            let next = world
                .resource_mut::<SceneManager>()
                .switch_scene(FIRST.parse().unwrap());
            world.resource_mut::<NextSwitch>().0 = Some(next);
        }
    }

    for on_remove in [true, false] {
        let mut app = app(pack(true));
        let world = app.world_mut().unwrap();
        let hooks = world.register_component_hooks::<Sector>();
        if on_remove {
            hooks.on_remove(request_next);
        } else {
            hooks.on_add(request_next);
        }
        switch_scene_sync(world, FIRST.parse().unwrap()).unwrap();
        world.init_resource::<NextSwitch>();

        let second = world
            .resource_mut::<SceneManager>()
            .switch_scene(SECOND.parse().unwrap());
        assert_eq!(settle(&mut app, second), SceneLoadState::Ready);
        let world = app.world();
        assert_eq!(
            world.resource::<SceneManager>().main_instance(),
            Some(second)
        );
        let next = world.resource::<NextSwitch>().0.unwrap();

        assert_eq!(settle(&mut app, next), SceneLoadState::Ready);
        let manager = app.world().resource::<SceneManager>();
        assert_eq!(manager.main_instance(), Some(next));
        assert!(manager.state(second).is_none());
    }
}

#[test]
fn a_reentrant_sync_switch_keeps_the_newest_main_scene() {
    #[derive(Default, Resource)]
    struct Reenter {
        armed: bool,
        activated: Option<SceneInstanceId>,
    }

    fn switch_again(mut world: DeferredWorld<'_>, _: HookContext) {
        if world
            .get_resource_mut::<Reenter>()
            .is_some_and(|mut reenter| std::mem::take(&mut reenter.armed))
        {
            world.commands().queue(|world: &mut World| {
                let instance = switch_scene_sync(world, FIRST.parse().unwrap()).unwrap();
                world.resource_mut::<Reenter>().activated = Some(instance);
            });
        }
    }

    for on_remove in [true, false] {
        let mut app = app(pack(true));
        let world = app.world_mut().unwrap();
        let hooks = world.register_component_hooks::<Sector>();
        if on_remove {
            hooks.on_remove(switch_again);
        } else {
            hooks.on_add(switch_again);
        }
        let original = switch_scene_sync(world, FIRST.parse().unwrap()).unwrap();
        world.insert_resource(Reenter {
            armed: true,
            ..Default::default()
        });
        let second = SECOND.parse().unwrap();
        assert!(matches!(
            switch_scene_sync(world, second),
            Err(SceneError::ActivationCancelled { scene_id }) if scene_id == second
        ));

        let newest = world.resource::<Reenter>().activated.unwrap();
        let manager = world.resource::<SceneManager>();
        assert_eq!(manager.main_instance(), Some(newest));
        assert_eq!(manager.state(original), None);
        let members = world
            .query::<&SceneMember>()
            .iter(world)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(members, [SceneMember(newest); 2]);
    }
}

#[test]
fn an_on_add_hook_can_cancel_its_own_sync_activation() {
    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    world
        .register_component_hooks::<Sector>()
        .on_add(|mut world, context| {
            let instance = world.get::<SceneMember>(context.entity).unwrap().0;
            world.commands().queue(move |world: &mut World| {
                world.resource_mut::<SceneManager>().despawn_scene(instance);
            });
        });
    let id = FIRST.parse().unwrap();
    let mut events = world
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();

    assert!(matches!(
        switch_scene_sync(world, id),
        Err(SceneError::ActivationCancelled { scene_id }) if scene_id == id
    ));
    let [SceneEvent::Unloaded { instance, scene_id }] = events
        .read(world.resource::<Messages<SceneEvent>>())
        .collect::<Vec<_>>()[..]
    else {
        panic!("expected only the unload event for the cancelled scene");
    };
    assert_eq!(*scene_id, id);
    let manager = world.resource::<SceneManager>();
    assert_eq!(manager.main_instance(), None);
    assert_eq!(manager.state(*instance), None);
    assert_eq!(world.query::<&SceneMember>().iter(world).count(), 0);
}

#[test]
fn an_unload_hook_can_cancel_the_incoming_scene_before_it_spawns() {
    #[derive(Resource)]
    struct Cancel(SceneInstanceId);

    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    world
        .register_component_hooks::<Sector>()
        .on_remove(|mut world, _| {
            if let Some(instance) = world.get_resource::<Cancel>().map(|cancel| cancel.0) {
                world.resource_mut::<SceneManager>().despawn_scene(instance);
            }
        });
    let first = FIRST.parse().unwrap();
    let second = SECOND.parse().unwrap();
    let original = switch_scene_sync(world, first).unwrap();
    let incoming = world.resource_mut::<SceneManager>().switch_scene(second);
    world.insert_resource(Cancel(incoming));
    let mut cursor = world
        .resource::<Messages<SceneEvent>>()
        .get_cursor_current();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update_with_delta(Duration::ZERO).unwrap();
        let world = app.world();
        let events = cursor
            .read(world.resource::<Messages<SceneEvent>>())
            .cloned()
            .collect::<Vec<_>>();
        if events.contains(&SceneEvent::Unloaded {
            instance: original,
            scene_id: first,
        }) {
            assert_eq!(
                events,
                [
                    SceneEvent::Unloaded {
                        instance: original,
                        scene_id: first
                    },
                    SceneEvent::Unloaded {
                        instance: incoming,
                        scene_id: second
                    },
                ]
            );
            let manager = world.resource::<SceneManager>();
            assert_eq!(manager.main_instance(), None);
            assert_eq!(manager.state(incoming), None);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "incoming scene never reached activation"
        );
        std::thread::yield_now();
    }
}

#[test]
fn dependency_removed_during_preload_keeps_the_main_instance() {
    for with_state in [false, true] {
        let mut app = app(pack(true));
        app.add_systems(
            CoreStage::TaskUpdate,
            (move |mut images: ResMut<Assets<Vec<u8>>>,
                   mut states: ResMut<AssetStates>,
                   mut events: ResMut<AssetEvents>| {
                let handle = images.iter().next().map(|(handle, _)| handle);
                if let Some(handle) = handle {
                    let removed = if with_state {
                        images
                            .remove_with_state(handle, &mut states, &mut events)
                            .unwrap()
                    } else {
                        images.remove(handle)
                    };
                    assert!(removed.is_some());
                }
            })
            .after(AssetLoadSet::Publish)
            .before(SceneUpdateSet),
        )
        .unwrap();
        let world = app.world_mut().unwrap();
        let main = switch_scene_sync(world, FIRST.parse().unwrap()).unwrap();
        let second = SECOND.parse().unwrap();
        let next = world.resource_mut::<SceneManager>().switch_scene(second);
        let error = SceneError::DependencyNotResident {
            scene_id: second,
            id: IMAGE.parse().unwrap(),
        };
        assert_eq!(
            settle(&mut app, next),
            SceneLoadState::Failed(error.to_string())
        );
        let world = app.world();
        let manager = world.resource::<SceneManager>();
        assert_eq!(manager.main_instance(), Some(main));
        assert!(
            manager
                .entities(main)
                .unwrap()
                .values()
                .all(|&entity| world.get_entity(entity).is_ok())
        );
    }
}

#[test]
fn switch_commits_the_same_scene_revision_whose_dependencies_it_requested() {
    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    let first = FIRST.parse().unwrap();
    let second = SECOND.parse().unwrap();
    switch_scene_sync(world, first).unwrap();
    let handle = load_asset_sync::<SceneAsset>(world, second).unwrap();
    let pending = world.resource_mut::<SceneManager>().switch_scene(second);
    app.update_with_delta(Duration::ZERO).unwrap();
    assert!(matches!(
        app.world().resource::<SceneManager>().state(pending),
        Some(SceneLoadState::LoadingDependencies { total: 1, .. })
    ));

    let replacement = cook_scene(
        &document(3, Some(MISSING.parse().unwrap())),
        &schemas(),
        |_| Err::<SceneDocument, _>("unexpected prefab"),
    )
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<Assets<SceneAsset>>()
        .insert(handle, SceneAsset::from_cooked(replacement.bytes).unwrap());
    assert_eq!(settle(&mut app, pending), SceneLoadState::Ready);
    let world = app.world();
    let manager = world.resource::<SceneManager>();
    let entity = *manager.entities(pending).unwrap().values().next().unwrap();
    assert_eq!(world.get::<Sector>(entity), Some(&Sector { number: 2 }));
    assert_eq!(
        world.get::<SpriteSource>(entity).unwrap().image,
        Some(IMAGE.parse().unwrap())
    );
}

#[test]
fn unloading_a_scene_keeps_independent_instances_parented_under_it() {
    use bevy_ecs::entity_disabling::Disabled;

    let mut app = app(pack(true));
    let world = app.world_mut().unwrap();
    let first = FIRST.parse().unwrap();
    let main = switch_scene_sync(world, first).unwrap();
    let separate = world.resource_mut::<SceneManager>().load_scene(first);
    assert_eq!(settle(&mut app, separate), SceneLoadState::Ready);
    let world = app.world_mut().unwrap();
    let manager = world.resource::<SceneManager>();
    let parent = *manager.entities(main).unwrap().values().next().unwrap();
    let other_root = *manager.entities(separate).unwrap().values().next().unwrap();
    let decoration = world.spawn(ChildOf(parent)).id();
    let disabled = world.spawn((SceneMember(main), Disabled)).id();
    world.entity_mut(other_root).insert(ChildOf(decoration));
    world.resource_mut::<SceneManager>().despawn_scene(main);
    app.update_with_delta(Duration::ZERO).unwrap();

    let world = app.world();
    assert!(world.get_entity(parent).is_err());
    assert!(world.get_entity(decoration).is_err());
    assert!(world.get_entity(disabled).is_err());
    assert_eq!(
        world.get::<SceneMember>(other_root),
        Some(&SceneMember(separate))
    );
    assert!(world.get::<ChildOf>(other_root).is_none());
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
