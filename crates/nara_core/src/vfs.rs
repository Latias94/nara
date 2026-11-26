//! Raw asset bytes at source-relative paths.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, io,
    ops::Bound,
};

#[cfg(any(unix, windows))]
mod native;
#[cfg(any(unix, windows))]
pub use native::{DirAssetSource, write_atomic};

/// A source-relative UTF-8 path with `/` separators and portable filenames.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetPath(String);

impl AssetPath {
    /// The source directory itself, with an empty string representation.
    pub const ROOT: Self = Self(String::new());

    /// Accepts a non-empty path without normalizing its spelling.
    pub fn new(path: impl Into<String>) -> io::Result<Self> {
        let path = path.into();
        if !path.split('/').all(is_portable_component) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a non-empty portable asset path",
            ));
        }
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn child(&self, name: &str) -> Self {
        if self.0.is_empty() {
            Self(name.to_owned())
        } else {
            Self(format!("{}/{name}", self.0))
        }
    }
}

impl fmt::Display for AssetPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn is_portable_component(name: &str) -> bool {
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name
            .bytes()
            .any(|byte| byte < 32 || b"\"*/:<>?\\|".contains(&byte))
    {
        return false;
    }
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    let stem = stem.trim_end_matches(' ');
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    if reserved
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
    {
        return false;
    }
    let device_number = stem.get(3..).unwrap_or_default();
    let is_device_number = matches!(device_number.as_bytes(), [b'1'..=b'9'])
        || matches!(device_number, "¹" | "²" | "³");
    let numbered_device = stem.get(..3).is_some_and(|prefix| {
        prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
    });
    !(numbered_device && is_device_number)
}

/// A synchronous byte source. Runtime owners choose how to schedule its I/O.
pub trait AssetSource: Send + Sync {
    /// Reads a complete file. A directory returns [`io::ErrorKind::IsADirectory`].
    fn read_bytes(&self, path: &AssetPath) -> io::Result<Vec<u8>>;

    /// Checks for a file or directory. I/O failures are reported as `false`.
    fn exists(&self, path: &AssetPath) -> bool;

    /// Lists immediate children in lexicographic order, with paths relative to the source root.
    fn list_dir(&self, dir: &AssetPath) -> io::Result<Vec<AssetPath>>;
}

/// A byte source whose directory structure is inferred from its files.
#[derive(Default)]
pub struct MemoryAssetSource {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemoryAssetSource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a file and creates its parent directories as needed.
    pub fn insert(&mut self, path: &AssetPath, bytes: impl Into<Vec<u8>>) -> io::Result<()> {
        if self.is_directory(path) {
            return Err(io::ErrorKind::IsADirectory.into());
        }
        if self.has_file_ancestor(path) {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        self.files.insert(path.0.clone(), bytes.into());
        Ok(())
    }

    fn files_below(&self, dir: &AssetPath) -> impl Iterator<Item = &str> {
        let prefix = if dir == &AssetPath::ROOT {
            String::new()
        } else {
            format!("{dir}/")
        };
        self.files
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .map_while(move |(path, _)| path.strip_prefix(&prefix))
    }

    fn is_directory(&self, path: &AssetPath) -> bool {
        path == &AssetPath::ROOT || self.files_below(path).next().is_some()
    }

    fn has_file_ancestor(&self, path: &AssetPath) -> bool {
        let path = path.as_str();
        path.match_indices('/')
            .any(|(end, _)| self.files.contains_key(&path[..end]))
    }
}

impl AssetSource for MemoryAssetSource {
    fn read_bytes(&self, path: &AssetPath) -> io::Result<Vec<u8>> {
        if let Some(bytes) = self.files.get(path.as_str()) {
            return Ok(bytes.clone());
        }
        Err(if self.is_directory(path) {
            io::ErrorKind::IsADirectory
        } else if self.has_file_ancestor(path) {
            io::ErrorKind::NotADirectory
        } else {
            io::ErrorKind::NotFound
        }
        .into())
    }

    fn exists(&self, path: &AssetPath) -> bool {
        self.files.contains_key(path.as_str()) || self.is_directory(path)
    }

    fn list_dir(&self, dir: &AssetPath) -> io::Result<Vec<AssetPath>> {
        let children: BTreeSet<_> = self
            .files_below(dir)
            .map(|path| path.split_once('/').map_or(path, |(name, _)| name))
            .collect();
        if children.is_empty() && dir != &AssetPath::ROOT {
            return Err(
                if self.files.contains_key(dir.as_str()) || self.has_file_ancestor(dir) {
                    io::ErrorKind::NotADirectory
                } else {
                    io::ErrorKind::NotFound
                }
                .into(),
            );
        }
        Ok(children.into_iter().map(|name| dir.child(name)).collect())
    }
}
