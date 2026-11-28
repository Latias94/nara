use crate::{AssetLoadContext, AssetLoader, NpakEntryKind, RuntimeAssetMode};

use super::ImageAsset;

/// Loads NIMG or source PNG images according to the runtime byte source's mode.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImageAssetLoader;

impl AssetLoader for ImageAssetLoader {
    type Asset = ImageAsset;
    type Error = ImageAssetLoaderError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Image)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<ImageAsset, Self::Error> {
        match (context.mode(), bytes) {
            #[cfg(feature = "cooked")]
            (RuntimeAssetMode::Cooked, bytes) => Ok(ImageAsset::from_cooked(bytes)?),
            #[cfg(not(feature = "cooked"))]
            (RuntimeAssetMode::Cooked, _) => Err(ImageAssetLoaderError::CookedDisabled),
            #[cfg(feature = "image-import")]
            (RuntimeAssetMode::Source, bytes) => {
                #[cfg(feature = "authoring")]
                if let Some(meta) = context.meta() {
                    return Ok(super::decode_png_source(bytes, &meta)?);
                }
                Ok(ImageAsset::from_png(bytes, super::ImageColorSpace::Srgb)?)
            }
            #[cfg(not(feature = "image-import"))]
            (RuntimeAssetMode::Source, _) => Err(ImageAssetLoaderError::SourceDisabled),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImageAssetLoaderError {
    #[cfg(not(feature = "cooked"))]
    #[error("loading NIMG images requires the 'cooked' feature")]
    CookedDisabled,
    #[cfg(not(feature = "image-import"))]
    #[error("loading source PNG images requires the 'image-import' feature")]
    SourceDisabled,
    #[cfg(feature = "cooked")]
    #[error(transparent)]
    Cooked(#[from] super::CookedImageError),
    #[cfg(feature = "image-import")]
    #[error(transparent)]
    Png(#[from] super::ImageDecodeError),
    #[cfg(all(feature = "image-import", feature = "authoring"))]
    #[error(transparent)]
    Import(#[from] super::ImageImportError),
}

#[cfg(all(
    test,
    feature = "image-import",
    feature = "authoring",
    any(unix, windows)
))]
mod tests {
    use std::fs;

    use ::image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
    use nara_app::App;

    use crate::image::{ImageAsset, ImageColorSpace, ImagePlugin};
    use crate::{
        AssetAppExt, AssetPlugin, AssetServer, Assets, RuntimeAssetSource, load_asset_sync,
    };

    #[test]
    fn project_png_uses_the_sidecar_color_space() {
        const ID: &str = "0193a100-1111-7000-8000-000000000001";
        let project = tempfile::tempdir().unwrap();
        let assets = project.path().join("assets");
        fs::create_dir(&assets).unwrap();
        let pixels = [12, 78, 134, 255];
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&pixels, 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        fs::write(assets.join("pixel.png"), png).unwrap();
        fs::write(
            assets.join("pixel.png.meta"),
            format!(r#"(id: "{ID}", importer: "nara.image", settings: (srgb: false))"#),
        )
        .unwrap();

        let mut app = App::new();
        app.add_plugins((AssetPlugin, ImagePlugin)).unwrap();
        app.set_asset_source(
            RuntimeAssetSource::from_project_dir(project.path(), "assets").unwrap(),
        )
        .unwrap();
        let world = app.world_mut().unwrap();
        let path_handle = world
            .resource_mut::<AssetServer>()
            .load_path::<ImageAsset>("pixel.png")
            .unwrap();
        let handle = load_asset_sync::<ImageAsset>(world, ID.parse().unwrap()).unwrap();
        let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
        assert_eq!(handle, path_handle);
        assert_eq!(image.color_space(), ImageColorSpace::Linear);
        assert_eq!(image.pixels(), pixels);
    }
}
