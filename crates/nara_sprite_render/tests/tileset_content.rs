use std::{collections::VecDeque, time::Duration};

use nara_app::{App, CoreStage};
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetPath, AssetPlugin, AssetRecord,
    AssetRef, AssetServer, AssetSourceKind, AssetStates, Assets, Handle, ImportArtifactKey,
    ImportArtifactRecord, ImportDependencyDigest, ImportProfile, ImportedAssetType, ImporterId,
    ImporterVersion, ProjectAssetDatabase, SourceHash, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit, Vec2};
use nara_ecs::{Entity, Resource, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_image::{
    ImageAsset, ImageExtent, ImageFormat, ImageImportSettings, ImagePreparePlugin,
    ImageSourceMetadata,
};
use nara_render::{
    Camera2d, RenderComponentsPlugin, RenderImage2d, RenderPlugin, RenderTarget, ViewportRect,
};
use nara_sprite::Sprite;
use nara_sprite_render::{
    __private::{QueuedSpriteItems, SpriteBatches, TextureUvRect},
    SpriteRenderPlugin,
};
use nara_tasks::{TaskKindConfig, TaskPlugin, TaskPoolConfig, TaskShutdownPolicy};
use nara_tilemap::{
    TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, TileSetAssetLimits,
    TileSetAssetPlugin, TileSetAssets, TileSetSource,
};
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
            let image = image(&image_record, width, byte);
            world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
                world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
                    images
                        .commit_loaded(
                            image_handle,
                            image,
                            &mut states,
                            &mut world.resource_mut::<AssetEvents>(),
                            None,
                            None,
                        )
                        .unwrap();
                });
            });
            if update_atlas {
                request(world, adaptable, width);
            }
        }
        Action::Remove => {
            world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
                world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
                    images
                        .remove_with_state(
                            image_handle,
                            &mut states,
                            &mut world.resource_mut::<AssetEvents>(),
                        )
                        .unwrap();
                });
            });
        }
    }
}

fn record(id: &str, path: &str, kind: AssetSourceKind) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(id).unwrap(),
        AssetPath::new(path).unwrap(),
        kind,
    )
}

fn image(record: &AssetRecord, width: u32, byte: u8) -> ImageAsset {
    let pixels = vec![byte; width as usize * 4];
    let settings = ImageImportSettings::default();
    let key = ImportArtifactKey::new(
        record.stable_id(),
        SourceHash::from_bytes(&pixels),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.image").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    );
    ImageAsset::new(
        ImageSourceMetadata::new(
            record.path().clone(),
            settings,
            ImportArtifactRecord::new(key).unwrap(),
        )
        .unwrap(),
        ImageExtent::new(width, 1),
        ImageFormat::Rgba8,
        pixels,
    )
    .unwrap()
}

fn request(world: &mut World, record: AssetRecord, width: u32) -> Handle<TileSet> {
    world.resource_scope(|world, mut owner: nara_ecs::Mut<TileSetAssets>| {
        world.resource_scope(|world, mut server: nara_ecs::Mut<AssetServer>| {
            world.resource_scope(|world, tilesets: nara_ecs::Mut<Assets<TileSet>>| {
                owner
                    .request(
                        record,
                        TileSetSource::new(
                            AssetRef::path("atlas.png").unwrap(),
                            TileAtlasLayout::grid(Vec2::ONE, width, 1),
                        ),
                        SourceHash::from_bytes(width.to_le_bytes()),
                        &mut server,
                        &tilesets,
                        &mut world.resource_mut::<AssetStates>(),
                    )
                    .unwrap()
            })
        })
    })
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

#[test]
fn sprite_and_tilesets_publish_exact_generations_and_revoke_without_releasing_held_packets() {
    let one = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::ONE);
    let mut app = App::new();
    app.add_plugins((
        TaskPlugin::new(TaskPoolConfig::new(one, one, one, TaskShutdownPolicy::default()).unwrap()),
        AssetPlugin,
        nara_reflect::ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        RenderComponentsPlugin,
        RenderPlugin,
        ImagePreparePlugin,
    ))
    .unwrap();
    app.add_plugins((TileSetAssetPlugin, SpriteRenderPlugin))
        .unwrap();
    app.insert_resource(TileSetAssets::with_limits(TileSetAssetLimits::new(
        ItemLimit::new(2).unwrap(),
        ItemLimit::new(4).unwrap(),
        ByteLimit::new(128).unwrap(),
    )))
    .unwrap();
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
    let camera = world
        .spawn((
            Camera2d {
                target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Camera2d::default()
            },
            Transform2d::default(),
        ))
        .id();
    let sprite = world
        .spawn((
            Sprite::from_texture(image_handle, Vec2::ONE),
            Transform2d::default(),
        ))
        .id();
    let mut fixed_map = nara_tilemap::Tilemap::new(Vec2::ONE).with_tileset(fixed);
    fixed_map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    let fixed_entity = world.spawn((fixed_map, Transform2d::default())).id();
    let mut adaptive_map = nara_tilemap::Tilemap::new(Vec2::ONE).with_tileset(adaptive);
    adaptive_map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    let adaptive_entity = world.spawn((adaptive_map, Transform2d::default())).id();
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
    app.add_systems(CoreStage::First, apply_edit).unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
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
