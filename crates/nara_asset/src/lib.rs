//! Asset identity, project metadata, typed handles, and in-memory asset tables.

mod artifact;
mod database;
#[cfg(feature = "serde")]
mod format;
mod identity;
#[cfg(feature = "serde")]
mod import;
mod product_identity;
mod reload;
mod revision;
mod runtime_codec;
mod server;
mod state;
mod storage;

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

pub use artifact::{
    ArtifactFormatVersion, ArtifactLabel, DigestParseError, ImportArtifactDigest,
    ImportArtifactKey, ImportArtifactPath, ImportArtifactPathError, ImportArtifactRecord,
    ImportDependency, ImportDependencyDigest, ImportDependencyRole, ImportLabelError,
    ImportLabelKind, ImportProfile, ImportSettingsHash, ImportedAssetType, ImporterId,
    ImporterVersion, SourceHash,
};
pub use database::{
    AssetDatabaseError, AssetMeta, AssetMetaIdentity, AssetRecord, AssetSourceKind,
    ProjectAssetDatabase,
};
#[cfg(feature = "serde")]
pub use format::{
    AssetMetaCandidate, AssetMetaFileLimits, AssetMetaFormatError, AssetMetaIdentityCandidate,
};
pub use identity::{
    AssetId, AssetPath, AssetPathError, AssetRef, StableAssetId, StableAssetIdError,
    StableAssetIdGenerationError,
};
#[cfg(feature = "serde")]
pub use import::{AssetImportError, AssetImportLimits, AssetImportSource, SourceAssetImporter};
pub use product_identity::{AssetProductIdentityError, AssetProductRef, ImportedProductId};
pub use reload::{
    ASSET_PLUGIN_DECLARATION, ASSET_PLUGIN_ID, ASSET_SOURCE_PLUGIN_DECLARATION,
    ASSET_SOURCE_PLUGIN_ID, AssetLoadGeneration, AssetLoadGenerations, AssetPlugin,
    AssetReloadDiagnostics, AssetReloadRequest, AssetReloadRequestAdmissionError,
    AssetReloadRequestId, AssetReloadRequestKind, AssetReloadRequestLimitKind,
    AssetReloadRequestLimits, AssetReloadRequests, AssetSourceChange,
    AssetSourceChangeAdmissionError, AssetSourceChangeKind, AssetSourceChangeLimitKind,
    AssetSourceChangeLimits, AssetSourceChanges, AssetSourcePlugin, AssetSourceRoot,
    AssetTaskUpdateSet, ImageReloadConsumer, ImageReloadDrainError, ImageReloadRegistrationError,
    register_image_reload_consumer,
};
pub use runtime_codec::{AssetDomain, AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec};
pub use server::{AssetError, AssetServer};
pub use state::{
    AssetDependencyGraph, AssetEvent, AssetEventKind, AssetEventLimits, AssetEventPushOutcome,
    AssetEvents, AssetPublicationRevision, AssetState, AssetStateError, AssetStateRevision,
    AssetStates, AssetVersion, ImageRecoveryContinuity, LoadState,
};
pub use storage::{
    Asset, AssetMutationBaseline, AssetSlotRevision, Assets, Handle, allocate_anonymous_handle,
};

/// Selects the export form for source-bound primary products.
///
/// Auxiliary products and primary products without source paths always preserve their complete
/// [`AssetRef::Product`] identity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AssetRefExportPolicy {
    #[default]
    Path,
    StableIdWhenKnown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRefError {
    InvalidPath(AssetPathError),
    MissingProjectDatabase(StableAssetId),
    UnsupportedProductSelection(AssetProductRef),
    UnexpectedSourceKind {
        asset_ref: String,
        expected: AssetSourceKind,
        actual: AssetSourceKind,
    },
    UnknownHandle(AssetId),
    Database(AssetDatabaseError),
    Asset(AssetError),
}

