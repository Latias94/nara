use nara::advanced_prelude::*;

pub fn publish_rgba(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    extent: ImageExtent,
    color_space: ImageColorSpace,
    pixels: Vec<u8>,
) -> Handle<ImageAsset> {
    let image = ImageAsset::new(extent, ImageFormat::Rgba8, color_space, pixels).unwrap();
    let texture = asset_server.reserve_anonymous::<ImageAsset>().unwrap();
    images
        .commit_loaded(texture, image, states, events, None, None)
        .unwrap();
    texture
}
