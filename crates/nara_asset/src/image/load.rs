use std::{
    io::{self, Read},
    path::Path,
};

use nara_core::vfs::{AssetPath, DirAssetSource};
use nara_ecs::{Resource, World};

use crate::plugin::{ensure_asset_plugin, publish_asset};
use crate::{
    AddAssetError, AssetPathError, AssetServer, AssetSourcePath, AssetStates, Assets, Handle,
};

use super::decode::MAX_PNG_BYTES;
use super::{ImageAsset, ImageColorSpace, ImageDecodeError};

/// The directory for native `load_image` calls. The default is `assets` in the current directory.
#[derive(Debug, Resource)]
pub struct ImageSourceDirectory {
    source: DirAssetSource,
}

impl ImageSourceDirectory {
    pub fn new(directory: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            source: DirAssetSource::new(directory)?,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.source.root()
    }

    pub(crate) fn read_image(
        &self,
        path: &AssetSourcePath,
        color_space: ImageColorSpace,
    ) -> Result<ImageAsset, ImageLoadError> {
        let path = AssetPath::new(path.as_str())?;
        let file = self.source.open(&path)?;
        if file.metadata()?.len() > MAX_PNG_BYTES as u64 {
            return Err(ImageDecodeError::EncodedTooLarge {
                limit: MAX_PNG_BYTES,
            }
            .into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_PNG_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok(ImageAsset::from_png(&bytes, color_space)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImageLoadError {
    #[error(transparent)]
    InvalidPath(#[from] AssetPathError),
    #[error("could not read image source: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Decode(#[from] ImageDecodeError),
    #[error(transparent)]
    Asset(#[from] AddAssetError),
}

/// Loads an sRGB PNG synchronously and reuses the handle for repeated loads of the same path.
///
/// `path` is relative to [`ImageSourceDirectory`]. Reads and failed decodes leave the current
/// image store unchanged. For embedded bytes use [`ImageAsset::from_png`] and [`crate::add_asset`].
pub fn load_image(
    world: &mut World,
    path: impl AsRef<str>,
) -> Result<Handle<ImageAsset>, ImageLoadError> {
    ensure_asset_plugin(world)?;
    let path = AssetSourcePath::new(path.as_ref())?;
    let server = world.resource::<AssetServer>();
    if let Some(id) = server.id_for_path(&path) {
        server
            .validate_type_claim::<ImageAsset>(id)
            .map_err(AddAssetError::from)?;
        if let Some(handle) = server.get_handle::<ImageAsset>(id)
            && let Some(images) = world.get_resource::<Assets<ImageAsset>>()
            && world.resource::<AssetStates>().published_slot_revision(id)
                == Some(&images.slot_revision(handle))
        {
            return Ok(handle);
        }
    }
    if !world.contains_resource::<ImageSourceDirectory>() {
        world.insert_resource(ImageSourceDirectory::new("assets")?);
    }
    let image = world
        .resource::<ImageSourceDirectory>()
        .read_image(&path, ImageColorSpace::Srgb)?;
    let handle = world
        .resource_mut::<AssetServer>()
        .reserve_source(path)
        .map_err(AddAssetError::from)?;
    publish_asset(world, handle, image).map_err(AddAssetError::from)?;
    Ok(handle)
}
