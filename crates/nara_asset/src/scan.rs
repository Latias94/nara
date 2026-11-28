use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use nara_core::{
    identity::try_uuid_v7,
    vfs::{AssetPath as VfsPath, DirAssetSource},
};

use crate::{
    AssetDatabaseError, AssetMeta, AssetMetaError, AssetRecord, AssetSourcePath,
    ProjectAssetDatabase, StableAssetId,
    meta::{
        IMAGE_IMPORTER, MATERIAL_IMPORTER, MAX_META_BYTES, PREFAB_IMPORTER, RAW_IMPORTER,
        SCENE_IMPORTER, SHADER_IMPORTER, TILESET_IMPORTER,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AssetScanMode {
    /// Reports missing sidecars without changing the project.
    #[default]
    ReadOnly,
    /// Creates missing sidecars in a trusted project.
    Discover,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetScanError {
    #[error("could not {operation} '{}': {source}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid asset entry '{}': {reason}", path.display())]
    InvalidEntry { path: PathBuf, reason: &'static str },
    #[error("asset '{path}' is missing its '.meta' sidecar")]
    MissingSidecar { path: AssetSourcePath },
    #[error("asset sidecar '{path}' has no matching regular source file with the same spelling")]
    OrphanSidecar { path: AssetSourcePath },
    #[error("invalid asset sidecar '{path}': {source}")]
    InvalidSidecar {
        path: VfsPath,
        #[source]
        source: AssetMetaError,
    },
    #[error(transparent)]
    Database(#[from] AssetDatabaseError),
}

impl ProjectAssetDatabase {
    /// Recursively indexes the selected project-relative source directories.
    ///
    /// Paths use their exact UTF-8 spelling relative to `project_root`. Missing directories are
    /// empty. Dot-prefixed entries are ignored during traversal and cannot be named as a root.
    /// Links and special files are rejected. Other regular files are sources or `.meta` sidecars.
    ///
    /// Returns the first error without a partial database. Discovery checks the inventory and
    /// all existing sidecars before writing. Each creation publishes a complete file without
    /// replacing an existing entry. A concurrent creator's sidecar is read and validated.
    /// Earlier successful creations remain on disk if a later write fails. The project should
    /// keep its source directory structure stable during the scan.
    pub fn scan(
        project_root: impl AsRef<Path>,
        roots: &[AssetSourcePath],
        mode: AssetScanMode,
    ) -> Result<Self, AssetScanError> {
        let project_root = project_root.as_ref();
        let source = DirAssetSource::new(project_root)
            .map_err(|error| io_error("open project directory", project_root, error))?;
        let files = inventory(&source, roots)?;
        let mut database = Self::default();
        let mut missing = Vec::new();

        for path in files.iter().filter(|path| is_sidecar(path.as_str())) {
            if let Some(base) = path.as_str().strip_suffix(".meta")
                && !is_sidecar(base)
                && files.contains(base)
            {
                continue;
            }
            return Err(AssetScanError::OrphanSidecar { path: path.clone() });
        }

        for path in files.iter().filter(|path| !is_sidecar(path.as_str())) {
            let sidecar = sidecar_path(path)?;
            if files.contains(sidecar.as_str()) {
                let meta = read_sidecar(&source, &sidecar)?;
                database.insert(AssetRecord::from_meta(path.clone(), meta))?;
            } else {
                missing.push((path.clone(), sidecar));
            }
        }

        for (path, sidecar) in missing {
            if mode == AssetScanMode::ReadOnly {
                return Err(AssetScanError::MissingSidecar { path });
            }
            discover_sidecar(&source, &mut database, path, &sidecar)?;
        }
        Ok(database)
    }
}

fn inventory(
    source: &DirAssetSource,
    roots: &[AssetSourcePath],
) -> Result<BTreeSet<AssetSourcePath>, AssetScanError> {
    let mut pending = BTreeSet::new();
    for root in roots.iter().collect::<BTreeSet<_>>() {
        if scan_root(source, root)? {
            pending.insert(root.clone());
        }
    }

    let mut directories = BTreeSet::new();
    let mut files = BTreeSet::new();
    while let Some(relative) = pending.pop_first() {
        if !directories.insert(relative.clone()) {
            continue;
        }
        let directory = native_path(source, relative.as_str());
        let mut entries = read_dir(&directory)?
            .collect::<io::Result<Vec<_>>>()
            .map_err(|error| io_error("list directory", &directory, error))?;
        entries.sort_by_cached_key(fs::DirEntry::file_name);
        let mut case_folded = BTreeMap::<String, AssetSourcePath>::new();
        for entry in entries {
            let name = entry.file_name();
            if name.as_encoded_bytes().starts_with(b".") {
                continue;
            }
            let name = name
                .to_str()
                .ok_or_else(|| invalid_entry(entry.path(), "expected a UTF-8 filename"))?;
            let path = logical_path(format!("{relative}/{name}"))?;
            if let Some(existing_path) = case_folded.insert(name.to_lowercase(), path.clone()) {
                return Err(AssetDatabaseError::CaseInsensitivePathCollision {
                    existing_path,
                    new_path: path,
                }
                .into());
            }
            let metadata = entry
                .metadata()
                .map_err(|error| io_error("inspect asset entry", entry.path(), error))?;
            if is_sidecar(name) {
                if !name.ends_with(".meta") || !is_regular_file(&metadata) {
                    return Err(invalid_entry(
                        entry.path(),
                        "expected a regular file with the exact suffix '.meta'",
                    ));
                }
                files.insert(path);
            } else if is_regular_file(&metadata) {
                files.insert(path);
            } else if metadata.is_dir() && !is_link(&metadata) {
                pending.insert(path);
            } else {
                return Err(invalid_entry(
                    entry.path(),
                    "expected a regular file or directory without links",
                ));
            }
        }
    }
    Ok(files)
}

fn scan_root(source: &DirAssetSource, root: &AssetSourcePath) -> Result<bool, AssetScanError> {
    validate_portable_path(root.as_str())?;
    if root.as_str().split('/').any(|part| part.starts_with('.')) {
        return Err(invalid_entry(
            root.as_str(),
            "asset roots must not name hidden directories",
        ));
    }
    let mut parent = source.root().to_path_buf();
    for name in root.as_str().split('/') {
        let Some(entry) = find_exact_entry(&parent, OsStr::new(name))? else {
            return Ok(false);
        };
        parent = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|error| io_error("inspect source directory", &parent, error))?;
        if !metadata.is_dir() || is_link(&metadata) || is_sidecar(name) {
            return Err(invalid_entry(
                parent,
                "expected a source directory without links or the reserved '.meta' suffix",
            ));
        }
    }
    Ok(true)
}

fn discover_sidecar(
    source: &DirAssetSource,
    database: &mut ProjectAssetDatabase,
    path: AssetSourcePath,
    sidecar: &VfsPath,
) -> Result<(), AssetScanError> {
    let asset = native_path(source, path.as_str());
    let metadata = fs::symlink_metadata(&asset)
        .map_err(|error| io_error("inspect asset before creating its sidecar", &asset, error))?;
    if !is_regular_file(&metadata) {
        return Err(invalid_entry(asset, "expected a regular asset file"));
    }

    let target = native_path(source, sidecar.as_str());
    let id = try_uuid_v7()
        .map(StableAssetId::from_uuid)
        .map_err(|error| io_error("generate asset UUID", &target, error))?;
    let meta = AssetMeta::generated(id, default_importer(&path))
        .map_err(|error| io_error("encode asset sidecar", &target, io::Error::other(error)))?;
    let proposed = AssetRecord::from_meta(path.clone(), meta.clone());
    database.validate_insert(&proposed)?;
    if publish_sidecar(&target, meta.as_ron().as_bytes())? {
        database.insert(proposed)?;
    } else {
        let exact = match (target.parent(), target.file_name()) {
            (Some(parent), Some(name)) => find_exact_entry(parent, name)?.is_some(),
            _ => false,
        };
        if !exact {
            return Err(invalid_entry(
                target,
                "concurrent sidecar did not have the exact expected spelling",
            ));
        }
        database.insert(AssetRecord::from_meta(path, read_sidecar(source, sidecar)?))?;
    }
    Ok(())
}

fn publish_sidecar(target: &Path, bytes: &[u8]) -> Result<bool, AssetScanError> {
    fn publish(target: &Path, bytes: &[u8]) -> io::Result<bool> {
        let parent = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
        if fs::canonicalize(parent)? != parent {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "sidecar parent no longer matches the scanned directory",
            ));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".nara-meta-")
            .tempfile_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(target) {
            Ok(_) => Ok(true),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error.error),
        }
    }

    publish(target, bytes).map_err(|error| io_error("create asset sidecar", target, error))
}

fn read_sidecar(source: &DirAssetSource, path: &VfsPath) -> Result<AssetMeta, AssetScanError> {
    let native = native_path(source, path.as_str());
    let io = |error| io_error("read asset sidecar", &native, error);
    let metadata = fs::symlink_metadata(&native).map_err(io)?;
    if !is_regular_file(&metadata) {
        return Err(invalid_entry(native, "expected a regular asset sidecar"));
    }
    let mut bytes = Vec::new();
    source
        .open(path)
        .map_err(io)?
        .take(MAX_META_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    let parse = || {
        if bytes.len() > MAX_META_BYTES {
            return Err(AssetMetaError::TooLarge);
        }
        let text =
            String::from_utf8(bytes).map_err(|error| AssetMetaError::Utf8(error.utf8_error()))?;
        AssetMeta::from_ron(text)
    };
    parse().map_err(|source| AssetScanError::InvalidSidecar {
        path: path.clone(),
        source,
    })
}

fn default_importer(path: &AssetSourcePath) -> &'static str {
    let name = path.as_str().to_ascii_lowercase();
    if name.ends_with(".png") {
        IMAGE_IMPORTER
    } else if name.ends_with(".ogg") || name.ends_with(".wav") {
        crate::meta::AUDIO_IMPORTER
    } else if name.ends_with(".ttf") || name.ends_with(".otf") {
        crate::meta::FONT_IMPORTER
    } else if name.ends_with(".scene.ron") {
        SCENE_IMPORTER
    } else if name.ends_with(".prefab.ron") {
        PREFAB_IMPORTER
    } else if name.ends_with(".material.ron") {
        MATERIAL_IMPORTER
    } else if name.ends_with(".tileset.ron") {
        TILESET_IMPORTER
    } else if name.ends_with(".wgsl") {
        SHADER_IMPORTER
    } else {
        RAW_IMPORTER
    }
}

fn is_sidecar(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("meta"))
}

