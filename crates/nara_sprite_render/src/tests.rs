use super::*;
use std::time::Duration;

use nara_app::{
    App, CoreStage, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
    RuntimeClosePolicy, RuntimeInstance, RuntimeObligationLedger,
};
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetPath, AssetStates, AssetVersion,
    Assets, Handle, ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportedAssetType, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Resource, World, schedule::IntoScheduleConfigs};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_image::{
    ImageAsset, ImageExtent, ImageFormat, ImageImportSettings, ImageSourceMetadata,
    PreparedImageResource, PreparedImageSnapshot, image_descriptor_hash, image_resource_key,
};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{
    Camera2d, ExtractedView, RenderImage2d, RenderResourceSnapshot, RenderTarget, ViewportRect,
};
use nara_sprite::{Sprite, TextureRegion};
use nara_tilemap::{TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, Tilemap};
use nara_transform::{GlobalTransform2d, Transform2d, TransformPlugin};

fn test_view() -> ExtractedView {
    ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport: ViewportRect::new(0, 0, 200, 100).unwrap(),
        mapping: nara_render::ViewMapping2d::orthographic(
            nara_core::Mat3::IDENTITY,
            100.0,
            ViewportRect::new(0, 0, 200, 100).unwrap(),
        )
        .unwrap(),
        order: 0,
        load: nara_render::ViewLoadOp::Clear(Color::BLACK),
    }
}

fn place_parented(world: &mut World, child: Entity, parent: Entity) {
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: parent,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            },
        ])
        .expect("the ordered hierarchy should be valid");
}

fn place_roots(world: &mut World, entities: &[Entity]) {
    let operations = entities
        .iter()
        .copied()
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        })
        .collect::<Vec<_>>();
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .expect("the ordered hierarchy should be valid");
}

fn instance() -> SpriteInstance {
    SpriteInstance::axis_aligned(Vec2::ZERO, Vec2::splat(0.5), Color::WHITE)
}

fn sprite_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin,
        nara_image::ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app
}

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn retire_runtime(runtime: RuntimeInstance) {
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
}

