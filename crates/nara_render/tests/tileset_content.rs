#![cfg(feature = "sprite-render")]
use std::{collections::VecDeque, time::Duration};

use nara_app::{App, CoreStage, RuntimeInstance};
use nara_asset::{
    AssetEvents, AssetPlugin, AssetRecord, AssetRef, AssetReloadDiagnostics, AssetServer,
    AssetSourceChanges, AssetSourceKind, AssetSourcePath, AssetSourcePlugin, AssetStates,
    AssetTaskUpdateSet, Assets, Handle, ImportDependencyDigest, ImportRecipe, ImporterId,
    ImporterVersion, LoadState, ProjectAssetDatabase, SourceHash, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit, Vec2};
use nara_ecs::{
    Entity, Resource, World, bevy_ecs::system::SystemState, schedule::IntoScheduleConfigs,
};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_render::image::{
    ImageAsset, ImageExtent, ImageFormat, ImageImportSettings, ImagePreparePlugin,
    ImageSourceMetadata,
};
use nara_render::sprite::Sprite;
use nara_render::sprite_render::{
    SpriteRenderPlugin,
    batching::{QueuedSpriteItems, SpriteBatchKind, SpriteBatches, TextureUvRect},
};
use nara_render::tilemap::{
    TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, TileSetAssetError,
    TileSetAssetLimits, TileSetAssetPlugin, TileSetAssets, TileSetContent, TileSetContentReceipt,
    TileSetSource, Tilemap,
};
use nara_render::{
    Camera2d, RenderComponentsPlugin, RenderImage2d, RenderPlugin, RenderTarget, ViewportRect,
};
use nara_tasks::{TaskKindConfig, TaskPlugin, TaskPoolConfig, TaskShutdownPolicy};
use nara_transform::{Transform2d, TransformPlugin};

#[derive(Resource)]
struct Edits {
    image: AssetRecord,
    image_handle: Handle<ImageAsset>,
    adaptable: AssetRecord,
    actions: VecDeque<Action>,
}

enum Action {
    Initial,
    Resize {
        width: u32,
        byte: u8,
        update_atlas: bool,
    },
    Remove,
    RemoveTileSetSource,
    RestoreTileSetCatalog,
    ModifyTileSetSource,
    RejectTileSetSource,
    RequestPendingTileSetDependency,
    PublishPendingTileSetDependency,
}

fn apply_edit(world: &mut World) {
    let (action, image_record, image_handle, adaptable) = {
        let mut edits = world.resource_mut::<Edits>();
        (
            edits.actions.pop_front(),
            edits.image.clone(),
            edits.image_handle,
            edits.adaptable.clone(),
        )
    };
    let Some(action) = action else {
        return;
    };
    match action {
        Action::Initial => {}
        Action::Resize {
            width,
            byte,
            update_atlas,
        } => {
            publish_image(world, image_handle, image(&image_record, width, byte));
            if update_atlas {
                request(world, adaptable, width);
            }
        }
        Action::Remove => remove_image(world, image_handle),
        Action::RemoveTileSetSource => {
            assert!(
                world
                    .resource_mut::<ProjectAssetDatabase>()
                    .remove_exact(&adaptable)
            );
            world
                .resource_mut::<AssetSourceChanges>()
                .removed(adaptable.path().clone())
                .unwrap();
        }
        Action::RestoreTileSetCatalog => world
            .resource_mut::<ProjectAssetDatabase>()
            .insert(adaptable)
            .unwrap(),
        Action::ModifyTileSetSource => world
            .resource_mut::<AssetSourceChanges>()
            .modified(adaptable.path().clone())
            .unwrap(),
        Action::RejectTileSetSource => SystemState::<TileSetContent>::new(world)
            .get_mut(world)
            .unwrap()
            .reject_source(&adaptable)
            .unwrap(),
        Action::RequestPendingTileSetDependency => {
            request_source(world, adaptable, "pending.png", 2);
        }
        Action::PublishPendingTileSetDependency => {
            let pending = pending_image_record();
            let handle = admit_image(world, &pending);
            publish_image(world, handle, image(&pending, 2, 70));
        }
    }
}

