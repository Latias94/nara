use super::*;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use crate::image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot, PreparedImages,
    image_resource_key,
};
use crate::material::{
    AlphaMode2d, Material2dDeclaration, Material2dLibrary, MaterialParameters,
    MaterialTextureDeclaration, SamplerDescriptor,
};
use crate::sprite::{Sprite, TextureRegion};
use crate::tilemap::{TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, Tilemap};
use crate::{Camera2d, ExtractedView, RenderImage2d, RenderTarget, ViewportRect};
use nara_app::{App, CoreStage, RuntimeCandidateRetirementState, RuntimeInstance};
use nara_asset::{AssetEvents, AssetPath, AssetStates, Assets, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, World, schedule::IntoScheduleConfigs};
use nara_scene::spatial::{ChildOf, GlobalTransform2d, Transform2d, Visibility};

mod material;
mod visibility;

#[derive(MaterialParameters)]
struct PaletteParameters {
    #[material(id = "palette.offset")]
    x: f32,
    #[material(id = "palette.strength")]
    y: f32,
    #[material(id = "palette.reserved-z")]
    z: f32,
    #[material(id = "palette.reserved-w")]
    w: f32,
}

fn palette_materials(id: &str, alpha_mode: AlphaMode2d) -> Material2dLibrary {
    let mut materials = Material2dLibrary::default();
    materials
        .declare(
            Material2dDeclaration::from_parameters::<PaletteParameters>(
                id,
                1,
                alpha_mode,
                vec![MaterialTextureDeclaration::new("palette", "material_texture", 1).unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let path = AssetPath::new("palette.wgsl").unwrap();
    let read_shader = |_: &AssetPath| Ok(Arc::<str>::from(include_str!("tests/palette.wgsl")));
    let prepared = materials.prepare_update(id, &[path], read_shader).unwrap();
    materials.publish(&prepared, read_shader).unwrap();
    materials
}

fn test_view() -> ExtractedView {
    ExtractedView {
        id: crate::RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport: ViewportRect::new(0, 0, 200, 100).unwrap(),
        mapping: crate::ViewMapping2d::orthographic(
            nara_core::Mat3::IDENTITY,
            100.0,
            ViewportRect::new(0, 0, 200, 100).unwrap(),
        )
        .unwrap(),
        order: 0,
        load: crate::ViewLoadOp::Clear(Color::BLACK),
    }
}

fn place_parented(world: &mut World, child: Entity, parent: Entity) {
    world.entity_mut(child).insert(ChildOf(parent));
}

fn instance() -> SpriteInstance {
    SpriteInstance::axis_aligned(Vec2::ZERO, Vec2::splat(0.5), Color::WHITE)
}

fn sprite_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        nara_scene::spatial::SpatialPlugin,
        crate::RenderPlugin::default(),
        crate::image::ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app
}

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeInstance::start(app).unwrap()
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
            world_z: 0.0,
            sort_key: 0,
            world_sort_y: 0.0,
        });
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_color(Vec2::new(16.0, 8.0), Color::WHITE),
            Transform2d::default(),
        ));

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
    app.world_mut().unwrap().spawn((
        Sprite::from_color(Vec2::new(4.0, 2.0), Color::WHITE),
        Transform2d {
            rotation: std::f32::consts::FRAC_PI_2,
            ..Transform2d::default()
        },
    ));

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let extracted = &runtime.world().resource::<ExtractedSprites>().as_slice()[0];

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
fn extraction_uses_sibling_order_when_entity_allocation_is_reversed() {
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
        app.world_mut()
            .unwrap()
            .spawn_empty()
            .add_children(&[red, blue]);

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
fn world_depth_and_authored_layers_sort_together_before_sibling_order() {
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    world.spawn((
        Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            ..Camera2d::default()
        },
        Transform2d::IDENTITY,
    ));
    let actor = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE).with_layer(2),
            Transform2d {
                z: -0.25,
                ..Transform2d::IDENTITY
            },
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::ONE).with_layer(1);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    let foreground = world
        .spawn((
            tilemap,
            Transform2d {
                z: 0.75,
                ..Transform2d::IDENTITY
            },
        ))
        .id();
    world
        .spawn(Transform2d {
            z: 4.0,
            ..Transform2d::IDENTITY
        })
        .add_children(&[foreground, actor]);
    let background = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::BLACK).with_layer(6),
            Transform2d {
                z: -0.5,
                ..Transform2d::IDENTITY
            },
        ))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let order = runtime
        .world()
        .resource::<QueuedSpriteItems>()
        .as_slice()
        .iter()
        .map(|item| (item.entity, item.layer, item.world_z))
        .collect::<Vec<_>>();
    runtime.finish().unwrap();
    assert_eq!(
        order,
        [
            (background, 6, -0.5),
            (foreground, 1, 4.75),
            (actor, 2, 3.75)
        ]
    );
}