impl Display for AssetRefError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
            Self::MissingProjectDatabase(id) => {
                write!(
                    formatter,
                    "stable asset id '{id}' requires a project asset database"
                )
            }
            Self::UnsupportedProductSelection(reference) => write!(
                formatter,
                "asset product '{reference}' cannot be selected by a primary runtime handle"
            ),
            Self::UnexpectedSourceKind {
                asset_ref,
                expected,
                actual,
            } => write!(
                formatter,
                "asset reference '{asset_ref}' expected source kind '{expected}', got '{actual}'"
            ),
            Self::UnknownHandle(id) => write!(formatter, "asset handle {:?} has no known path", id),
            Self::Database(error) => Display::fmt(error, formatter),
            Self::Asset(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for AssetRefError {}

impl From<AssetPathError> for AssetRefError {
    fn from(error: AssetPathError) -> Self {
        Self::InvalidPath(error)
    }
}

impl From<AssetError> for AssetRefError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl From<AssetDatabaseError> for AssetRefError {
    fn from(error: AssetDatabaseError) -> Self {
        Self::Database(error)
    }
}

impl AssetRef {
    /// Resolves an existing complete product binding or reserves a source-path handle.
    ///
    /// Product lookup supports primary and auxiliary selections without a source database. Unknown
    /// products reject without allocating an identity. Stable source IDs require a real project
    /// database. A successful resolution proves no actual content existence or residency.
    pub fn resolve<T: 'static>(
        &self,
        asset_server: &mut AssetServer,
    ) -> Result<Handle<T>, AssetRefError> {
        match self {
            Self::Path(path) => asset_server.reserve::<T>(path.as_str()).map_err(Into::into),
            Self::StableId(id) => Err(AssetRefError::MissingProjectDatabase(*id)),
            Self::Product(reference) => {
                asset_server.resolve_product(*reference).map_err(Into::into)
            }
        }
    }

    /// Resolves a primary product handle through its registered source.
    ///
    /// A source record proves only its primary product. Auxiliary selections reject before any
    /// allocation or binding, even when the server already has their complete product identity.
    pub fn resolve_with_database<T: 'static>(
        &self,
        asset_server: &mut AssetServer,
        database: &ProjectAssetDatabase,
    ) -> Result<Handle<T>, AssetRefError> {
        self.validate_primary_selection()?;
        let record = database.resolve_ref(self)?;
        asset_server.reserve_record(record).map_err(Into::into)
    }

    /// Resolves a primary product handle after checking the registered source kind.
    pub fn resolve_with_database_as<T: 'static>(
        &self,
        asset_server: &mut AssetServer,
        database: &ProjectAssetDatabase,
        expected_source_kind: &AssetSourceKind,
    ) -> Result<Handle<T>, AssetRefError> {
        let record = self.validate_with_database_as(database, expected_source_kind)?;
        asset_server.reserve_record(record).map_err(Into::into)
    }

    /// Checks a primary selection's enclosing source identity and kind without reserving a handle.
    ///
    /// Auxiliary selections are rejected. Use `ProjectAssetDatabase::resolve_ref` to look up
    /// their source, then let the concrete domain check membership in its admitted product.
    pub fn validate_with_database_as<'a>(
        &self,
        database: &'a ProjectAssetDatabase,
        expected_source_kind: &AssetSourceKind,
    ) -> Result<&'a AssetRecord, AssetRefError> {
        self.validate_primary_selection()?;
        let record = database.resolve_ref(self)?;
        if record.source_kind() != expected_source_kind {
            return Err(AssetRefError::UnexpectedSourceKind {
                asset_ref: self.to_string(),
                expected: expected_source_kind.clone(),
                actual: record.source_kind().clone(),
            });
        }
        Ok(record)
    }

    fn validate_primary_selection(&self) -> Result<(), AssetRefError> {
        if let Self::Product(reference) = self
            && !self.is_primary()
        {
            return Err(AssetRefError::UnsupportedProductSelection(*reference));
        }
        Ok(())
    }

    pub fn from_handle<T: 'static>(
        asset_server: &AssetServer,
        handle: Handle<T>,
    ) -> Result<Self, AssetRefError> {
        Self::from_handle_with_policy(asset_server, handle, AssetRefExportPolicy::Path)
    }

    /// Exports a handle while preserving complete auxiliary and pathless primary product identity.
    ///
    /// The policy selects a path or stable source ID only for a source-bound primary product.
    pub fn from_handle_with_policy<T: 'static>(
        asset_server: &AssetServer,
        handle: Handle<T>,
        policy: AssetRefExportPolicy,
    ) -> Result<Self, AssetRefError> {
        if let Err(error) = asset_server.validate_type_claim::<T>(handle.id()) {
            return match error {
                AssetError::UnknownAssetId(_) => Err(AssetRefError::UnknownHandle(handle.id())),
                error => Err(AssetRefError::Asset(error)),
            };
        }
        let path = asset_server.path(handle.id());
        if let Some(product) = asset_server.product(handle.id()) {
            if product.product() != ImportedProductId::PRIMARY || path.is_none() {
                return Ok(Self::Product(product));
            }
            if policy == AssetRefExportPolicy::StableIdWhenKnown {
                return Ok(Self::StableId(product.source()));
            }
        }

        let path = path.ok_or_else(|| AssetRefError::UnknownHandle(handle.id()))?;
        Self::path(path).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
    }

    fn other_stable_id() -> StableAssetId {
        StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap()
    }

    fn asset_record(path: &str) -> AssetRecord {
        AssetRecord::new(
            stable_id(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        )
    }

    #[test]
    fn reserves_stable_path_handles() {
        let mut server = AssetServer::new();
        let first = server.reserve::<String>("player.png").unwrap();
        let second = server.reserve::<String>("player.png").unwrap();

        assert_eq!(first, second);
        assert_eq!(server.path(first.id()), Some("player.png"));
    }

    #[test]
    fn typed_claims_are_idempotent_and_reject_incompatible_types() {
        let mut server = AssetServer::new();
        let first = server.reserve::<String>("player.png").unwrap();
        let repeat = server.reserve::<String>("player.png").unwrap();

        assert_eq!(first, repeat);
        assert!(matches!(
            server.reserve::<u32>("player.png"),
            Err(AssetError::AssetTypeMismatch { .. })
        ));
        assert_eq!(server.path(first.id()), Some("player.png"));
    }

    #[test]
    fn unknown_ids_cannot_be_claimed_as_typed_handles() {
        let mut server = AssetServer::new();

        assert_eq!(
            server.claim::<String>(AssetId::from_raw(u64::MAX)),
            Err(AssetError::UnknownAssetId(AssetId::from_raw(u64::MAX)))
        );
    }

    #[test]
    fn rejected_typed_record_claims_restore_staged_bindings() {
        let mut server = AssetServer::new();
        let first = asset_record("textures/player.png");
        let handle = server.reserve::<String>(first.path().as_str()).unwrap();
        let baseline = server.clone();
        assert!(matches!(
            server.reserve_record::<u32>(&first),
            Err(AssetError::AssetTypeMismatch { .. })
        ));
        assert_eq!(server, baseline);
        assert_eq!(server.stable_id(handle.id()), None);
    }

    #[test]
    fn untyped_identity_claims_participate_in_catalog_snapshots() {
        let mut server = AssetServer::new();
        let id = server
            .reserve_record_id(&asset_record("image.png"))
            .unwrap();
        let baseline = server.clone();

        let handle = server.claim::<String>(id).unwrap();

        assert_ne!(server, baseline);
        assert_eq!(server.claim::<String>(id).unwrap(), handle);
        assert_eq!(server.get_handle::<String>(id), Some(handle));
        assert!(server.get_handle::<u32>(id).is_none());
        assert!(baseline.get_handle::<String>(id).is_none());
    }

    #[test]
    fn anonymous_identities_are_unique_and_have_explicit_catalog_membership() {
        let detached = allocate_anonymous_handle::<String>().unwrap();
        let mut server = AssetServer::new();
        let owned = server.reserve_anonymous::<String>().unwrap();

        assert_ne!(detached.id(), owned.id());
        assert_eq!(server.claim::<String>(owned.id()).unwrap(), owned);
        assert_eq!(
            server.claim::<String>(detached.id()),
            Err(AssetError::UnknownAssetId(detached.id()))
        );
        assert!(matches!(
            server.claim::<u32>(owned.id()),
            Err(AssetError::AssetTypeMismatch { .. })
        ));
    }

    #[test]
    fn independent_asset_servers_allocate_distinct_runtime_ids() {
        let mut first_server = AssetServer::new();
        let mut second_server = AssetServer::new();
        let first = first_server.reserve::<String>("first.png").unwrap();
        let second = second_server.reserve::<String>("second.png").unwrap();
        let mut second_assets = Assets::default();
        second_assets.insert(second, String::from("second"));

        assert_ne!(first.id(), second.id());
        assert!(second_assets.get(first).is_none());
        assert_eq!(
            AssetRef::from_handle(&second_server, first),
            Err(AssetRefError::UnknownHandle(first.id()))
        );
    }

    #[test]
    fn cloned_asset_servers_allocate_distinct_new_runtime_ids() {
        let mut original = AssetServer::new();
        let existing = original.reserve::<String>("existing.png").unwrap();
        let mut clone = original.clone();

        let original_new = original.reserve::<String>("original.png").unwrap();
        let clone_new = clone.reserve::<String>("clone.png").unwrap();

        assert_eq!(original.path(existing.id()), Some("existing.png"));
        assert_ne!(original_new.id(), clone_new.id());
    }

    #[test]
    fn asset_server_equality_ignores_discarded_allocator_ids() {
        let mut original = AssetServer::new();
        original.reserve::<String>("existing.png").unwrap();
        let clone = original.clone();

        AssetServer::new().reserve_anonymous::<String>().unwrap();

        assert_eq!(original, clone);
    }

    #[test]
    fn rejects_non_logical_asset_paths() {
        assert_eq!(AssetPath::new(""), Err(AssetPathError::Empty));
        assert_eq!(
            AssetPath::new("/absolute.png"),
            Err(AssetPathError::Absolute)
        );
        assert_eq!(
            AssetPath::new("textures\\player.png"),
            Err(AssetPathError::ContainsBackslash)
        );
        assert_eq!(
            AssetPath::new("textures/../player.png"),
            Err(AssetPathError::ContainsParentDirectory)
        );
        assert_eq!(
            AssetPath::new("textures//player.png"),
            Err(AssetPathError::ContainsEmptySegment)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn asset_path_deserialization_validates_shape() {
        let error = serde_json::from_str::<AssetPath>(r#""textures/../player.png""#).unwrap_err();

        assert!(error.to_string().contains(".."));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn stable_asset_id_deserialization_validates_shape() {
        let error = serde_json::from_str::<StableAssetId>(r#""asset-123""#).unwrap_err();

        assert!(error.to_string().contains("invalid stable asset id"));
    }

    #[test]
    fn resolves_and_exports_asset_refs_by_path() {
        let mut server = AssetServer::new();
        let asset_ref = AssetRef::path("textures/player.png").unwrap();

        let handle = asset_ref.resolve::<String>(&mut server).unwrap();

        assert_eq!(server.path(handle.id()), Some("textures/player.png"));
        assert_eq!(
            AssetRef::from_handle(&server, handle).unwrap(),
            AssetRef::path("textures/player.png").unwrap()
        );
    }

    #[test]
    fn stable_asset_ids_resolve_through_project_database() {
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(asset_record("textures/player.png"))
            .unwrap();

        let mut server = AssetServer::new();
        let path_ref = AssetRef::path("textures/player.png").unwrap();
        let stable_ref = AssetRef::stable_id(stable_id().to_string()).unwrap();
        let primary_ref = AssetRef::product(
            AssetProductRef::new(stable_id(), ImportedProductId::PRIMARY).unwrap(),
        );

        let path_handle = path_ref
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();
        let stable_handle = stable_ref
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();
        let primary_handle = primary_ref
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();
        let checked_primary_handle = primary_ref
            .resolve_with_database_as::<String>(&mut server, &database, &AssetSourceKind::Image)
            .unwrap();

        assert_eq!(path_handle, stable_handle);
        assert_eq!(path_handle, primary_handle);
        assert_eq!(path_handle, checked_primary_handle);
        assert_eq!(server.path(path_handle.id()), Some("textures/player.png"));
        assert_eq!(server.stable_id(path_handle.id()), Some(stable_id()));
    }

    #[test]
    fn product_source_lookup_does_not_prove_product_membership() {
        let mut database = ProjectAssetDatabase::default();
        let record = asset_record("textures/player.png");
        database.insert(record.clone()).unwrap();
        let auxiliary =
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();

        for product in [ImportedProductId::PRIMARY, auxiliary] {
            let known = AssetRef::product(AssetProductRef::new(stable_id(), product).unwrap());
            let unknown =
                AssetRef::product(AssetProductRef::new(other_stable_id(), product).unwrap());
            assert_eq!(database.resolve_ref(&known), Ok(&record));
            assert_eq!(
                database.resolve_ref(&unknown),
                Err(AssetDatabaseError::UnknownStableId(other_stable_id()))
            );
        }
    }

    #[test]
    fn source_database_routes_reject_auxiliary_products_even_when_bound() {
        let record = asset_record("textures/player.png");
        let mut database = ProjectAssetDatabase::default();
        database.insert(record.clone()).unwrap();
        let product = AssetProductRef::new(
            stable_id(),
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
        )
        .unwrap();
        let reference = AssetRef::product(product);
        let expected = AssetRefError::UnsupportedProductSelection(product);

        for existing_primary in [false, true] {
            let mut server = AssetServer::new();
            if existing_primary {
                server.reserve_record::<u32>(&record).unwrap();
            }
            let baseline = server.clone();
            assert_eq!(
                reference.resolve::<String>(&mut server),
                Err(AssetRefError::Asset(AssetError::UnknownProduct(product)))
            );
            assert_eq!(server, baseline);
            let auxiliary = server.reserve_product::<String>(product).unwrap();
            assert_eq!(reference.resolve::<String>(&mut server), Ok(auxiliary));
            let baseline = server.clone();
            assert_eq!(
                reference.resolve_with_database::<String>(&mut server, &database),
                Err(expected.clone())
            );
            assert_eq!(server, baseline);
            for kind in [AssetSourceKind::Image, AssetSourceKind::Scene] {
                assert_eq!(
                    reference.resolve_with_database_as::<String>(&mut server, &database, &kind),
                    Err(expected.clone())
                );
                assert_eq!(server, baseline);
                assert_eq!(
                    reference.validate_with_database_as(&database, &kind),
                    Err(expected.clone())
                );
            }
            assert_eq!(server, baseline);
        }
        assert_eq!(
            reference.validate_with_database_as(
                &ProjectAssetDatabase::default(),
                &AssetSourceKind::Image,
            ),
            Err(expected.clone())
        );
        assert_eq!(
            expected.to_string(),
            format!("asset product '{product}' cannot be selected by a primary runtime handle")
        );
    }

    #[test]
    fn source_primary_resolution_requires_database_kind_identity_and_type_proofs() {
        let product = AssetProductRef::new(stable_id(), ImportedProductId::PRIMARY).unwrap();
        let reference = AssetRef::product(product);
        let mut server = AssetServer::new();
        let baseline = server.clone();
        assert_eq!(
            reference.resolve::<String>(&mut server),
            Err(AssetRefError::Asset(AssetError::UnknownProduct(product)))
        );
        let mut database = ProjectAssetDatabase::default();
        assert_eq!(
            reference.resolve_with_database::<String>(&mut server, &database),
            Err(AssetRefError::Database(
                AssetDatabaseError::UnknownStableId(stable_id())
            ))
        );
        let record = asset_record("textures/player.png");
        database.insert(record.clone()).unwrap();
        let wrong_kind = AssetRefError::UnexpectedSourceKind {
            asset_ref: reference.to_string(),
            expected: AssetSourceKind::Scene,
            actual: AssetSourceKind::Image,
        };
        assert_eq!(
            reference.validate_with_database_as(&database, &AssetSourceKind::Scene),
            Err(wrong_kind.clone())
        );
        assert_eq!(
            reference.resolve_with_database_as::<String>(
                &mut server,
                &database,
                &AssetSourceKind::Scene,
            ),
            Err(wrong_kind)
        );
        assert_eq!(server, baseline);
        assert_eq!(
            reference.validate_with_database_as(&database, &AssetSourceKind::Image),
            Ok(&record)
        );

        let handle = reference
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();
        let baseline = server.clone();
        assert_eq!(reference.resolve::<String>(&mut server), Ok(handle));
        assert_eq!(server, baseline);
        assert!(matches!(
            reference.resolve_with_database_as::<u32>(
                &mut server,
                &database,
                &AssetSourceKind::Image,
            ),
            Err(AssetRefError::Asset(AssetError::AssetTypeMismatch { .. }))
        ));
        assert_eq!(server, baseline);
        assert_eq!(
            AssetRef::from_handle_with_policy(
                &server,
                handle,
                AssetRefExportPolicy::StableIdWhenKnown,
            )
            .unwrap(),
            AssetRef::StableId(stable_id())
        );

        let mut conflict_server = AssetServer::new();
        conflict_server
            .reserve_record::<String>(&AssetRecord::new(
                other_stable_id(),
                record.path().clone(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        let baseline = conflict_server.clone();
        assert!(matches!(
            reference.resolve_with_database::<String>(&mut conflict_server, &database),
            Err(AssetRefError::Asset(
                AssetError::AssetIdAlreadyBoundToStableId { .. }
            ))
        ));
        assert_eq!(conflict_server, baseline);
    }

    #[test]
    fn asset_server_rejects_path_rebound_to_different_stable_id() {
        let mut server = AssetServer::new();
        let first = AssetRecord::new(
            stable_id(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );
        let second = AssetRecord::new(
            other_stable_id(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );

        let handle = server.reserve_record::<String>(&first).unwrap();
        let baseline = server.clone();
        let error = server.reserve_record::<String>(&second).unwrap_err();

        assert!(matches!(
            error,
            AssetError::AssetIdAlreadyBoundToStableId { .. }
        ));
        assert_eq!(server, baseline);
        assert_eq!(server.stable_id(handle.id()), Some(stable_id()));
    }

    #[test]
    fn asset_server_rejects_stable_id_rebound_to_different_path() {
        let mut server = AssetServer::new();
        let first = asset_record("textures/player.png");
        let second = AssetRecord::new(
            stable_id(),
            AssetPath::new("textures/player-v2.png").unwrap(),
            AssetSourceKind::Image,
        );

        let handle = server.reserve_record::<String>(&first).unwrap();
        let baseline = server.clone();
        let error = server.reserve_record::<String>(&second).unwrap_err();

        assert!(matches!(
            error,
            AssetError::AssetIdAlreadyBoundToPath { .. }
        ));
        assert_eq!(server, baseline);
        assert_eq!(server.path(handle.id()), Some("textures/player.png"));
    }

    #[test]
    fn stable_asset_ids_require_database_for_resolution() {
        let mut server = AssetServer::new();
        let asset_ref = AssetRef::stable_id(stable_id().to_string()).unwrap();

        assert_eq!(
            asset_ref.resolve::<String>(&mut server),
            Err(AssetRefError::MissingProjectDatabase(stable_id()))
        );
    }

    #[test]
    fn project_database_rejects_duplicate_stable_ids() {
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(asset_record("textures/player.png"))
            .unwrap();

        let error = database
            .insert(AssetRecord::new(
                stable_id(),
                AssetPath::new("textures/other.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap_err();

        assert!(matches!(
            error,
            AssetDatabaseError::DuplicateStableId { .. }
        ));
    }

    #[test]
    fn project_database_rejects_case_insensitive_path_collisions() {
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(asset_record("textures/player.png"))
            .unwrap();

        let error = database
            .insert(AssetRecord::new(
                StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap(),
                AssetPath::new("Textures/Player.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap_err();

        assert!(matches!(
            error,
            AssetDatabaseError::CaseInsensitivePathCollision { .. }
        ));
    }

    #[test]
    fn project_database_rejects_mismatched_meta_paths() {
        let mut database = ProjectAssetDatabase::default();
        let identity = AssetMetaIdentity::new(
            stable_id(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );

        let error = database
            .insert_identity(AssetPath::new("textures/other.png").unwrap(), identity)
            .unwrap_err();

        assert!(matches!(error, AssetDatabaseError::MetaPathMismatch { .. }));
    }

    #[test]
    fn project_database_rejects_filesystem_paths_outside_asset_root() {
        let temp_root = unique_temp_root();
        let asset_root = temp_root.join("assets");
        let outside_root = temp_root.join("outside");
        fs::create_dir_all(asset_root.join("textures")).unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        let source = asset_root.join("textures").join("player.png");
        let outside = outside_root.join("player.png");
        fs::write(&source, b"png").unwrap();
        fs::write(&outside, b"png").unwrap();

        let logical =
            ProjectAssetDatabase::logical_path_from_source_path(&asset_root, &source).unwrap();
        let outside_error =
            ProjectAssetDatabase::logical_path_from_source_path(&asset_root, &outside).unwrap_err();

        assert_eq!(logical.as_str(), "textures/player.png");
        assert!(matches!(
            outside_error,
            AssetDatabaseError::SourceOutsideRoot
        ));

        remove_temp_root(&temp_root);
    }

    #[test]
    fn assets_store_under_reserved_handles_without_allocator_collisions() {
        let mut server = AssetServer::new();
        let reserved = server.reserve::<String>("textures/player.png").unwrap();
        let mut assets = Assets::<String>::default();

        assert_eq!(assets.insert(reserved, "reserved".to_string()), None);
        let added = assets.add(&mut server, "generated".to_string()).unwrap();

        assert_ne!(reserved, added);
        assert_eq!(assets.get(reserved).map(String::as_str), Some("reserved"));
        assert_eq!(assets.get(added).map(String::as_str), Some("generated"));
    }

    #[test]
    fn stores_typed_assets_with_asset_server_allocation() {
        let mut server = AssetServer::new();
        let mut assets = Assets::<String>::default();
        let handle = assets.add(&mut server, "texture".to_string()).unwrap();

        assert_eq!(assets.get(handle).map(String::as_str), Some("texture"));
    }

    fn unique_temp_root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nara_asset_test_{}_{}", std::process::id(), stamp))
    }

    fn remove_temp_root(path: &Path) {
        if let Err(error) = fs::remove_dir_all(path) {
            panic!(
                "failed to remove temp test directory {}: {error}",
                path.display()
            );
        }
    }
}