#[test]
fn extraction_clears_stale_sprites_and_uses_explicit_identity_transform() {
    let mut app = sprite_app();
    app.world_mut()
        .expect("app should allow world mutation")
        .resource_mut::<ExtractedSprites>()
        .push(ExtractedSprite {
            entity: Entity::PLACEHOLDER,
            source_order: 99,
            kind: ExtractedSpriteKind::Sprite,
            material: ExtractedSpriteMaterial::from_color(Color::WHITE),
            material_extension: None,
            texture_region: TextureUvRect::FULL,
            world_center: Vec2::new(9.0, 9.0),
            world_x_axis: Vec2::X,
            world_y_axis: Vec2::Y,
            color: Color::WHITE,
            layer: 0,
            sort_key: 0,
            world_sort_y: 0.0,
        });
    let sprite = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_color(Vec2::new(16.0, 8.0), Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    place_roots(app.world_mut().unwrap(), &[sprite]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let extracted = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted.as_slice()[0].world_center, Vec2::ZERO);
    assert_eq!(extracted.as_slice()[0].world_x_axis, Vec2::new(8.0, 0.0));
    assert_eq!(extracted.as_slice()[0].world_y_axis, Vec2::new(0.0, 4.0));
    runtime.finish().unwrap();
}

#[test]
fn sprite_extraction_preserves_rotation_axes() {
    let mut app = sprite_app();
    let sprite = app
        .world_mut()
        .unwrap()
        .spawn((
            Sprite::from_color(Vec2::new(4.0, 2.0), Color::WHITE),
            Transform2d {
                rotation: std::f32::consts::FRAC_PI_2,
                ..Transform2d::default()
            },
        ))
        .id();
    place_roots(app.world_mut().unwrap(), &[sprite]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let extracted = runtime.world().resource::<ExtractedSprites>().as_slice()[0];

    assert!(
        extracted
            .world_x_axis
            .abs_diff_eq(Vec2::new(0.0, 2.0), 0.000_001)
    );
    assert!(
        extracted
            .world_y_axis
            .abs_diff_eq(Vec2::new(-1.0, 0.0), 0.000_001)
    );
    runtime.finish().unwrap();
}

#[test]
fn extraction_uses_authored_structure_when_entity_allocation_is_reversed() {
    fn extracted_colors(reverse_allocation: bool) -> Vec<Color> {
        let mut app = sprite_app();
        let red_color = Color::rgb(1.0, 0.0, 0.0);
        let blue_color = Color::rgb(0.0, 0.0, 1.0);
        let (red, blue) = if reverse_allocation {
            let blue = app
                .world_mut()
                .unwrap()
                .spawn((
                    Sprite::from_color(Vec2::ONE, blue_color),
                    Transform2d::default(),
                ))
                .id();
            let red = app
                .world_mut()
                .unwrap()
                .spawn((
                    Sprite::from_color(Vec2::ONE, red_color),
                    Transform2d::default(),
                ))
                .id();
            (red, blue)
        } else {
            let red = app
                .world_mut()
                .unwrap()
                .spawn((
                    Sprite::from_color(Vec2::ONE, red_color),
                    Transform2d::default(),
                ))
                .id();
            let blue = app
                .world_mut()
                .unwrap()
                .spawn((
                    Sprite::from_color(Vec2::ONE, blue_color),
                    Transform2d::default(),
                ))
                .id();
            (red, blue)
        };
        place_roots(app.world_mut().unwrap(), &[red, blue]);

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let extracted = runtime.world().resource::<ExtractedSprites>();
        assert_eq!(
            extracted
                .as_slice()
                .iter()
                .map(|sprite| sprite.source_order)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let colors = extracted
            .as_slice()
            .iter()
            .map(|sprite| sprite.color)
            .collect();
        runtime.finish().unwrap();
        colors
    }

    let expected = vec![Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)];
    assert_eq!(extracted_colors(false), expected);
    assert_eq!(
        extracted_colors(true),
        vec![Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0),]
    );
}

#[test]
fn equal_depth_sprite_and_tile_follow_authored_hierarchy_order() {
    let mut app = sprite_app();
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    let foreground = app
        .world_mut()
        .unwrap()
        .spawn((tilemap, Transform2d::default()))
        .id();
    place_roots(app.world_mut().unwrap(), &[foreground, actor]);
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let order = runtime
        .world()
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .map(|sprite| (sprite.entity, sprite.source_order))
        .collect::<Vec<_>>();
    runtime.finish().unwrap();
    assert_eq!(order, vec![(foreground, 0), (actor, 1)]);
}

#[test]
fn y_sort_anchors_use_world_affines_for_sprites_and_tile_cells() {
    let mut app = sprite_app();
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE).with_y_sort_anchor(Vec2::new(2.0, 0.0)),
            Transform2d {
                translation: Vec2::new(0.0, 10.0),
                rotation: std::f32::consts::FRAC_PI_2,
                ..Transform2d::default()
            },
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::new(4.0, 4.0)).with_y_sort_anchor(Vec2::new(0.0, -2.0));
    tilemap.set_cell(TileCoord::new(0, 1), TileCell::new(TileIndex::new(0)));
    let tilemap = app
        .world_mut()
        .unwrap()
        .spawn((tilemap, Transform2d::from_translation(Vec2::new(0.0, 3.0))))
        .id();
    place_roots(app.world_mut().unwrap(), &[actor, tilemap]);
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let anchors = runtime
        .world()
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .map(|sprite| sprite.world_sort_y)
        .collect::<Vec<_>>();
    runtime.finish().unwrap();
    assert!((anchors[0] - 12.0).abs() < 0.000_001);
    assert!((anchors[1] - 7.0).abs() < 0.000_001);
}

#[test]
fn tilemap_cells_lower_to_world_positions() {
    let mut app = sprite_app();
    let mut tilemap = Tilemap::new(Vec2::new(10.0, 20.0)).with_layer(2);
    tilemap.set_cell(
        TileCoord::new(1, -1),
        TileCell::new(TileIndex::new(3)).with_color(Color::rgb(0.5, 0.25, 1.0)),
    );
    let tilemap = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            tilemap,
            Transform2d {
                translation: Vec2::new(5.0, 5.0),
                ..Transform2d::default()
            },
        ))
        .id();
    place_roots(app.world_mut().unwrap(), &[tilemap]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let extracted = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(
        extracted.as_slice()[0].kind,
        ExtractedSpriteKind::TilemapCell {
            coord: TileCoord::new(1, -1),
            tile: TileIndex::new(3),
        }
    );
    assert_eq!(extracted.as_slice()[0].world_center, Vec2::new(20.0, -5.0));
    assert_eq!(extracted.as_slice()[0].world_x_axis, Vec2::new(5.0, 0.0));
    assert_eq!(extracted.as_slice()[0].world_y_axis, Vec2::new(0.0, 10.0));
    assert_eq!(extracted.as_slice()[0].layer, 2);
    runtime.finish().unwrap();
}

