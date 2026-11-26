#[cfg(any(unix, windows))]
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::{AssetId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathError, ComponentSchema,
    ComponentTypeId, ComponentValue, validate_component_value,
};
use nara_scene::authoring::{
    SceneAsset, SceneComponent, SceneDocument, SceneDocumentError, SceneEntity, SceneSchemaRebind,
    prefab::{ExpandedScene, PrefabError, PrefabOverrideError},
};
use thiserror::Error;

use crate::{EditorRegistry, SelectionState};

mod prefabs;
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
}

/// An authored document and its optional native save location.
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

/// Synchronous scene editing. History owns just the rows changed by completed edits.
pub struct EditorSession {
    document: EditorDocument,
    prefabs: BTreeMap<AssetId, SceneDocument>,
    expanded: Option<Arc<ExpandedScene>>,
    registry: EditorRegistry,
    selection: SelectionState,
    history: CommandHistory,
    gesture: Option<ActiveGesture>,
    saved_revision: Option<u64>,
}

/// Identifies the owner of one continuous editor gesture.
#[derive(Clone, Debug)]
pub struct EditorGesture(Arc<()>);

struct ActiveGesture {
    owner: EditorGesture,
    before: SceneDocument,
    expanded_before: Option<Arc<ExpandedScene>>,
}

impl EditorSession {
    pub fn new(document: EditorDocument, registry: EditorRegistry) -> Self {
        #[cfg(any(unix, windows))]
        let saved_revision = document.path.is_some().then_some(0);
        #[cfg(not(any(unix, windows)))]
        let saved_revision = None;
        Self {
            document,
            prefabs: BTreeMap::new(),
            expanded: None,
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
        self.expanded
            .as_ref()
            .map_or(&self.document.scene, |expanded| &expanded.document)
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
        self.expanded = expanded;
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
        self.expanded = prefabs::expand(&self.document.scene, &prefabs, &self.registry)?;
        self.prefabs = prefabs;
        self.prune_selection();
        Ok(())
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
        self.expanded = expanded;
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
                .is_some_and(|gesture| gesture.before != self.document.scene)
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
        let before = &self.document.scene;
        let mut after = before.clone();
        let mut projection = EditProjection {
            initial: self.expanded.as_ref(),
            changed: BTreeSet::new(),
        };
        apply_command(
            &mut after,
            &self.registry,
            &self.prefabs,
            &mut projection,
            command,
        )?;
        after.entities.sort_unstable_by_key(|row| row.id);
        if *before == after {
            return Ok(false);
        }
        after.validate()?;
        let expanded = prefabs::expand(&after, &self.prefabs, &self.registry)?;
        if self.gesture.is_none() {
            self.history.push(ScenePatch::between(before, &after));
        }
        self.document.scene = after;
        self.expanded = expanded;
        self.prune_selection();
        Ok(true)
    }

    pub fn begin_gesture(&mut self) -> Result<EditorGesture, EditorError> {
        self.require_idle()?;
        let owner = EditorGesture(Arc::new(()));
        self.gesture = Some(ActiveGesture {
            owner: owner.clone(),
            before: self.document.scene.clone(),
            expanded_before: self.expanded.clone(),
        });
        Ok(owner)
    }

    pub fn end_gesture(&mut self, owner: &EditorGesture) -> bool {
        if !self.owns_gesture(owner) {
            return false;
        }
        self.gesture.take().is_some_and(|gesture| {
            self.history
                .push(ScenePatch::between(&gesture.before, &self.document.scene))
        })
    }

    pub fn cancel_gesture(&mut self, owner: &EditorGesture) -> bool {
        if !self.owns_gesture(owner) {
            return false;
        }
        let Some(gesture) = self.gesture.take() else {
            return false;
        };
        self.document.scene = gesture.before;
        self.expanded = gesture.expanded_before;
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
        self.expanded = prefabs::expand(&scene, &self.prefabs, &self.registry)?;
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
        self.expanded = prefabs::expand(&scene, &self.prefabs, &self.registry)?;
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
        for asset in scene.assets.values_mut() {
            if let Some(path) = hint(asset.id) {
                asset.hint = path;
            }
        }
        let mut referenced = BTreeSet::new();
        prefabs::collect_authored_assets(scene, &self.registry, &mut referenced);
        for id in referenced {
            if let Some(path) = hint(id) {
                prefabs::declare_asset(scene, SceneAsset { id, hint: path });
            }
        }
    }

    fn prune_selection(&mut self) {
        let entities = self
            .preview_scene()
            .entities
            .iter()
            .map(|row| row.id)
            .collect();
        self.selection.retain(&entities);
    }
}

struct EditProjection<'a> {
    initial: Option<&'a Arc<ExpandedScene>>,
    changed: BTreeSet<EntityUuid>,
}