fn pending_image_record() -> AssetRecord {
    record(
        "30000000-0000-4000-8000-000000000004",
        "pending.png",
        AssetSourceKind::Image,
    )
}

fn admit_image(world: &mut World, record: &AssetRecord) -> Handle<ImageAsset> {
    let mut database = world.resource_mut::<ProjectAssetDatabase>();
    if database.record_for_path(record.path()).is_none() {
        database.insert(record.clone()).unwrap();
    }
    world
        .resource_mut::<AssetServer>()
        .reserve_record(record)
        .unwrap()
}

fn publish_image(world: &mut World, handle: Handle<ImageAsset>, image: ImageAsset) {
    world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
            images
                .commit_loaded(
                    handle,
                    image,
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                    None,
                    None,
                )
                .unwrap();
        });
    });
}

fn remove_image(world: &mut World, handle: Handle<ImageAsset>) {
    world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
            images
                .remove_with_state(
                    handle,
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                )
                .unwrap();
        });
    });
}

fn record(id: &str, path: &str, kind: AssetSourceKind) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(id).unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn image(record: &AssetRecord, width: u32, byte: u8) -> ImageAsset {
    let pixels = vec![byte; width as usize * 4];
    let settings = ImageImportSettings::default();
    let import_recipe = ImportRecipe::new(
        record.stable_id(),
        SourceHash::from_bytes(&pixels),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.image").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
    );
    ImageAsset::from_import(
        ImageSourceMetadata::new(record.path().clone(), settings, import_recipe).unwrap(),
        ImageExtent::new(width, 1),
        ImageFormat::Rgba8,
        pixels,
    )
    .unwrap()
}

fn request(world: &mut World, record: AssetRecord, width: u32) -> Handle<TileSet> {
    request_source(world, record, "atlas.png", width)
}

fn request_source(
    world: &mut World,
    record: AssetRecord,
    path: &str,
    width: u32,
) -> Handle<TileSet> {
    let source = TileSetSource::new(
        AssetRef::path(path).unwrap(),
        TileAtlasLayout::grid(Vec2::ONE, width, 1),
    );
    let hash = SourceHash::from_bytes(source.encode_ron_bytes().unwrap());
    SystemState::<TileSetContent>::new(world)
        .get_mut(world)
        .unwrap()
        .request(record, source, hash, &nara_asset::AssetPathScope::ROOT)
        .unwrap()
}

fn observations(world: &World) -> Vec<(Entity, u32, u8, TextureUvRect)> {
    world
        .resource::<QueuedSpriteItems>()
        .as_slice()
        .iter()
        .filter_map(|item| {
            let image = item.image.as_ref()?;
            Some((
                item.entity,
                image.resource().extent().width,
                image.resource().pixels()[0],
                item.instance.uv,
            ))
        })
        .collect()
}

fn content_app(limits: TileSetAssetLimits) -> App {
    let one = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::ONE);
    let mut app = App::new();
    app.add_plugins((
        TaskPlugin::new(TaskPoolConfig::new(one, one, one, TaskShutdownPolicy::default()).unwrap()),
        AssetPlugin,
        nara_reflect::ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        RenderComponentsPlugin,
        RenderPlugin::default(),
        ImagePreparePlugin,
    ))
    .unwrap();
    app.init_resource::<ProjectAssetDatabase>().unwrap();
    app.add_plugins((TileSetAssetPlugin, SpriteRenderPlugin))
        .unwrap();
    app.insert_resource(TileSetAssets::with_limits(limits))
        .unwrap();
    app.add_systems(CoreStage::First, apply_edit).unwrap();
    app
}

fn spawn_camera(world: &mut World) -> Entity {
    world
        .spawn((
            Camera2d {
                target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Camera2d::default()
            },
            Transform2d::default(),
        ))
        .id()
}