fn sidecar_path(path: &AssetSourcePath) -> Result<VfsPath, AssetScanError> {
    let sidecar = path.meta_path();
    VfsPath::new(sidecar.as_str())
        .map_err(|_| invalid_entry(sidecar, "expected a portable project-relative sidecar path"))
}

fn logical_path(path: String) -> Result<AssetSourcePath, AssetScanError> {
    validate_portable_path(&path)?;
    AssetSourcePath::new(path.as_str())
        .map_err(|_| invalid_entry(path, "expected a portable project-relative asset path"))
}

fn validate_portable_path(path: &str) -> Result<(), AssetScanError> {
    VfsPath::new(path)
        .map(|_| ())
        .map_err(|_| invalid_entry(path, "expected a portable project-relative asset path"))
}

fn native_path(source: &DirAssetSource, relative: &str) -> PathBuf {
    let mut path = source.root().to_path_buf();
    path.extend(relative.split('/'));
    path
}

fn read_dir(path: &Path) -> Result<fs::ReadDir, AssetScanError> {
    fs::read_dir(path).map_err(|error| io_error("list directory", path, error))
}

fn find_exact_entry(
    directory: &Path,
    name: &OsStr,
) -> Result<Option<fs::DirEntry>, AssetScanError> {
    for entry in read_dir(directory)? {
        let entry = entry.map_err(|error| io_error("list directory", directory, error))?;
        if entry.file_name() == name {
            return Ok(Some(entry));
        }
    }
    Ok(None)
}

