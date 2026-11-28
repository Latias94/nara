use std::{collections::BTreeMap, fmt, sync::Arc};

use nara_core::{AssetId, vfs::MemoryAssetSource};
use nara_ecs::Resource;

use crate::{AssetError, AssetLoadError, AssetPath, NpakEntryKind};

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

    /// Reports the entry's runtime kind when the source has a type index.
    fn kind(&self, _id: AssetId) -> Option<NpakEntryKind> {
        None
    }

    /// Checks a typed request before the server claims a handle. Sources without a type index
    /// can leave format validation to the loader.
    fn validate_kind(&self, id: AssetId, expected: NpakEntryKind) -> Result<(), AssetError> {
        validate_kind(id, expected, self.kind(id))
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
        Self::from_memory_with_kinds(mode, files, [])
    }

    /// Adds a type index for UUIDs that must be resolved through untyped dependency references.
    /// Bytes use the same virtual paths as [`Self::from_memory`].
    pub fn from_memory_with_kinds(
        mode: RuntimeAssetMode,
        files: MemoryAssetSource,
        kinds: impl IntoIterator<Item = (AssetId, NpakEntryKind)>,
    ) -> Self {
        Self::new(MemoryBackend {
            mode,
            files,
            kinds: kinds.into_iter().collect(),
        })
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
    kinds: BTreeMap<AssetId, NpakEntryKind>,
}

impl RuntimeAssetBackend for MemoryBackend {
    fn mode(&self) -> RuntimeAssetMode {
        self.mode
    }

    fn read_asset(
        &self,
        id: AssetId,
        expected: Option<NpakEntryKind>,
    ) -> Result<Vec<u8>, AssetLoadError> {
        check_read_kind(self, id, expected)?;
        super::read_asset(&self.files, id).map_err(|source| AssetLoadError::Read { id, source })
    }

    fn kind(&self, id: AssetId) -> Option<NpakEntryKind> {
        self.kinds.get(&id).copied()
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

    fn kind(&self, id: AssetId) -> Option<NpakEntryKind> {
        self.0.entry(id).map(|entry| entry.kind)
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

    fn read_asset(
        &self,
        id: AssetId,
        expected: Option<NpakEntryKind>,
    ) -> Result<Vec<u8>, AssetLoadError> {
        use std::io::Read;

        const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
        let limit = match expected {
            Some(NpakEntryKind::TileSet) => crate::tilemap::MAX_TILESET_SOURCE_BYTES as u64,
            Some(NpakEntryKind::SpriteAnimation | NpakEntryKind::PropertyAnimation) => {
                8 * 1024 * 1024
            }
            _ => MAX_SOURCE_BYTES,
        };
        check_read_kind(self, id, expected)?;
        let file = super::ProjectAssetSource::new(&self.directory, &self.database)
            .open_asset(id)
            .map_err(|source| AssetLoadError::Read { id, source })?;
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| AssetLoadError::Read { id, source })?;
        if bytes.len() as u64 > limit {
            return Err(AssetLoadError::Read {
                id,
                source: std::io::Error::new(
                    std::io::ErrorKind::FileTooLarge,
                    format!("asset exceeds {limit} bytes"),
                ),
            });
        }
        Ok(bytes)
    }

    fn resolve_path(&self, path: &AssetPath) -> Option<AssetId> {
        let record = self.database.record_for_path(&self.scope.resolve(path))?;
        AssetId::try_from_uuid(record.stable_id().as_uuid()).ok()
    }

    fn kind(&self, id: AssetId) -> Option<NpakEntryKind> {
        use crate::meta::{
            AUDIO_IMPORTER, FONT_IMPORTER, IMAGE_IMPORTER, MATERIAL_IMPORTER,
            PROPERTY_ANIMATION_IMPORTER, RAW_IMPORTER, SCENE_IMPORTER, SHADER_IMPORTER,
            SPRITE_ANIMATION_IMPORTER, TILESET_IMPORTER,
        };

        let record = self.database.record_for_stable_id(id.into())?;
        match record.meta()?.importer() {
            AUDIO_IMPORTER => Some(NpakEntryKind::Audio),
            FONT_IMPORTER => Some(NpakEntryKind::Font),
            IMAGE_IMPORTER => Some(NpakEntryKind::Image),
            MATERIAL_IMPORTER => Some(NpakEntryKind::Material),
            RAW_IMPORTER => Some(NpakEntryKind::Raw),
            SCENE_IMPORTER => Some(NpakEntryKind::Scene),
            SHADER_IMPORTER => Some(NpakEntryKind::Shader),
            TILESET_IMPORTER => Some(NpakEntryKind::TileSet),
            SPRITE_ANIMATION_IMPORTER => Some(NpakEntryKind::SpriteAnimation),
            PROPERTY_ANIMATION_IMPORTER => Some(NpakEntryKind::PropertyAnimation),
            _ => None,
        }
    }

    fn validate_kind(&self, id: AssetId, expected: NpakEntryKind) -> Result<(), AssetError> {
        let actual = self.kind(id);
        if actual.is_none() && self.database.record_for_stable_id(id.into()).is_some() {
            return Err(AssetError::UnknownRuntimeKind(id));
        }
        validate_kind(id, expected, actual)
    }

    fn meta(&self, id: AssetId) -> Option<crate::AssetMeta> {
        self.database
            .record_for_stable_id(crate::StableAssetId::from_uuid(id.as_uuid()))?
            .meta()
            .cloned()
    }
}

fn validate_kind(
    id: AssetId,
    expected: NpakEntryKind,
    actual: Option<NpakEntryKind>,
) -> Result<(), AssetError> {
    if let Some(actual) = actual.filter(|&actual| actual != expected) {
        return Err(AssetError::WrongRuntimeKind {
            id,
            expected,
            actual,
        });
    }
    Ok(())
}

fn check_read_kind(
    backend: &impl RuntimeAssetBackend,
    id: AssetId,
    expected: Option<NpakEntryKind>,
) -> Result<(), AssetLoadError> {
    if let Some(expected) = expected {
        backend
            .validate_kind(id, expected)
            .map_err(|error| match error {
                AssetError::WrongRuntimeKind {
                    id,
                    expected,
                    actual,
                } => AssetLoadError::WrongKind {
                    id,
                    expected,
                    actual,
                },
                error => AssetLoadError::Request(error),
            })?;
    }
    Ok(())
}
