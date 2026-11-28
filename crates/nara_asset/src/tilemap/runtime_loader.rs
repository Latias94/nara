use crate::{AssetError, AssetLoadContext, AssetLoader, NpakEntryKind, RuntimeAssetMode};

use super::{TileSetAsset, TileSetAssetError};

/// Loads tile set metadata and checks the referenced image's kind when indexed by the source.
/// Consumers can request the image independently with its own typed loader.
#[derive(Debug, Default, Clone, Copy)]
pub struct TileSetLoader;

impl AssetLoader for TileSetLoader {
    type Asset = TileSetAsset;
    type Error = TileSetLoaderError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::TileSet)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<TileSetAsset, Self::Error> {
        let asset = match context.mode() {
            RuntimeAssetMode::Cooked => TileSetAsset::from_cooked(bytes)?,
            #[cfg(feature = "authoring")]
            RuntimeAssetMode::Source => {
                super::source::validate_source_len(bytes.len())?;
                let source = std::str::from_utf8(bytes).map_err(TileSetLoaderError::SourceUtf8)?;
                TileSetAsset::from_ron(source)?
            }
            #[cfg(not(feature = "authoring"))]
            RuntimeAssetMode::Source => return Err(TileSetLoaderError::SourceDisabled),
        };
        context.validate_dependency_kind(asset.image, NpakEntryKind::Image)?;
        Ok(asset)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TileSetLoaderError {
    #[cfg(not(feature = "authoring"))]
    #[error("loading source tile sets requires the 'authoring' feature")]
    SourceDisabled,
    #[cfg(feature = "authoring")]
    #[error("tile set source is not UTF-8")]
    SourceUtf8(#[source] std::str::Utf8Error),
    #[error(transparent)]
    TileSet(#[from] TileSetAssetError),
    #[error(transparent)]
    ImageKind(#[from] AssetError),
}
