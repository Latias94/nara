//! Asset identity, project metadata, typed handles, and in-memory asset tables.

mod artifact;
mod database;
mod identity;
mod import;
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
    AssetDatabaseError, AssetMeta, AssetRecord, AssetSourceKind, MissingMetaPolicy,
    ProjectAssetDatabase,
};
pub use identity::{
    AssetId, AssetPath, AssetPathError, AssetRef, StableAssetId, StableAssetIdError,
};
pub use import::{
    ImportError, ImportRequest, Importer, ImporterDescriptor, ImporterDescriptorError,
    ImporterRegistry, ImporterRegistryError, ImporterSelectionError, SourceExtension,
};
pub use server::{AssetError, AssetServer};
pub use state::{
    AssetDependencyGraph, AssetEvent, AssetEventKind, AssetEvents, AssetState, AssetStateError,
    AssetStates, AssetVersion, LoadState,
};
pub use storage::{Asset, Assets, Handle};

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
    pub fn resolve<T>(&self, asset_server: &mut AssetServer) -> Result<Handle<T>, AssetRefError> {
        match self {
            Self::Path(path) => asset_server.reserve::<T>(path.as_str()).map_err(Into::into),
            Self::StableId(id) => Err(AssetRefError::MissingProjectDatabase(*id)),
        }
    }

    pub fn resolve_with_database<T>(
        &self,
        asset_server: &mut AssetServer,
        database: &ProjectAssetDatabase,
    ) -> Result<Handle<T>, AssetRefError> {
        let record = database.resolve_ref(self)?;
        asset_server.reserve_record(record).map_err(Into::into)
    }

    pub fn from_handle<T>(
        asset_server: &AssetServer,
        handle: Handle<T>,
    ) -> Result<Self, AssetRefError> {
        Self::from_handle_with_policy(asset_server, handle, AssetRefExportPolicy::Path)
    }

    pub fn from_handle_with_policy<T>(
        asset_server: &AssetServer,
        handle: Handle<T>,
        policy: AssetRefExportPolicy,
    ) -> Result<Self, AssetRefError> {
        if policy == AssetRefExportPolicy::StableIdWhenKnown
            && let Some(stable_id) = asset_server.stable_id(handle.id())
        {
            return Ok(Self::StableId(stable_id));
        }

        let path = asset_server
            .path(handle.id())
            .ok_or_else(|| AssetRefError::UnknownHandle(handle.id()))?;
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

        let path_handle = path_ref
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();
        let stable_handle = stable_ref
            .resolve_with_database::<String>(&mut server, &database)
            .unwrap();

        assert_eq!(path_handle, stable_handle);
        assert_eq!(server.path(path_handle.id()), Some("textures/player.png"));
        assert_eq!(server.stable_id(path_handle.id()), Some(stable_id()));
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
        let meta = AssetMeta::new(
            stable_id(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );

        let error = database
            .insert_meta(AssetPath::new("textures/other.png").unwrap(), meta)
            .unwrap_err();

        assert!(matches!(error, AssetDatabaseError::MetaPathMismatch { .. }));
    }

    #[test]
    fn project_database_diagnoses_or_generates_missing_meta_by_policy() {
        let source_path = AssetPath::new("textures/player.png").unwrap();
        let mut database = ProjectAssetDatabase::default();

        let error = database
            .insert_source_with_meta_policy(source_path.clone(), None, MissingMetaPolicy::Diagnose)
            .unwrap_err();

        assert_eq!(
            error,
            AssetDatabaseError::MissingMeta {
                path: source_path.clone()
            }
        );

        database
            .insert_source_with_meta_policy(
                source_path.clone(),
                None,
                MissingMetaPolicy::Generate {
                    stable_id: stable_id(),
                    source_kind: AssetSourceKind::Image,
                },
            )
            .unwrap();

        let record = database.record_for_path(&source_path).unwrap();
        assert_eq!(record.stable_id(), stable_id());
        assert_eq!(record.source_kind(), &AssetSourceKind::Image);
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
