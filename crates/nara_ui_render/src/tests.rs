use std::time::Duration;

use nara_app::App;
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetId, AssetPath, AssetStates, Assets,
    Handle, ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportSettingsHash, ImportedAssetType, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_core::{Color, Vec2};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageSourceMetadata, image_resource_key,
};
use nara_material::SamplerDescriptor;
use nara_render::{Camera2d, ViewportRect};
use nara_scene::Parent;
use nara_ui::{UiNode, UiPanel, UiRect, UiRoot, UiStyle};

use crate::{
    ExtractedUiItems, QueuedUiItems, UiBatches, UiClipRect, UiRenderPlugin, UiRenderStats,
    ui_rect_to_clip_instance,
};

#[test]
fn colored_panel_queues_without_image_asset() {
    let mut app = ui_app();
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(10.0, 20.0, 50.0, 25.0)),
        UiPanel::from_color(Color::rgb(0.8, 0.2, 0.1)),
        Parent(root),
    ));

    app.run_once(Duration::ZERO).unwrap();

    let extracted = app.world().resource::<ExtractedUiItems>();
    let queued = app.world().resource::<QueuedUiItems>();
    let batches = app.world().resource::<UiBatches>();
    let stats = app.world().resource::<UiRenderStats>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(queued.len(), 1);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches.as_slice()[0].material.image, None);
    assert_eq!(batches.total_instances(), 1);
    assert_eq!(stats.missing_images, 0);
    assert_eq!(stats.unprepared_images, 0);
}

#[test]
fn image_panel_uses_prepared_image_material_key_when_ready() {
    let image = Handle::new(AssetId::from_raw(7));
    let mut app = ui_app();
    insert_loaded_image(&mut app, image);
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0)),
        UiPanel::from_image(image).with_sampler(SamplerDescriptor::NEAREST_CLAMP),
        Parent(root),
    ));

    app.run_once(Duration::ZERO).unwrap();

    let batches = app.world().resource::<UiBatches>();
    let stats = app.world().resource::<UiRenderStats>();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches.as_slice()[0].material.image,
        Some(image_resource_key(image))
    );
    assert_eq!(
        batches.as_slice()[0].material.sampler,
        SamplerDescriptor::NEAREST_CLAMP
    );
    assert_eq!(stats.missing_images, 0);
    assert_eq!(stats.unprepared_images, 0);
}

#[test]
fn missing_image_panel_queues_fallback_material_and_records_stats() {
    let missing = Handle::new(AssetId::from_raw(99));
    let mut app = ui_app();
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0)),
        UiPanel::from_image(missing).with_tint(Color::rgba(1.0, 1.0, 1.0, 0.25)),
        Parent(root),
    ));

    app.run_once(Duration::ZERO).unwrap();

    let batches = app.world().resource::<UiBatches>();
    let stats = app.world().resource::<UiRenderStats>();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches.as_slice()[0].material.image, None);
    assert_eq!(stats.missing_images, 1);
    assert_eq!(stats.unprepared_images, 0);
}

#[test]
fn clipping_splits_batches_with_same_material() {
    let mut app = ui_app();
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    let clip_parent = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 64.0, 64.0)).clipping_children(),
            Parent(root),
        ))
        .id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(4.0, 4.0, 16.0, 16.0)),
        UiPanel::from_color(Color::WHITE),
        Parent(clip_parent),
    ));
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(80.0, 0.0, 16.0, 16.0)),
        UiPanel::from_color(Color::WHITE),
        Parent(root),
    ));

    app.run_once(Duration::ZERO).unwrap();

    let batches = app.world().resource::<UiBatches>();
    assert_eq!(batches.len(), 2);
    let clip_rects = batches
        .as_slice()
        .iter()
        .map(|batch| batch.clip_rect)
        .collect::<Vec<_>>();
    assert!(clip_rects.contains(&None));
    assert!(clip_rects.contains(&Some(
        UiClipRect::from_rect(UiRect::from_origin_size(0.0, 0.0, 64.0, 64.0)).unwrap()
    )));
}

#[test]
fn ui_rect_projection_uses_top_left_origin_and_flipped_uv() {
    let viewport = ViewportRect::new(0, 0, 200, 100).unwrap();
    let instance = ui_rect_to_clip_instance(
        viewport,
        UiRect::from_origin_size(50.0, 25.0, 100.0, 50.0),
        Color::WHITE,
    )
    .unwrap();

    assert_eq!(instance.center, Vec2::ZERO);
    assert_eq!(instance.x_axis, Vec2::new(0.5, 0.0));
    assert_eq!(instance.y_axis, Vec2::new(0.0, -0.5));
    assert_eq!(instance.uv.min, Vec2::new(0.0, 1.0));
    assert_eq!(instance.uv.size, Vec2::new(1.0, -1.0));
}

fn ui_app() -> App {
    let mut app = App::new();
    app.add_plugin(UiRenderPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    app
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
    app.world_mut().insert_resource(images);
    app.world_mut().insert_resource(states);
}

fn test_image() -> ImageAsset {
    let stable_id = StableAssetId::parse_str("ce2ab2f8-c58b-48e3-b94e-9465340262a1").unwrap();
    let source_hash = SourceHash::from_bytes(b"test ui image");
    let key = ImportArtifactKey::new(
        stable_id,
        source_hash,
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-image").unwrap(),
        ImporterVersion::new(1),
        ImportSettingsHash::default(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    );
    let artifact = ImportArtifactRecord::new(key).unwrap();
    let source = ImageSourceMetadata::new(
        stable_id,
        AssetPath::new("textures/ui-test.png").unwrap(),
        source_hash,
        artifact,
    );
    ImageAsset::new(
        source,
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![255; 16],
    )
}
