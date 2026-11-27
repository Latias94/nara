use std::{fs::File, io};

use nara_core::vfs::DirAssetSource;

use crate::{AssetRecord, ProjectAssetDatabase, StableAssetId};

use super::{AssetId, AssetPath, AssetSource, asset_id_path, check_root_dir, lookup_id};

/// A native development directory viewed through its project's stable-ID index.
///
/// The directory is rooted at the project. Reads use the registered path and the directory
/// source's confinement checks. Recreate the adapter with the updated index after discovery.
#[derive(Debug, Clone, Copy)]
pub struct ProjectAssetSource<'a> {
    directory: &'a DirAssetSource,
    database: &'a ProjectAssetDatabase,
}

impl<'a> ProjectAssetSource<'a> {
    #[must_use]
    pub const fn new(directory: &'a DirAssetSource, database: &'a ProjectAssetDatabase) -> Self {
        Self {
            directory,
            database,
        }
    }

    /// Opens the registered regular file for streaming or a caller-bounded read.
    pub fn open_asset(&self, id: AssetId) -> io::Result<File> {
        let record = self.record(id).ok_or(io::ErrorKind::NotFound)?;
        self.directory
            .open(&AssetPath::new(record.path().as_str())?)
    }

    fn record(&self, id: AssetId) -> Option<&'a AssetRecord> {
        self.database
            .record_for_stable_id(StableAssetId::from_uuid(id.as_uuid()))
    }
}

impl AssetSource for ProjectAssetSource<'_> {
    fn read_bytes(&self, path: &AssetPath) -> io::Result<Vec<u8>> {
        let record = lookup_id(path, |id| self.record(id))?;
        self.directory
            .read_bytes(&AssetPath::new(record.path().as_str())?)
    }

    fn exists(&self, path: &AssetPath) -> bool {
        path == &AssetPath::ROOT
            || lookup_id(path, Some)
                .and_then(|id| self.open_asset(id))
                .is_ok()
    }

    fn list_dir(&self, dir: &AssetPath) -> io::Result<Vec<AssetPath>> {
        check_root_dir(dir, |id| self.record(id))?;
        let mut paths: Vec<_> = self
            .database
            .records()
            .filter_map(|record| AssetId::try_from_uuid(record.stable_id().as_uuid()).ok())
            .map(asset_id_path)
            .collect();
        paths.sort_unstable();
        Ok(paths)
    }
}
