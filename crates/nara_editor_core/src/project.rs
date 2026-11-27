use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use nara_asset::{
    AssetScanError, AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
};
use nara_core::{
    AssetId, DocumentId, UuidIdError,
    vfs::{AssetPath, DirAssetSource, write_atomic},
};
use nara_project::ProjectManifest;
use nara_reflect::{ComponentSchema, ComponentSchemaCatalog};
use nara_scene::authoring::{
    SceneAsset, SceneDocument, SceneDocumentError,
    prefab::{PrefabError, expand_prefabs_for_editor},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EditorDocument, EditorError, document::read_editor_source};

const SETTINGS: &str = ".nara.ron";
const SETTINGS_LIMIT: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid nara.toml: {0}")]
    Manifest(String),
    #[error("invalid .nara.ron: {0}")]
    Settings(#[from] ron::error::SpannedError),
    #[error("could not serialize .nara.ron: {0}")]
    Serialize(#[from] ron::Error),
    #[error("{0} does not name an authored scene or prefab")]
    NotDocument(AssetPath),
    #[error("invalid entry scene asset UUID: {0}")]
    EntryAsset(#[from] UuidIdError),
    #[error("'{0}' is not a valid project asset directory")]
    AssetDirectory(String),
    #[error(transparent)]
    Assets(#[from] AssetScanError),
    #[error("prefab asset {0} is missing or is not a prefab")]
    MissingPrefab(AssetId),
    #[error("prefab {0} has changed on disk; refresh Assets and try again")]
    PrefabChanged(AssetId),
    #[error("scene or prefab asset {0} is no longer present in the project")]
    MissingDocument(AssetId),
    #[error("document {0} was replaced on disk; reopen it before saving")]
    DocumentChanged(DocumentId),
    #[error("applying the override would leave '{}' invalid: {source}", path.display())]
    PrefabDependent {
        path: PathBuf,
        #[source]
        source: PrefabError,
    },
    #[error("could not check authored document '{}': {source}", path.display())]
    AuthoredDocument {
        path: PathBuf,
        #[source]
        source: SceneDocumentError,
    },
    #[error(transparent)]
    Scene(#[from] SceneDocumentError),
    #[error(transparent)]
    Document(#[from] EditorError),
}

/// An opened project's tooling manifest, asset root, and saved editor entry scene.
#[derive(Clone)]
pub struct EditorProject {
    directory: PathBuf,
    assets: PathBuf,
    asset_prefix: AssetSourcePath,
    database: ProjectAssetDatabase,
    manifest: ProjectManifest,
    entry_scene: Option<AssetPath>,
    entry_asset: Option<AssetId>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    scene: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scene_id: Option<String>,
}

impl EditorProject {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let directory = fs::canonicalize(directory)?;
        let manifest = ProjectManifest::parse_toml(&read_small(&directory.join("nara.toml"))?)
            .map_err(|error| ProjectError::Manifest(error.to_string()))?;
        let asset_prefix = AssetSourcePath::new(manifest.paths.assets.as_str())
            .map_err(|_| ProjectError::AssetDirectory(manifest.paths.assets.clone()))?;
        let assets = directory.join(&manifest.paths.assets);
        let settings = match read_small(&directory.join(SETTINGS)) {
            Ok(source) => ron::from_str::<Settings>(&source)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(error) => return Err(error.into()),
        };
        let entry_scene = settings.scene.map(AssetPath::new).transpose()?;
        let entry_asset = settings.scene_id.as_deref().map(str::parse).transpose()?;
        if let Some(path) = &entry_scene
            && AssetKind::of(path) != AssetKind::Scene
        {
            return Err(ProjectError::NotDocument(path.clone()));
        }
        let database = ProjectAssetDatabase::scan(
            &directory,
            std::slice::from_ref(&asset_prefix),
            AssetScanMode::Discover,
        )?;
        let mut project = Self {
            directory,
            assets,
            asset_prefix,
            database,
            manifest,
            entry_scene,
            entry_asset,
        };
        project.update_entry_scene();
        Ok(project)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    pub fn entry_scene(&self) -> Option<&AssetPath> {
        self.entry_scene.as_ref()
    }

    pub fn asset_source(&self) -> io::Result<DirAssetSource> {
        let source = DirAssetSource::new(&self.assets)?;
        within(source.root(), &self.directory)?;
        Ok(source)
    }

    /// Asset observation uses the configured project-relative root, including when it is empty.
    pub fn asset_directory(&self) -> &Path {
        &self.assets
    }

    pub fn asset_database(&self) -> &ProjectAssetDatabase {
        &self.database
    }

    pub fn refresh_assets(&mut self) -> Result<(), ProjectError> {
        self.database = self.scan(AssetScanMode::Discover)?;
        self.update_entry_scene();
        Ok(())
    }

    /// Rescans external changes without creating sidecars while another tool is moving files.
    pub fn rescan_assets(&mut self) -> Result<(), ProjectError> {
        self.database = self.scan(AssetScanMode::ReadOnly)?;
        self.update_entry_scene();
        Ok(())
    }

    fn update_entry_scene(&mut self) {
        if let Some(asset) = self.entry_asset {
            self.entry_scene = self.asset_hint(asset);
        } else if let Some(asset) = self
            .entry_scene
            .as_ref()
            .and_then(|path| self.asset_reference(path))
        {
            self.entry_asset = Some(asset.id);
        }
    }

    fn scan(&self, mode: AssetScanMode) -> Result<ProjectAssetDatabase, ProjectError> {
        Ok(ProjectAssetDatabase::scan(
            &self.directory,
            std::slice::from_ref(&self.asset_prefix),
            mode,
        )?)
    }

    pub fn assets(&self) -> AssetBrowserModel {
        AssetBrowserModel {
            entries: self
                .database
                .records()
                .filter_map(|record| {
                    let path = self.relative_asset_path(record.path())?;
                    Some(AssetEntry {
                        id: record.stable_id().try_into().ok()?,
                        kind: match record.source_kind() {
                            AssetSourceKind::Scene => AssetKind::Scene,
                            AssetSourceKind::Prefab => AssetKind::Prefab,
                            AssetSourceKind::Image => AssetKind::Image,
                            _ => AssetKind::File,
                        },
                        path,
                    })
                })
                .collect(),
        }
    }

    pub fn asset_reference(&self, path: &AssetPath) -> Option<SceneAsset> {
        let path_in_project = self
            .asset_prefix
            .joined(&AssetSourcePath::new(path.as_str()).ok()?);
        let record = self.database.record_for_path(&path_in_project)?;
        Some(SceneAsset {
            id: record.stable_id().try_into().ok()?,
            hint: path.clone(),
        })
    }

    pub fn asset_hint(&self, id: AssetId) -> Option<AssetPath> {
        self.relative_asset_path(self.database.record_for_stable_id(id.into())?.path())
    }

    fn relative_asset_path(&self, path: &AssetSourcePath) -> Option<AssetPath> {
        let relative = path
            .as_str()
            .strip_prefix(self.asset_prefix.as_str())?
            .strip_prefix('/')?;
        AssetPath::new(relative).ok()
    }

    /// Opens a scene or prefab on the project's asset mount.
    pub fn open_document(&self, path: &AssetPath) -> Result<EditorDocument, ProjectError> {
        self.open_document_with(path, &[])
    }

    pub fn open_document_with(
        &self,
        path: &AssetPath,
        schemas: &[ComponentSchema],
    ) -> Result<EditorDocument, ProjectError> {
        if !matches!(AssetKind::of(path), AssetKind::Scene | AssetKind::Prefab) {
            return Err(ProjectError::NotDocument(path.clone()));
        }
        Ok(EditorDocument::open_with(
            self.document_path(path)?,
            schemas,
        )?)
    }

    /// Reopens the exact sidecar identity, checking known component payloads strictly.
    pub fn reload_document(
        &self,
        id: AssetId,
        schemas: &[ComponentSchema],
    ) -> Result<EditorDocument, ProjectError> {
        Ok(EditorDocument::open_for_reload(
            self.indexed_document_path(&self.database, id)?,
            schemas,
        )?)
    }

    fn document_path(&self, path: &AssetPath) -> Result<PathBuf, ProjectError> {
        let root = self.asset_source()?;
        let physical = fs::canonicalize(root.root().join(path.as_str()))?;
        within(&physical, root.root())?;
        Ok(physical)
    }

    /// Resolves a named document again before an overwrite. A move with its sidecar is accepted.
    pub fn document_save_path(
        &mut self,
        asset: AssetId,
        document: DocumentId,
    ) -> Result<PathBuf, ProjectError> {
        self.refresh_assets()?;
        let path = self.indexed_document_path(&self.database, asset)?;
        if SceneDocument::read(&path)?.id != document {
            return Err(ProjectError::DocumentChanged(document));
        }
        Ok(path)
    }

    pub fn relative_document_path(&self, path: &Path) -> io::Result<AssetPath> {
        let root = self.asset_source()?;
        within(path, root.root())?;
        let relative = path
            .strip_prefix(root.root())
            .map_err(io::Error::other)?
            .iter()
            .map(|part| part.to_str().ok_or(io::ErrorKind::InvalidInput))
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        AssetPath::new(relative)
    }

    /// Parses only prefabs reachable from the active document.
    pub fn load_prefabs(
        &self,
        scene: &SceneDocument,
        schemas: &[ComponentSchema],
    ) -> Result<BTreeMap<AssetId, SceneDocument>, ProjectError> {
        self.load_prefabs_using(scene, |path| read_editor_source(path, schemas))
    }

    /// Parses changed prefab sources strictly against the same schema as the preview.
    pub fn reload_prefabs(
        &self,
        scene: &SceneDocument,
        schemas: &[ComponentSchema],
    ) -> Result<BTreeMap<AssetId, SceneDocument>, ProjectError> {
        self.load_prefabs_using(scene, |path| SceneDocument::read_with(path, schemas))
    }

    fn load_prefabs_using(
        &self,
        scene: &SceneDocument,
        read: impl Fn(&Path) -> Result<SceneDocument, SceneDocumentError>,
    ) -> Result<BTreeMap<AssetId, SceneDocument>, ProjectError> {
        let mut pending = prefab_references(scene).collect::<Vec<_>>();
        let mut prefabs = BTreeMap::new();
        while let Some(id) = pending.pop() {
            if let std::collections::btree_map::Entry::Vacant(entry) = prefabs.entry(id) {
                let path = self.prefab_path(&self.database, id)?;
                let prefab = read(&path)
                    .map_err(|source| ProjectError::AuthoredDocument { path, source })?;
                pending.extend(prefab_references(&prefab));
                entry.insert(prefab);
            }
        }
        Ok(prefabs)
    }

    /// Checks for external changes before publishing an upstream edit.
    pub fn write_prefab(
        &self,
        id: AssetId,
        prefab: &SceneDocument,
        expected: &BTreeMap<AssetId, SceneDocument>,
        catalog: &ComponentSchemaCatalog,
    ) -> Result<(), ProjectError> {
        let index = self.scan(AssetScanMode::ReadOnly)?;
        let schemas = catalog.components();
        let path = self.prefab_path(&index, id)?;
        if !expected.contains_key(&id) {
            return Err(ProjectError::PrefabChanged(id));
        }
        let mut files = BTreeMap::new();
        let mut dependents = BTreeMap::<_, Vec<_>>::new();
        for record in index.records() {
            let is_prefab = match record.source_kind() {
                AssetSourceKind::Prefab => true,
                AssetSourceKind::Scene => false,
                _ => continue,
            };
            let Ok(asset) = AssetId::try_from(record.stable_id()) else {
                continue;
            };
            let dependent_path = self.indexed_document_path(&index, asset)?;
            let source = read_editor_source(&dependent_path, schemas).map_err(|source| {
                ProjectError::AuthoredDocument {
                    path: dependent_path.clone(),
                    source,
                }
            })?;
            for target in prefab_references(&source) {
                dependents.entry(target).or_default().push(asset);
            }
            files.insert(asset, (dependent_path, is_prefab, source));
        }
        for (&asset, document) in expected {
            let (_, _, current) = files
                .get(&asset)
                .filter(|(_, is_prefab, _)| *is_prefab)
                .ok_or(ProjectError::MissingPrefab(asset))?;
            if current.to_ron_with(schemas)? != document.to_ron_with(schemas)? {
                return Err(ProjectError::PrefabChanged(asset));
            }
        }

        // The upstream edit is persisted before the current scene's next Save. Check the actual
        // on-disk dependents too, including the overrides they still carry.
        let mut pending = vec![id];
        let mut visited = BTreeSet::new();
        while let Some(asset) = pending.pop() {
            if !visited.insert(asset) {
                continue;
            }
            if let Some(children) = dependents.get(&asset) {
                pending.extend(children);
            }
            let (dependent_path, _, source) = files
                .get(&asset)
                .ok_or(ProjectError::MissingDocument(asset))?;
            let document = if asset == id { prefab } else { source };
            expand_prefabs_for_editor(document, catalog, |target| {
                if target == id {
                    return Ok(prefab.clone());
                }
                files
                    .get(&target)
                    .filter(|(_, is_prefab, _)| *is_prefab)
                    .map(|(_, _, source)| source.clone())
                    .ok_or(ProjectError::MissingPrefab(target))
            })
            .map_err(|source| ProjectError::PrefabDependent {
                path: dependent_path.clone(),
                source,
            })?;
        }
        Ok(prefab.write_with(path, schemas)?)
    }

    fn prefab_path(
        &self,
        index: &ProjectAssetDatabase,
        id: AssetId,
    ) -> Result<PathBuf, ProjectError> {
        if index
            .record_for_stable_id(id.into())
            .is_none_or(|record| record.source_kind() != &AssetSourceKind::Prefab)
        {
            return Err(ProjectError::MissingPrefab(id));
        }
        self.indexed_document_path(index, id)
    }

    fn indexed_document_path(
        &self,
        index: &ProjectAssetDatabase,
        id: AssetId,
    ) -> Result<PathBuf, ProjectError> {
        let record = index
            .record_for_stable_id(id.into())
            .filter(|record| {
                matches!(
                    record.source_kind(),
                    AssetSourceKind::Scene | AssetSourceKind::Prefab
                )
            })
            .ok_or(ProjectError::MissingDocument(id))?;
        let path = self
            .relative_asset_path(record.path())
            .ok_or(ProjectError::MissingDocument(id))?;
        self.document_path(&path)
    }

    pub fn set_entry_scene(&mut self, scene: Option<AssetPath>) -> Result<(), ProjectError> {
        if let Some(path) = &scene {
            if AssetKind::of(path) != AssetKind::Scene {
                return Err(ProjectError::NotDocument(path.clone()));
            }
            self.open_document(path)?;
        }
        let asset = scene.as_ref().and_then(|path| self.asset_reference(path));
        let settings = Settings {
            scene: scene.as_ref().map(ToString::to_string),
            scene_id: asset.as_ref().map(|asset| asset.id.to_string()),
        };
        let mut text = ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default())?;
        text.push('\n');
        write_atomic(&self.directory.join(SETTINGS), text.as_bytes())?;
        self.entry_scene = scene;
        self.entry_asset = asset.map(|asset| asset.id);
        Ok(())
    }
}

fn prefab_references(document: &SceneDocument) -> impl Iterator<Item = AssetId> + '_ {
    document
        .entities
        .iter()
        .filter_map(|row| row.prefab.as_ref().map(|prefab| prefab.asset))
}

fn read_small(path: &Path) -> io::Result<String> {
    if !path.metadata()?.is_file() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut text = String::new();
    File::open(path)?
        .take(SETTINGS_LIMIT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > SETTINGS_LIMIT {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(text)
}

fn within(candidate: &Path, root: &Path) -> io::Result<()> {
    if !candidate.starts_with(root) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Scene,
    Prefab,
    Image,
    File,
}

impl AssetKind {
    pub fn of(path: &AssetPath) -> Self {
        let name = path.as_str();
        if name.ends_with(".scene.ron") {
            Self::Scene
        } else if name.ends_with(".prefab.ron") {
            Self::Prefab
        } else if name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("png"))
        {
            Self::Image
        } else {
            Self::File
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub id: AssetId,
    pub path: AssetPath,
    pub kind: AssetKind,
}

#[derive(Debug, Clone, Default)]
pub struct AssetBrowserModel {
    entries: Vec<AssetEntry>,
}

impl AssetBrowserModel {
    pub fn entries(&self) -> &[AssetEntry] {
        &self.entries
    }
}