fn spawn_tilemap(world: &mut World, tileset: Handle<TileSet>) -> Entity {
    let mut map = Tilemap::new(Vec2::ONE).with_tileset(tileset);
    map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    world.spawn((map, Transform2d::default())).id()
}

#[test]
fn sprite_and_tilesets_publish_exact_generations_and_revoke_without_releasing_held_packets() {
    let mut app = content_app(TileSetAssetLimits::new(
        ItemLimit::new(2).unwrap(),
        ItemLimit::new(4).unwrap(),
        ByteLimit::new(128).unwrap(),
    ));
    let image_record = record(
        "30000000-0000-4000-8000-000000000001",
        "atlas.png",
        AssetSourceKind::Image,
    );
    let fixed_record = record(
        "30000000-0000-4000-8000-000000000002",
        "fixed.tileset.ron",
        AssetSourceKind::Other(String::from("tileset")),
    );
    let adaptable = record(
        "30000000-0000-4000-8000-000000000003",
        "adaptable.tileset.ron",
        AssetSourceKind::Other(String::from("tileset")),
    );
    let world = app.world_mut().unwrap();
    for record in [&image_record, &fixed_record, &adaptable] {
        world
            .resource_mut::<ProjectAssetDatabase>()
            .insert(record.clone())
            .unwrap();
    }
    let image_handle = world
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&image_record)
        .unwrap();
    world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
            images
                .commit_loaded(
                    image_handle,
                    image(&image_record, 2, 10),
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                    None,
                    None,
                )
                .unwrap();
        });
    });
    let fixed = request(world, fixed_record, 2);
    let adaptive = request(world, adaptable.clone(), 4);
    let camera = spawn_camera(world);
    let sprite = world
        .spawn((
            Sprite::from_texture(image_handle, Vec2::ONE),
            Transform2d::default(),
        ))
        .id();
    let fixed_entity = spawn_tilemap(world, fixed);
    let adaptive_entity = spawn_tilemap(world, adaptive);
    HierarchyWriter::new(world)
        .apply_batch(
            &[camera, sprite, fixed_entity, adaptive_entity].map(|entity| {
                HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                }
            }),
        )
        .unwrap();
    world.insert_resource(Edits {
        image: image_record,
        image_handle,
        adaptable,
        actions: VecDeque::from([
            Action::Initial,
            Action::Resize {
                width: 4,
                byte: 20,
                update_atlas: true,
            },
            Action::Resize {
                width: 4,
                byte: 25,
                update_atlas: false,
            },
            Action::Resize {
                width: 8,
                byte: 30,
                update_atlas: false,
            },
            Action::Remove,
        ]),
    });
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let first = observations(runtime.world());
    runtime.drive(Duration::ZERO).unwrap();
    let mixed = observations(runtime.world());
    runtime.drive(Duration::ZERO).unwrap();
    let same_extent = observations(runtime.world());
    runtime.drive(Duration::ZERO).unwrap();
    let incompatible = observations(runtime.world());
    let retained_packet = runtime.world().resource::<SpriteBatches>().clone();
    for _ in 0..3 {
        runtime.drive(Duration::ZERO).unwrap();
    }
    let revoked = observations(runtime.world());
    let held_retention = runtime.world().resource::<TileSetAssets>().retention();
    let held_exact_widths = retained_packet
        .as_slice()
        .iter()
        .filter_map(|batch| batch.image.as_ref())
        .map(|image| image.resource().extent().width)
        .collect::<Vec<_>>();
    drop(retained_packet);
    let released_retention = runtime.world().resource::<TileSetAssets>().retention();
    runtime.finish().unwrap();
    assert_eq!(first.len(), 2);
    assert!(
        first
            .iter()
            .all(|(_, width, byte, _)| *width == 2 && *byte == 10)
    );
    assert_eq!(mixed.len(), 3);
    assert!(mixed.contains(&(sprite, 4, 20, TextureUvRect::FULL)));
    assert!(mixed.contains(&(
        fixed_entity,
        2,
        10,
        TextureUvRect::new(Vec2::new(0.5, 0.0), Vec2::new(0.5, 1.0))
    )));
    assert!(mixed.contains(&(
        adaptive_entity,
        4,
        20,
        TextureUvRect::new(Vec2::new(0.25, 0.0), Vec2::new(0.25, 1.0))
    )));
    assert_eq!(same_extent.len(), 3);
    assert!(same_extent.contains(&(sprite, 4, 25, TextureUvRect::FULL)));
    assert!(same_extent.contains(&(
        fixed_entity,
        2,
        10,
        TextureUvRect::new(Vec2::new(0.5, 0.0), Vec2::new(0.5, 1.0))
    )));
    assert!(same_extent.contains(&(
        adaptive_entity,
        4,
        25,
        TextureUvRect::new(Vec2::new(0.25, 0.0), Vec2::new(0.25, 1.0))
    )));
    assert_eq!(incompatible.len(), 3);
    assert!(incompatible.contains(&(sprite, 8, 30, TextureUvRect::FULL)));
    assert!(incompatible.contains(&(
        fixed_entity,
        2,
        10,
        TextureUvRect::new(Vec2::new(0.5, 0.0), Vec2::new(0.5, 1.0))
    )));
    assert!(incompatible.contains(&(
        adaptive_entity,
        4,
        25,
        TextureUvRect::new(Vec2::new(0.25, 0.0), Vec2::new(0.25, 1.0))
    )));
    assert!(revoked.is_empty());
    assert_eq!(held_exact_widths, vec![8, 2, 4]);
    assert_eq!((held_retention.images, held_retention.bytes), (2, 24));
    assert_eq!(
        (released_retention.images, released_retention.bytes),
        (0, 0)
    );
}

