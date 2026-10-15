use image::ImageEncoder;
use nara::advanced_prelude::*;
use nara::hierarchy::{HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter};

const TEXTURE_STABLE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let record = AssetRecord::new(
        StableAssetId::parse_str(TEXTURE_STABLE_ID)?,
        AssetPath::new("textures/generated.png")?,
        AssetSourceKind::Image,
    );
    let mut database = ProjectAssetDatabase::default();
    database.insert(record.clone())?;

    let png_bytes = rgba_png(
        2,
        2,
        &[
            255, 0, 0, 255, //
            0, 255, 0, 255, //
            0, 0, 255, 255, //
            255, 255, 0, 255,
        ],
    )?;
    let mut asset_server = AssetServer::new();
    let texture = AssetRef::stable_id(TEXTURE_STABLE_ID)?
        .resolve_with_database::<ImageAsset>(&mut asset_server, &database)?;

    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let imported = ImageImporter::default().import_image(
        ImageBytesImportRequest::new(
            record.clone(),
            png_bytes.into_boxed_slice(),
            ImageImportSettings::default(),
        ),
        texture,
        AssetVersion::ZERO,
        &asset_server,
        &images,
        &states,
    )?;
    imported.commit(&asset_server, &mut images, &mut states, &mut events)?;

    let mut app = App::new();
    app.add_plugins((Runtime2dPlugins, Presentation2dPlugins))?;
    {
        let world = app.world_mut()?;
        world.insert_resource(asset_server);
        world.insert_resource(images);
        world.insert_resource(states);
        let camera = world
            .spawn((
                Name::new("camera"),
                Transform2d::default(),
                Camera2d {
                    target: RenderTarget::Image(Handle::<RenderImage2d>::new(AssetId::from_raw(
                        500,
                    ))),
                    viewport: Some(ViewportRect::new(0, 0, 64, 64).unwrap()),
                    viewport_height: 64.0,
                    clear_color: Some(Color::rgb(0.02, 0.03, 0.04)),
                    ..Camera2d::default()
                },
            ))
            .id();
        let sprite = world
            .spawn((
                Name::new("imported texture sprite"),
                Transform2d::default(),
                Sprite::from_texture(texture, Vec2::new(32.0, 32.0)),
            ))
            .id();
        let mut hierarchy = HierarchyWriter::new(world);
        hierarchy.place(
            camera,
            HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        )?;
        hierarchy.place(
            sprite,
            HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        )?;
    }

    let mut runtime = RuntimeInstance::start(app)?;
    runtime.drive(std::time::Duration::ZERO)?;

    let prepared = runtime
        .world()
        .resource::<PreparedRenderResources<PreparedImageResource>>();
    assert!(prepared.get_ready(image_resource_key(texture)).is_some());
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);

    let stats = runtime
        .world()
        .resource::<nara::sprite_render::SpriteRenderStats>();
    assert_eq!(stats.queued_items, 1);
    assert_eq!(
        runtime
            .world()
            .resource::<AssetServer>()
            .stable_id(texture.id())
            .map(|id| id.to_string()),
        Some(TEXTURE_STABLE_ID.to_string())
    );

    runtime.finish()?;
    Ok(())
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.write_image(pixels, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(bytes)
}
