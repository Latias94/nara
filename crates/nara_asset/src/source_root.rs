use std::path::{Component, Path, PathBuf};

use crate::{AssetDatabaseError, AssetPath, AssetPathScope, AssetSourcePath};

/// A native directory paired with its owned logical source scope.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct AssetSourceRoot {
    root: PathBuf,
    scope: AssetPathScope,
}

impl AssetSourceRoot {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, scope: AssetPathScope) -> Self {
        Self {
            root: root.into(),
            scope,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn scope(&self) -> &AssetPathScope {
        &self.scope
    }

    /// Derives a path relative to this native root, refusing locations outside its exact scope.
    pub fn source_path(&self, path: &AssetSourcePath) -> Result<PathBuf, AssetDatabaseError> {
        let local = self
            .scope
            .relative_path(path)
            .ok_or(AssetDatabaseError::SourceOutsideRoot)?;
        Ok(local
            .split('/')
            .fold(self.root.clone(), |path, segment| path.join(segment)))
    }

    /// Observes an existing native source within this root and returns its canonical location.
    pub fn logical_path_from_source_path(
        &self,
        source_path: impl AsRef<Path>,
    ) -> Result<AssetSourcePath, AssetDatabaseError> {
        let root = self
            .root
            .canonicalize()
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))?;
        let source = source_path
            .as_ref()
            .canonicalize()
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))?;
        let relative = source
            .strip_prefix(root)
            .map_err(|_| AssetDatabaseError::SourceOutsideRoot)?;

        let mut segments = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(segment) => {
                    segments.push(segment.to_str().ok_or(AssetDatabaseError::NonUtf8Path)?);
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AssetDatabaseError::SourceOutsideRoot);
                }
            }
        }
        let local = AssetPath::new(segments.join("/"))
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))?;
        Ok(self.scope.resolve(&local))
    }
}