fn atlas_reload_app() -> (App, Handle<TileSet>) {
    let mut app = content_app(TileSetAssetLimits::new(
        ItemLimit::ONE,
        ItemLimit::new(4).unwrap(),
        ByteLimit::new(16).unwrap(),
    ));
    let image_record = record(
        "30000000-0000-4000-8000-000000000001",
        "atlas.png",
        AssetSourceKind::Image,
    );
    let tileset_record = record(
        "30000000-0000-4000-8000-000000000002",
        "fixed.tileset.ron",
        AssetSourceKind::Other(String::from("tileset")),
    );
    let world = app.world_mut().unwrap();
    for record in [&image_record, &tileset_record] {
        world
            .resource_mut::<ProjectAssetDatabase>()
            .insert(record.clone())
            .unwrap();
    }
    let image_handle = world
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&image_record)
        .unwrap();
    let tileset = request(world, tileset_record.clone(), 2);
    let camera = spawn_camera(world);
    let sprite = world
        .spawn((
            Sprite::from_texture(image_handle, Vec2::ONE),
            Transform2d::default(),
        ))
        .id();
    let tilemap = spawn_tilemap(world, tileset);
    HierarchyWriter::new(world)
        .apply_batch(
            &[camera, sprite, tilemap].map(|entity| HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }),
        )
        .unwrap();
    world.insert_resource(Edits {
        image: image_record,
        image_handle,
        adaptable: tileset_record,
        actions: VecDeque::from([10, 20, 25].map(|byte| Action::Resize {
            width: 2,
            byte,
            update_atlas: false,
        })),
    });
    (app, tileset)
}

fn atlas_reload_runtime() -> (RuntimeInstance, Handle<TileSet>) {
    let (app, tileset) = atlas_reload_app();
    (RuntimeInstance::start(app).unwrap(), tileset)
}

fn assert_atlas_packet(batches: &SpriteBatches, sprite: u8, tilemap: u8) {
    assert_eq!(batches.len(), 2);
    for (kind, byte) in [
        (SpriteBatchKind::Sprite, sprite),
        (SpriteBatchKind::Tilemap, tilemap),
    ] {
        let batch = batches
            .as_slice()
            .iter()
            .find(|batch| batch.kind == kind)
            .unwrap();
        let image = batch.image.as_ref().unwrap();
        assert_eq!(image.resource().pixels(), &[byte; 8], "{kind:?}");
    }
}