#[test]
fn parented_sprite_uses_the_completed_global_affine() {
    let mut app = sprite_app();
    let parent_transform = Transform2d {
        translation: Vec2::new(20.0, 30.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(2.0, 3.0),
    };
    let child_transform = Transform2d::from_translation(Vec2::new(4.0, -2.0));
    let (parent, child) = {
        let world = app.world_mut().unwrap();
        let parent = world.spawn(parent_transform).id();
        let child = world
            .spawn((
                Sprite::from_color(Vec2::new(4.0, 2.0), Color::WHITE),
                child_transform,
            ))
            .id();
        place_parented(world, child, parent);
        (parent, child)
    };

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let expected = parent_transform.matrix() * child_transform.matrix();
    let extracted = runtime.world().resource::<ExtractedSprites>().as_slice()[0];
    assert_eq!(extracted.entity, child);
    assert!(
        extracted
            .world_center
            .abs_diff_eq(expected.transform_point2(Vec2::ZERO), 0.000_001)
    );
    assert!(
        extracted
            .world_x_axis
            .abs_diff_eq(expected.transform_vector2(Vec2::new(2.0, 0.0)), 0.000_001)
    );
    assert!(
        extracted
            .world_y_axis
            .abs_diff_eq(expected.transform_vector2(Vec2::new(0.0, 1.0)), 0.000_001)
    );
    assert!(runtime.world().get::<GlobalTransform2d>(parent).is_some());
    runtime.finish().unwrap();
}

#[test]
fn moving_a_sprite_parent_updates_extraction_without_mutating_child_local_state() {
    let mut app = sprite_app();
    let child_local = Transform2d::from_translation(Vec2::new(4.0, -2.0));
    let (parent, child) = {
        let world = app.world_mut().unwrap();
        let parent = world.spawn(Transform2d::default()).id();
        let child = world
            .spawn((
                Sprite::from_color(Vec2::new(4.0, 2.0), Color::WHITE),
                child_local,
            ))
            .id();
        place_parented(world, child, parent);
        (parent, child)
    };

    app.add_systems(
        CoreStage::Update,
        move |time: nara_ecs::Res<nara_app::RealTime>,
              mut transforms: nara_ecs::Query<&mut Transform2d>| {
            if time.frame == 2 {
                transforms.get_mut(parent).unwrap().translation = Vec2::new(20.0, 30.0);
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let first_center = runtime.world().resource::<ExtractedSprites>().as_slice()[0].world_center;
    runtime.drive(Duration::ZERO).unwrap();

    let world = runtime.world();
    assert_eq!(*world.get::<Transform2d>(child).unwrap(), child_local);
    assert_eq!(first_center, Vec2::new(4.0, -2.0));
    assert_eq!(
        world.resource::<ExtractedSprites>().as_slice()[0].world_center,
        Vec2::new(24.0, 28.0)
    );
    runtime.finish().unwrap();
}

#[test]
fn parented_tilemap_uses_the_completed_global_affine() {
    let mut app = sprite_app();
    let parent_transform = Transform2d {
        translation: Vec2::new(-10.0, 15.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(2.0, 2.0),
    };
    let child_transform = Transform2d::from_translation(Vec2::new(3.0, 4.0));
    let child = {
        let world = app.world_mut().unwrap();
        let parent = world.spawn(parent_transform).id();
        let mut tilemap = Tilemap::new(Vec2::new(10.0, 20.0));
        tilemap.set_cell(TileCoord::new(1, 0), TileCell::new(TileIndex::new(2)));
        let child = world.spawn((tilemap, child_transform)).id();
        place_parented(world, child, parent);
        child
    };

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let expected = parent_transform.matrix() * child_transform.matrix();
    let extracted = runtime.world().resource::<ExtractedSprites>().as_slice()[0];
    assert_eq!(extracted.entity, child);
    assert!(
        extracted
            .world_center
            .abs_diff_eq(expected.transform_point2(Vec2::new(15.0, 10.0)), 0.000_001)
    );
    assert!(
        extracted
            .world_x_axis
            .abs_diff_eq(expected.transform_vector2(Vec2::new(5.0, 0.0)), 0.000_001)
    );
    assert!(
        extracted
            .world_y_axis
            .abs_diff_eq(expected.transform_vector2(Vec2::new(0.0, 10.0)), 0.000_001)
    );
    runtime.finish().unwrap();
}

#[test]
fn missing_sprite_local_transform_preserves_the_prior_extraction() {
    let mut app = sprite_app();
    let prior = placeholder_extracted_sprite();
    app.world_mut()
        .unwrap()
        .resource_mut::<ExtractedSprites>()
        .push(prior);
    let sprite = app
        .world_mut()
        .unwrap()
        .spawn(Sprite::from_color(Vec2::ONE, Color::WHITE))
        .id();
    place_roots(app.world_mut().unwrap(), &[sprite]);

    let mut runtime = start_runtime(app);
    runtime
        .drive(Duration::ZERO)
        .expect_err("a sprite without explicit local spatial authority must fault");

    assert_eq!(
        runtime.world().resource::<ExtractedSprites>().as_slice(),
        &[prior]
    );
    retire_runtime(runtime);
}

#[test]
fn missing_tilemap_global_transform_preserves_the_prior_extraction() {
    let mut app = sprite_app();
    let prior = placeholder_extracted_sprite();
    app.world_mut()
        .unwrap()
        .resource_mut::<ExtractedSprites>()
        .push(prior);
    let tilemap = app
        .world_mut()
        .unwrap()
        .spawn((Tilemap::default(), Transform2d::default()))
        .id();
    place_roots(app.world_mut().unwrap(), &[tilemap]);
    app.insert_resource(RemoveCompletedGlobal(tilemap)).unwrap();
    app.add_systems(
        CoreStage::Extract,
        remove_completed_global
            .after(nara_transform::__private::TransformSet::Propagate)
            .before(nara_render::__private::RenderExtractSet::Views),
    )
    .unwrap();

    let mut runtime = start_runtime(app);
    runtime
        .drive(Duration::ZERO)
        .expect_err("a tilemap without completed global state must fault");

    assert_eq!(
        runtime.world().resource::<ExtractedSprites>().as_slice(),
        &[prior]
    );
    retire_runtime(runtime);
}

#[test]
fn queueing_records_missing_prepared_texture_snapshot() {
    let mut app = sprite_app();
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Camera2d {
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                ..Camera2d::default()
            },
            Transform2d::default(),
        ));
    let sprite = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(
                nara_asset::allocate_anonymous_handle().unwrap(),
                Vec2::new(10.0, 10.0),
            ),
            Transform2d::default(),
        ))
        .id();
    place_roots(app.world_mut().unwrap(), &[sprite]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let queued = runtime.world().resource::<QueuedSpriteItems>();
    let stats = runtime.world().resource::<SpriteRenderStats>();
    assert!(queued.is_empty());
    assert_eq!(stats.unprepared_textures, 1);
    runtime.finish().unwrap();
}

#[test]
fn queueing_uses_prepared_image_resource_keys_and_uvs() {
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let mut app = sprite_app();
    insert_loaded_image(&mut app, image);
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Camera2d {
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                ..Camera2d::default()
            },
            Transform2d::default(),
        ));
    let sprite = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_texture_region(TextureRegion::new(Vec2::new(0.25, 0.0), Vec2::splat(0.5))),
            Transform2d::default(),
        ))
        .id();
    place_roots(app.world_mut().unwrap(), &[sprite]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let queued = runtime.world().resource::<QueuedSpriteItems>();
    let batches = runtime.world().resource::<SpriteBatches>();
    let stats = runtime.world().resource::<SpriteRenderStats>();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued.as_slice()[0]
            .image
            .as_ref()
            .map(|snapshot| snapshot.identity().key()),
        Some(image_resource_key(image))
    );
    assert_eq!(
        queued.as_slice()[0].instance.uv,
        TextureUvRect::new(Vec2::new(0.25, 0.0), Vec2::splat(0.5))
    );
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches.as_slice()[0]
            .image
            .as_ref()
            .map(|snapshot| snapshot.identity().key()),
        Some(image_resource_key(image))
    );
    assert_eq!(stats.unprepared_textures, 0);
    runtime.finish().unwrap();
}

