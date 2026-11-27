#[cfg(any(unix, windows))]
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::{AssetId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathError, ComponentSchema,
    ComponentTypeId, ComponentValue,
};
use nara_scene::authoring::{
    SceneAsset, SceneComponent, SceneDocument, SceneDocumentError, SceneEntity, SceneSchemaRebind,
    prefab::{ExpandedScene, PrefabError, PrefabOverrideError},
};
use thiserror::Error;

use crate::{EditorRegistry, SelectionState};

mod edits;
mod gesture;
mod history;
mod prefabs;

use edits::{RowSlot, row_index, row_mut};
use gesture::ActiveGesture;
use history::{CommandHistory, ScenePatch};
pub use prefabs::PrefabFieldOverride;

/// Typed changes to an authored scene or prefab. A batch is admitted as a single edit.
#[derive(Debug, Clone)]
pub enum EditorCommand {
    DeclareAsset(SceneAsset),
    Create(SceneEntity),
    Delete(Vec<EntityUuid>),
    Rename {
        entity: EntityUuid,
        name: String,
    },
    Reparent {
        entities: Vec<EntityUuid>,
        parent: Option<EntityUuid>,
    },
    SetField {
        entities: Vec<EntityUuid>,
        component: ComponentTypeId,
        path: ComponentFieldPath,
        value: ComponentValue,
    },
    SetComponent {
        entities: Vec<EntityUuid>,
        component: ComponentTypeId,
        value: SceneComponent,
    },
    RemoveComponent {
        entities: Vec<EntityUuid>,
        component: ComponentTypeId,
    },
    RevertPrefabOverride(PrefabFieldOverride),
    Batch(Vec<Self>),
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error(transparent)]
    Scene(#[from] SceneDocumentError),
    #[error(transparent)]
    Prefab(#[from] PrefabError),
    #[error(transparent)]
    PrefabOverride(#[from] PrefabOverrideError),
    #[error("prefab asset {0} is not available")]
    MissingPrefab(AssetId),
    #[error("entity {0} is not an editable prefab instance")]
    NotPrefabInstance(EntityUuid),
    #[error("the prefab override changed; try again from the Inspector")]
    StalePrefabOverride,
    #[error("this edit is not available on an inherited prefab child")]
    InheritedPrefabChild,
    #[error("entity reference {0} cannot be represented in the upstream prefab")]
    PrefabReferenceScope(EntityUuid),
    #[error("the field would resolve to a different value after applying it to the prefab")]
    PrefabApplyChangedValue,
    #[error("applying the override would invalidate an undo state: {0}")]
    PrefabHistoryWouldBreak(#[source] Box<Self>),
    #[error("could not publish prefab {asset}: {message}")]
    PublishPrefab { asset: AssetId, message: String },
    #[error(transparent)]
    Codec(#[from] ComponentCodecError),
    #[error(transparent)]
    FieldPath(#[from] ComponentFieldPathError),
    #[error("entity {0} does not exist")]
    MissingEntity(EntityUuid),
    #[error("entity {0} already exists")]
    DuplicateEntity(EntityUuid),
    #[error("an entity name must contain visible text")]
    EmptyName,
    #[error("'{0}' is not editable with the current Schema catalog")]
    ReadOnlyComponent(ComponentTypeId),
    #[error("field '{0}' is not editable")]
    ReadOnlyField(ComponentFieldPath),
    #[error("entity {entity} does not have component '{component}'")]
    MissingComponent {
        entity: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("finish or cancel the current gesture before continuing")]
    GestureActive,
    #[error("this gesture is no longer active")]
    StaleGesture,
    #[error("save requires a document path")]
    NoPath,
    #[error("the external file has a different document ID")]
    ReloadedDocumentId,
    #[error("reloading changed sources would invalidate an undo or redo state: {0}")]
    ReloadHistoryConflict(#[source] Box<Self>),
}

/// An authored document and its optional native save location.
#[derive(Clone)]
pub struct EditorDocument {
    scene: SceneDocument,
    #[cfg(any(unix, windows))]
    path: Option<PathBuf>,
}

impl EditorDocument {
    pub fn new(mut scene: SceneDocument) -> Result<Self, EditorError> {
        scene.validate()?;
        scene.entities.sort_unstable_by_key(|row| row.id);
        Ok(Self {
            scene,
            #[cfg(any(unix, windows))]
            path: None,
        })
    }

    #[cfg(any(unix, windows))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EditorError> {
        Self::open_with(path, &[])
    }

    #[cfg(any(unix, windows))]
    pub fn open_with(
        path: impl AsRef<Path>,
        schemas: &[ComponentSchema],
    ) -> Result<Self, EditorError> {
        let path = path.as_ref();
        let mut document = Self::new(read_editor_source(path, schemas)?)?;
        document.path = Some(path.to_path_buf());
        Ok(document)
    }

    #[cfg(any(unix, windows))]
    pub(crate) fn open_for_reload(
        path: PathBuf,
        schemas: &[ComponentSchema],
    ) -> Result<Self, EditorError> {
        let mut document = Self::new(SceneDocument::read_with(&path, schemas)?)?;
        document.path = Some(path);
        Ok(document)
    }

    pub fn scene(&self) -> &SceneDocument {
        &self.scene
    }

    #[cfg(any(unix, windows))]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(any(unix, windows))]
pub(crate) fn read_editor_source(
    path: &Path,
    schemas: &[ComponentSchema],
) -> Result<SceneDocument, SceneDocumentError> {
    let mut scene = SceneDocument::read(path)?;
    // Cached project schema may lag the file while the project does not compile.
    // Keep undecodable project records opaque. Built-ins are checked by `read`.
    scene.resolve_unknown_components(schemas)?;
    Ok(scene)
}

/// Synchronous scene editing with bounded Undo and Redo history for completed edits.
pub struct EditorSession {
    document: EditorDocument,
    prefabs: BTreeMap<AssetId, SceneDocument>,
    projection: SceneProjection,
    registry: EditorRegistry,
    selection: SelectionState,
    history: CommandHistory,
    gesture: Option<ActiveGesture>,
    saved_revision: Option<u64>,
}

/// Identifies the owner of one continuous editor gesture.
#[derive(Clone, Debug)]
pub struct EditorGesture(Arc<()>);

#[derive(Clone)]
enum SceneProjection {
    Authored,
    AwaitingPrefabs,
    Expanded(Arc<ExpandedScene>),
}

impl SceneProjection {
    fn admitted(expanded: Option<Arc<ExpandedScene>>) -> Self {
        expanded.map_or(Self::Authored, Self::Expanded)
    }

    fn expanded(&self) -> Option<&Arc<ExpandedScene>> {
        match self {
            Self::Expanded(expanded) => Some(expanded),
            Self::Authored | Self::AwaitingPrefabs => None,
        }
    }

    fn scene<'a>(&'a self, authored: &'a SceneDocument) -> &'a SceneDocument {
        self.expanded()
            .map_or(authored, |expanded| &expanded.document)
    }

    fn replace_rows(
        &mut self,
        scene: &mut SceneDocument,
        rows: impl IntoIterator<Item = (RowSlot, SceneEntity)>,
    ) {
        for (slot, row) in rows {
            if let (Self::Expanded(expanded), Some(projected)) = (&mut *self, slot.projected) {
                Arc::make_mut(expanded).document.entities[projected].clone_from(&row);
            }
            scene.entities[slot.authored] = row;
        }
    }
}

impl EditorSession {
    pub fn new(document: EditorDocument, registry: EditorRegistry) -> Self {
        #[cfg(any(unix, windows))]
        let saved_revision = document.path.is_some().then_some(0);
        #[cfg(not(any(unix, windows)))]
        let saved_revision = None;
        let projection = if document
            .scene
            .entities
            .iter()
            .any(|row| row.prefab.is_some())
        {
            SceneProjection::AwaitingPrefabs
        } else {
            SceneProjection::Authored
        };
        Self {
            document,
            prefabs: BTreeMap::new(),
            projection,
            registry,
            selection: SelectionState::default(),
            history: CommandHistory::default(),
            gesture: None,
            saved_revision,
        }
    }

    pub fn document(&self) -> &EditorDocument {
        &self.document
    }

    /// The current scene projected through its prefab instances for selection and preview.
    pub fn preview_scene(&self) -> &SceneDocument {
        self.projection.scene(&self.document.scene)
    }

    pub fn registry(&self) -> &EditorRegistry {
        &self.registry
    }

    pub fn set_registry(
        &mut self,
        registry: EditorRegistry,
    ) -> Result<Vec<SceneDocumentError>, EditorError> {
        self.require_idle()?;
        let rebind = SceneSchemaRebind::new(
            self.registry.catalog().components(),
            registry.catalog().components(),
        )?;
        let mut scene = self.document.scene.clone();
        scene.validate()?;
        let mut diagnostics = rebind.rebind_rows(&mut scene.entities, &scene.assets)?;
        let mut prefabs = self.prefabs.clone();
        for prefab in prefabs.values_mut() {
            prefab.validate()?;
            diagnostics.extend(rebind.rebind_rows(&mut prefab.entities, &prefab.assets)?);
        }
        let mut history = self.history.clone();
        history.rebind(&rebind, &scene.assets)?;
        let expanded = prefabs::expand(&scene, &prefabs, &registry)?;
        self.projection = SceneProjection::admitted(expanded);
        self.document.scene = scene;
        self.prefabs = prefabs;
        self.history = history;
        self.registry = registry;
        self.prune_selection();
        Ok(diagnostics)
    }

    pub fn set_prefabs(
        &mut self,
        prefabs: BTreeMap<AssetId, SceneDocument>,
    ) -> Result<(), EditorError> {
        self.require_idle()?;
        self.projection = SceneProjection::admitted(prefabs::expand(
            &self.document.scene,
            &prefabs,
            &self.registry,
        )?);
        self.prefabs = prefabs;
        self.prune_selection();
        Ok(())
    }

    /// Captures the edit state for a background preparation. Publish only against the same
    /// local edit state, keeping the current selection when it is published.
    pub fn snapshot_for_background(&self) -> Result<Self, EditorError> {
        self.require_idle()?;
        Ok(Self {
            document: self.document.clone(),
            prefabs: self.prefabs.clone(),
            projection: self.projection.clone(),
            registry: self.registry.clone(),
            selection: self.selection.clone(),
            history: self.history.clone(),
            gesture: None,
            saved_revision: self.saved_revision,
        })
    }

    /// Installs externally prepared sources without clearing the local Undo history. A changed
    /// clean scene adds an Undo step. Dirty scenes keep their authored rows and can still pick up
    /// a moved save location or updated prefab dependencies.
    #[cfg(any(unix, windows))]
    pub fn reload_sources(
        &mut self,
        disk: Option<EditorDocument>,
        replacements: BTreeMap<AssetId, SceneDocument>,
    ) -> Result<bool, EditorError> {
        self.require_idle()?;
        let changed_scene = match &disk {
            Some(disk) if disk.scene.id != self.document.scene.id => {
                return Err(EditorError::ReloadedDocumentId);
            }
            Some(disk) if !self.is_dirty() && self.document.scene != disk.scene => {
                Some(&disk.scene)
            }
            _ => None,
        };
        let scene = changed_scene.unwrap_or(&self.document.scene);
        let mut library = self.prefabs.clone();
        library.extend(replacements);
        let changed = changed_scene.is_some() || self.prefabs != library;
        if changed {
            let expanded = prefabs::expand(scene, &library, &self.registry)?;
            let mut history = self.history.clone();
            if let Some(scene) = changed_scene {
                history.insert_before_redo(ScenePatch::between(&self.document.scene, scene));
            }
            let (undo, redo) = history.entries.split_at_mut(history.cursor);
            let validate = |scene: &SceneDocument| -> Result<(), EditorError> {
                scene.validate()?;
                prefabs::expand(scene, &library, &self.registry)?;
                Ok(())
            };
            let history_error = |error| EditorError::ReloadHistoryConflict(Box::new(error));
            let mut past = scene.clone();
            for entry in undo.iter().rev() {
                entry.patch.apply(&mut past, false);
                validate(&past).map_err(history_error)?;
            }
            let mut future = scene.clone();
            for entry in redo {
                let before = future.clone();
                entry.patch.apply(&mut future, true);
                validate(&future).map_err(history_error)?;
                if changed_scene.is_some() {
                    entry.patch = ScenePatch::between(&before, &future);
                }
            }
            self.projection = SceneProjection::admitted(expanded);
            self.prefabs = library;
            self.history = history;
            if let Some(scene) = changed_scene {
                self.document.scene = scene.clone();
                self.saved_revision = Some(self.history.revision());
            }
            self.prune_selection();
        }
        if let Some(disk) = disk {
            self.document.path = disk.path;
        }
        Ok(changed)
    }

    /// Preserves a live selection when an asynchronously prepared session is published.
    pub fn keep_selection_from(&mut self, current: &Self) {
        self.selection.clone_from(&current.selection);
        self.prune_selection();
    }

    pub fn open_document(&mut self, document: EditorDocument) -> Result<(), EditorError> {
        self.open_document_with_prefabs(document, BTreeMap::new())
    }

    pub fn open_document_with_prefabs(
        &mut self,
        document: EditorDocument,
        prefabs: BTreeMap<AssetId, SceneDocument>,
    ) -> Result<(), EditorError> {
        self.require_idle()?;
        let expanded = prefabs::expand(&document.scene, &prefabs, &self.registry)?;
        let registry = std::mem::take(&mut self.registry);
        *self = Self::new(document, registry);
        self.prefabs = prefabs;
        self.projection = SceneProjection::admitted(expanded);
        Ok(())
    }

    pub fn selection(&self) -> &SelectionState {
        &self.selection
    }

    pub fn selection_mut(&mut self) -> &mut SelectionState {
        &mut self.selection
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_revision != Some(self.history.revision())
            || self
                .gesture
                .as_ref()
                .is_some_and(|gesture| gesture.changed(&self.document.scene))
    }

    pub fn can_undo(&self) -> bool {
        self.gesture.is_none() && self.history.cursor != 0
    }

    pub fn can_redo(&self) -> bool {
        self.gesture.is_none() && self.history.cursor != self.history.entries.len()
    }

    pub fn gesture_active(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn apply(&mut self, command: EditorCommand) -> Result<bool, EditorError> {
        self.require_idle()?;
        self.apply_edit(command)
    }

    pub fn owns_gesture(&self, owner: &EditorGesture) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| Arc::ptr_eq(&gesture.owner.0, &owner.0))
    }

    pub fn apply_gesture(
        &mut self,
        owner: &EditorGesture,
        command: EditorCommand,
    ) -> Result<bool, EditorError> {
        if !self.owns_gesture(owner) {
            return Err(EditorError::StaleGesture);
        }
        self.apply_edit(command)
    }

    fn apply_edit(&mut self, command: EditorCommand) -> Result<bool, EditorError> {
        let scene = &mut self.document.scene;
        if let Some(slots) = edits::local_slots(scene, &self.projection, &command) {
            let rows = edits::prepare_rows(scene, &self.registry, slots, command)?;
            if rows.is_empty() {
                return Ok(false);
            }
            if let Some(gesture) = &mut self.gesture {
                gesture.record_rows(scene, &rows);
            } else {
                self.history
                    .push(ScenePatch::from_rows(rows.iter().map(|(slot, row)| {
                        (scene.entities[slot.authored].clone(), row.clone())
                    })));
            }
            self.projection.replace_rows(scene, rows);
        } else {
            let Some((after, projection)) = edits::prepare_document(
                scene,
                &self.registry,
                &self.prefabs,
                self.projection.expanded(),
                command,
            )?
            else {
                return Ok(false);
            };
            let before = std::mem::replace(scene, after);
            let previous_projection = std::mem::replace(&mut self.projection, projection);
            if let Some(gesture) = &mut self.gesture {
                gesture.record_document(before, previous_projection);
            } else {
                self.history.push(ScenePatch::between(&before, scene));
            }
        }
        self.prune_selection();
        Ok(true)
    }

    pub fn begin_gesture(&mut self) -> Result<EditorGesture, EditorError> {
        self.require_idle()?;
        let owner = EditorGesture(Arc::new(()));
        self.gesture = Some(ActiveGesture::new(owner.clone()));
        Ok(owner)
    }

    pub fn end_gesture(&mut self, owner: &EditorGesture) -> bool {
        if !self.owns_gesture(owner) {
            return false;
        }
        self.gesture
            .take()
            .is_some_and(|gesture| self.history.push(gesture.finish(&self.document.scene)))
    }

    pub fn cancel_gesture(&mut self, owner: &EditorGesture) -> bool {
        if !self.owns_gesture(owner) {
            return false;
        }
        let Some(gesture) = self.gesture.take() else {
            return false;
        };
        gesture.restore(&mut self.document.scene, &mut self.projection);
        self.prune_selection();
        true
    }

    pub fn undo(&mut self) -> Result<bool, EditorError> {
        self.require_idle()?;
        if self.history.cursor == 0 {
            return Ok(false);
        }
        let cursor = self.history.cursor - 1;
        let mut scene = self.document.scene.clone();
        self.history.entries[cursor].patch.apply(&mut scene, false);
        self.projection =
            SceneProjection::admitted(prefabs::expand(&scene, &self.prefabs, &self.registry)?);
        self.document.scene = scene;
        self.history.cursor = cursor;
        self.prune_selection();
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditorError> {
        self.require_idle()?;
        let Some(entry) = self.history.entries.get(self.history.cursor) else {
            return Ok(false);
        };
        let mut scene = self.document.scene.clone();
        entry.patch.apply(&mut scene, true);
        self.projection =
            SceneProjection::admitted(prefabs::expand(&scene, &self.prefabs, &self.registry)?);
        self.document.scene = scene;
        self.history.cursor += 1;
        self.prune_selection();
        Ok(true)
    }

    #[cfg(any(unix, windows))]
    pub fn save(&mut self) -> Result<(), EditorError> {
        let path = self.document.path.clone().ok_or(EditorError::NoPath)?;
        self.save_as(path)
    }

    #[cfg(any(unix, windows))]
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), EditorError> {
        self.require_idle()?;
        self.document
            .scene
            .write_with(path.as_ref(), self.registry.catalog().components())?;
        self.document.path = Some(path.as_ref().to_path_buf());
        self.saved_revision = Some(self.history.revision());
        Ok(())
    }

    /// Saves to a new path, failing when another file already owns that name.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn save_new(&mut self, path: &Path) -> Result<(), EditorError> {
        use std::io::{self, Write as _};

        self.require_idle()?;
        let source = self
            .document
            .scene
            .to_ron_with(self.registry.catalog().components())?;
        (|| -> io::Result<()> {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let mut staging = tempfile::NamedTempFile::new_in(parent)?;
            staging.write_all(source.as_bytes())?;
            staging.as_file().sync_all()?;
            staging
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
            Ok(())
        })()
        .map_err(|source| SceneDocumentError::Io {
            path: path.to_owned(),
            source,
        })?;
        self.document.path = Some(path.to_owned());
        self.saved_revision = Some(self.history.revision());
        Ok(())
    }

    fn require_idle(&self) -> Result<(), EditorError> {
        if self.gesture_active() {
            Err(EditorError::GestureActive)
        } else {
            Ok(())
        }
    }

    /// Refreshes informational paths using the asset index that admitted their IDs.
    pub fn refresh_asset_hints(&mut self, hint: impl Fn(AssetId) -> Option<AssetPath>) {
        let scene = &mut self.document.scene;
        let mut assets = scene.assets.clone();
        let hint = |id| hint(id).filter(|path| *path != AssetPath::ROOT);
        for asset in assets.values_mut() {
            if let Some(path) = hint(asset.id) {
                asset.hint = path;
            }
        }
        let mut referenced = BTreeSet::new();
        prefabs::collect_authored_assets(scene, &self.registry, &mut referenced);
        for id in referenced {
            if let Some(path) = hint(id) {
                prefabs::declare_asset(&mut assets, SceneAsset { id, hint: path });
            }
        }
        if assets == scene.assets {
            return;
        }
        if let Some(gesture) = &mut self.gesture {
            gesture.record_assets(scene, &self.projection);
        }
        if let SceneProjection::Expanded(expanded) = &mut self.projection {
            let expanded = Arc::make_mut(expanded);
            expanded.document.assets.clone_from(&assets);
            for asset in assets.values().rev() {
                expanded.asset_hints.insert(asset.id, asset.hint.clone());
            }
        }
        scene.assets = assets;
    }

    fn prune_selection(&mut self) {
        let preview = self.projection.scene(&self.document.scene);
        self.selection
            .retain(|entity| row_index(preview, entity).is_some());
    }
}