#[test]
fn y_sort_anchors_use_world_affines_for_sprites_and_tile_cells() {
    let mut app = sprite_app();
    app.world_mut().unwrap().spawn((
        Sprite::from_color(Vec2::ONE, Color::WHITE).with_y_sort_anchor(Vec2::new(2.0, 0.0)),
        Transform2d {
            translation: Vec2::new(0.0, 10.0),
            rotation: std::f32::consts::FRAC_PI_2,
            ..Transform2d::default()
        },
    ));
    let mut tilemap = Tilemap::new(Vec2::new(4.0, 4.0)).with_y_sort_anchor(Vec2::new(0.0, -2.0));
    tilemap.set_cell(TileCoord::new(0, 1), TileCell::new(TileIndex::new(0)));
    app.world_mut()
        .unwrap()
        .spawn((tilemap, Transform2d::from_xy(0.0, 3.0)));
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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            tilemap,
            Transform2d {
                translation: Vec2::new(5.0, 5.0),
                ..Transform2d::default()
            },
        ));

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
        ..Transform2d::IDENTITY
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
    let extracted = &runtime.world().resource::<ExtractedSprites>().as_slice()[0];
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
    assert_eq!(
        runtime
            .world()
            .get::<GlobalTransform2d>(parent)
            .unwrap()
            .affine(),
        parent_transform.affine()
    );
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
        ..Transform2d::IDENTITY
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
    let extracted = &runtime.world().resource::<ExtractedSprites>().as_slice()[0];
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
fn hidden_hierarchy_leaves_only_explicitly_visible_world_renderables() {
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    world.spawn((
        Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            ..Camera2d::default()
        },
        Transform2d::IDENTITY,
    ));
    let parent = world.spawn(Visibility::Inherited).id();
    let sprite = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::IDENTITY,
            ChildOf(parent),
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    world.spawn((tilemap, Transform2d::IDENTITY, ChildOf(parent)));
    app.add_systems(
        CoreStage::Update,
        move |time: nara_ecs::Res<nara_app::RealTime>, mut commands: nara_ecs::Commands| match time
            .frame
        {
            2 => {
                commands.entity(parent).insert(Visibility::Hidden);
            }
            3 => {
                commands.entity(sprite).insert(Visibility::Visible);
            }
            _ => {}
        },
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let first = runtime.world().resource::<ExtractedSprites>().clone();
    assert_eq!(first.len(), 2);
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        2
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedSprites>().is_empty());
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        0
    );
    runtime.drive(Duration::ZERO).unwrap();
    let restored = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(restored.as_slice(), &first.as_slice()[..1]);
    assert_eq!(restored.as_slice()[0].entity, sprite);
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        1
    );
    runtime.finish().unwrap();
}