#[test]
fn custom_material_captures_second_texture_and_preserves_stock_order() {
    use nara_material::{Material2dDefinition, Material2dDefinitions, Material2dInput};
    static DEFINITIONS: [Material2dDefinition; 1] = [Material2dDefinition {
        id: "test.palette",
        fragment_shader: include_str!("../../../examples/support/palette_material.wgsl"),
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Opaque,
    }];
    let texture = nara_asset::allocate_anonymous_handle().unwrap();
    let definitions = Material2dDefinitions::new(&DEFINITIONS).unwrap();
    let material = definitions
        .instantiate("test.palette", [1.0, 0.5, 0.0, 0.0], texture)
        .unwrap();
    let mut app = sprite_app();
    insert_loaded_image(&mut app, texture);
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((
            Camera2d {
                target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    let custom = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            material,
            Transform2d::default(),
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    let tile = world
        .spawn((tilemap, material, Transform2d::default()))
        .id();
    place_roots(world, &[camera, stock, custom, tile]);
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let queued = runtime
        .world()
        .resource::<QueuedSpriteItems>()
        .as_slice()
        .to_vec();
    let batches = runtime.world().resource::<SpriteBatches>().clone();
    runtime.finish().unwrap();
    assert_eq!(
        queued.iter().map(|item| item.entity).collect::<Vec<_>>(),
        vec![stock, custom, tile]
    );
    assert!(queued[0].material_extension.is_none());
    for item in &queued[1..] {
        let extension = item.material_extension.as_ref().unwrap();
        assert_eq!(extension.instance, material);
        assert_eq!(
            extension.texture.identity().key(),
            image_resource_key(texture)
        );
        assert_eq!(extension.texture.resource().pixels(), &[255; 16]);
        assert_eq!(item.material.alpha_mode, AlphaMode2d::Opaque);
    }
    assert_eq!(batches.total_instances(), 3);
}

#[test]
fn missing_custom_material_texture_skips_only_that_item() {
    use nara_material::{Material2dDefinition, Material2dDefinitions, Material2dInput};
    static DEFINITIONS: [Material2dDefinition; 1] = [Material2dDefinition {
        id: "test.palette",
        fragment_shader: "fn material_fragment() {}",
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Blend,
    }];
    let definitions = Material2dDefinitions::new(&DEFINITIONS).unwrap();
    let material = definitions
        .instantiate(
            "test.palette",
            [0.0; 4],
            nara_asset::allocate_anonymous_handle().unwrap(),
        )
        .unwrap();
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((
            Camera2d {
                target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    let custom = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            material,
            Transform2d::default(),
        ))
        .id();
    place_roots(world, &[camera, stock, custom]);
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let queued = runtime
        .world()
        .resource::<QueuedSpriteItems>()
        .as_slice()
        .to_vec();
    let unprepared = runtime
        .world()
        .resource::<SpriteRenderStats>()
        .unprepared_textures;
    runtime.finish().unwrap();
    assert_eq!(
        queued.iter().map(|item| item.entity).collect::<Vec<_>>(),
        vec![stock]
    );
    assert_eq!(unprepared, 1);
}

#[test]
fn foreground_tile_follows_actor_by_layer() {
    let actor = queued_item(0);
    let foreground_tile = QueuedSpriteItem {
        kind: SpriteBatchKind::Tilemap,
        layer: 1,
        ..queued_item(1)
    };
    let mut items = vec![foreground_tile.clone(), actor.clone()];

    items.sort_by(compare_queued_sprite_items);

    assert_eq!(
        items.iter().map(|item| item.entity).collect::<Vec<_>>(),
        vec![actor.entity, foreground_tile.entity],
    );
}

#[test]
fn y_sort_is_total_and_stays_within_layer_and_sort_group() {
    let mut items = vec![
        QueuedSpriteItem {
            world_sort_y: -10.0,
            kind: SpriteBatchKind::Tilemap,
            ..queued_item(0)
        },
        QueuedSpriteItem {
            world_sort_y: 0.0,
            ..queued_item(1)
        },
        QueuedSpriteItem {
            world_sort_y: 10.0,
            ..queued_item(2)
        },
        QueuedSpriteItem {
            world_sort_y: -0.0,
            kind: SpriteBatchKind::Tilemap,
            ..queued_item(3)
        },
        QueuedSpriteItem {
            sort_key: 1,
            world_sort_y: 100.0,
            ..queued_item(4)
        },
        QueuedSpriteItem {
            layer: 1,
            sort_key: -1,
            world_sort_y: 1_000.0,
            ..queued_item(5)
        },
    ];

    items.sort_by(compare_queued_sprite_items);

    assert_eq!(
        items
            .iter()
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        vec![2, 1, 3, 0, 4, 5],
    );
    items.reverse();
    items.sort_by(compare_queued_sprite_items);
    assert_eq!(
        items
            .iter()
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        vec![2, 1, 3, 0, 4, 5],
    );
}

#[test]
fn sorting_preserves_source_order_for_equal_keys_and_batches_adjacent_items() {
    let mut items = vec![
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(1).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 0,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 0,
            instance: instance(),
            image: None,
            material_extension: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(2).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 0,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 1,
            instance: instance(),
            image: None,
            material_extension: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(3).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 1,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 2,
            instance: instance(),
            image: None,
            material_extension: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(4).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 1,
            sort_key: 1,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 3,
            instance: instance(),
            image: None,
            material_extension: None,
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(
        items
            .iter()
            .take(2)
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        items.iter().map(|item| item.entity).collect::<Vec<_>>(),
        (1..=4)
            .map(|index| Entity::from_raw_u32(index).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        items
            .iter()
            .map(|item| (item.layer, item.sort_key))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 0), (1, 0), (1, 1)]
    );
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].instances.len(), 2);
    assert_eq!(batches[1].instances.len(), 1);
    assert_eq!(batches[2].instances.len(), 1);
}

#[test]
fn transparent_material_changes_preserve_painter_order_and_split_adjacent_batches() {
    let texture_a = prepared_image_snapshot();
    let texture_b = prepared_image_snapshot();
    let mut items = vec![
        QueuedSpriteItem {
            material: material_key(),
            image: Some(texture_a.clone()),
            ..queued_item(0)
        },
        QueuedSpriteItem {
            material: material_key(),
            image: Some(texture_b.clone()),
            kind: SpriteBatchKind::Tilemap,
            ..queued_item(1)
        },
        QueuedSpriteItem {
            material: material_key(),
            image: Some(texture_a.clone()),
            ..queued_item(2)
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(
        items
            .iter()
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        batches
            .iter()
            .map(|batch| {
                batch
                    .image
                    .as_ref()
                    .map(|snapshot| snapshot.identity().key())
            })
            .collect::<Vec<_>>(),
        vec![
            Some(texture_a.identity().key()),
            Some(texture_b.identity().key()),
            Some(texture_a.identity().key()),
        ]
    );
    assert!(batches.iter().all(|batch| batch.instances.len() == 1));
}

#[test]
fn same_image_with_different_samplers_splits_batches() {
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let mut app = sprite_app();
    insert_loaded_image(&mut app, image);
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Camera2d {
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                ..Camera2d::default()
            },
            Transform2d::default(),
        ));
    let default_sampler = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0)),
            Transform2d::default(),
        ))
        .id();
    let nearest_sampler = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
            Transform2d::default(),
        ))
        .id();
    place_roots(
        app.world_mut().unwrap(),
        &[default_sampler, nearest_sampler],
    );

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let batches = runtime.world().resource::<SpriteBatches>();
    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches.as_slice()[0]
            .image
            .as_ref()
            .map(|snapshot| snapshot.identity().key()),
        Some(image_resource_key(image))
    );
    assert_eq!(
        batches.as_slice()[1]
            .image
            .as_ref()
            .map(|snapshot| snapshot.identity().key()),
        Some(image_resource_key(image))
    );
    assert_ne!(
        batches.as_slice()[0].material.sampler,
        batches.as_slice()[1].material.sampler
    );
    runtime.finish().unwrap();
}