impl<'a> EditProjection<'a> {
    fn before_edit(&mut self, entity: EntityUuid) -> Option<&'a Arc<ExpandedScene>> {
        self.initial.filter(|_| self.changed.insert(entity))
    }
}

fn apply_command(
    scene: &mut SceneDocument,
    registry: &EditorRegistry,
    prefabs: &BTreeMap<AssetId, SceneDocument>,
    projection: &mut EditProjection<'_>,
    command: EditorCommand,
) -> Result<(), EditorError> {
    match command {
        EditorCommand::DeclareAsset(asset) => prefabs::declare_asset(scene, asset),
        EditorCommand::Create(entity) => {
            projection.initial = None;
            if scene.entities.iter().any(|row| row.id == entity.id) {
                return Err(EditorError::DuplicateEntity(entity.id));
            }
            check_name(&entity.name)?;
            scene.entities.push(entity);
        }
        EditorCommand::Delete(entities) => {
            projection.initial = None;
            delete_subtrees(scene, entities)?;
        }
        EditorCommand::Rename { entity, name } => {
            check_name(&name)?;
            row_mut(scene, entity)?.name = name;
        }
        EditorCommand::Reparent { entities, parent } => {
            projection.initial = None;
            for entity in entities {
                row_mut(scene, entity)?.parent = parent;
            }
        }
        EditorCommand::SetField {
            entities,
            component,
            path,
            value,
        } => {
            let schema = registry.editable(&component)?;
            prefabs::check_editable_field(schema, &path)?;
            for entity in entities {
                let view = projection.before_edit(entity);
                prefabs::set_field(
                    scene, prefabs, registry, view, entity, &component, &path, &value,
                )?;
            }
        }
        EditorCommand::SetComponent {
            entities,
            component,
            value,
        } => {
            let schema = registry.editable(&component)?;
            let payload = prefabs::known(&component, &value, schema)?;
            validate_component_value(schema, payload)?;
            for entity in entities {
                let view = projection.before_edit(entity);
                prefabs::set_component(scene, prefabs, registry, view, entity, &component, &value)?;
            }
        }
        EditorCommand::RemoveComponent {
            entities,
            component,
        } => {
            projection.initial = None;
            for entity in entities {
                prefabs::remove_component(scene, prefabs, registry, entity, &component)?;
            }
        }
        EditorCommand::RevertPrefabOverride(edit) => {
            projection.initial = None;
            prefabs::revert(scene, &edit)?;
        }
        EditorCommand::Batch(commands) => {
            for command in commands {
                apply_command(scene, registry, prefabs, projection, command)?;
            }
        }
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), EditorError> {
    if name.trim().is_empty() {
        Err(EditorError::EmptyName)
    } else {
        Ok(())
    }
}

fn row_mut(scene: &mut SceneDocument, entity: EntityUuid) -> Result<&mut SceneEntity, EditorError> {
    scene
        .entities
        .iter_mut()
        .find(|row| row.id == entity)
        .ok_or(EditorError::MissingEntity(entity))
}

fn delete_subtrees(
    scene: &mut SceneDocument,
    entities: Vec<EntityUuid>,
) -> Result<(), EditorError> {
    let mut children = BTreeMap::<_, Vec<_>>::new();
    let mut present = BTreeSet::new();
    for row in &scene.entities {
        present.insert(row.id);
        if let Some(parent) = row.parent {
            children.entry(parent).or_default().push(row.id);
        }
    }
    let mut removed = BTreeSet::new();
    let mut pending = entities;
    while let Some(entity) = pending.pop() {
        if !present.contains(&entity) {
            return Err(EditorError::MissingEntity(entity));
        }
        if removed.insert(entity) {
            pending.extend(children.remove(&entity).unwrap_or_default());
        }
    }
    scene.entities.retain(|row| !removed.contains(&row.id));
    Ok(())
}

#[derive(Clone, Default)]
struct CommandHistory {
    entries: Vec<HistoryEntry>,
    cursor: usize,
    next_revision: u64,
}

#[derive(Clone)]
struct HistoryEntry {
    patch: ScenePatch,
    revision: u64,
}

impl CommandHistory {
    fn revision(&self) -> u64 {
        self.cursor
            .checked_sub(1)
            .map_or(0, |index| self.entries[index].revision)
    }

    fn push(&mut self, patch: ScenePatch) -> bool {
        if patch.entities.is_empty() && patch.assets.is_none() {
            return false;
        }
        self.entries.truncate(self.cursor);
        self.next_revision += 1;
        self.entries.push(HistoryEntry {
            patch,
            revision: self.next_revision,
        });
        self.cursor += 1;
        true
    }

    fn rebind(
        &mut self,
        rebind: &SceneSchemaRebind<'_>,
        assets: &BTreeMap<String, SceneAsset>,
    ) -> Result<(), SceneDocumentError> {
        let (undo, redo) = self.entries.split_at_mut(self.cursor);
        rebind_history_rows(undo.iter_mut().rev(), rebind, assets, false)?;
        rebind_history_rows(redo.iter_mut(), rebind, assets, true)
    }
}

fn rebind_history_rows<'a>(
    entries: impl Iterator<Item = &'a mut HistoryEntry>,
    rebind: &SceneSchemaRebind<'_>,
    mut assets: &'a BTreeMap<String, SceneAsset>,
    forward: bool,
) -> Result<(), SceneDocumentError> {
    for entry in entries {
        let patch = &mut entry.patch;
        let (before, after) = patch
            .assets
            .as_ref()
            .map_or((assets, assets), |patch| (&patch.before, &patch.after));
        rebind.rebind_rows(&mut patch.before, before)?;
        rebind.rebind_rows(&mut patch.after, after)?;
        assets = if forward { after } else { before };
    }
    Ok(())
}