#[test]
fn missing_global_transform_preserves_the_prior_extraction() {
    for tilemap in [false, true] {
        let mut app = sprite_app();
        let prior = placeholder_extracted_sprite();
        let world = app.world_mut().unwrap();
        world.resource_mut::<ExtractedSprites>().push(prior.clone());
        if tilemap {
            world.spawn(Tilemap::default());
        } else {
            world.spawn(Sprite::from_color(Vec2::ONE, Color::WHITE));
        }

        let mut runtime = start_runtime(app);
        runtime
            .drive(Duration::ZERO)
            .expect_err("a visible renderable needs a global transform");
        assert_eq!(
            runtime.world().resource::<ExtractedSprites>().as_slice(),
            &[prior]
        );
        retire_runtime(runtime);
    }
}

#[test]
fn deferred_tilemap_spawn_extracts_its_world_geometry_in_the_same_frame() {
    let mut app = sprite_app();
    app.add_systems(CoreStage::PostUpdate, |mut commands: nara_ecs::Commands| {
        let mut tilemap = Tilemap::new(Vec2::splat(4.0));
        tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
        let parent = commands.spawn(Transform2d::from_xy(5.0, 10.0)).id();
        commands.spawn((tilemap, Transform2d::from_xy(7.0, 9.0), ChildOf(parent)));
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let extracted = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted.as_slice()[0].world_center, Vec2::new(14.0, 21.0));
    runtime.finish().unwrap();
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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(
                nara_asset::allocate_anonymous_handle().unwrap(),
                Vec2::new(10.0, 10.0),
            ),
            Transform2d::default(),
        ));

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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_texture_region(TextureRegion::new(Vec2::new(0.25, 0.0), Vec2::splat(0.5))),
            Transform2d::default(),
        ));

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
    let prepared = runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(image))
        .unwrap();
    assert_eq!(queued.as_slice()[0].image.as_ref(), Some(prepared));
    assert_eq!(batches.as_slice()[0].image.as_ref(), Some(prepared));
    runtime.finish().unwrap();
}

#[test]
fn queued_image_snapshots_survive_successor_publication_and_cache_removal() {
    use nara_ecs::bevy_ecs::system::RunSystemOnce;

    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut world = World::new();
    let mut extracted = ExtractedSprites::default();
    let mut sprite = placeholder_extracted_sprite();
    sprite.material.image = Some(handle);
    extracted.push(sprite);
    let mut views = crate::ExtractedViews::default();
    views.push(test_view());
    world.insert_resource(extracted);
    world.insert_resource(views);
    world.init_resource::<PreparedImages>();
    world.init_resource::<QueuedSpriteItems>();
    world.init_resource::<SpriteBatches>();
    world.init_resource::<SpriteRenderStats>();

    let [old, new] = [vec![255; 16], [0, 255, 0, 255].repeat(4)].map(|pixels| {
        let image = test_image_with_pixels(pixels);
        let version = images
            .commit_loaded(handle, image, &mut states, &mut events)
            .unwrap();
        let snapshot = PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
        assert_eq!(snapshot.identity().asset_version(), version);
        world
            .resource_mut::<PreparedImages>()
            .insert(snapshot.clone());
        world
            .run_system_once(crate::sprite_render::queue::queue_sprites)
            .unwrap();
        world
            .run_system_once(crate::sprite_render::queue::sort_and_batch_sprites)
            .unwrap();
        assert_eq!(world.resource::<QueuedSpriteItems>().len(), 1);
        assert_eq!(
            world.resource::<QueuedSpriteItems>().as_slice()[0]
                .image
                .as_ref(),
            Some(&snapshot)
        );
        assert_eq!(world.resource::<SpriteRenderStats>().unprepared_textures, 0);
        let batches = world.resource::<SpriteBatches>().clone();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches.as_slice()[0].image.as_ref(), Some(&snapshot));
        batches
    });

    world
        .resource_mut::<PreparedImages>()
        .remove(image_resource_key(handle));
    drop(images);
    drop(states);
    world
        .run_system_once(crate::sprite_render::queue::queue_sprites)
        .unwrap();
    world
        .run_system_once(crate::sprite_render::queue::sort_and_batch_sprites)
        .unwrap();
    assert!(world.resource::<QueuedSpriteItems>().is_empty());
    assert!(world.resource::<SpriteBatches>().is_empty());
    assert_eq!(world.resource::<SpriteRenderStats>().unprepared_textures, 1);

    let old = old.as_slice()[0].image.as_ref().unwrap();
    let new = new.as_slice()[0].image.as_ref().unwrap();
    assert_eq!(old.identity().key(), new.identity().key());
    assert_ne!(
        old.identity().asset_version(),
        new.identity().asset_version()
    );
    assert_ne!(
        old.identity().slot_revision(),
        new.identity().slot_revision()
    );
    assert_eq!(old.resource().pixels(), &[255; 16]);
    assert_eq!(new.resource().pixels(), [0, 255, 0, 255].repeat(4));
}

