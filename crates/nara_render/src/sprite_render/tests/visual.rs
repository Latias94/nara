use crate::particles::{ParticleEmitter2d, ParticleEmitterConfig, ParticlePlugin};
use crate::sprite::{NineSlice, SpriteAnchor, SpriteScaleMode};

use super::*;

fn visual_app(extent: ImageExtent) -> (App, Handle<ImageAsset>) {
    let mut app = sprite_app();
    let image = ImageAsset::new(
        extent,
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![255; extent.pixel_count() as usize * 4],
    )
    .unwrap();
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let mut assets = Assets::default();
    let mut states = AssetStates::default();
    assets
        .commit_loaded(handle, image, &mut states, &mut AssetEvents::default())
        .unwrap();
    let world = app.world_mut().unwrap();
    world.insert_resource(assets);
    world.insert_resource(states);
    world.spawn((
        Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 200, 200).unwrap()),
            viewport_height: 200.0,
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    (app, handle)
}

#[test]
fn stretch_flips_uvs_on_either_axis_and_keeps_invalid_authored_regions_rejected() {
    let (mut app, image) = visual_app(ImageExtent::new(16, 16));
    let region = TextureRegion::new(Vec2::new(0.125, 0.25), Vec2::new(0.25, 0.5));
    let sprite = Sprite::from_texture(image, Vec2::splat(4.0)).with_texture_region(region);
    let world = app.world_mut().unwrap();
    for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
        world.spawn((
            sprite.clone().with_flip(flip_x, flip_y),
            Transform2d::default(),
        ));
    }
    world.spawn((
        sprite
            .with_flip(true, false)
            .with_texture_region(TextureRegion::new(Vec2::splat(0.5), Vec2::new(-0.25, 0.25))),
        Transform2d::default(),
    ));

    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let world = runtime.world();
    assert_eq!(
        world
            .resource::<SpriteRenderStats>()
            .invalid_texture_regions,
        1
    );
    let batches = world.resource::<SpriteBatches>().as_slice();
    assert_eq!(batches.len(), 1);
    let instances = &batches[0].instances;
    let expected = [
        ([0.125, 0.25], [0.25, 0.5]),
        ([0.375, 0.25], [-0.25, 0.5]),
        ([0.125, 0.75], [0.25, -0.5]),
        ([0.375, 0.75], [-0.25, -0.5]),
    ];
    assert_eq!(instances.len(), expected.len());
    for (instance, (start, size)) in instances.iter().zip(expected) {
        assert_eq!(instance.uv, TextureUvRect::new(start.into(), size.into()));
        assert_eq!(instance.center, instances[0].center);
        assert_eq!(instance.x_axis, instances[0].x_axis);
        assert_eq!(instance.y_axis, instances[0].y_axis);
    }
    runtime.finish().unwrap();
}

#[test]
fn sliced_atlas_preserves_asymmetric_borders_and_one_batch_through_flips() {
    let (mut app, image) = visual_app(ImageExtent::new(32, 16));
    let region = TextureRegion::new(Vec2::splat(0.25), Vec2::splat(0.5));
    let mut sprite = Sprite::from_texture(image, Vec2::new(30.0, 18.0))
        .with_texture_region(region)
        .with_y_sort_anchor(Vec2::new(1.0, 6.0))
        .with_scale_mode(SpriteScaleMode::Sliced(NineSlice::new(3.0, 5.0, 2.0, 1.0)));
    sprite.anchor = SpriteAnchor {
        normalized: Vec2::new(0.25, -0.25),
    };
    let transform = Transform2d {
        translation: Vec2::new(10.0, -6.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(2.0, 1.0),
        z: 4.0,
    };
    let world = app.world_mut().unwrap();
    let regular = world.spawn((sprite.clone(), transform)).id();
    let flipped = world.spawn((sprite.with_flip(true, true), transform)).id();

    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let world = runtime.world();
    let extracted = world.resource::<ExtractedSprites>().as_slice();
    let batches = world.resource::<SpriteBatches>().as_slice();
    assert_eq!(extracted.len(), 18);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].instances.len(), 18);
    assert_eq!(world.resource::<SpriteRenderStats>().extracted_sprites, 2);

    let affine = transform.affine();
    let sprite_center = Vec2::new(-7.5, 4.5);
    let expected = [
        (
            regular,
            [-15.0, -12.0, 10.0, 15.0],
            [9.0, 7.0, -8.0, -9.0],
            [8.0, 11.0, 19.0, 24.0],
            [4.0, 6.0, 11.0, 12.0],
        ),
        (
            flipped,
            [-15.0, -10.0, 12.0, 15.0],
            [9.0, 8.0, -7.0, -9.0],
            [24.0, 19.0, 11.0, 8.0],
            [12.0, 11.0, 6.0, 4.0],
        ),
    ];
    for (index, quad) in extracted.iter().enumerate() {
        let (entity, xs, ys, us, vs) = expected[index / 9];
        let column = index % 3;
        let row = (index % 9) / 3;
        let center = sprite_center
            + Vec2::new(
                (xs[column] + xs[column + 1]) * 0.5,
                (ys[row] + ys[row + 1]) * 0.5,
            );
        let half_width = (xs[column + 1] - xs[column]) * 0.5;
        let half_height = (ys[row] - ys[row + 1]) * 0.5;
        let start = Vec2::new(us[column] / 32.0, vs[row] / 16.0);
        let end = Vec2::new(us[column + 1] / 32.0, vs[row + 1] / 16.0);
        let uv = TextureUvRect::new(start, end - start);
        assert_eq!(quad.entity, entity);
        assert_eq!(quad.source_order, index as u64);
        assert_eq!(quad.world_z, transform.z);
        assert_eq!(
            quad.world_sort_y,
            affine.transform_point2(Vec2::new(1.0, 6.0)).y
        );
        assert_eq!(quad.texture_region, uv);
        assert_eq!(batches[0].instances[index].uv, uv);
        assert!(
            quad.world_center
                .abs_diff_eq(affine.transform_point2(center), 0.000_01)
        );
        assert!(
            quad.world_x_axis
                .abs_diff_eq(affine.transform_vector2(Vec2::X * half_width), 0.000_01)
        );
        assert!(
            quad.world_y_axis
                .abs_diff_eq(affine.transform_vector2(Vec2::Y * half_height), 0.000_01)
        );
    }
    runtime.finish().unwrap();
}

