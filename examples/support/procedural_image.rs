use image::ImageEncoder;
use nara::advanced_prelude::*;

pub fn import_rgba(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    record: AssetRecord,
    extent: ImageExtent,
    pixels: &[u8],
    settings: ImageImportSettings,
) -> Handle<ImageAsset> {
    let mut database = ProjectAssetDatabase::default();
    database.insert(record.clone()).unwrap();
    let texture = AssetRef::StableId(record.stable_id())
        .resolve_with_database::<ImageAsset>(asset_server, &database)
        .unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            pixels,
            extent.width,
            extent.height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let imported = ImageImporter::default()
        .import_image(
            ImageBytesImportRequest::new(record, png.into_boxed_slice(), settings),
            texture,
            states.version(texture.id()).unwrap_or(AssetVersion::ZERO),
            asset_server,
            images,
            states,
        )
        .unwrap();
    imported
        .commit(asset_server, images, states, &mut AssetEvents::default())
        .unwrap();
    texture
}