#[derive(Clone, Default)]
struct ScenePatch {
    entities: BTreeSet<EntityUuid>,
    before: Vec<SceneEntity>,
    after: Vec<SceneEntity>,
    assets: Option<AssetPatch>,
}

#[derive(Clone)]
struct AssetPatch {
    before: BTreeMap<String, SceneAsset>,
    after: BTreeMap<String, SceneAsset>,
}

impl ScenePatch {
    fn between(before: &SceneDocument, after: &SceneDocument) -> Self {
        let after_rows: BTreeMap<_, _> = after.entities.iter().map(|row| (row.id, row)).collect();
        let before_ids: BTreeSet<_> = before.entities.iter().map(|row| row.id).collect();
        let mut patch = Self::default();
        for row in &before.entities {
            if after_rows.get(&row.id).copied() != Some(row) {
                patch.entities.insert(row.id);
                patch.before.push(row.clone());
            }
        }
        for row in &after.entities {
            if !before_ids.contains(&row.id) || patch.entities.contains(&row.id) {
                patch.entities.insert(row.id);
                patch.after.push(row.clone());
            }
        }
        if before.assets != after.assets {
            patch.assets = Some(AssetPatch {
                before: before.assets.clone(),
                after: after.assets.clone(),
            });
        }
        patch
    }

    fn apply(&self, scene: &mut SceneDocument, forward: bool) {
        let rows = if forward { &self.after } else { &self.before };
        scene
            .entities
            .retain(|row| !self.entities.contains(&row.id));
        scene.entities.extend(rows.iter().cloned());
        scene.entities.sort_unstable_by_key(|row| row.id);
        if let Some(assets) = &self.assets {
            scene.assets = if forward {
                &assets.after
            } else {
                &assets.before
            }
            .clone();
        }
    }
}
