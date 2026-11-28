use nara_asset::{AssetLoadContext, AssetLoader, NpakEntryKind, RuntimeAssetMode};
use thiserror::Error;

use super::{PropertyAnimationClip, PropertyAnimationClipError};

/// Loads NPAN property animations, with `.anim.ron` support under `authoring`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PropertyAnimationLoader;

impl AssetLoader for PropertyAnimationLoader {
    type Asset = PropertyAnimationClip;
    type Error = PropertyAnimationLoaderError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::PropertyAnimation)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        match context.mode() {
            RuntimeAssetMode::Cooked => Ok(PropertyAnimationClip::from_cooked(bytes)?),
            #[cfg(feature = "authoring")]
            RuntimeAssetMode::Source => {
                super::source::validate_source_len(bytes.len())?;
                let source = std::str::from_utf8(bytes)?;
                Ok(PropertyAnimationClip::from_ron(source)?)
            }
            #[cfg(not(feature = "authoring"))]
            RuntimeAssetMode::Source => Err(PropertyAnimationLoaderError::SourceDisabled),
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PropertyAnimationLoaderError {
    #[cfg(not(feature = "authoring"))]
    #[error("loading source property animations requires the 'authoring' feature")]
    SourceDisabled,
    #[cfg(feature = "authoring")]
    #[error("property animation source is not UTF-8")]
    SourceUtf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Clip(#[from] PropertyAnimationClipError),
}