#[test]
fn sliced_borders_clamp_in_source_and_destination_without_gaps() {
    let (mut app, image) = visual_app(ImageExtent::new(8, 8));
    let borders = NineSlice {
        left: f32::MAX,
        right: f32::MAX,
        top: f32::NAN,
        bottom: -1.0,
    };
    let world = app.world_mut().unwrap();
    for width in [4.0, 12.0] {
        world.spawn((
            Sprite::from_texture(image, Vec2::new(width, 6.0))
                .with_scale_mode(SpriteScaleMode::Sliced(borders)),
            Transform2d::default(),
        ));
    }

    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let world = runtime.world();
    let quads = world.resource::<ExtractedSprites>().as_slice();
    let expected = [
        (-1.0, 1.0, 0.0, 0.5),
        (1.0, 1.0, 0.5, 0.5),
        (-4.0, 2.0, 0.0, 0.5),
        (0.0, 2.0, 0.5, 0.0),
        (4.0, 2.0, 0.5, 0.5),
    ];
    assert_eq!(quads.len(), expected.len());
    for (quad, (center_x, half_width, start, span)) in quads.iter().zip(expected) {
        assert_eq!(quad.world_center, Vec2::new(center_x, 0.0));
        assert!(
            quad.world_x_axis
                .abs_diff_eq(Vec2::X * half_width, 0.000_001)
        );
        assert_eq!(quad.world_y_axis, Vec2::new(0.0, 3.0));
        assert_eq!(
            quad.texture_region,
            TextureUvRect::new(Vec2::new(start, 0.0), Vec2::new(span, 1.0))
        );
    }
    assert_eq!(world.resource::<SpriteBatches>().total_instances(), 5);
    runtime.finish().unwrap();
}

#[test]
fn additive_sprite_keeps_its_custom_material_and_splits_the_blend_batch() {
    let (mut app, texture) = visual_app(ImageExtent::new(2, 2));
    let materials = palette_materials("test.visuals", AlphaMode2d::Blend);
    let binding = materials
        .current("test.visuals")
        .unwrap()
        .bind(
            &PaletteParameters {
                x: 1.0,
                y: 1.0,
                z: 0.0,
                w: 0.0,
            },
            BTreeMap::from([("palette".to_owned(), texture)]),
        )
        .unwrap();
    let world = app.world_mut().unwrap();
    for alpha_mode in [AlphaMode2d::Blend, AlphaMode2d::Additive] {
        world.spawn((
            Sprite::from_texture(texture, Vec2::ONE).with_alpha_mode(alpha_mode),
            Transform2d::default(),
            binding.clone(),
        ));
    }

    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let batches = runtime.world().resource::<SpriteBatches>().as_slice();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].material.alpha_mode, AlphaMode2d::Blend);
    assert_eq!(batches[1].material.alpha_mode, AlphaMode2d::Additive);
    for batch in batches {
        assert_eq!(batch.material_extension.as_ref().unwrap().binding, binding);
    }
    runtime.finish().unwrap();
}

#[test]
fn particles_use_sprite_batches_and_follow_inherited_visibility() {
    let (mut app, texture) = visual_app(ImageExtent::new(2, 2));
    app.add_plugins(ParticlePlugin).unwrap();
    let config = ParticleEmitterConfig {
        max_particles: 2,
        burst: 2,
        size_over_life: [2.0; 2],
        texture: Some(texture),
        alpha_mode: AlphaMode2d::Additive,
        layer: 3,
        sort_key: 8,
        local_space: true,
        ..ParticleEmitterConfig::default()
    };
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::from_xy(5.0, -3.0)).id();
    let emitter = world
        .spawn((
            ParticleEmitter2d::new(config.clone()).unwrap(),
            ChildOf(parent),
        ))
        .id();
    let hidden_parent = world
        .spawn((Transform2d::default(), Visibility::Hidden))
        .id();
    world.spawn((
        ParticleEmitter2d::new(config).unwrap(),
        ChildOf(hidden_parent),
    ));
    world.spawn((
        Sprite::from_texture(texture, Vec2::ONE)
            .with_alpha_mode(AlphaMode2d::Additive)
            .with_layer(3)
            .with_sort_key(8),
        Transform2d::default(),
    ));

    let mut runtime = start_runtime(app);
    runtime
        .drive(nara_app::FixedTime::DEFAULT_TIMESTEP)
        .unwrap();
    let world = runtime.world();
    let quads = world.resource::<ExtractedSprites>().as_slice();
    let batches = world.resource::<SpriteBatches>().as_slice();
    assert_eq!(quads.len(), 3);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].instances.len(), 3);
    assert_eq!(batches[0].material.alpha_mode, AlphaMode2d::Additive);
    for quad in &quads[..2] {
        assert_eq!(quad.entity, emitter);
        assert_eq!(quad.world_center, Vec2::new(5.0, -3.0));
        assert_eq!(quad.world_x_axis, Vec2::X);
        assert_eq!(quad.world_y_axis, Vec2::Y);
        assert_eq!(quad.texture_region, TextureUvRect::FULL);
    }
    runtime.finish().unwrap();
}
