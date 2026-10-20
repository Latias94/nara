use super::*;
use nara_core::Mat3;
use nara_ecs::system::RunSystemOnce;
use nara_material::{Material2dDefinition, Material2dDefinitions, Material2dInput};

#[test]
fn clip_bounds_keep_touching_affine_and_uncertain_quads() {
    let half_x = Vec2::new(0.5, 0.0);
    let half_y = Vec2::new(0.0, 0.5);
    let cases = [
        (Vec2::new(1.5, 0.0), half_x, half_y, true),
        (Vec2::new(-1.5, 0.0), half_x, half_y, true),
        (Vec2::new(0.0, 1.5), half_x, half_y, true),
        (Vec2::new(0.0, -1.5), half_x, half_y, true),
        (Vec2::new(1.51, 0.0), half_x, half_y, false),
        (Vec2::new(-1.51, 0.0), half_x, half_y, false),
        (Vec2::new(0.0, 1.51), half_x, half_y, false),
        (Vec2::new(0.0, -1.51), half_x, half_y, false),
        (
            Vec2::new(1.2, 0.0),
            Vec2::new(-0.4, 0.3),
            Vec2::new(0.3, 0.4),
            true,
        ),
        (
            Vec2::new(1.2, 0.0),
            Vec2::new(-0.5, 0.2),
            Vec2::new(0.7, 0.4),
            true,
        ),
        (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, true),
        (Vec2::splat(5.0), Vec2::ZERO, Vec2::ZERO, false),
        (Vec2::new(f32::NAN, 5.0), half_x, half_y, true),
        (Vec2::new(f32::INFINITY, 5.0), half_x, half_y, true),
        (
            Vec2::new(f32::MAX, 5.0),
            Vec2::new(f32::MAX, 0.0),
            half_y,
            true,
        ),
    ];
    for (index, (center, x_axis, y_axis, retained)) in cases.into_iter().enumerate() {
        let sprite = extracted(index as u32, center, x_axis, y_axis);
        let (queued, _) = run_queue(&[sprite], &[clip_view()]);
        assert_eq!(
            !queued.is_empty(),
            retained,
            "case {index}: {center:?}, {x_axis:?}, {y_axis:?}"
        );
    }
}

#[test]
fn clip_culling_retains_f32_vertex_cancellation_inside_an_exactly_offscreen_bound() {
    let sprite = extracted(
        0,
        Vec2::new(67_108_864.0, 0.0),
        Vec2::new(6.0, 0.5),
        Vec2::new(67_108_856.0, 0.0),
    );
    let projected =
        crate::queue::project_sprite_to_view_with_uv(&clip_view(), &sprite, TextureUvRect::FULL)
            .unwrap();
    let exact_left = f64::from(projected.center.x)
        - f64::from(projected.x_axis.x).abs()
        - f64::from(projected.y_axis.x).abs();
    assert_eq!(exact_left, 2.0);
    // Match the stock vertex shader's f32 additions, rather than reassociating its geometry.
    let vertex = (projected.center - projected.x_axis) - projected.y_axis;
    assert_eq!(vertex, Vec2::new(0.0, -0.5));
    let (queued, _) = run_queue(&[sprite], &[clip_view()]);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].instance, projected);
}

#[test]
fn affine_camera_and_offset_viewport_cull_the_projected_quad() {
    let viewport = ViewportRect::new(17, 29, 320, 180).unwrap();
    let mapping = nara_render::ViewMapping2d::orthographic(
        Mat3::from_translation(Vec2::new(10.0, -5.0))
            * Mat3::from_scale(Vec2::new(-2.0, 0.5))
            * Mat3::from_angle(0.7),
        180.0,
        viewport,
    )
    .unwrap();
    let view = ExtractedView {
        viewport,
        mapping,
        ..clip_view()
    };
    let origin = mapping.clip_to_world(Vec2::ZERO);
    let world_x = mapping.clip_to_world(Vec2::new(0.3, 0.2)) - origin;
    let world_y = mapping.clip_to_world(Vec2::new(-0.2, 0.4)) - origin;
    let sprites = [
        extracted(
            0,
            mapping.clip_to_world(Vec2::new(0.9, 0.1)),
            world_x,
            world_y,
        ),
        extracted(
            1,
            mapping.clip_to_world(Vec2::new(3.0, 0.1)),
            world_x,
            world_y,
        ),
    ];
    let (queued, _) = run_queue(&sprites, &[view]);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].entity, sprites[0].entity);
    assert!(
        queued[0]
            .instance
            .center
            .abs_diff_eq(Vec2::new(0.9, 0.1), 0.000_01)
    );
}

