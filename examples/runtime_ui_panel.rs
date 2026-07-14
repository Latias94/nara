use image::ImageEncoder;
use nara::{advanced_prelude::*, backend_prelude::*};

const UI_TEXTURE_STABLE_ID: &str = "f7d2d9c7-2b13-49fe-8b89-83d0f98f0c3f";

fn main() -> Result<(), AppRunError> {
    let mut app = App::new();
    app.add_plugins(RuntimeUiPlugins)?
        .add_plugin(WindowPlugin {
            primary_window: Some(Window::new(
                "nara runtime ui",
                WindowResolution::new(960, 540),
            )),
        })?
        .add_plugin(WinitPlugin::default())?
        .add_plugin(WgpuRenderPlugin)?
        .add_startup_systems(StartupStage::Scene, setup_scene)?;
    app.world_mut()?.insert_resource(AssetServer::new());

    app.run()?;
    Ok(())
}

fn setup_scene(
    mut commands: Commands,
    mut asset_server: ResMut<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
) {
    let image = import_ui_texture(&mut asset_server, &mut images, &mut states);

    commands.spawn((
        Name::new("camera"),
        Camera2d {
            clear_color: Some(Color::rgb(0.06, 0.07, 0.08)),
            viewport_height: 540.0,
            ..Camera2d::default()
        },
    ));

    let root = commands
        .spawn((Name::new("ui root"), UiRoot::primary_window()))
        .id();
    commands.spawn((
        Name::new("panel background"),
        UiNode::new(UiStyle::absolute(32.0, 32.0, 320.0, 160.0))
            .with_z_index(0)
            .clipping_children(),
        UiPanel::from_color(Color::rgba(0.12, 0.14, 0.16, 0.92)),
        Parent(root),
    ));
    commands.spawn((
        Name::new("accent image"),
        UiNode::new(UiStyle::absolute(56.0, 56.0, 96.0, 96.0)).with_z_index(1),
        UiPanel::from_image(image).with_sampler(SamplerDescriptor::NEAREST_CLAMP),
        Parent(root),
    ));
    commands.spawn((
        Name::new("status strip"),
        UiNode::new(UiStyle::absolute(168.0, 72.0, 152.0, 48.0))
            .with_z_index(2)
            .focusable(),
        UiPanel::from_color(Color::rgba(0.22, 0.58, 0.86, 0.88)),
        Parent(root),
    ));
}

fn import_ui_texture(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
) -> Handle<ImageAsset> {
    let record = AssetRecord::new(
        StableAssetId::parse_str(UI_TEXTURE_STABLE_ID).unwrap(),
        AssetPath::new("textures/ui-demo.png").unwrap(),
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
                ImportDependencyDigest::empty(),
                ImportSettingsHash::default(),
                ImportProfile::default(),
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
