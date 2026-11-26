#![cfg(feature = "image-import")]

use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use nara_app::App;
use nara_asset::image::{ImageAsset, ImageColorSpace, ImageDecodeError, ImageExtent, ImagePlugin};
use nara_asset::{AssetEvents, AssetPlugin, AssetStates, Assets, add_asset};

fn png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(pixels, width, height, ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

#[test]
fn embedded_png_is_ready_immediately_and_shared_pixels_survive_replacement() {
    let original = [1, 2, 3, 4];
    let source = png(1, 1, &original);
    let image = ImageAsset::from_png(&source, ImageColorSpace::Linear).unwrap();
    assert_eq!(image.extent(), ImageExtent::new(1, 1));
    assert_eq!(image.color_space(), ImageColorSpace::Linear);
    let pixels = image.shared_pixels();
    let mut app = App::new();
    app.add_plugins((AssetPlugin, ImagePlugin)).unwrap();
    let world = app.world_mut().unwrap();
    let handle = add_asset(world, image).unwrap();

    let states = world.resource::<AssetStates>();
    let assets = world.resource::<Assets<ImageAsset>>();
    assert!(
        states
            .state(handle.id())
            .unwrap()
            .usable_version()
            .is_some()
    );
    assert_eq!(assets.get(handle).unwrap().pixels(), original);
    assert_eq!(
        states.published_slot_revision(handle.id()),
        Some(&assets.slot_revision(handle))
    );
    let first_slot = assets.slot_revision(handle);

    world
        .resource_mut::<Assets<ImageAsset>>()
        .remove(handle)
        .unwrap();
    let replacement =
        ImageAsset::from_png(&png(1, 1, &[5, 6, 7, 8]), ImageColorSpace::Srgb).unwrap();
    let assets = &mut *world.resource_mut::<Assets<ImageAsset>>();
    assets.insert(handle, replacement);
    assert_ne!(assets.slot_revision(handle), first_slot);
    assert_eq!(pixels.as_slice(), original);
}

#[test]
fn png_decoder_limits_wire_size_and_dimensions() {
    assert!(matches!(
        ImageAsset::from_png(&vec![0; 16 * 1024 * 1024 + 1], ImageColorSpace::Srgb),
        Err(ImageDecodeError::EncodedTooLarge { .. })
    ));
    let too_wide = png(16_385, 1, &vec![0; 16_385 * 4]);
    assert!(matches!(
        ImageAsset::from_png(&too_wide, ImageColorSpace::Srgb),
        Err(ImageDecodeError::Decode(image::ImageError::Limits(_)))
    ));
}

#[cfg(any(unix, windows))]
#[test]
fn native_png_load_reuses_registered_handles_and_can_retry_after_decode_failure() {
    use nara_asset::AssetServer;
    use nara_asset::image::{ImageLoadError, ImageSourceDirectory, load_image};

    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("player.png");
    std::fs::write(&path, b"invalid PNG").unwrap();
    let mut app = App::new();
    app.insert_resource(ImageSourceDirectory::new(source.path()).unwrap())
        .unwrap();
    app.add_plugins((AssetPlugin, ImagePlugin)).unwrap();
    let world = app.world_mut().unwrap();
    let before = world.resource::<AssetServer>().clone();
    assert!(matches!(
        load_image(world, "player.png"),
        Err(ImageLoadError::Decode(_))
    ));
    assert!(matches!(
        load_image(world, "../player.png"),
        Err(ImageLoadError::InvalidPath(_))
    ));
    assert_eq!(world.resource::<AssetServer>(), &before);

    let pixels = [31, 58, 100, 123];
    std::fs::write(&path, png(1, 1, &pixels)).unwrap();
    let handle = load_image(world, "player.png").unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(load_image(world, "player.png").unwrap(), handle);
    assert_eq!(
        world
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .pixels(),
        pixels
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), 1);
}