#[test]
fn custom_material_captures_second_texture_and_preserves_stock_order() {
    let texture = nara_asset::allocate_anonymous_handle().unwrap();
    let materials = palette_materials("test.palette", AlphaMode2d::Opaque);
    let material = materials
        .current("test.palette")
        .unwrap()
        .bind(
            &PaletteParameters {
                x: 1.0,
                y: 0.5,
                z: 0.0,
                w: 0.0,
            },
            BTreeMap::from([("palette".to_owned(), texture)]),
        )
        .unwrap();
    let mut app = sprite_app();
    insert_loaded_image(&mut app, texture);
    let world = app.world_mut().unwrap();
    world.spawn((
        Camera2d {
            target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            viewport_height: 100.0,
            ..Default::default()
        },
        Transform2d::default(),
    ));
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    let custom = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            material.clone(),
            Transform2d::default(),
        ))
        .id();
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    let tile = world
        .spawn((tilemap, material.clone(), Transform2d::default()))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let queued = runtime
        .world()
        .resource::<QueuedSpriteItems>()
        .as_slice()
        .to_vec();
    let extracted = runtime.world().resource::<ExtractedSprites>().clone();
    let batches = runtime.world().resource::<SpriteBatches>().clone();
    runtime.finish().unwrap();
    assert_eq!(
        queued.iter().map(|item| item.entity).collect::<Vec<_>>(),
        vec![stock, custom, tile]
    );
    assert!(queued[0].material_extension.is_none());
    for item in &queued[1..] {
        let extension = item.material_extension.as_ref().unwrap();
        let original = extracted
            .as_slice()
            .iter()
            .find(|draw| draw.entity == item.entity)
            .unwrap()
            .material_extension
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(extension, original));
        assert_eq!(extension.binding, material);
        assert_eq!(extension.textures.len(), 1);
        assert_eq!(extension.textures[0].0, 1);
        assert_eq!(
            extension.textures[0].1.identity().key(),
            image_resource_key(texture)
        );
        assert_eq!(extension.textures[0].1.resource().pixels(), &[255; 16]);
        assert_eq!(item.material.alpha_mode, AlphaMode2d::Opaque);
    }
    assert_eq!(batches.total_instances(), 3);
}