#[test]
fn target_or_domain_kind_changes_split_batches() {
    let mut items = vec![
        QueuedSpriteItem {
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            ..queued_item(0)
        },
        QueuedSpriteItem {
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Tilemap,
            ..queued_item(1)
        },
        QueuedSpriteItem {
            target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
            kind: SpriteBatchKind::Sprite,
            ..queued_item(2)
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].kind, SpriteBatchKind::Sprite);
    assert_eq!(batches[1].kind, SpriteBatchKind::Tilemap);
    assert!(
        batches
            .iter()
            .any(|batch| batch.kind == SpriteBatchKind::Tilemap)
    );
    assert!(
        batches
            .iter()
            .any(|batch| batch.kind == SpriteBatchKind::Sprite)
    );
    assert!(
        batches
            .iter()
            .any(|batch| matches!(batch.target, RenderTarget::Image(_)))
    );
}

#[test]
fn generated_instances_fit_camera_viewport_aspect_and_position() {
    let view = ExtractedView {
        mapping: nara_render::ViewMapping2d::orthographic(
            nara_core::Mat3::from_translation(Vec2::new(10.0, 5.0)),
            100.0,
            test_view().viewport,
        )
        .unwrap(),
        ..test_view()
    };
    let instance = world_to_clip_instance(
        &view,
        Vec2::new(60.0, 30.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, 5.0),
        Color::WHITE,
    )
    .unwrap();

    assert!(instance.center.abs_diff_eq(Vec2::new(0.5, 0.5), 0.000_001));
    assert!(
        instance
            .half_size()
            .abs_diff_eq(Vec2::new(0.1, 0.1), 0.000_001)
    );
}

