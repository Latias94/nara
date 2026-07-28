use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    path::{Component, Path},
};

use nara_ecs::Resource;

use crate::{AssetPath, AssetRef, StableAssetId};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AssetSourceKind {
    Unknown,
    Image,
    Scene,
    Prefab,
    Other(String),
}

impl Display for AssetSourceKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("unknown"),
            Self::Image => formatter.write_str("image"),
            Self::Scene => formatter.write_str("scene"),
            Self::Prefab => formatter.write_str("prefab"),
            Self::Other(kind) => formatter.write_str(kind),
        }
    }
}

impl AssetSourceKind {
    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Other(kind) => kind.capacity(),
            Self::Unknown | Self::Image | Self::Scene | Self::Prefab => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct AssetMeta {
    pub stable_id: StableAssetId,
    pub path: AssetPath,
    pub source_kind: AssetSourceKind,
}

impl AssetMeta {
    #[must_use]
    pub const fn new(
        stable_id: StableAssetId,
        path: AssetPath,
        source_kind: AssetSourceKind,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    stable_id: StableAssetId,
    path: AssetPath,
    source_kind: AssetSourceKind,
}

impl AssetRecord {
    #[must_use]
    pub const fn new(
        stable_id: StableAssetId,
        path: AssetPath,
        source_kind: AssetSourceKind,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_kind,
        }
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        &self.source_kind
    }

