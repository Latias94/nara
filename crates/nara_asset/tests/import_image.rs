#![cfg(all(feature = "authoring", any(feature = "image-import", feature = "cook")))]

use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use nara_asset::{
    AssetMeta,
    image::{
        ImageColorSpace, ImageImportError, ImageImportFilter, ImageImportSettings,
        decode_png_source,
    },
};

fn sidecar(importer: &str, settings: &str) -> AssetMeta {
    AssetMeta::from_ron(format!(
        "AssetMeta(id:\"0193a100-1111-7000-8000-000000000001\",importer:\"{importer}\",settings:{settings})"
    ))
    .unwrap()
}

#[test]
fn png_import_obeys_source_color_profile_in_development_and_cook() {
    let pixels = [51, 99, 131, 189];
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(&pixels, 1, 1, ExtendedColorType::Rgba8)
        .unwrap();
    let default = sidecar("nara.image", "()");
    let linear = sidecar("nara.image", "(srgb: false, filter: Linear)");
    assert_eq!(
        ImageImportSettings::from_meta(&linear).unwrap().filter,
        ImageImportFilter::Linear
    );
    for (meta, expected) in [
        (default, ImageColorSpace::Srgb),
        (linear, ImageColorSpace::Linear),
    ] {
        let imported = decode_png_source(&encoded, &meta).unwrap();
        assert_eq!(imported.pixels(), pixels);
        assert_eq!(imported.color_space(), expected);
        #[cfg(feature = "cook")]
        {
            let cooked = nara_asset::cook::cook_png(&encoded, &meta).unwrap();
            assert_eq!(&cooked[..4], b"NIMG");
            assert_eq!(
                nara_asset::image::ImageAsset::from_cooked(&cooked).unwrap(),
                imported
            );
        }
    }
    assert!(matches!(
        decode_png_source(&encoded, &sidecar("nara.image", "(mipmaps: true)")),
        Err(ImageImportError::UnsupportedMipmaps)
    ));
    assert!(matches!(
        decode_png_source(&encoded, &sidecar("nara.image", "(sgrb: false)")),
        Err(ImageImportError::Settings(_))
    ));
    assert!(matches!(
        decode_png_source(&encoded, &sidecar("nara.raw", "()")),
        Err(ImageImportError::WrongImporter(_))
    ));
}