#[test]
fn successive_atlas_generations_publish_in_their_first_frame_with_a_two_generation_budget() {
    let (mut runtime, tileset) = atlas_reload_runtime();
    for byte in [10, 20, 25] {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        assert_atlas_packet(world.resource::<SpriteBatches>(), byte, byte);
        let owner = world.resource::<TileSetAssets>();
        assert_eq!(owner.error(tileset), None);
        assert_eq!(owner.retention().bytes, 8);
    }
    runtime.finish().unwrap();
}

#[test]
fn dropping_a_retained_packet_retries_the_pending_atlas_when_capacity_is_available() {
    let (mut runtime, tileset) = atlas_reload_runtime();
    runtime.drive(Duration::ZERO).unwrap();
    let retained_packet = runtime.world().resource::<SpriteBatches>().clone();
    runtime.drive(Duration::ZERO).unwrap();
    assert_atlas_packet(runtime.world().resource::<SpriteBatches>(), 20, 20);
    runtime.drive(Duration::ZERO).unwrap();

    let world = runtime.world();
    assert_atlas_packet(world.resource::<SpriteBatches>(), 25, 20);
    let owner = world.resource::<TileSetAssets>();
    assert_eq!(
        owner.error(tileset),
        Some(&TileSetAssetError::RetentionCapacity)
    );
    assert_eq!(owner.retention().bytes, 16);
    let failed_state = world.resource::<AssetStates>().state_revision(tileset.id());
    let failed_events = world.resource::<AssetEvents>().len();
    for _ in 0..3 {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        assert_atlas_packet(world.resource::<SpriteBatches>(), 25, 20);
        assert_eq!(
            world.resource::<AssetStates>().state_revision(tileset.id()),
            failed_state
        );
        assert_eq!(world.resource::<AssetEvents>().len(), failed_events);
        assert_eq!(world.resource::<TileSetAssets>().retention().bytes, 16);
    }

    assert_atlas_packet(&retained_packet, 10, 10);
    drop(retained_packet);
    assert_eq!(
        runtime
            .world()
            .resource::<TileSetAssets>()
            .retention()
            .bytes,
        8
    );
    runtime.drive(Duration::ZERO).unwrap();
    let world = runtime.world();
    assert_atlas_packet(world.resource::<SpriteBatches>(), 25, 25);
    let owner = world.resource::<TileSetAssets>();
    assert_eq!(owner.error(tileset), None);
    assert_eq!(owner.retention().bytes, 8);
    assert_eq!(world.resource::<AssetEvents>().len(), failed_events + 1);
    runtime.finish().unwrap();
}

#[test]
fn source_removal_revokes_new_tilemap_batches_while_a_shared_sprite_keeps_drawing() {
    let (mut app, tileset) = atlas_reload_app();
    app.add_plugins(AssetSourcePlugin).unwrap();
    let world = app.world_mut().unwrap();
    let mut edits = world.resource_mut::<Edits>();
    edits.actions.truncate(1);
    edits
        .actions
        .extend([Action::RemoveTileSetSource, Action::RestoreTileSetCatalog]);

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let retained_packet = runtime.world().resource::<SpriteBatches>().clone();
    let mut saw_missing_record = false;
    let observations: Vec<_> = (0..4)
        .map(|_| {
            runtime.drive(Duration::ZERO).unwrap();
            let world = runtime.world();
            let states = world.resource::<AssetStates>();
            let state = states.state(tileset.id()).cloned();
            let has_slot = states.published_slot_revision(tileset.id()).is_some();
            let retains_value = world.resource::<Assets<TileSet>>().get(tileset).is_some();
            let batches = world.resource::<SpriteBatches>().clone();
            saw_missing_record |= world
                .resource::<AssetReloadDiagnostics>()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "asset.reload-record-missing");
            (state, has_slot, retains_value, batches)
        })
        .collect();
    let world = runtime.world();
    let held = world.resource::<TileSetAssets>().retention();

    assert_atlas_packet(&retained_packet, 10, 10);
    let retained_sprite = retained_packet
        .as_slice()
        .iter()
        .find(|batch| batch.kind == SpriteBatchKind::Sprite)
        .unwrap()
        .clone();
    drop(retained_packet);
    let released = world.resource::<TileSetAssets>().retention();
    runtime.finish().unwrap();

    for (state, has_slot, retains_value, batches) in observations {
        let state = state.unwrap();
        assert_eq!(state.load_state(), &LoadState::Removed);
        assert!(state.usable_version().is_none());
        assert!(!has_slot);
        assert!(!retains_value);
        assert_eq!(batches.len(), 1, "revoked TileSet emitted a new batch");
        assert_eq!(batches.as_slice(), std::slice::from_ref(&retained_sprite));
    }
    assert!(saw_missing_record);
    assert_eq!((held.images, held.bytes), (1, 8));
    assert_eq!((released.images, released.bytes), (0, 0));
}

