use nara_app::{App, RuntimeInstance};
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetPath, AssetStates, Assets, Handle,
    ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportedAssetType, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_core::{Color, Vec2};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageImportSettings, ImageSourceMetadata,
};
use nara_material::{
    AlphaMode2d, Material2dDefinition, Material2dDefinitions, Material2dInput, SamplerDescriptor,
};
use nara_render::{Camera2d, RenderImage2d, RenderTarget, ViewportRect};
use nara_sprite::Sprite;
use nara_sprite_render::__private::SpriteBatches;
use nara_tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
use nara_transform::Transform2d;

use crate::{
    quad::WgpuQuadBatch,
    readback_tests::{readback_pixel, render_batches_to_rgba},
    sprite::collect_sprite_quad_batches,
};

static MATERIALS: [Material2dDefinition; 1] = [Material2dDefinition {
    id: "probe.palette",
    fragment_shader: include_str!("../../../examples/support/palette_material.wgsl"),
    input: Material2dInput::SPRITE_FRAGMENT_V1,
    alpha_mode: AlphaMode2d::Blend,
}];

#[test]
fn native_ordered_tiles_and_palette_material_render_captured_pixels() {
    let batches = captured_batches();
    let pixels = render_batches_to_rgba(&batches).unwrap();
    for (x, y, expected) in [
        (11, 16, [255, 0, 0, 255]),
        (32, 16, [137, 188, 137, 255]),
        (53, 16, [255, 0, 0, 255]),
        (11, 48, [0, 255, 0, 255]),
        (32, 48, [0, 0, 255, 255]),
        (53, 48, [0, 188, 188, 255]),
    ] {
        let actual = readback_pixel(&pixels, x, y);
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(&actual, expected)| actual.abs_diff(expected) <= 2),
            "pixel ({x}, {y}): expected {expected:?}, got {actual:?}"
        );
    }
}

#[test]
fn invalid_material_shader_is_a_local_content_rejection() {
    let mut batches = captured_batches();
    let material = batches
        .iter_mut()
        .find_map(|batch| batch.material_extension.as_mut())
        .unwrap();
    material.definition.fragment_shader = "this is not valid WGSL";

    assert!(matches!(
        render_batches_to_rgba(&batches),
        Err(crate::WgpuRenderError::ContentRejected { .. })
    ));
}

fn captured_batches() -> Vec<WgpuQuadBatch> {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let indices = publish_image(
        &mut images,
        &mut states,
        &[0, 0, 0, 255],
        ImageColorSpace::Linear,
    );
    let palette = publish_image(
        &mut images,
        &mut states,
        &[255, 0, 0, 255, 0, 255, 0, 255],
        ImageColorSpace::Srgb,
    );
    let alternate = publish_image(
        &mut images,
        &mut states,
        &[255, 255, 0, 255, 0, 0, 255, 255],
        ImageColorSpace::Srgb,
    );
    app.insert_resource(images).unwrap();
    app.insert_resource(states).unwrap();
    let materials = Material2dDefinitions::new(&MATERIALS).unwrap();
    app.insert_resource(materials.clone()).unwrap();
    let world = app.world_mut().unwrap();
    let mut entities = vec![
        world
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                    viewport: Some(ViewportRect::new(0, 0, 64, 64).unwrap()),
                    viewport_height: 64.0,
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id(),
    ];
    let blue = Color::rgb(0.0, 0.0, 1.0);
    for x in [-21.0, 0.0] {
        entities.push(
            world
                .spawn((
                    Sprite::from_color(Vec2::splat(16.0), blue)
                        .with_alpha_mode(AlphaMode2d::Opaque),
                    Transform2d::from_translation(Vec2::new(x, 16.0)),
                ))
                .id(),
        );
        let mut tilemap = Tilemap::new(Vec2::splat(16.0)).with_layer(1);
        tilemap.set_cell(
            TileCoord::new(0, 0),
            TileCell::new(TileIndex::new(0)).with_color(Color::rgba(
                1.0,
                0.0,
                0.0,
                if x == 0.0 { 0.5 } else { 1.0 },
            )),
        );
        entities.push(
            world
                .spawn((
                    tilemap,
                    Transform2d::from_translation(Vec2::new(x - 8.0, 8.0)),
                ))
                .id(),
        );
    }
    entities.push(
        world
            .spawn((
                Sprite::from_color(Vec2::splat(16.0), Color::rgba(0.0, 1.0, 0.0, 0.5))
                    .with_layer(2),
                Transform2d::from_translation(Vec2::new(0.0, 16.0)),
            ))
            .id(),
    );
    for (center, offset, texture) in [
        (Vec2::new(21.0, 16.0), 0.0, palette),
        (Vec2::new(-21.0, -16.0), 1.0, palette),
        (Vec2::new(0.0, -16.0), 1.0, alternate),
        (Vec2::new(21.0, -16.0), 1.0, palette),
    ] {
        entities.push(
            world
                .spawn((
                    Sprite::from_texture(indices, Vec2::splat(16.0))
                        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
                        .with_layer(1),
                    materials
                        .instantiate("probe.palette", [offset, 1.0, 0.0, 0.0], texture)
                        .unwrap(),
                    Transform2d::from_translation(center),
                ))
                .id(),
        );
    }
    entities.push(
        world
            .spawn((
                Sprite::from_color(Vec2::splat(16.0), Color::rgba(0.0, 0.0, 1.0, 0.5))
                    .with_layer(2),
                Transform2d::from_translation(Vec2::new(21.0, -16.0)),
            ))
            .id(),
    );
    HierarchyWriter::new(world)
        .apply_batch(
            &entities
                .into_iter()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let batches = runtime.world().resource::<SpriteBatches>().clone();
    runtime.finish().unwrap();
    // The real Runtime and asset stores are gone; both images must come from captured snapshots.
    collect_sprite_quad_batches(&batches, 0)
}

fn publish_image(
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    pixels: &[u8],
    color_space: ImageColorSpace,
) -> Handle<ImageAsset> {
    let settings = ImageImportSettings::new(color_space);
    let artifact = ImportArtifactRecord::new(ImportArtifactKey::new(
        StableAssetId::parse_str("eeebd827-c451-43d3-b9ab-d87fab37802f").unwrap(),
        SourceHash::from_bytes(pixels),
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-material-probe").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    ))
    .unwrap();
    let image = ImageAsset::new(
        ImageSourceMetadata::new(
            AssetPath::new("probe/palette.png").unwrap(),
            settings,
            artifact,
        )
        .unwrap(),
        ImageExtent::new((pixels.len() / 4) as u32, 1),
        ImageFormat::Rgba8,
        pixels.to_vec(),
    )
    .unwrap();
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let source_hash = image.source().source_hash();
    let import_hash = image.source().artifact().key().digest();
    images
        .commit_loaded(
            handle,
            image,
            states,
            &mut AssetEvents::default(),
            Some(source_hash),
            Some(import_hash),
        )
        .unwrap();
    handle
}
