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
pub struct AssetMetaIdentity {
    stable_id: StableAssetId,
    path: AssetPath,
    source_kind: AssetSourceKind,
}

impl AssetMetaIdentity {
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

    #[cfg(feature = "serde")]
    pub(crate) fn retained_bytes(&self) -> Option<usize> {
        self.path
            .retained_bytes()
            .checked_add(self.source_kind.retained_bytes())
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct AssetMeta<TSettings> {
    asset: AssetMetaIdentity,
    settings: TSettings,
}

impl<TSettings> AssetMeta<TSettings> {
    #[must_use]
    pub const fn new(asset: AssetMetaIdentity, settings: TSettings) -> Self {
        Self { asset, settings }
    }

    #[must_use]
    pub const fn identity(&self) -> &AssetMetaIdentity {
        &self.asset
    }

    #[must_use]
    pub const fn settings(&self) -> &TSettings {
        &self.settings
    }

    #[must_use]
    pub fn into_parts(self) -> (AssetMetaIdentity, TSettings) {
        (self.asset, self.settings)
    }
}

impl From<AssetMetaIdentity> for AssetRecord {
    fn from(identity: AssetMetaIdentity) -> Self {
        Self::new(identity.stable_id, identity.path, identity.source_kind)
    }
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
    pub fn insert_identity(
        &mut self,
        source_path: AssetPath,
        identity: AssetMetaIdentity,
    ) -> Result<(), AssetDatabaseError> {
        if source_path != identity.path {
            return Err(AssetDatabaseError::MetaPathMismatch {
                source_path,
                meta_path: identity.path,
            });
        }

        self.insert(identity.into())
    }

    pub fn insert(&mut self, record: AssetRecord) -> Result<(), AssetDatabaseError> {
        self.validate_insert(&record)?;

        self.by_stable_id
            .insert(record.stable_id(), record.path().clone());
        self.case_folded_paths
            .insert(record.path().case_fold_key(), record.path().clone());
        self.by_path.insert(record.path().clone(), record);
        Ok(())
    }

    /// Checks whether a record can be inserted without modifying the database.
    ///
    /// The caller must retain exclusive mutation authority until a later insertion. A successful
    /// check does not reserve the identity against another writer.
    pub fn validate_insert(&self, record: &AssetRecord) -> Result<(), AssetDatabaseError> {
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

    /// Observes registered records in deterministic source-path order.
    pub fn records(&self) -> impl ExactSizeIterator<Item = &AssetRecord> {
        self.by_path.values()
    }

    /// Removes a record only while the source path still has exactly the expected identity.
    ///
    /// This updates all catalogue indexes together. An equal record is a comparison guard, not
    /// proof of ownership; callers remain responsible for retiring only their own registrations.
    pub fn remove_exact(&mut self, expected: &AssetRecord) -> bool {
        if self.record_for_path(expected.path()) != Some(expected) {
            return false;
        }
        self.by_path.remove(expected.path());
        self.by_stable_id.remove(&expected.stable_id());
        self.case_folded_paths
            .remove(&expected.path().case_fold_key());
        true
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

#[cfg(test)]
mod insertion_tests {
    use super::*;

    #[test]
    fn exact_removal_preserves_foreign_records_and_releases_every_identity_index() {
        let own = AssetRecord::new(
            StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
            AssetPath::new("textures/Icon.png").unwrap(),
            AssetSourceKind::Image,
        );
        let foreign = AssetRecord::new(
            StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap(),
            AssetPath::new("textures/foreign.png").unwrap(),
            AssetSourceKind::Image,
        );
        let mut database = ProjectAssetDatabase::default();
        database.insert(own.clone()).unwrap();
        database.insert(foreign.clone()).unwrap();
        let wrong_kind =
            AssetRecord::new(own.stable_id(), own.path().clone(), AssetSourceKind::Scene);
        let before = database.clone();
        assert!(!database.remove_exact(&wrong_kind));
        assert_eq!(database, before);
        assert!(database.remove_exact(&own));
        assert!(!database.remove_exact(&own));
        assert_eq!(database.records().collect::<Vec<_>>(), vec![&foreign]);
        assert_eq!(database.record_for_stable_id(own.stable_id()), None);
        assert_eq!(
            database.record_for_stable_id(foreign.stable_id()),
            Some(&foreign)
        );

        let renamed = AssetRecord::new(
            own.stable_id(),
            AssetPath::new("textures/icon.png").unwrap(),
            AssetSourceKind::Image,
        );
        database.insert(renamed.clone()).unwrap();
        assert_eq!(database.record_for_path(renamed.path()), Some(&renamed));
    }

    #[test]
    fn insertion_preflight_checks_collisions_without_mutation_or_reservation() {
        let first_id = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
        let second_id = StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap();
        let first = AssetRecord::new(
            first_id,
            AssetPath::new("textures/Icon.png").unwrap(),
            AssetSourceKind::Image,
        );
        let mut database = ProjectAssetDatabase::default();
        database.insert(first.clone()).unwrap();
        let original = database.clone();
        assert!(matches!(
            database.validate_insert(&first),
            Err(AssetDatabaseError::DuplicatePath { .. })
        ));
        assert!(matches!(
            database.validate_insert(&AssetRecord::new(
                first_id,
                AssetPath::new("textures/other.png").unwrap(),
                AssetSourceKind::Image
            )),
            Err(AssetDatabaseError::DuplicateStableId { .. })
        ));
        assert!(matches!(
            database.validate_insert(&AssetRecord::new(
                second_id,
                AssetPath::new("textures/icon.png").unwrap(),
                AssetSourceKind::Image
            )),
            Err(AssetDatabaseError::CaseInsensitivePathCollision { .. })
        ));
        assert_eq!(database, original);
        let accepted = AssetRecord::new(
            second_id,
            AssetPath::new("textures/new.png").unwrap(),
            AssetSourceKind::Image,
        );
        assert_eq!(database.validate_insert(&accepted), Ok(()));
        assert_eq!(database, original);
        database.insert(accepted.clone()).unwrap();
        assert_eq!(database.record_for_stable_id(second_id), Some(&accepted));
        assert_eq!(database.len(), 2);
    }
}
