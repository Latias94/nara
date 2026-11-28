use nara_asset::{AssetLoadContext, AssetLoadError, AssetLoader, NpakEntryKind, RuntimeAssetMode};
use nara_core::AssetId;

use super::{Material2dBinding, SpriteMaterialAsset, SpriteMaterialAssetError};

/// A sprite material with its WGSL dependency validated and bound to the stock quad interface.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteMaterialBindingAsset(pub Material2dBinding);

/// Loads an NSMT or source sprite material and reads its shader from the same runtime source.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteMaterialLoader;

impl AssetLoader for SpriteMaterialLoader {
    type Asset = SpriteMaterialBindingAsset;
    type Error = SpriteMaterialLoaderError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Material)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<SpriteMaterialBindingAsset, Self::Error> {
        let material = match context.mode() {
            RuntimeAssetMode::Cooked => SpriteMaterialAsset::from_cooked(bytes)?,
            #[cfg(feature = "material-authoring")]
            RuntimeAssetMode::Source => SpriteMaterialAsset::from_ron(
                std::str::from_utf8(bytes).map_err(SpriteMaterialLoaderError::SourceUtf8)?,
            )?,
            #[cfg(not(feature = "material-authoring"))]
            RuntimeAssetMode::Source => return Err(SpriteMaterialLoaderError::SourceDisabled),
        };
        let id = material.shader();
        let shader = context.read_dependency(id, Some(NpakEntryKind::Shader))?;
        let shader = std::str::from_utf8(&shader)
            .map_err(|source| SpriteMaterialLoaderError::ShaderUtf8 { id, source })?;
        Ok(SpriteMaterialBindingAsset(material.bind(shader)?))
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpriteMaterialLoaderError {
    #[cfg(not(feature = "material-authoring"))]
    #[error("loading source sprite materials requires the 'material-authoring' feature")]
    SourceDisabled,
    #[cfg(feature = "material-authoring")]
    #[error("sprite material source is not UTF-8")]
    SourceUtf8(#[source] std::str::Utf8Error),
    #[error("sprite material shader '{id}' is not UTF-8")]
    ShaderUtf8 {
        id: AssetId,
        source: std::str::Utf8Error,
    },
    #[error(transparent)]
    Material(#[from] SpriteMaterialAssetError),
    #[error(transparent)]
    Dependency(#[from] AssetLoadError),
}

#[cfg(any(feature = "sprite-render", feature = "sprite-source-hydration"))]
pub(crate) fn register_sprite_material_loader(
    app: &mut nara_app::App,
) -> Result<(), nara_app::PluginError> {
    use nara_asset::{AssetAppExt, AssetServer};

    if let Some(server) = app.world().get_resource::<AssetServer>()
        && !server
            .loader_registry()
            .contains::<SpriteMaterialBindingAsset>()
    {
        app.register_asset_loader(SpriteMaterialLoader)?;
    }
    Ok(())
}