#[test]
fn missing_custom_material_texture_skips_only_that_item() {
    let materials = palette_materials("test.palette", AlphaMode2d::Blend);
    let material = materials
        .current("test.palette")
        .unwrap()
        .bind(
            &PaletteParameters {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 0.0,
            },
            BTreeMap::from([(
                "palette".to_owned(),
                nara_asset::allocate_anonymous_handle().unwrap(),
            )]),
        )
        .unwrap();
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    world.spawn((
        Camera2d {
            target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            viewport_height: 100.0,
            ..Default::default()
        },
        Transform2d::default(),
    ));
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    world.spawn((
        Sprite::from_color(Vec2::ONE, Color::WHITE),
        material,
        Transform2d::default(),
    ));
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
    let extracted = runtime.world().resource::<ExtractedSprites>().as_slice();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].entity, stock);
    assert_eq!(extracted[0].source_order, 0);
    assert!(extracted[0].material_extension.is_none());
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .missing_materials,
        0
    );
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
            world_z: 0.0,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 0,
            instance: instance(),
            image: None,
            material_extension: None,
            tile_chunk: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(2).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 0,
            world_z: 0.0,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 1,
            instance: instance(),
            image: None,
            material_extension: None,
            tile_chunk: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(3).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 1,
            world_z: 0.0,
            sort_key: 0,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 2,
            instance: instance(),
            image: None,
            material_extension: None,
            tile_chunk: None,
        },
        QueuedSpriteItem {
            entity: Entity::from_raw_u32(4).unwrap(),
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 1,
            world_z: 0.0,
            sort_key: 1,
            world_sort_y: 0.0,
            material: material_key(),
            source_order: 3,
            instance: instance(),
            image: None,
            material_extension: None,
            tile_chunk: None,
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
fn adjacent_instance_tints_share_one_batch_and_preserve_colors_and_order() {
    let colors = [
        Color::rgba(1.0, 0.0, 0.0, 0.5),
        Color::rgba(0.0, 0.0, 1.0, 0.5),
    ];
    let mut items = colors
        .into_iter()
        .enumerate()
        .map(|(index, color)| QueuedSpriteItem {
            material: SpriteMaterialKey::from_material(ExtractedSpriteMaterial::from_color(color)),
            instance: SpriteInstance::axis_aligned(Vec2::new(index as f32, 0.0), Vec2::ONE, color),
            ..queued_item(index as u32)
        })
        .collect::<Vec<_>>();
    items.sort_by(compare_queued_sprite_items);

    let batches = build_sprite_batches(&items);

    assert_eq!(
        items
            .iter()
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0]
            .instances
            .iter()
            .map(|instance| instance.color)
            .collect::<Vec<_>>(),
        colors
    );
    assert_eq!(
        batches[0]
            .instances
            .iter()
            .map(|instance| instance.center.x)
            .collect::<Vec<_>>(),
        [0.0, 1.0]
    );
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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0)),
            Transform2d::default(),
        ));
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
            Transform2d::default(),
        ));

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
        mapping: crate::ViewMapping2d::orthographic(
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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()));

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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()));

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
fn tilemap_extraction_context_is_scoped_to_each_map_and_frame() {
    let indices = [0, 0, 2, 2, 1, 0, 3, 3, 2, 4, 4, 0];
    let images = [
        nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap(),
        nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap(),
    ];
    let handles = [
        nara_asset::allocate_anonymous_handle::<TileSet>().unwrap(),
        nara_asset::allocate_anonymous_handle::<TileSet>().unwrap(),
    ];
    let untextured = nara_asset::allocate_anonymous_handle::<TileSet>().unwrap();
    let layouts = [[(2, 1), (2, 2)], [(1, 4), (1, 1)]];
    let samplers = [
        [
            SamplerDescriptor::LINEAR_CLAMP,
            SamplerDescriptor::NEAREST_CLAMP,
        ],
        [
            SamplerDescriptor::NEAREST_CLAMP,
            SamplerDescriptor::LINEAR_CLAMP,
        ],
    ];
    let tints = [
        Color::rgba(0.5, 0.25, 1.0, 0.5),
        Color::rgba(0.25, 1.0, 0.5, 1.0),
    ];
    let tile_sizes = [
        Vec2::new(3.25, 5.5),
        Vec2::new(6.25, 3.5),
        Vec2::new(4.5, 7.25),
    ];
    let changed_tile_size = Vec2::new(7.5, 2.25);
    let transforms = [
        Transform2d {
            translation: Vec2::new(10.0, -6.0),
            rotation: 0.41,
            scale: Vec2::new(-1.5, 0.75),
            ..Transform2d::IDENTITY
        },
        Transform2d {
            translation: Vec2::new(-2.0, 3.0),
            rotation: -0.23,
            scale: Vec2::new(0.5, 2.5),
            ..Transform2d::IDENTITY
        },
        Transform2d::default(),
    ];
    let mut tilesets = Assets::<TileSet>::default();
    for index in 0..2 {
        let (columns, rows) = layouts[0][index];
        tilesets.insert(
            handles[index],
            TileSet::from_image(
                images[index],
                TileAtlasLayout::grid(Vec2::splat(2.0), columns, rows),
            )
            .with_sampler(samplers[0][index])
            .with_tint(tints[index])
            .with_alpha_mode(AlphaMode2d::Opaque),
        );
    }
    tilesets.insert(untextured, TileSet::new(Vec2::ONE));
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    world.insert_resource(tilesets);
    let entities = std::array::from_fn::<_, 3, _>(|map_index| {
        let mut map = Tilemap::new(tile_sizes[map_index])
            .with_layer(map_index as i32 + 3)
            .with_sort_key(map_index as i32 - 2)
            .with_y_sort_anchor(Vec2::new(1.0, -2.0));
        map.tileset = handles.get(map_index).copied();
        for index in (0..indices.len()).rev() {
            let color = if index % 2 == 0 {
                Color::rgba(0.5, 0.25, 1.0, 0.5)
            } else {
                Color::WHITE
            };
            map.set_cell(
                TileCoord::new(index as i32 - 6, (index % 3) as i32 - 1),
                TileCell::new(TileIndex::new(indices[index])).with_color(color),
            );
        }
        map.clear_dirty_chunks();
        world.spawn((map, transforms[map_index])).id()
    });
    world
        .spawn_empty()
        .add_children(&[entities[1], entities[0], entities[2]]);
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        if world.resource::<nara_app::RealTime>().frame != 2 {
            return;
        }
        {
            let mut tilesets = world.resource_mut::<Assets<TileSet>>();
            for index in 0..2 {
                let (columns, rows) = layouts[1][index];
                let tileset = tilesets.get_mut(handles[index]).unwrap();
                *tileset = tileset
                    .clone()
                    .with_image(
                        images[index],
                        TileAtlasLayout::grid(Vec2::splat(2.0), columns, rows),
                    )
                    .with_sampler(samplers[1][index]);
            }
        }
        world.get_mut::<Tilemap>(entities[0]).unwrap().tile_size = changed_tile_size;
        world.get_mut::<Tilemap>(entities[2]).unwrap().tileset = Some(untextured);
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = (0..3)
        .map(|_| {
            runtime.drive(Duration::ZERO).unwrap();
            (
                runtime.world().resource::<ExtractedSprites>().clone(),
                *runtime.world().resource::<SpriteRenderStats>(),
            )
        })
        .collect::<Vec<_>>();
    runtime.finish().unwrap();

    for (frame, (extracted, stats)) in frames.iter().enumerate() {
        let stage = usize::from(frame > 0);
        let mut expected = Vec::new();
        for map_index in [1, 0, 2] {
            let affine = transforms[map_index].affine();
            let tile_size = if map_index == 0 && stage == 1 {
                changed_tile_size
            } else {
                tile_sizes[map_index]
            };
            let material = if map_index == 2 {
                ExtractedSpriteMaterial::from_color(Color::WHITE)
            } else {
                ExtractedSpriteMaterial {
                    image: Some(images[map_index]),
                    sampler: samplers[stage][map_index],
                    alpha_mode: AlphaMode2d::Opaque,
                    tint: tints[map_index],
                }
            };
            for (index, tile) in indices.into_iter().enumerate() {
                let texture_region = if map_index == 2 {
                    TextureUvRect::FULL
                } else {
                    let (columns, rows) = layouts[stage][map_index];
                    if tile >= columns * rows {
                        continue;
                    }
                    TextureUvRect::new(
                        Vec2::new(
                            (tile % columns) as f32 / columns as f32,
                            (tile / columns) as f32 / rows as f32,
                        ),
                        Vec2::new(1.0 / columns as f32, 1.0 / rows as f32),
                    )
                };
                let coord = TileCoord::new(index as i32 - 6, (index % 3) as i32 - 1);
                let center = Vec2::new(
                    (coord.x as f32 + 0.5) * tile_size.x,
                    (coord.y as f32 + 0.5) * tile_size.y,
                );
                let color = if index % 2 == 0 {
                    Color::rgba(
                        material.tint.r * 0.5,
                        material.tint.g * 0.25,
                        material.tint.b,
                        material.tint.a * 0.5,
                    )
                } else {
                    material.tint
                };
                expected.push(ExtractedSprite {
                    entity: entities[map_index],
                    source_order: expected.len() as u64,
                    kind: ExtractedSpriteKind::TilemapCell {
                        coord,
                        tile: TileIndex::new(tile),
                    },
                    material,
                    material_extension: None,
                    texture_region,
                    world_center: affine.transform_point2(center),
                    world_x_axis: affine.transform_vector2(Vec2::new(tile_size.x * 0.5, 0.0)),
                    world_y_axis: affine.transform_vector2(Vec2::new(0.0, tile_size.y * 0.5)),
                    color,
                    layer: map_index as i32 + 3,
                    world_z: 0.0,
                    sort_key: map_index as i32 - 2,
                    world_sort_y: affine.transform_point2(center + Vec2::new(1.0, -2.0)).y,
                });
            }
        }
        assert_eq!(extracted.as_slice(), expected);
        assert_eq!(stats.extracted_tile_cells, [27, 26, 26][frame]);
        assert_eq!(stats.invalid_tile_regions, [9, 10, 10][frame]);
        assert_eq!(stats.missing_tilesets, 0);
    }
    assert_eq!(frames[1], frames[2]);
}

