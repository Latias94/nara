use std::{fmt, sync::Arc};

use nara_core::{AssetId, vfs::MemoryAssetSource};
use nara_ecs::Resource;

use crate::{AssetLoadError, AssetPath, NpakEntryKind};

/// Selects the byte format the runtime loader should decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAssetMode {
    Source,
    Cooked,
}

/// A shareable byte source indexed by persistent asset UUIDs.
pub trait RuntimeAssetBackend: Send + Sync + 'static {
    fn mode(&self) -> RuntimeAssetMode;

    fn read_asset(
        &self,
        id: AssetId,
        expected_cooked_kind: Option<NpakEntryKind>,
    ) -> Result<Vec<u8>, AssetLoadError>;

    fn resolve_path(&self, _path: &AssetPath) -> Option<AssetId> {
        None
    }

    #[cfg(feature = "authoring")]
    fn meta(&self, _id: AssetId) -> Option<crate::AssetMeta> {
        None
    }
}

/// A byte source shared between the asset server and its loader jobs.
#[derive(Clone, Resource)]
pub struct RuntimeAssetSource(Arc<dyn RuntimeAssetBackend>);

impl RuntimeAssetSource {
    pub fn new(backend: impl RuntimeAssetBackend) -> Self {
        Self(Arc::new(backend))
    }

    #[must_use]
    pub fn backend(&self) -> &dyn RuntimeAssetBackend {
        self.0.as_ref()
    }

    /// Uses the virtual UUID paths returned by [`super::asset_id_path`].
    pub fn from_memory(mode: RuntimeAssetMode, files: MemoryAssetSource) -> Self {
        Self::new(MemoryBackend { mode, files })
    }

    #[cfg(all(feature = "cooked", any(unix, windows)))]
    pub fn from_npak(path: impl AsRef<std::path::Path>) -> Result<Self, AssetLoadError> {
        let archive = crate::cooked::NpakArchive::open(path)?;
        Ok(Self::from_npak_archive(Arc::new(archive)))
    }

    #[cfg(feature = "cooked")]
    pub fn from_npak_archive(archive: Arc<crate::cooked::NpakArchive>) -> Self {
        Self::new(ArchiveBackend(archive))
    }

    /// Opens a read-only index of the project's existing sidecars. Logical load paths are
    /// relative to `asset_root`, which is itself relative to `project_root`.
    #[cfg(all(feature = "authoring", any(unix, windows)))]
    pub fn from_project_dir(
        project_root: impl AsRef<std::path::Path>,
        asset_root: &str,
    ) -> Result<Self, AssetLoadError> {
        use crate::{AssetPathScope, AssetScanMode, AssetSourcePath, ProjectAssetDatabase};
        use nara_core::vfs::DirAssetSource;

        let asset_root = AssetSourcePath::new(asset_root)?;
        let directory = DirAssetSource::new(project_root)?;
        let database = ProjectAssetDatabase::scan(
            directory.root(),
            std::slice::from_ref(&asset_root),
            AssetScanMode::ReadOnly,
        )?;
        Ok(Self::new(ProjectBackend {
            directory,
            database,
            scope: AssetPathScope::under(asset_root),
        }))
    }
}

impl fmt::Debug for RuntimeAssetSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeAssetSource")
            .field("mode", &self.0.mode())
            .finish_non_exhaustive()
    }
}

impl PartialEq for RuntimeAssetSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RuntimeAssetSource {}

struct MemoryBackend {
    mode: RuntimeAssetMode,
    files: MemoryAssetSource,
}

impl RuntimeAssetBackend for MemoryBackend {
    fn mode(&self) -> RuntimeAssetMode {
        self.mode
    }

    fn read_asset(&self, id: AssetId, _: Option<NpakEntryKind>) -> Result<Vec<u8>, AssetLoadError> {
        super::read_asset(&self.files, id).map_err(|source| AssetLoadError::Read { id, source })
    }
}

#[cfg(feature = "cooked")]
struct ArchiveBackend(Arc<crate::cooked::NpakArchive>);

#[cfg(feature = "cooked")]
impl RuntimeAssetBackend for ArchiveBackend {
    fn mode(&self) -> RuntimeAssetMode {
        RuntimeAssetMode::Cooked
    }

    fn read_asset(
        &self,
        id: AssetId,
        kind: Option<NpakEntryKind>,
    ) -> Result<Vec<u8>, AssetLoadError> {
        let bytes = match kind {
            Some(kind) => self.0.read_kind(id, kind)?,
            None => self.0.read(id)?,
        };
        Ok(bytes)
    }
}

#[cfg(all(feature = "authoring", any(unix, windows)))]
struct ProjectBackend {
    directory: nara_core::vfs::DirAssetSource,
    database: crate::ProjectAssetDatabase,
    scope: crate::AssetPathScope,
}

#[cfg(all(feature = "authoring", any(unix, windows)))]
impl RuntimeAssetBackend for ProjectBackend {
    fn mode(&self) -> RuntimeAssetMode {
        RuntimeAssetMode::Source
    }

    fn read_asset(&self, id: AssetId, _: Option<NpakEntryKind>) -> Result<Vec<u8>, AssetLoadError> {
        use std::io::Read;

        const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
        let file = super::ProjectAssetSource::new(&self.directory, &self.database)
            .open_asset(id)
            .map_err(|source| AssetLoadError::Read { id, source })?;
        let mut bytes = Vec::new();
        file.take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| AssetLoadError::Read { id, source })?;
        if bytes.len() as u64 > MAX_SOURCE_BYTES {
            return Err(AssetLoadError::Read {
                id,
                source: std::io::Error::new(
                    std::io::ErrorKind::FileTooLarge,
                    "asset exceeds 256 MiB",
                ),
            });
        }
        Ok(bytes)
    }

    fn resolve_path(&self, path: &AssetPath) -> Option<AssetId> {
        let record = self.database.record_for_path(&self.scope.resolve(path))?;
        AssetId::try_from_uuid(record.stable_id().as_uuid()).ok()
    }

    fn meta(&self, id: AssetId) -> Option<crate::AssetMeta> {
        self.database
            .record_for_stable_id(crate::StableAssetId::from_uuid(id.as_uuid()))?
            .meta()
            .cloned()
    }
}
