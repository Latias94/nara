use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use super::{AssetPath, AssetSource, is_portable_component};

/// A native byte source for a trusted development directory.
///
/// Resolved paths must stay under the root. Lookups use the host filesystem's case rules.
/// The project must control the directory during lookups and reads.
#[derive(Debug, Clone)]
pub struct DirAssetSource {
    root: PathBuf,
}

impl DirAssetSource {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        Ok(Self { root })
    }

    /// Opens a regular file for streaming or a caller-bounded read.
    pub fn open(&self, path: &AssetPath) -> io::Result<fs::File> {
        fs::File::open(self.resolve_file(path)?)
    }

    fn resolve(&self, path: &AssetPath) -> io::Result<(PathBuf, fs::FileType)> {
        let mut candidate = self.root.clone();
        candidate.extend(path.as_str().split_terminator('/'));
        let resolved = fs::canonicalize(candidate)?;
        if !resolved.starts_with(&self.root) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let kind = fs::metadata(&resolved)?.file_type();
        Ok((resolved, kind))
    }

    fn resolve_file(&self, path: &AssetPath) -> io::Result<PathBuf> {
        let (resolved, kind) = self.resolve(path)?;
        if kind.is_dir() {
            return Err(io::ErrorKind::IsADirectory.into());
        }
        if !kind.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a regular file",
            ));
        }
        Ok(resolved)
    }
}

impl AssetSource for DirAssetSource {
    fn read_bytes(&self, path: &AssetPath) -> io::Result<Vec<u8>> {
        fs::read(self.resolve_file(path)?)
    }

    fn exists(&self, path: &AssetPath) -> bool {
        self.resolve(path)
            .is_ok_and(|(_, kind)| kind.is_file() || kind.is_dir())
    }

    fn list_dir(&self, dir: &AssetPath) -> io::Result<Vec<AssetPath>> {
        let (resolved, kind) = self.resolve(dir)?;
        if !kind.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        let mut children = Vec::new();
        for entry in fs::read_dir(resolved)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| is_portable_component(name)) else {
                continue;
            };
            let kind = entry.file_type()?;
            let child = dir.child(name);
            if kind.is_dir() || kind.is_file() || (kind.is_symlink() && self.exists(&child)) {
                children.push(child);
            }
        }
        children.sort_unstable();
        Ok(children)
    }
}

/// Creates or replaces a file in a trusted directory using a same-directory rename.
///
/// The parent must already exist. The new file is synced before publication and receives
/// the temporary file's metadata. Crash durability of the directory entry requires a
/// separate directory sync. Sharing violations and other rename failures are returned.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let spelling = path.as_os_str().as_encoded_bytes();
    let name = path
        .file_name()
        .filter(|name| !spelling.contains(&0) && spelling.ends_with(name.as_encoded_bytes()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "expected a file path"))?;
    #[cfg(windows)]
    if !name.to_str().is_some_and(is_portable_component) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a portable filename",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)?;
    let target = parent.join(name);
    let staging_suffix = if target
        .extension()
        .is_some_and(|ext| ext.as_encoded_bytes().eq_ignore_ascii_case(b"tmp"))
    {
        ".new"
    } else {
        ".tmp"
    };
    let mut temporary = tempfile::Builder::new()
        .prefix(".nara-write-")
        .suffix(staging_suffix)
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(target)?;
    Ok(())
}
