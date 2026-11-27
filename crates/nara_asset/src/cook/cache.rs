use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use nara_core::vfs::DirAssetSource;

use crate::{cooked::MAX_NPAK_ENTRY_BYTES, scan::is_link};

const CACHE_MAGIC: &[u8; 4] = b"NIC1";
const HEADER_BYTES: usize = CACHE_MAGIC.len() + blake3::OUT_LEN;

/// An import key scoped to source bytes, sidecar bytes, importer version, and target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportCacheKey(blake3::Hash);

impl ImportCacheKey {
    #[must_use]
    pub fn new(source: &[u8], meta: &[u8], importer_version: &str, target: &str) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(b"nara.import-cache.key.v1\0");
        for bytes in [source, meta, importer_version.as_bytes(), target.as_bytes()] {
            hash.update(&(bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Self(hash.finalize())
    }

    fn filename(self) -> String {
        format!("asset-{}.bin", self.0.to_hex())
    }

    fn checksum(self, payload: &[u8]) -> blake3::Hash {
        let mut hash = blake3::Hasher::new();
        hash.update(b"nara.import-cache.payload.v1\0");
        hash.update(self.0.as_bytes());
        hash.update(payload);
        hash.finalize()
    }
}

/// A project-local cache of opaque import artifacts under `.nara/cache/`.
///
/// The project must control its directory structure while the cache operates. Cache directories
/// and entries reject links and special files. Concurrent producers can compute the same key,
/// but publication is atomic and the first complete result wins.
#[derive(Debug, Clone)]
pub struct ImportCache {
    project_root: PathBuf,
}

impl ImportCache {
    /// Opens or creates the cache directories beneath the canonical project root.
    pub fn new(project_root: impl AsRef<Path>) -> io::Result<Self> {
        let project_root = DirAssetSource::new(project_root)?.root().to_path_buf();
        let state = project_root.join(".nara");
        create_cache_directory(&state)?;
        create_cache_directory(&state.join("cache"))?;
        let cache = Self { project_root };
        cache.directory()?;
        Ok(cache)
    }

    /// Reads and verifies one cached artifact. A missing entry returns `None`.
    pub fn read(&self, key: ImportCacheKey) -> io::Result<Option<Vec<u8>>> {
        read_cached(&self.directory()?.join(key.filename()), key)
    }

    /// Runs `produce` on a miss and publishes its successful result without clobbering a winner.
    ///
    /// The result is bounded to one NPAK entry. Errors from the producer leave the key vacant
    /// unless another producer successfully publishes it.
    pub fn get_or_insert_with<E>(
        &self,
        key: ImportCacheKey,
        produce: impl FnOnce() -> Result<Vec<u8>, E>,
    ) -> Result<Vec<u8>, ImportCacheError<E>> {
        if let Some(bytes) = self.read(key)? {
            return Ok(bytes);
        }
        let bytes = produce().map_err(ImportCacheError::Produce)?;
        if bytes.len() > MAX_NPAK_ENTRY_BYTES {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "import artifact is too large").into(),
            );
        }
        let directory = self.directory()?;
        let path = directory.join(key.filename());
        let mut staging = tempfile::Builder::new()
            .prefix(".nara-cache-")
            .tempfile_in(directory)?;
        staging.write_all(CACHE_MAGIC)?;
        staging.write_all(key.checksum(&bytes).as_bytes())?;
        staging.write_all(&bytes)?;
        staging.as_file().sync_all()?;
        match staging.persist_noclobber(&path) {
            Ok(_) => Ok(bytes),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                self.read(key)?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "cache winner disappeared").into()
                })
            }
            Err(error) => Err(error.error.into()),
        }
    }

    fn directory(&self) -> io::Result<PathBuf> {
        let state = self.project_root.join(".nara");
        check_cache_directory(&state)?;
        let cache = state.join("cache");
        check_cache_directory(&cache)?;
        if !fs::canonicalize(&cache)?.starts_with(&self.project_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cache directory escapes the project root",
            ));
        }
        Ok(cache)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportCacheError<E> {
    #[error("could not access the import cache: {0}")]
    Io(#[from] io::Error),
    #[error("could not produce the import artifact: {0}")]
    Produce(E),
}

fn create_cache_directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    check_cache_directory(path)
}

fn check_cache_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "cache directory '{}' must be a real directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn read_cached(path: &Path, key: ImportCacheKey) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || is_link(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("cache entry '{}' must be a regular file", path.display()),
        ));
    }
    let mut file = File::open(path)?;
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid cached import artifact '{}'", path.display()),
        )
    };
    let len = file
        .metadata()?
        .len()
        .checked_sub(HEADER_BYTES as u64)
        .filter(|len| *len <= MAX_NPAK_ENTRY_BYTES as u64)
        .ok_or_else(invalid)? as usize;
    let mut header = [0; HEADER_BYTES];
    file.read_exact(&mut header)?;
    if &header[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(io::Error::other)?;
    bytes.resize(len, 0);
    file.read_exact(&mut bytes)?;
    if file.read(&mut [0])? != 0 || &header[CACHE_MAGIC.len()..] != key.checksum(&bytes).as_bytes()
    {
        return Err(invalid());
    }
    Ok(Some(bytes))
}
