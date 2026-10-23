use std::error::Error;

use image::ImageEncoder;
use nara::advanced_prelude::*;
use nara::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
};

use super::{ACTIVATE_ACTION, DOMAIN, UI_TEXTURE_STABLE_ID};

#[derive(Resource)]
pub(super) struct GalleryImage(pub(super) Handle<ImageAsset>);

pub(super) fn setup_gallery(
    mut commands: Commands,
    mut asset_server: ResMut<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
) {
    let image = import_ui_texture(&mut asset_server, &mut images, &mut states);
    commands.insert_resource(GalleryImage(image));

    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.06, 0.07, 0.08)),
            viewport_height: 270.0,
            ..Camera2d::default()
        },
    ));

    let root = commands
        .spawn((
            Name::new("gallery root"),
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(DOMAIN, "root").unwrap(),
            UiSemantics::new(UiSemanticRole::Group, "Presentation gallery").unwrap(),
        ))
        .id();
    let panel = commands
        .spawn((
            Name::new("clipped panel"),
            UiNode::new(
                UiStyle::absolute(24.0, 24.0, 280.0, 180.0).with_overflow(UiOverflow::Clip),
            ),
            UiVisual::solid(UiColor::rgba(0.12, 0.14, 0.16, 0.96)),
            UiRuntimeKey::new(DOMAIN, "panel").unwrap(),
        ))
        .id();
    let accent = commands
        .spawn((
            Name::new("clipped image"),
            UiNode::new(UiStyle::absolute(220.0, 18.0, 96.0, 96.0).with_z_index(1)),
            UiVisual::image(
                UiImageId::new(UI_TEXTURE_STABLE_ID).unwrap(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            ),
            UiRuntimeKey::new(DOMAIN, "image").unwrap(),
        ))
        .id();
    let status = commands
        .spawn((
            Name::new("packet status"),
            UiNode::new(UiStyle::absolute(18.0, 18.0, 188.0, 44.0).with_z_index(2)),
            UiVisual::solid(UiColor::rgba(0.22, 0.58, 0.86, 0.88)),
            UiText::new("Owned packet primitives", 16.0)
                .unwrap()
                .with_brush(UiBrush::Solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
            UiRuntimeKey::new(DOMAIN, "status").unwrap(),
        ))
        .id();
    let button = commands
        .spawn((
            Name::new("activation button"),
            UiButton::new(
                "Activate packet",
                UiActionId::new(ACTIVATE_ACTION).unwrap(),
                16.0,
            )
            .unwrap(),
            UiButtonVisuals::default(),
            UiNode::new(UiStyle::absolute(18.0, 84.0, 160.0, 48.0).with_z_index(2)),
            UiVisual::solid(UiColor::rgba(0.24, 0.62, 0.46, 0.96)),
            UiRuntimeKey::new(DOMAIN, "activate").unwrap(),
        ))
        .id();
    commands.apply_hierarchy_batch(vec![
        HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
        HierarchyOperation::Place {
            entity: panel,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
        },
        HierarchyOperation::Place {
            entity: accent,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: panel }),
        },
        HierarchyOperation::Place {
            entity: status,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: panel }),
        },
        HierarchyOperation::Place {
            entity: button,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: panel }),
        },
    ]);
}

pub(super) fn capture_prepared_image(
    world: &World,
    handle: Handle<ImageAsset>,
) -> Result<PreparedImageSnapshot, Box<dyn Error>> {
    let key = image_resource_key(handle);
    world
        .resource::<PreparedImages>()
        .get(key)
        .cloned()
        .ok_or_else(|| "the gallery image was not prepared".into())
}

pub(super) fn surface_id(value: u64) -> UiSurfaceId {
    UiSurfaceId::new(value).expect("gallery surface IDs are non-zero")
}

fn import_ui_texture(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
) -> Handle<ImageAsset> {
    let record = AssetRecord::new(
        StableAssetId::parse_str(UI_TEXTURE_STABLE_ID).unwrap(),
        AssetPath::new("textures/ui-presentation-gallery.png").unwrap(),
        AssetSourceKind::Image,
    );
    let mut database = ProjectAssetDatabase::default();
    database.insert(record.clone()).unwrap();
    let png_bytes = rgba_png(
        2,
        2,
        &[
            255, 255, 255, 255, 24, 120, 220, 255, 36, 196, 128, 255, 240, 72, 96, 255,
        ],
    )
    .unwrap();
    let image = AssetRef::stable_id(UI_TEXTURE_STABLE_ID)
        .unwrap()
        .resolve_with_database::<ImageAsset>(asset_server, &database)
        .unwrap();
    let imported = ImageImporter::default()
        .import_image(
            ImageBytesImportRequest::new(
                record.clone(),
                png_bytes.into_boxed_slice(),
                ImageImportSettings::default(),
            ),
            image,
            states.version(image.id()).unwrap_or(AssetVersion::ZERO),
            asset_server,
            images,
            states,
        )
        .unwrap();
    let mut events = AssetEvents::default();
    imported
        .commit(asset_server, images, states, &mut events)
        .unwrap();
    image
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.write_image(pixels, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(bytes)
}