#[test]
fn a_missing_tileset_handle_does_not_render_its_cells_as_white_quads() {
    let mut app = sprite_app();
    app.world_mut().unwrap().spawn((
        Camera2d {
            target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            viewport_height: 100.0,
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    let missing = nara_asset::allocate_anonymous_handle::<TileSet>().unwrap();
    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_tileset(missing);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    app.world_mut()
        .unwrap()
        .spawn((tilemap, Transform2d::default()));

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let extracted = runtime.world().resource::<ExtractedSprites>().len();
    let queued = runtime.world().resource::<QueuedSpriteItems>().len();
    let instances = runtime
        .world()
        .resource::<SpriteBatches>()
        .total_instances();
    let missing_tilesets = runtime
        .world()
        .resource::<SpriteRenderStats>()
        .missing_tilesets;
    runtime.finish().unwrap();

    assert_eq!(extracted, 0);
    assert_eq!(queued, 0);
    assert_eq!(instances, 0);
    assert_eq!(missing_tilesets, 1);
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
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((tilemap, Transform2d::default()));

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
        world_z: 0.0,
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
        world_z: 0.0,
        sort_key: 0,
        world_sort_y: 0.0,
        material: material_key(),
        source_order: entity_index as u64,
        instance: instance(),
        image: None,
        material_extension: None,
        tile_chunk: None,
    }
}

fn insert_loaded_image(app: &mut App, handle: Handle<ImageAsset>) {
    let image = test_image();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(handle, image, &mut states, &mut events)
        .unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(images);
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(states);
}

fn test_image() -> ImageAsset {
    test_image_with_pixels(vec![255; 16])
}

fn test_image_with_pixels(pixels: Vec<u8>) -> ImageAsset {
    ImageAsset::new(
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels,
    )
    .unwrap()
}

fn prepared_image_snapshot() -> PreparedImageSnapshot {
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let image = test_image();
    images
        .commit_loaded(handle, image, &mut states, &mut AssetEvents::default())
        .unwrap();
    PreparedImageSnapshot::capture(handle, &images, &states).unwrap()
}

fn material_key() -> SpriteMaterialKey {
    SpriteMaterialKey {
        sampler: SamplerDescriptor::default(),
        alpha_mode: AlphaMode2d::Blend,
    }
}