#[test]
fn app_pipeline_extracts_tilemaps_into_batches() {
    let target = RenderTarget::Image(RenderImage2d::allocate_handle().unwrap());
    let mut app = sprite_app();
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Camera2d {
                target,
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Camera2d::default()
            },
            Transform2d::default(),
        ));
    let mut tilemap = Tilemap::new(Vec2::new(10.0, 10.0));
    tilemap.set_cell(
        TileCoord::new(0, 0),
        TileCell::new(TileIndex::new(1)).with_color(Color::rgb(1.0, 0.0, 0.0)),
    );
    let tilemap = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()))
        .id();
    place_roots(app.world_mut().unwrap(), &[tilemap]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let batches = runtime.world().resource::<SpriteBatches>();
    assert_eq!(batches.total_instances(), 1);
    assert_eq!(batches.as_slice()[0].kind, SpriteBatchKind::Tilemap);
    assert_eq!(batches.as_slice()[0].target, target);
    assert!(
        batches.as_slice()[0].instances[0]
            .center
            .abs_diff_eq(Vec2::new(0.1, 0.1), 0.000_001)
    );
    runtime.finish().unwrap();
}

#[test]
fn tilemap_extraction_applies_tileset_image_and_atlas_uvs() {
    let tileset = nara_asset::allocate_anonymous_handle().unwrap();
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let mut tilesets = Assets::<TileSet>::default();
    tilesets.insert(
        tileset,
        TileSet::from_image(image, TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2)),
    );

    let mut app = sprite_app();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(tilesets);
    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_tileset(tileset);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(5)));
    let tilemap = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()))
        .id();
    place_roots(app.world_mut().unwrap(), &[tilemap]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let extracted = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted.as_slice()[0].material.image, Some(image));
    assert_eq!(
        extracted.as_slice()[0].texture_region,
        TextureUvRect::new(Vec2::new(0.25, 0.5), Vec2::new(0.25, 0.5))
    );
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .invalid_tile_regions,
        0
    );
    runtime.finish().unwrap();
}

