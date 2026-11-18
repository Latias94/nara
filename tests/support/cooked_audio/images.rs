use nara::{
    fs::ContentDigest,
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
};
use serde_json::{Value, json};
use two_level_game::{animation::atlas_product, markers::marker_image_product};

const FRAME_COLORS: [[u8; 4]; 4] = [
    [64, 160, 240, 255],
    [32, 216, 232, 255],
    [240, 152, 48, 255],
    [112, 224, 80, 255],
];

pub fn atlas() -> ImageAsset {
    let mut pixels = Vec::with_capacity(4096);
    for y in 0..16 {
        for color in FRAME_COLORS {
            for x in 0..16 {
                let pixel = if x == 0 || x == 15 || y == 0 || y == 15 {
                    [16, 32, 48, 255]
                } else if y == 5 && (x == 5 || x == 10) {
                    [255, 255, 255, 255]
                } else {
                    color
                };
                pixels.extend_from_slice(&pixel);
            }
        }
    }
    ImageAsset::from_product(
        atlas_product(),
        ImageExtent::new(64, 16),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels,
    )
    .unwrap()
}

pub fn atlas_proof(image: &ImageAsset) -> Value {
    let expected = atlas();
    assert_eq!(image.product(), Some(atlas_product()));
    assert_eq!(image.extent(), expected.extent());
    assert_eq!(image.format(), ImageFormat::Rgba8);
    assert_eq!(image.color_space(), ImageColorSpace::Srgb);
    assert!(image.source().is_none());
    assert_eq!(image.pixels(), expected.pixels());
    assert_eq!(image.content_hash(), expected.content_hash());
    json!({
        "role": "explorer_atlas",
        "product": atlas_product().to_string(),
        "format": image.format(),
        "color_space": image.color_space(),
        "extent": image.extent(),
        "columns": 4,
        "rows": 1,
        "cell_extent": ImageExtent::new(16, 16),
        "frame_center_rgba": FRAME_COLORS,
        "pixel_bytes": image.pixels().len(),
        "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
        "content_hash": image.content_hash().as_bytes(),
        "source_free": true,
    })
}

pub fn marker_badge() -> ImageAsset {
    ImageAsset::from_product(
        marker_image_product(),
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        [255, 192, 64, 255].repeat(4),
    )
    .unwrap()
}

pub fn marker_proof(image: &ImageAsset) -> Value {
    let expected = marker_badge();
    assert_eq!(image.product(), expected.product());
    assert_eq!(image.extent(), expected.extent());
    assert_eq!(image.format(), expected.format());
    assert_eq!(image.color_space(), expected.color_space());
    assert_eq!(image.pixels(), expected.pixels());
    assert_eq!(image.content_hash(), expected.content_hash());
    assert!(image.source().is_none());
    json!({
        "role": "marker_badge",
        "product": marker_image_product().to_string(),
        "format": image.format(),
        "color_space": image.color_space(),
        "extent": image.extent(),
        "pixel_bytes": image.pixels().len(),
        "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
        "content_hash": image.content_hash().as_bytes(),
        "source_free": true,
    })
}
