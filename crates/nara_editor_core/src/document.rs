#[cfg(any(unix, windows))]
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::EntityUuid;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldPath, ComponentFieldPathError,
    ComponentTypeId, ComponentValue, validate_component_value,
};
use nara_scene::authoring::{SceneComponent, SceneDocument, SceneDocumentError, SceneEntity};
use thiserror::Error;

use crate::{EditorRegistry, SelectionState};

/// Typed changes to an authored scene or prefab. A batch is admitted as a single edit.
#[derive(Debug, Clone)]
pub enum EditorCommand {
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
    Batch(Vec<Self>),
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error(transparent)]
    Scene(#[from] SceneDocumentError),
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
        let path = path.as_ref();
        let mut document = Self::new(SceneDocument::read(path)?)?;
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

/// Synchronous scene editing. History owns just the rows changed by completed edits.
pub struct EditorSession {
    document: EditorDocument,
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
}

impl EditorSession {
    pub fn new(document: EditorDocument, registry: EditorRegistry) -> Self {
        #[cfg(any(unix, windows))]
        let saved_revision = document.path.is_some().then_some(0);
        #[cfg(not(any(unix, windows)))]
        let saved_revision = None;
        Self {
            document,
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

    pub fn registry(&self) -> &EditorRegistry {
        &self.registry
    }

    pub fn set_registry(&mut self, registry: EditorRegistry) -> Result<(), EditorError> {
        self.require_idle()?;
        self.registry = registry;
        Ok(())
    }

    pub fn open_document(&mut self, document: EditorDocument) -> Result<(), EditorError> {
        self.require_idle()?;
        let registry = std::mem::take(&mut self.registry);
        *self = Self::new(document, registry);
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
                .is_some_and(|gesture| gesture.before.entities != self.document.scene.entities)
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
        apply_command(&mut after, &self.registry, command)?;
        after.entities.sort_unstable_by_key(|row| row.id);
        if before.entities == after.entities {
            return Ok(false);
        }
        after.validate()?;
        if self.gesture.is_none() {
            self.history.push(ScenePatch::between(before, &after));
        }
        self.document.scene = after;
        self.prune_selection();
        Ok(true)
    }

    pub fn begin_gesture(&mut self) -> Result<EditorGesture, EditorError> {
        self.require_idle()?;
        let owner = EditorGesture(Arc::new(()));
        self.gesture = Some(ActiveGesture {
            owner: owner.clone(),
            before: self.document.scene.clone(),
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
        self.prune_selection();
        true
    }

    pub fn undo(&mut self) -> Result<bool, EditorError> {
        self.require_idle()?;
        if self.history.cursor == 0 {
            return Ok(false);
        }
        self.history.cursor -= 1;
        self.history.entries[self.history.cursor]
            .patch
            .apply(&mut self.document.scene, false);
        self.prune_selection();
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditorError> {
        self.require_idle()?;
        let Some(entry) = self.history.entries.get(self.history.cursor) else {
            return Ok(false);
        };
        entry.patch.apply(&mut self.document.scene, true);
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
        self.document.scene.write(path.as_ref())?;
        self.document.path = Some(path.as_ref().to_path_buf());
        self.saved_revision = Some(self.history.revision());
        Ok(())
    }

    /// Saves to a new path, failing when another file already owns that name.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub fn save_new(&mut self, path: &Path) -> Result<(), EditorError> {
        use std::io::{self, Write as _};

        self.require_idle()?;
        let source = self.document.scene.to_ron()?;
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

    fn prune_selection(&mut self) {
        let entities = self
            .document
            .scene
            .entities
            .iter()
            .map(|row| row.id)
            .collect();
        self.selection.retain(&entities);
    }
}

fn apply_command(
    scene: &mut SceneDocument,
    registry: &EditorRegistry,
    command: EditorCommand,
) -> Result<(), EditorError> {
    match command {
        EditorCommand::Create(entity) => {
            if scene.entities.iter().any(|row| row.id == entity.id) {
                return Err(EditorError::DuplicateEntity(entity.id));
            }
            check_name(&entity.name)?;
            scene.entities.push(entity);
        }
        EditorCommand::Delete(entities) => delete_subtrees(scene, entities)?,
        EditorCommand::Rename { entity, name } => {
            check_name(&name)?;
            row_mut(scene, entity)?.name = name;
        }
        EditorCommand::Reparent { entities, parent } => {
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
            if !schema.fields().iter().any(|field| {
                field.path() == &path && field.has_capability(ComponentCapability::Edit)
            }) {
                return Err(EditorError::ReadOnlyField(path));
            }
            for entity in entities {
                let record = row_mut(scene, entity)?
                    .components
                    .get_mut(&component)
                    .ok_or_else(|| EditorError::MissingComponent {
                        entity,
                        component: component.clone(),
                    })?;
                if record.version != schema.version() {
                    return Err(EditorError::ReadOnlyComponent(component));
                }
                record.value.set_path(&path, value.clone())?;
                validate_component_value(schema, &record.value)?;
            }
        }
        EditorCommand::SetComponent {
            entities,
            component,
            value,
        } => {
            let schema = registry.editable(&component)?;
            if value.version != schema.version() {
                return Err(EditorError::ReadOnlyComponent(component));
            }
            validate_component_value(schema, &value.value)?;
            for entity in entities {
                let components = &mut row_mut(scene, entity)?.components;
                if let Some(current) = components.get(&component) {
                    if current.version != value.version {
                        return Err(EditorError::ReadOnlyComponent(component));
                    }
                    for field in schema.fields() {
                        if !field.has_capability(ComponentCapability::Edit)
                            && current.value.get_path(field.path()).ok()
                                != value.value.get_path(field.path()).ok()
                        {
                            return Err(EditorError::ReadOnlyField(field.path().clone()));
                        }
                    }
                }
                components.insert(component.clone(), value.clone());
            }
        }
        EditorCommand::RemoveComponent {
            entities,
            component,
        } => {
            for entity in entities {
                row_mut(scene, entity)?.components.remove(&component);
            }
        }
        EditorCommand::Batch(commands) => {
            for command in commands {
                apply_command(scene, registry, command)?;
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

#[derive(Default)]
struct CommandHistory {
    entries: Vec<HistoryEntry>,
    cursor: usize,
    next_revision: u64,
}

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
        if patch.entities.is_empty() {
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
}

#[derive(Default)]
struct ScenePatch {
    entities: BTreeSet<EntityUuid>,
    before: Vec<SceneEntity>,
    after: Vec<SceneEntity>,
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
        patch
    }

    fn apply(&self, scene: &mut SceneDocument, forward: bool) {
        let rows = if forward { &self.after } else { &self.before };
        scene
            .entities
            .retain(|row| !self.entities.contains(&row.id));
        scene.entities.extend(rows.iter().cloned());
        scene.entities.sort_unstable_by_key(|row| row.id);
    }
}