#[test]
fn tilemap_extraction_skips_out_of_range_atlas_tiles() {
    let tileset = nara_asset::allocate_anonymous_handle().unwrap();
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let mut tilesets = Assets::<TileSet>::default();
    tilesets.insert(
        tileset,
        TileSet::from_image(image, TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2)),
    );

    let mut app = sprite_app();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(tilesets);
    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_tileset(tileset);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(8)));
    let tilemap = app
        .world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()))
        .id();
    place_roots(app.world_mut().unwrap(), &[tilemap]);

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.world().resource::<ExtractedSprites>().len(), 0);
    let stats = runtime.world().resource::<SpriteRenderStats>();
    assert_eq!(stats.invalid_tile_regions, 1);
    assert_eq!(stats.extracted_tile_cells, 0);
    runtime.finish().unwrap();
}

#[test]
fn dirty_tile_chunks_can_clear_without_losing_authored_cells() {
    let mut tilemap = Tilemap::default();
    let coord = TileCoord::new(-1, 2);
    let cell = TileCell::new(TileIndex::new(5));

    tilemap.set_cell(coord, cell);
    tilemap.clear_dirty_chunks();

    assert_eq!(tilemap.get_cell(coord), Some(&cell));
    assert_eq!(tilemap.dirty_chunks().count(), 0);
}

