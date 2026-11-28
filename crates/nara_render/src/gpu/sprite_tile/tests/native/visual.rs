use nara_app::{App, FixedTime, RuntimeInstance};
use nara_scene::spatial::{SpatialPlugin, Transform2d};

use crate::image::ImagePreparePlugin;
use crate::particles::{ParticleEmitter2d, ParticleEmitterConfig, ParticlePlugin};
use crate::sprite::{NineSlice, Sprite, SpriteScaleMode};
use crate::sprite_render::SpriteRenderPlugin;
use crate::{Camera2d, RenderPlugin};

use super::*;

#[test]
#[ignore = "requires a native WGPU adapter"]
fn native_sliced_sprites_mirror_borders_and_add_alpha_weighted_color() {
    let mut harness = NativeSceneTest::new();
    let clear = Color::rgba(0.25, 0.5, 0.75, 0.5);
    let mut app = sprite_scene(&harness, clear);
    let world = app.world_mut().unwrap();

    let pixels = (0..6)
        .flat_map(|row| (0..6).flat_map(move |column| texel(column, row)))
        .collect();
    let image = ImageAsset::new(
        ImageExtent::new(6, 6),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        pixels,
    )
    .unwrap();
    let texture = nara_asset::allocate_anonymous_handle().unwrap();
    let mut assets = Assets::default();
    let mut states = AssetStates::default();
    assets
        .commit_loaded(texture, image, &mut states, &mut AssetEvents::default())
        .unwrap();
    world.insert_resource(assets);
    world.insert_resource(states);

    let sprite = Sprite::from_texture(texture, Vec2::splat(4.0))
        .with_scale_mode(SpriteScaleMode::Sliced(NineSlice::new(1.0, 2.0, 2.0, 1.0)))
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_tint(Color::rgba(1.0, 1.0, 1.0, 0.5));
    world.spawn((sprite.clone(), Transform2d::from_xy(-2.0, 0.0)));
    world.spawn((
        sprite
            .with_flip(true, true)
            .with_alpha_mode(AlphaMode2d::Additive),
        Transform2d::from_xy(2.0, 0.0),
    ));

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let world = runtime.world();
    let batches = world.resource::<SpriteBatches>();
    assert_eq!(batches.len(), 2);
    assert!(
        batches
            .as_slice()
            .iter()
            .all(|batch| batch.instances.len() == 9)
    );
    let input = harness.capture(world, world.resource::<ExtractedViews>());
    runtime.finish().unwrap();

    let (pixels, _) = harness.draw_pixels(&input);
    let columns = [0, 2, 4, 5];
    let rows = [0, 1, 3, 5];
    for (y, row) in pixels.into_iter().enumerate() {
        for (x, actual) in row.into_iter().enumerate() {
            let additive = x >= 4;
            let sample = if additive {
                texel(columns[7 - x], rows[3 - y])
            } else {
                texel(columns[x], rows[y])
            };
            let [red, green, blue, opacity] = sample;
            let alpha = f32::from(opacity) * 0.5 / 255.0;
            let destination_factor = if additive { 1.0 } else { 1.0 - alpha };
            let channel = |sample: u8, background: f32| {
                (f32::from(sample) * alpha + background * clear.a * destination_factor * 255.0)
                    .round() as u8
            };
            assert_pixel(
                actual,
                [
                    channel(red, clear.r),
                    channel(green, clear.g),
                    channel(blue, clear.b),
                    ((alpha + clear.a * (1.0 - alpha)) * 255.0).round() as u8,
                ],
            );
        }
    }
    let (_, warm) = harness.draw_pixels(&input);
    assert_no_native_creates(warm);
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn native_particles_extract_after_fixed_step_and_add_to_the_target() {
    let mut harness = NativeSceneTest::new();
    let mut app = sprite_scene(&harness, Color::rgb(0.0, 0.0, 0.25));
    app.add_plugins(ParticlePlugin).unwrap();
    let emitter = ParticleEmitter2d::new(ParticleEmitterConfig {
        max_particles: 1,
        burst: 1,
        size_over_life: [2.0; 2],
        color_over_life: [Color::rgba(1.0, 0.5, 0.0, 0.5); 2],
        alpha_mode: AlphaMode2d::Additive,
        ..ParticleEmitterConfig::default()
    })
    .unwrap();
    app.world_mut()
        .unwrap()
        .spawn((emitter, Transform2d::from_xy(1.0, 0.0)));

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<SpriteBatches>().is_empty());
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let world = runtime.world();
    assert_eq!(world.resource::<SpriteBatches>().total_instances(), 1);
    let input = harness.capture(world, world.resource::<ExtractedViews>());
    runtime.finish().unwrap();

    let (pixels, _) = harness.draw_pixels(&input);
    for (y, row) in pixels.into_iter().enumerate() {
        for (x, actual) in row.into_iter().enumerate() {
            let expected = if (4..=5).contains(&x) && (1..=2).contains(&y) {
                [128, 64, 64, 255]
            } else {
                [0, 0, 64, 255]
            };
            assert_pixel(actual, expected);
        }
    }
}

fn sprite_scene(harness: &NativeSceneTest, clear: Color) -> App {
    let mut app = App::new();
    app.add_plugins((
        SpatialPlugin,
        RenderPlugin::default(),
        ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app.world_mut().unwrap().spawn((
        Camera2d {
            target: harness.logical_target(),
            viewport: ViewportRect::from_extent(harness.packet_target.extent()),
            viewport_height: 4.0,
            clear_color: Some(clear),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    app
}

fn texel(column: u8, row: u8) -> [u8; 4] {
    let alpha = if column == 5 && row == 0 { 0 } else { 128 };
    [column * 16, row * 16, 48, alpha]
}
