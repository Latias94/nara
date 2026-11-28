use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use nara_asset::{
    AssetDatabaseError, AssetMeta, AssetMetaError, AssetRecord, AssetScanError, AssetScanMode,
    AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
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
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::{EditorSession, document::stage_new_file};

const SETTINGS: &str = ".nara.ron";
const SMALL_FILE_LIMIT: u64 = 64 * 1024;

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
    #[error("{0} does not name a project scene")]
    NotScene(AssetPath),
    #[error("invalid entry scene asset UUID: {0}")]
    EntryAsset(#[from] UuidIdError),
    #[error("entry scene asset {0} is missing or is not a scene")]
    InvalidEntryScene(AssetId),
    #[error("'{0}' is not a valid project asset directory")]
    AssetDirectory(String),
    #[error(transparent)]
    Assets(#[from] AssetScanError),
    #[error(transparent)]
    Sidecar(#[from] AssetMetaError),
    #[error(transparent)]
    AssetIndex(#[from] AssetDatabaseError),
    #[error("prefab asset {0} is missing or is not a prefab")]
    MissingPrefab(AssetId),
    #[error("prefab {0} has changed on disk; refresh Assets and try again")]
    PrefabChanged(AssetId),
    #[error("scene or prefab asset {0} is no longer present in the project")]
    MissingDocument(AssetId),
    #[error("more than one scene or prefab uses asset ID {0}")]
    AmbiguousDocument(AssetId),
    #[error("document {0} was replaced on disk; reopen it before saving")]
    DocumentChanged(DocumentId),
    #[error("A file already exists at {}. Choose a new filename.", .0.display())]
    DocumentExists(PathBuf),
    #[error("could not remove the new sidecar '{}' after saving failed ({save}): {source}", path.display())]
    NewDocumentCleanup {
        path: PathBuf,
        save: EditorError,
        #[source]
        source: io::Error,
    },
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
        let entry_asset: Option<AssetId> =
            settings.scene_id.as_deref().map(str::parse).transpose()?;
        let entry_scene = if entry_asset.is_none() {
            settings.scene.map(AssetPath::new).transpose()?
        } else {
            None
        };
        if let Some(path) = &entry_scene
            && AssetKind::of(path) != AssetKind::Scene
        {
            return Err(ProjectError::NotScene(path.clone()));
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
            database: ProjectAssetDatabase::default(),
            manifest,
            entry_scene,
            entry_asset,
        };
        project.install_database(database)?;
        if let Some(id) = entry_asset
            && project.entry_scene.is_none()
        {
            return Err(ProjectError::InvalidEntryScene(id));
        }
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
        self.install_database(self.scan(AssetScanMode::Discover)?)
    }

    /// Rescans external changes without creating sidecars while another tool is moving files.
    pub fn rescan_assets(&mut self) -> Result<(), ProjectError> {
        self.install_database(self.scan(AssetScanMode::ReadOnly)?)
    }

    fn install_database(&mut self, database: ProjectAssetDatabase) -> Result<(), ProjectError> {
        (self.entry_scene, self.entry_asset) = self.resolve_entry(&database)?;
        self.database = database;
        Ok(())
    }

    fn resolve_entry(
        &self,
        database: &ProjectAssetDatabase,
    ) -> Result<(Option<AssetPath>, Option<AssetId>), ProjectError> {
        if let Some(id) = self.entry_asset {
            let path = database
                .record_for_stable_id(id.into())
                .map(|record| {
                    self.relative_asset_path(record.path())
                        .filter(|path| is_scene_record(path, record))
                        .ok_or(ProjectError::InvalidEntryScene(id))
                })
                .transpose()?;
            return Ok((path, Some(id)));
        }
        let id = match &self.entry_scene {
            Some(path) => self.entry_id_at(database, path)?,
            None => None,
        };
        Ok((self.entry_scene.clone(), id))
    }

    fn entry_id_at(
        &self,
        database: &ProjectAssetDatabase,
        path: &AssetPath,
    ) -> Result<Option<AssetId>, ProjectError> {
        let key = self.project_asset_path(path)?;
        let Some(record) = database.record_for_path(&key) else {
            return Ok(None);
        };
        if !is_scene_record(path, record) {
            return Err(ProjectError::NotScene(path.clone()));
        }
        Ok(Some(record.stable_id().try_into()?))
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
        let record = self
            .database
            .record_for_path(&self.project_asset_path(path).ok()?)?;
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

    fn project_asset_path(&self, path: &AssetPath) -> io::Result<AssetSourcePath> {
        let relative = AssetSourcePath::new(path.as_str()).map_err(io::Error::other)?;
        Ok(self.asset_prefix.joined(&relative))
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
        let mut physical = root.root().to_path_buf();
        physical.extend(path.as_str().split('/'));
        let physical = physical.canonicalize()?;
        within(&physical, root.root())?;
        Ok(physical)
    }

    /// Resolves a named document again before an overwrite. A move with its sidecar is accepted.
    pub fn document_save_path(
        &mut self,
        asset: AssetId,
        document: DocumentId,
    ) -> Result<PathBuf, ProjectError> {
        let record = self
            .database
            .record_for_stable_id(asset.into())
            .ok_or(ProjectError::MissingDocument(asset))?;
        let kind = match record.source_kind() {
            AssetSourceKind::Scene => AssetKind::Scene,
            AssetSourceKind::Prefab => AssetKind::Prefab,
            _ => return Err(ProjectError::MissingDocument(asset)),
        };
        let previous = self
            .relative_asset_path(record.path())
            .ok_or(ProjectError::MissingDocument(asset))?;
        let source = self.asset_source()?;
        let (relative, path, meta) = match document_at(source.root(), &previous, asset, kind) {
            Some((path, meta)) => (previous.clone(), path, meta),
            None => find_moved_document(source.root(), asset, kind)?,
        };
        if SceneDocument::read(&path)?.id != document {
            return Err(ProjectError::DocumentChanged(document));
        }
        if relative != previous {
            let mut database = self.database.clone();
            database.remove_exact(record);
            let next = self.project_asset_path(&relative)?;
            self.remove_replaced_record(&mut database, &next, &path)?;
            database.insert(AssetRecord::from_meta(next, meta))?;
            self.install_database(database)?;
        }
        Ok(path)
    }

    /// Writes a new scene and its stable asset ID without discovering other sources.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn save_new_scene(
        &mut self,
        session: &mut EditorSession,
        destination: &Path,
    ) -> Result<AssetId, ProjectError> {
        let mut staged_session = session.snapshot_for_background()?;
        let filename = destination
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                destination
                    .as_os_str()
                    .as_encoded_bytes()
                    .ends_with(name.as_bytes())
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "use a portable scene filename")
            })?;
        let parent = destination
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "scene path has no parent"))?
            .canonicalize()?;
        let destination = parent.join(filename);
        let path = self.relative_document_path(&destination)?;
        if AssetKind::of(&path) != AssetKind::Scene
            || path.as_str().split('/').any(|part| part.starts_with('.'))
        {
            return Err(ProjectError::NotScene(path));
        }
        let folded_filename = filename.to_lowercase();
        let folded_sidecar = format!("{folded_filename}.meta");
        for entry in fs::read_dir(&parent)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name == folded_filename || name == folded_sidecar {
                return Err(ProjectError::DocumentExists(entry.path()));
            }
        }

        let mut database = self.database.clone();
        let key = self.project_asset_path(&path)?;
        let id = AssetId::try_from_uuid(nara_core::identity::try_uuid_v7()?)?;
        self.remove_replaced_record(&mut database, &key, &destination)?;
        let meta = format!("AssetMeta(id: \"{id}\", importer: \"nara.scene\", settings: ())\n");
        database.insert(AssetRecord::from_meta(key, AssetMeta::from_ron(&meta)?))?;
        let entry = self.resolve_entry(&database)?;

        staged_session.refresh_asset_hints(|id| self.asset_hint(id));
        let staged_scene = staged_session.stage_new(&destination)?;
        let sidecar = parent.join(format!("{filename}.meta"));
        let staged_sidecar = stage_new_file(&sidecar, meta.as_bytes())?;
        drop(
            staged_sidecar
                .persist_noclobber(&sidecar)
                .map_err(|error| error.error)?,
        );
        if let Err(save) = staged_session.publish_new(&destination, staged_scene) {
            if read_small(&sidecar).is_ok_and(|current| current == meta)
                && let Err(source) = fs::remove_file(&sidecar)
                && source.kind() != io::ErrorKind::NotFound
            {
                return Err(ProjectError::NewDocumentCleanup {
                    path: sidecar,
                    save,
                    source,
                });
            }
            return Err(save.into());
        }
        self.database = database;
        (self.entry_scene, self.entry_asset) = entry;
        *session = staged_session;
        Ok(id)
    }

    fn remove_replaced_record(
        &self,
        database: &mut ProjectAssetDatabase,
        key: &AssetSourcePath,
        destination: &Path,
    ) -> io::Result<()> {
        let Some(previous) = database.record_for_path_ignoring_case(key).cloned() else {
            return Ok(());
        };
        if previous.path() != key {
            let old_path = self.directory.join(previous.path().as_str());
            match old_path.canonicalize() {
                Ok(actual) if actual != destination => return Ok(()),
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        database.remove_exact(&previous);
        Ok(())
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
        let asset = if let Some(path) = &scene {
            if AssetKind::of(path) != AssetKind::Scene {
                return Err(ProjectError::NotScene(path.clone()));
            }
            self.open_document(path)?;
            self.entry_id_at(&self.database, path)?
        } else {
            None
        };
        let settings = Settings {
            scene: scene.as_ref().map(ToString::to_string),
            scene_id: asset.map(|asset| asset.to_string()),
        };
        let mut text = ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default())?;
        text.push('\n');
        write_atomic(&self.directory.join(SETTINGS), text.as_bytes())?;
        self.entry_scene = scene;
        self.entry_asset = asset;
        Ok(())
    }
}