#[derive(Resource)]
struct RemoveCompletedGlobal(Entity);

fn remove_completed_global(world: &mut World) {
    let entity = world.resource::<RemoveCompletedGlobal>().0;
    world.entity_mut(entity).remove::<GlobalTransform2d>();
}

fn placeholder_extracted_sprite() -> ExtractedSprite {
    ExtractedSprite {
        entity: Entity::PLACEHOLDER,
        source_order: 99,
        kind: ExtractedSpriteKind::Sprite,
        material: ExtractedSpriteMaterial::from_color(Color::WHITE),
        material_extension: None,
        texture_region: TextureUvRect::FULL,
        world_center: Vec2::new(9.0, 9.0),
        world_x_axis: Vec2::X,
        world_y_axis: Vec2::Y,
        color: Color::WHITE,
        layer: 0,
        sort_key: 0,
        world_sort_y: 0.0,
    }
}

fn queued_item(entity_index: u32) -> QueuedSpriteItem {
    QueuedSpriteItem {
        entity: Entity::from_raw_u32(entity_index.saturating_add(1)).unwrap(),
        view_index: 0,
        view_order: 0,
        target: RenderTarget::PrimaryWindow,
        kind: SpriteBatchKind::Sprite,
        layer: 0,
        sort_key: 0,
        world_sort_y: 0.0,
        material: material_key(),
        source_order: entity_index as u64,
        instance: instance(),
        image: None,
        material_extension: None,
    }
}

fn insert_loaded_image(app: &mut App, handle: Handle<ImageAsset>) {
    let image = test_image();
    let source_hash = image.source().source_hash();
    let import_hash = image.source().artifact().key().digest();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(
            handle,
            image,
            &mut states,
            &mut events,
            Some(source_hash),
            Some(import_hash),
        )
        .unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(images);
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(states);
}

fn test_image() -> ImageAsset {
    let stable_id = StableAssetId::parse_str("ce2ab2f8-c58b-48e3-b94e-9465340262a1").unwrap();
    let source_hash = SourceHash::from_bytes(b"test image");
    let key = ImportArtifactKey::new(
        stable_id,
        source_hash,
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-image").unwrap(),
        ImporterVersion::new(1),
        ImageImportSettings::default().settings_hash(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    );
    let artifact = ImportArtifactRecord::new(key).unwrap();
    let source = ImageSourceMetadata::new(
        AssetPath::new("textures/test.png").unwrap(),
        ImageImportSettings::default(),
        artifact,
    )
    .unwrap();
    ImageAsset::new(
        source,
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        vec![255; 16],
    )
    .unwrap()
}

fn prepared_image_snapshot() -> PreparedImageSnapshot {
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let mut images = Assets::<ImageAsset>::default();
    assert!(images.insert(handle, test_image()).is_none());
    let image = images.get(handle).unwrap();
    PreparedImageSnapshot::new(
        RenderResourceSnapshot::new(
            image_resource_key(handle),
            AssetVersion::from_raw(1),
            images.slot_revision(handle),
            image_descriptor_hash(image),
        ),
        PreparedImageResource::from_image(image),
    )
}

fn material_key() -> SpriteMaterialKey {
    SpriteMaterialKey {
        sampler: SamplerDescriptor::default(),
        alpha_mode: AlphaMode2d::Blend,
        tint: ColorKey::from_color(Color::WHITE),
    }
}
