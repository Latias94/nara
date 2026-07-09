use image::ImageEncoder;
use nara::{advanced_prelude::*, backend_prelude::*};

const WINDOW_TEXTURE_STABLE_ID: &str = "b73f0f16-09e8-4265-b090-b689b41c197e";

fn main() -> Result<(), AppRunError> {
    let mut app = App::new();
    app.add_plugins(Runtime2dPlugins)?
        .add_plugin(WindowPlugin {
            primary_window: Some(Window::new(
                "nara windowed sprites",
                WindowResolution::new(1280, 720),
            )),
        })?
        .add_plugin(WinitPlugin::default())?
        .add_plugin(WgpuRenderPlugin)?
        .add_startup_systems(StartupStage::Scene, setup_scene);
    app.world_mut().insert_resource(AssetServer::new());

    app.run()?;
    Ok(())
}

fn setup_scene(
    mut commands: Commands,
    mut asset_server: ResMut<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
) {
    let texture = import_demo_texture(&mut asset_server, &mut images, &mut states);

    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.05, 0.06, 0.07)),
            viewport_height: 720.0,
            ..Camera2d::default()
        },
    ));

    commands.spawn((
        Name::new("imported texture"),
        Transform2d::from_translation(Vec2::new(-80.0, 20.0)),
        Sprite::from_texture(texture, Vec2::new(80.0, 80.0)).with_layer(1),
    ));
    commands.spawn((
        Name::new("rose square"),
        Transform2d {
            translation: Vec2::new(40.0, -20.0),
            rotation: 0.35,
            scale: Vec2::splat(1.0),
        },
        Sprite::from_color(Vec2::new(96.0, 48.0), Color::rgb(0.95, 0.25, 0.35)).with_layer(2),
    ));

    let mut tilemap = Tilemap::new(Vec2::new(32.0, 32.0)).with_layer(0);
    for x in -4..=4 {
        tilemap.set_cell(
            TileCoord::new(x, -3),
            TileCell::new(TileIndex::new(x.unsigned_abs()))
                .with_color(Color::rgb(0.18, 0.36, 0.22)),
        );
    }
    commands.spawn((
        Name::new("floor"),
        Transform2d::from_translation(Vec2::new(0.0, 0.0)),
        tilemap,
    ));
}

fn import_demo_texture(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
) -> Handle<ImageAsset> {
    let record = AssetRecord::new(
        StableAssetId::parse_str(WINDOW_TEXTURE_STABLE_ID).unwrap(),
        AssetPath::new("textures/windowed-demo.png").unwrap(),
        AssetSourceKind::Image,
    );
    let mut database = ProjectAssetDatabase::default();
    database.insert(record.clone()).unwrap();
    let png_bytes = rgba_png(
        2,
        2,
        &[
            255, 255, 255, 255, //
            40, 180, 255, 255, //
            255, 80, 120, 255, //
            30, 40, 60, 255,
        ],
    )
    .unwrap();
    let imported = ImageImporter::default()
        .import_job(&ImportJobInput::new(
            record.clone(),
            png_bytes,
            ImportDependencyDigest::empty(),
            ImportSettingsHash::default(),
            ImportProfile::default(),
        ))
        .unwrap();
    let texture = AssetRef::stable_id(WINDOW_TEXTURE_STABLE_ID)
        .unwrap()
        .resolve_with_database::<ImageAsset>(asset_server, &database)
        .unwrap();
    let source_hash = imported.value().source().source_hash();
    let artifact_hash = imported.artifact().key().digest();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(
            texture,
            imported.into_value(),
            states,
            &mut events,
            Some(source_hash),
            Some(artifact_hash),
        )
        .unwrap();
    texture
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.write_image(pixels, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(bytes)
}