#[test]
fn failed_source_notification_and_dependency_retry_keep_the_last_good_tilemap() {
    let (mut app, tileset) = atlas_reload_app();
    app.add_plugins(AssetSourcePlugin).unwrap();
    let mut edits = app.world_mut().unwrap().resource_mut::<Edits>();
    edits.actions.truncate(1);
    edits.actions.extend([
        Action::ModifyTileSetSource,
        Action::RequestPendingTileSetDependency,
        Action::Initial,
        Action::PublishPendingTileSetDependency,
        Action::ModifyTileSetSource,
        Action::RejectTileSetSource,
        Action::RejectTileSetSource,
    ]);

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames: Vec<_> = (0..8)
        .map(|_| {
            runtime.drive(Duration::ZERO).unwrap();
            let world = runtime.world();
            let states = world.resource::<AssetStates>();
            (
                states.state(tileset.id()).cloned().unwrap(),
                states.published_slot_revision(tileset.id()).cloned(),
                world.resource::<SpriteBatches>().clone(),
                states.state_revision(tileset.id()),
            )
        })
        .collect();
    runtime.finish().unwrap();

    let (initial, initial_slot, initial_batches, _) = &frames[0];
    assert_eq!(initial.load_state(), &LoadState::Loaded);
    assert!(initial_slot.is_some());
    assert_atlas_packet(initial_batches, 10, 10);
    assert!(
        matches!(frames[1].0.load_state(), LoadState::Failed { message }
        if message == "asset.reload-source-consumer-missing")
    );
    for (state, slot, batches, _) in &frames[1..4] {
        assert!(matches!(state.load_state(), LoadState::Failed { .. }));
        assert_eq!(state.usable_version(), initial.usable_version());
        assert_eq!(slot, initial_slot);
        assert_atlas_packet(batches, 10, 10);
    }
    let (recovered, recovered_slot, recovered_batches, _) = &frames[4];
    assert_eq!(recovered.load_state(), &LoadState::Loaded);
    assert!(recovered.usable_version().unwrap() > initial.usable_version().unwrap());
    assert!(recovered_slot.is_some());
    assert_ne!(recovered_slot, initial_slot);
    assert_atlas_packet(recovered_batches, 10, 70);
    assert_atlas_packet(initial_batches, 10, 10);
    for (state, slot, batches, _) in &frames[6..] {
        assert!(matches!(state.load_state(), LoadState::Failed { message }
            if message == &TileSetAssetError::InvalidSource.to_string()));
        assert_eq!(state.usable_version(), recovered.usable_version());
        assert_eq!(slot, recovered_slot);
        assert_atlas_packet(batches, 10, 70);
    }
    assert_ne!(frames[5].3, frames[6].3);
    assert_eq!(frames[6].3, frames[7].3);
}

#[test]
fn fresh_source_request_before_revocation_cleanup_keeps_waiting_for_its_image() {
    for revoke_tileset in [true, false] {
        assert_pending_request_survives_retirement(revoke_tileset);
    }
}