    pub(crate) fn retained_bytes(&self) -> Option<usize> {
        self.path
            .retained_bytes()
            .checked_add(self.source_kind.retained_bytes())
    }
}

impl From<AssetMeta> for AssetRecord {
    fn from(meta: AssetMeta) -> Self {
        Self::new(meta.stable_id, meta.path, meta.source_kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingMetaPolicy {
    Diagnose,
    Generate {
        stable_id: StableAssetId,
        source_kind: AssetSourceKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetDatabaseError {
    DuplicatePath {
        path: AssetPath,
    },
    DuplicateStableId {
        stable_id: StableAssetId,
        existing_path: AssetPath,
        new_path: AssetPath,
    },
    CaseInsensitivePathCollision {
        existing_path: AssetPath,
        new_path: AssetPath,
    },
    MetaPathMismatch {
        source_path: AssetPath,
        meta_path: AssetPath,
    },
    MissingMeta {
        path: AssetPath,
    },
    UnknownPath(AssetPath),
    UnknownStableId(StableAssetId),
    NonUtf8Path,
    SourceOutsideRoot,
    Filesystem(String),
}

impl Display for AssetDatabaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePath { path } => {
                write!(formatter, "asset path '{path}' is already registered")
            }
            Self::DuplicateStableId {
                stable_id,
                existing_path,
                new_path,
            } => write!(
                formatter,
                "stable asset id '{stable_id}' is already registered for '{existing_path}', not '{new_path}'"
            ),
            Self::CaseInsensitivePathCollision {
                existing_path,
                new_path,
            } => write!(
                formatter,
                "asset paths '{existing_path}' and '{new_path}' collide on case-insensitive filesystems"
            ),
            Self::MetaPathMismatch {
                source_path,
                meta_path,
            } => write!(
                formatter,
                "asset meta path '{meta_path}' does not match source path '{source_path}'"
            ),
            Self::MissingMeta { path } => write!(formatter, "asset path '{path}' is missing .meta"),
            Self::UnknownPath(path) => write!(formatter, "asset path '{path}' is not registered"),
            Self::UnknownStableId(id) => {
                write!(formatter, "stable asset id '{id}' is not registered")
            }
            Self::NonUtf8Path => formatter.write_str("asset source path is not valid UTF-8"),
            Self::SourceOutsideRoot => {
                formatter.write_str("asset source path is outside the asset root")
            }
            Self::Filesystem(error) => write!(formatter, "asset filesystem error: {error}"),
        }
    }
}

impl Error for AssetDatabaseError {}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct ProjectAssetDatabase {
    by_path: BTreeMap<AssetPath, AssetRecord>,
    by_stable_id: BTreeMap<StableAssetId, AssetPath>,
    case_folded_paths: BTreeMap<String, AssetPath>,
}

impl ProjectAssetDatabase {
    pub fn insert_source_with_meta_policy(
        &mut self,
        source_path: AssetPath,
        meta: Option<AssetMeta>,
        missing_meta_policy: MissingMetaPolicy,
    ) -> Result<(), AssetDatabaseError> {
        match meta {
            Some(meta) => self.insert_meta(source_path, meta),
            None => match missing_meta_policy {
                MissingMetaPolicy::Diagnose => {
                    Err(AssetDatabaseError::MissingMeta { path: source_path })
                }
                MissingMetaPolicy::Generate {
                    stable_id,
                    source_kind,
                } => self.insert(AssetRecord::new(stable_id, source_path, source_kind)),
            },
        }
    }

    pub fn insert_meta(
        &mut self,
        source_path: AssetPath,
        meta: AssetMeta,
    ) -> Result<(), AssetDatabaseError> {
        if source_path != meta.path {
            return Err(AssetDatabaseError::MetaPathMismatch {
                source_path,
                meta_path: meta.path,
            });
        }

        self.insert(meta.into())
    }

    pub fn insert(&mut self, record: AssetRecord) -> Result<(), AssetDatabaseError> {
        if self.by_path.contains_key(record.path()) {
            return Err(AssetDatabaseError::DuplicatePath {
                path: record.path().clone(),
            });
        }

        if let Some(existing_path) = self.by_stable_id.get(&record.stable_id()).cloned() {
            return Err(AssetDatabaseError::DuplicateStableId {
                stable_id: record.stable_id(),
                existing_path,
                new_path: record.path().clone(),
            });
        }

        let case_key = record.path().case_fold_key();
        if let Some(existing_path) = self.case_folded_paths.get(&case_key).cloned()
            && existing_path != *record.path()
        {
            return Err(AssetDatabaseError::CaseInsensitivePathCollision {
                existing_path,
                new_path: record.path().clone(),
            });
        }

        self.by_stable_id
            .insert(record.stable_id(), record.path().clone());
        self.case_folded_paths
            .insert(case_key, record.path().clone());
        self.by_path.insert(record.path().clone(), record);
        Ok(())
    }

    #[must_use]
    pub fn record_for_path(&self, path: &AssetPath) -> Option<&AssetRecord> {
        self.by_path.get(path)
    }

    #[must_use]
    pub fn record_for_stable_id(&self, stable_id: StableAssetId) -> Option<&AssetRecord> {
        self.by_stable_id
            .get(&stable_id)
            .and_then(|path| self.by_path.get(path))
    }

    pub fn resolve_ref(&self, asset_ref: &AssetRef) -> Result<&AssetRecord, AssetDatabaseError> {
        match asset_ref {
            AssetRef::Path(path) => self
                .record_for_path(path)
                .ok_or_else(|| AssetDatabaseError::UnknownPath(path.clone())),
            AssetRef::StableId(id) => self
                .record_for_stable_id(*id)
                .ok_or(AssetDatabaseError::UnknownStableId(*id)),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    pub fn logical_path_from_source_path(
        asset_root: impl AsRef<Path>,
        source_path: impl AsRef<Path>,
    ) -> Result<AssetPath, AssetDatabaseError> {
        let asset_root = asset_root
            .as_ref()
            .canonicalize()
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))?;
        let source_path = source_path
            .as_ref()
            .canonicalize()
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))?;
        let relative = source_path
            .strip_prefix(&asset_root)
            .map_err(|_| AssetDatabaseError::SourceOutsideRoot)?;

        let mut segments = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(segment) => {
                    let Some(segment) = segment.to_str() else {
                        return Err(AssetDatabaseError::NonUtf8Path);
                    };
                    segments.push(segment);
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AssetDatabaseError::SourceOutsideRoot);
                }
            }
        }

        AssetPath::new(segments.join("/"))
            .map_err(|error| AssetDatabaseError::Filesystem(error.to_string()))
    }
}