fn is_scene_record(path: &AssetPath, record: &AssetRecord) -> bool {
    AssetKind::of(path) == AssetKind::Scene && record.source_kind() == &AssetSourceKind::Scene
}

fn document_at(
    root: &Path,
    path: &AssetPath,
    id: AssetId,
    kind: AssetKind,
) -> Option<(PathBuf, AssetMeta)> {
    if AssetKind::of(path) != kind {
        return None;
    }
    let mut document = root.to_path_buf();
    document.extend(path.as_str().split('/'));
    let sidecar = document.with_file_name(format!("{}.meta", document.file_name()?.to_str()?));
    if !is_exact_file(&sidecar) {
        return None;
    }
    let meta = AssetMeta::from_ron(read_small(&sidecar).ok()?).ok()?;
    let matches_kind = matches!(
        (kind, meta.source_kind()),
        (AssetKind::Scene, AssetSourceKind::Scene) | (AssetKind::Prefab, AssetSourceKind::Prefab)
    );
    (meta.id() == id.into() && matches_kind && is_exact_file(&document)).then_some((document, meta))
}

fn is_exact_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        && path.canonicalize().is_ok_and(|resolved| resolved == path)
}

fn find_moved_document(
    root: &Path,
    id: AssetId,
    kind: AssetKind,
) -> Result<(AssetPath, PathBuf, AssetMeta), ProjectError> {
    let mut directories = vec![(String::new(), root.to_path_buf())];
    let mut found = None;
    while let Some((prefix, directory)) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| !name.starts_with('.')) else {
                continue;
            };
            let file_type = entry.file_type()?;
            if file_type.is_dir()
                && entry
                    .path()
                    .canonicalize()
                    .is_ok_and(|resolved| resolved == entry.path())
            {
                directories.push((format!("{prefix}{name}/"), entry.path()));
            } else if file_type.is_file()
                && let Some(base) = name.strip_suffix(".meta")
                && let Ok(path) = AssetPath::new(format!("{prefix}{base}"))
                && let Some((native, meta)) = document_at(root, &path, id, kind)
            {
                if found.is_some() {
                    return Err(ProjectError::AmbiguousDocument(id));
                }
                found = Some((path, native, meta));
            }
        }
    }
    found.ok_or(ProjectError::MissingDocument(id))
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
        .take(SMALL_FILE_LIMIT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > SMALL_FILE_LIMIT {
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
        let name = path.as_str().to_ascii_lowercase();
        if name.ends_with(".scene.ron") {
            Self::Scene
        } else if name.ends_with(".prefab.ron") {
            Self::Prefab
        } else if name.ends_with(".png") {
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