fn io_error(
    operation: &'static str,
    path: impl Into<PathBuf>,
    source: io::Error,
) -> AssetScanError {
    AssetScanError::Io {
        operation,
        path: path.into(),
        source,
    }
}

fn invalid_entry(path: impl Into<PathBuf>, reason: &'static str) -> AssetScanError {
    AssetScanError::InvalidEntry {
        path: path.into(),
        reason,
    }
}

fn is_regular_file(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && !is_link(metadata)
}

pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discovery_race_adopts_the_winner_and_cleans_up_its_own_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let path = AssetSourcePath::new("ship.png").unwrap();
        let sidecar = sidecar_path(&path).unwrap();
        fs::write(root.path().join(path.as_str()), []).unwrap();
        let winner = concat!(
            "AssetMeta(id: \"0193a100-1111-7000-8000-000000000001\", ",
            "importer: \"nara.image\", settings: ())\n"
        );
        fs::write(root.path().join(sidecar.as_str()), winner).unwrap();

        let source = DirAssetSource::new(root.path()).unwrap();
        let mut database = ProjectAssetDatabase::default();
        discover_sidecar(&source, &mut database, path.clone(), &sidecar).unwrap();
        let record = database.record_for_path(&path).unwrap();
        assert_eq!(record.meta().unwrap().as_ron(), winner);
        assert_eq!(
            fs::read_to_string(root.path().join(sidecar.as_str())).unwrap(),
            winner
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
