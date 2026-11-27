use ::image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

use super::*;

#[test]
fn png_at_the_pixel_limit_converts_16_bit_channels_to_rgba8() {
    let width = 4096;
    let height = (MAX_IMPORT_PIXELS / u64::from(width)) as u32;
    let extent = ImageExtent::new(width, height);
    assert_eq!(extent.pixel_count(), MAX_IMPORT_PIXELS);
    let mut encoded = Vec::new();
    {
        let pixel = [0x1111_u16, 0x4444, 0xaaaa, 0xffff].map(u16::to_ne_bytes);
        let pixels = pixel.as_flattened().repeat(MAX_IMPORT_PIXELS as usize);
        PngEncoder::new(&mut encoded)
            .write_image(&pixels, width, height, ExtendedColorType::Rgba16)
            .unwrap();
    }
    assert!(encoded.len() < MAX_PNG_BYTES);

    let decoded = ImageAsset::from_png(&encoded, ImageColorSpace::Srgb).unwrap();
    assert_eq!(decoded.extent(), extent);
    assert_eq!(decoded.pixels().len(), MAX_IMPORT_PIXELS as usize * 4);
    assert!(
        decoded
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0x11, 0x44, 0xaa, 0xff])
    );
}