fn assert_pending_request_survives_retirement(revoke_tileset: bool) {
    let (mut app, tileset) = atlas_reload_app();
    app.add_plugins(AssetSourcePlugin).unwrap();
    let world = app.world_mut().unwrap();
    let pending = admit_image(world, &pending_image_record());
    world
        .resource_mut::<AssetStates>()
        .set_loading(pending.id());
    let mut edits = world.resource_mut::<Edits>();
    edits.actions.truncate(1);
    edits.actions.extend([
        if revoke_tileset {
            Action::RemoveTileSetSource
        } else {
            Action::ModifyTileSetSource
        },
        Action::Initial,
        Action::PublishPendingTileSetDependency,
    ]);
    let mut original: Option<TileSetContentReceipt> = None;
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |world: &mut World| match world.resource::<nara_app::RealTime>().frame {
            1 => {
                original = world.resource::<TileSetAssets>().content_receipt(
                    tileset,
                    world.resource::<Assets<TileSet>>(),
                    world.resource::<AssetStates>(),
                );
            }
            2 => {
                let claim = original.take().unwrap();
                world.resource_scope(|world, mut owner: nara_ecs::Mut<TileSetAssets>| {
                    let states = world.resource::<AssetStates>();
                    let slot = world.resource::<Assets<TileSet>>();
                    assert_eq!(
                        states
                            .state(tileset.id())
                            .unwrap()
                            .usable_version()
                            .is_none(),
                        revoke_tileset
                    );
                    assert!(slot.get(tileset).is_some());
                    drop(
                        owner
                            .prepare_publications(
                                Vec::new(),
                                &[claim],
                                world.resource::<AssetServer>(),
                                Some(world.resource::<ProjectAssetDatabase>()),
                                slot,
                                states,
                            )
                            .unwrap(),
                    );
                });
                let edits = world.resource::<Edits>();
                let source = edits.adaptable.clone();
                let original_image = edits.image_handle;
                if revoke_tileset {
                    world
                        .resource_mut::<ProjectAssetDatabase>()
                        .insert(source.clone())
                        .unwrap();
                }
                assert_eq!(request_source(world, source, "pending.png", 2), tileset);
                if !revoke_tileset {
                    remove_image(world, original_image);
                }
            }
            _ => {}
        })
        .after(AssetTaskUpdateSet::ResolveSourceChanges)
        .before(AssetTaskUpdateSet::ApplyResults),
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let observations: Vec<_> = (0..4)
        .map(|_| {
            runtime.drive(Duration::ZERO).unwrap();
            let world = runtime.world();
            let state = world
                .resource::<AssetStates>()
                .state(tileset.id())
                .unwrap()
                .clone();
            let owner_error = world.resource::<TileSetAssets>().error(tileset).cloned();
            (
                state,
                owner_error,
                world.resource::<SpriteBatches>().clone(),
            )
        })
        .collect();
    runtime.finish().unwrap();

    assert_atlas_packet(&observations[0].2, 10, 10);
    for (state, owner_error, batches) in &observations[1..3] {
        assert_eq!(state.load_state(), &LoadState::Loading);
        assert!(state.usable_version().is_none());
        assert!(owner_error.is_none());
        assert_eq!(batches.len(), usize::from(revoke_tileset));
        assert!(
            batches
                .as_slice()
                .iter()
                .all(|batch| batch.kind == SpriteBatchKind::Sprite)
        );
    }
    assert_eq!(observations[3].0.load_state(), &LoadState::Loaded);
    let recovered = &observations[3].2;
    if revoke_tileset {
        assert_atlas_packet(recovered, 10, 70);
    } else {
        assert_eq!(recovered.len(), 1);
        let batch = &recovered.as_slice()[0];
        assert_eq!(batch.kind, SpriteBatchKind::Tilemap);
        assert_eq!(batch.image.as_ref().unwrap().resource().pixels(), &[70; 8]);
    }
}
