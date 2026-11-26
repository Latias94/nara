use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

#[cfg(feature = "runtime")]
use nara_ecs::Resource;

use crate::{AssetPath, AssetPathScope, AssetRef, AssetSourcePath, StableAssetId};

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

/// One stable source at its canonical logical catalogue location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    stable_id: StableAssetId,
    path: AssetSourcePath,
    source_kind: AssetSourceKind,
    #[cfg(feature = "authoring")]
    meta: Option<crate::AssetMeta>,
}

impl AssetRecord {
    #[must_use]
    pub const fn new(
        stable_id: StableAssetId,
        path: AssetSourcePath,
        source_kind: AssetSourceKind,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_kind,
            #[cfg(feature = "authoring")]
            meta: None,
        }
    }

    #[cfg(all(feature = "authoring", any(unix, windows)))]
    pub(crate) fn from_meta(path: AssetSourcePath, meta: crate::AssetMeta) -> Self {
        Self {
            stable_id: meta.id(),
            path,
            source_kind: meta.source_kind(),
            meta: Some(meta),
        }
    }

    /// Returns the sidecar of a scanned record. Manually registered records have no sidecar.
    #[cfg(feature = "authoring")]
    #[must_use]
    pub const fn meta(&self) -> Option<&crate::AssetMeta> {
        self.meta.as_ref()
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetSourcePath {
        &self.path
    }

    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        &self.source_kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetDatabaseError {
    DuplicatePath {
        path: AssetSourcePath,
    },
    DuplicateStableId {
        stable_id: StableAssetId,
        existing_path: AssetSourcePath,
        new_path: AssetSourcePath,
    },
    CaseInsensitivePathCollision {
        existing_path: AssetSourcePath,
        new_path: AssetSourcePath,
    },
    UnknownPath(AssetSourcePath),
    UnknownStableId(StableAssetId),
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
            Self::UnknownPath(path) => write!(formatter, "asset path '{path}' is not registered"),
            Self::UnknownStableId(id) => {
                write!(formatter, "stable asset id '{id}' is not registered")
            }
        }
    }
}

impl Error for AssetDatabaseError {}

/// A canonical source index without an implicit authored-reference scope.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "runtime", derive(Resource))]
pub struct ProjectAssetDatabase {
    by_path: BTreeMap<AssetSourcePath, AssetRecord>,
    by_stable_id: BTreeMap<StableAssetId, AssetSourcePath>,
    case_folded_paths: BTreeMap<String, AssetSourcePath>,
}

impl ProjectAssetDatabase {
    pub fn insert(&mut self, record: AssetRecord) -> Result<(), AssetDatabaseError> {
        self.validate_insert(&record)?;

        self.by_stable_id
            .insert(record.stable_id(), record.path().clone());
        self.case_folded_paths
            .insert(record.path().as_str().to_lowercase(), record.path().clone());
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

        let case_key = record.path().as_str().to_lowercase();
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
    pub fn record_for_path(&self, path: &AssetSourcePath) -> Option<&AssetRecord> {
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
            .remove(&expected.path().as_str().to_lowercase());
        true
    }

    /// Borrows this canonical index with an explicit authored-reference scope.
    #[must_use]
    pub const fn resolver<'a>(&'a self, scope: &'a AssetPathScope) -> AssetReferenceResolver<'a> {
        AssetReferenceResolver {
            database: self,
            scope,
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
}

/// A borrowed canonical catalogue and its explicit authored-reference context.
#[derive(Debug, Clone, Copy)]
pub struct AssetReferenceResolver<'a> {
    database: &'a ProjectAssetDatabase,
    scope: &'a AssetPathScope,
}

impl<'a> AssetReferenceResolver<'a> {
    #[must_use]
    pub const fn database(self) -> &'a ProjectAssetDatabase {
        self.database
    }

    #[must_use]
    pub const fn scope(self) -> &'a AssetPathScope {
        self.scope
    }

    #[must_use]
    pub fn record_for_path(self, path: &AssetPath) -> Option<&'a AssetRecord> {
        if self.scope.prefix().is_none() {
            return self.database.by_path.get(path.as_str());
        }
        self.database.record_for_path(&self.scope.resolve(path))
    }

    /// Resolves only the enclosing source, including for an explicit product selection.
    ///
    /// A source record does not prove product membership. The concrete domain must validate
    /// the selection against its admitted value.
    pub fn resolve_ref(self, asset_ref: &AssetRef) -> Result<&'a AssetRecord, AssetDatabaseError> {
        match asset_ref {
            AssetRef::Path(path) if self.scope.prefix().is_none() => self
                .database
                .by_path
                .get(path.as_str())
                .ok_or_else(|| AssetDatabaseError::UnknownPath(self.scope.resolve(path))),
            AssetRef::Path(path) => {
                let source_path = self.scope.resolve(path);
                self.database
                    .record_for_path(&source_path)
                    .ok_or(AssetDatabaseError::UnknownPath(source_path))
            }
            AssetRef::StableId(id) => self
                .database
                .record_for_stable_id(*id)
                .ok_or(AssetDatabaseError::UnknownStableId(*id)),
            AssetRef::Product(reference) => self
                .database
                .record_for_stable_id(reference.source())
                .ok_or(AssetDatabaseError::UnknownStableId(reference.source())),
        }
    }
}

#[cfg(test)]
mod insertion_tests {
    use super::*;

    #[test]
    fn exact_removal_preserves_foreign_records_and_releases_every_identity_index() {
        let own = AssetRecord::new(
            StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
            AssetSourcePath::new("textures/Icon.png").unwrap(),
            AssetSourceKind::Image,
        );
        let foreign = AssetRecord::new(
            StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap(),
            AssetSourcePath::new("textures/foreign.png").unwrap(),
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
            AssetSourcePath::new("textures/icon.png").unwrap(),
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
            AssetSourcePath::new("textures/Icon.png").unwrap(),
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
                AssetSourcePath::new("textures/other.png").unwrap(),
                AssetSourceKind::Image
            )),
            Err(AssetDatabaseError::DuplicateStableId { .. })
        ));
        assert!(matches!(
            database.validate_insert(&AssetRecord::new(
                second_id,
                AssetSourcePath::new("textures/icon.png").unwrap(),
                AssetSourceKind::Image
            )),
            Err(AssetDatabaseError::CaseInsensitivePathCollision { .. })
        ));
        assert_eq!(database, original);
        let accepted = AssetRecord::new(
            second_id,
            AssetSourcePath::new("textures/new.png").unwrap(),
            AssetSourceKind::Image,
        );
        assert_eq!(database.validate_insert(&accepted), Ok(()));
        assert_eq!(database, original);
        database.insert(accepted.clone()).unwrap();
        assert_eq!(database.record_for_stable_id(second_id), Some(&accepted));
        assert_eq!(database.len(), 2);
    }
}