#[test]
fn each_view_keeps_its_order_while_offscreen_material_diagnostics_run_once() {
    static DEFINITIONS: [Material2dDefinition; 1] = [Material2dDefinition {
        id: "test.visibility-material",
        fragment_shader: "fn material_fragment() {}",
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Blend,
    }];
    let half_x = Vec2::new(0.1, 0.0);
    let half_y = Vec2::new(0.0, 0.1);
    let mut sprites = vec![
        extracted(0, Vec2::ZERO, half_x, half_y),
        extracted(1, Vec2::new(10.0, 0.0), half_x, half_y),
        extracted(
            2,
            Vec2::new(5.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(0.0, 0.1),
        ),
        extracted(3, Vec2::ZERO, half_x, half_y),
        extracted(4, Vec2::ZERO, half_x, half_y),
        extracted(5, Vec2::splat(100.0), Vec2::X, Vec2::Y),
        extracted(6, Vec2::splat(100.0), Vec2::X, Vec2::Y),
        extracted(7, Vec2::splat(100.0), Vec2::X, Vec2::Y),
    ];
    sprites[0].layer = 2;
    sprites[1].layer = -1;
    sprites[2].layer = 1;
    sprites[3].layer = 1;
    sprites[3].world_sort_y = 10.0;
    sprites[4].layer = 1;
    sprites[4].sort_key = 1;
    sprites[4].world_sort_y = 20.0;
    sprites[5].material.image = Some(nara_asset::allocate_anonymous_handle().unwrap());
    sprites[6].texture_region = TextureUvRect::new(Vec2::ZERO, Vec2::splat(-1.0));
    sprites[7].material_extension = Some(
        Material2dDefinitions::new(&DEFINITIONS)
            .unwrap()
            .instantiate(
                "test.visibility-material",
                [0.0; 4],
                nara_asset::allocate_anonymous_handle().unwrap(),
            )
            .unwrap(),
    );
    let views = [
        ExtractedView {
            order: 7,
            ..clip_view()
        },
        ExtractedView {
            order: -2,
            mapping: nara_render::ViewMapping2d::orthographic(
                Mat3::from_translation(Vec2::new(10.0, 0.0)),
                2.0,
                clip_view().viewport,
            )
            .unwrap(),
            ..clip_view()
        },
    ];
    let (queued, stats) = run_queue(&sprites, &views);
    assert_eq!(stats.unprepared_textures, 2);
    assert_eq!(stats.invalid_texture_regions, 1);
    assert_eq!(stats.queued_items, 6);
    assert_eq!(
        queued
            .iter()
            .map(|item| (item.view_index, item.entity))
            .collect::<Vec<_>>(),
        vec![
            (1, sprites[1].entity),
            (1, sprites[2].entity),
            (0, sprites[3].entity),
            (0, sprites[2].entity),
            (0, sprites[4].entity),
            (0, sprites[0].entity),
        ]
    );
    let (without_views, stats) = run_queue(&sprites, &[]);
    assert!(without_views.is_empty());
    assert_eq!(stats.unprepared_textures, 2);
    assert_eq!(stats.invalid_texture_regions, 1);
}

#[test]
fn direct_tile_changes_are_reobserved_even_after_dirty_chunks_are_cleared() {
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((
            Camera2d {
                viewport: Some(clip_view().viewport),
                viewport_height: 2.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let mut tiles = Tilemap::new(Vec2::ONE);
    tiles.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    tiles.set_cell(TileCoord::new(4, 0), TileCell::new(TileIndex::new(0)));
    let tilemap = world.spawn((tiles, Transform2d::default())).id();
    place_roots(world, &[camera, tilemap]);
    app.add_systems(
        CoreStage::Update,
        move |time: nara_ecs::Res<nara_app::RealTime>,
              mut maps: nara_ecs::Query<&mut Tilemap>,
              mut transforms: nara_ecs::Query<&mut Transform2d>| {
            let mut map = maps.get_mut(tilemap).unwrap();
            match time.frame {
                2 => {
                    map.tile_size = Vec2::new(0.2, 0.5);
                    map.layer.index = 7;
                    map.sort_key = -3;
                    map.y_sort_anchor = Some(Vec2::new(0.5, 1.0));
                    map.set_cell(
                        TileCoord::new(0, 0),
                        TileCell::new(TileIndex::new(0)).with_color(Color::rgb(1.0, 0.0, 0.0)),
                    );
                }
                3 => {
                    map.remove_cell(TileCoord::new(0, 0));
                }
                4 => {
                    *map = Tilemap::new(Vec2::ONE);
                    map.set_cell(TileCoord::new(10, 0), TileCell::new(TileIndex::new(0)));
                }
                5 => {
                    transforms.get_mut(tilemap).unwrap().translation = Vec2::new(-10.0, 0.0);
                }
                _ => {}
            }
            map.clear_dirty_chunks();
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let old_batches = runtime.world().resource::<SpriteBatches>().clone();
    assert_eq!(old_batches.total_instances(), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .extracted_tile_cells,
        2
    );

    runtime.drive(Duration::ZERO).unwrap();
    let queued = runtime.world().resource::<QueuedSpriteItems>().as_slice();
    assert_eq!(queued.len(), 2);
    assert!(
        queued
            .iter()
            .all(|item| item.layer == 7 && item.sort_key == -3 && item.world_sort_y == 1.25)
    );
    assert_eq!(queued[0].instance.color, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(queued[0].instance.x_axis, Vec2::new(0.1, 0.0));
    assert_eq!(queued[0].instance.y_axis, Vec2::new(0.0, 0.25));
    assert_eq!(
        old_batches.as_slice()[0].instances[0].x_axis,
        Vec2::new(0.5, 0.0)
    );

    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        1
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .extracted_tile_cells,
        1
    );
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        0
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteBatches>()
            .total_instances(),
        1
    );
    assert_eq!(
        runtime.world().resource::<QueuedSpriteItems>().as_slice()[0]
            .instance
            .center,
        Vec2::splat(0.5)
    );
    runtime.finish().unwrap();
}

fn clip_view() -> ExtractedView {
    let viewport = ViewportRect::new(0, 0, 100, 100).unwrap();
    ExtractedView {
        viewport,
        mapping: nara_render::ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        ..test_view()
    }
}

fn extracted(index: u32, center: Vec2, x_axis: Vec2, y_axis: Vec2) -> ExtractedSprite {
    ExtractedSprite {
        entity: Entity::from_raw_u32(index + 1).unwrap(),
        source_order: u64::from(index),
        world_center: center,
        world_x_axis: x_axis,
        world_y_axis: y_axis,
        ..placeholder_extracted_sprite()
    }
}

fn run_queue(
    sprites: &[ExtractedSprite],
    views: &[ExtractedView],
) -> (Vec<QueuedSpriteItem>, SpriteRenderStats) {
    let mut world = World::new();
    let mut extracted = ExtractedSprites::default();
    for sprite in sprites {
        extracted.push(sprite.clone());
    }
    let mut extracted_views = nara_render::ExtractedViews::default();
    for view in views {
        extracted_views.push(*view);
    }
    world.insert_resource(extracted);
    world.insert_resource(extracted_views);
    world.init_resource::<QueuedSpriteItems>();
    world.init_resource::<SpriteRenderStats>();
    world.init_resource::<SpriteBatches>();
    world.run_system_once(crate::queue::queue_sprites).unwrap();
    world
        .run_system_once(crate::queue::sort_and_batch_sprites)
        .unwrap();
    (
        world.resource::<QueuedSpriteItems>().as_slice().to_vec(),
        *world.resource::<SpriteRenderStats>(),
    )
}
