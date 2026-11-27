//! Stable asset IDs in a flat virtual byte source.

use std::io;

use nara_core::{
    AssetId,
    vfs::{AssetPath, AssetSource},
};

/// Returns the canonical lower-case, hyphenated UUID path at the source root.
#[must_use]
pub fn asset_id_path(id: AssetId) -> AssetPath {
    AssetPath::new(id.to_string()).expect("a UUID is a portable filename")
}

/// Reads an asset by stable ID using a directory, archive, or memory source.
pub fn read_asset(source: &dyn AssetSource, id: AssetId) -> io::Result<Vec<u8>> {
    source.read_bytes(&asset_id_path(id))
}

#[cfg(any(feature = "cooked", unix, windows))]
pub(crate) fn lookup_id<T>(
    path: &AssetPath,
    lookup: impl FnOnce(AssetId) -> Option<T>,
) -> io::Result<T> {
    let path = path.as_str();
    if path.is_empty() {
        return Err(io::ErrorKind::IsADirectory.into());
    }
    let (name, child) = path
        .split_once('/')
        .map_or((path, false), |(name, _)| (name, true));
    let id: AssetId = name.parse().map_err(|_| io::ErrorKind::NotFound)?;
    if id.as_uuid().hyphenated().encode_lower(&mut [0; 36]) != name {
        return Err(io::ErrorKind::NotFound.into());
    }
    let entry = lookup(id).ok_or(io::ErrorKind::NotFound)?;
    if child {
        return Err(io::ErrorKind::NotADirectory.into());
    }
    Ok(entry)
}

#[cfg(any(feature = "cooked", unix, windows))]
pub(crate) fn check_root_dir<T>(
    path: &AssetPath,
    lookup: impl FnOnce(AssetId) -> Option<T>,
) -> io::Result<()> {
    if path == &AssetPath::ROOT {
        return Ok(());
    }
    lookup_id(path, lookup)?;
    Err(io::ErrorKind::NotADirectory.into())
}

#[cfg(any(unix, windows))]
mod project;
#[cfg(any(unix, windows))]
pub use project::ProjectAssetSource;
