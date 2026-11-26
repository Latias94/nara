use std::rc::Rc;

use nara_core::{DocumentId, EntityUuid};
use nara_editor_core::{EditorCommand, EditorSession, InspectorValue};
use nara_reflect::{
    ComponentCapability, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueShape,
};
use nara_scene::authoring::{SceneComponent, SceneEntity};

pub(super) const CHANGED: &str = "The target or value changed. Cancel and reopen this edit.";
pub(super) const SELECTION_CHANGED: &str = "The Inspector selection changed.";

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct Selection {
    document: DocumentId,
    pub targets: Vec<EntityUuid>,
}

impl Selection {
    pub fn new(session: &EditorSession) -> Self {
        Self {
            document: session.document().scene().id,
            targets: session.selection().entities().to_vec(),
        }
    }

    pub fn matches(&self, session: &EditorSession) -> bool {
        self.document == session.document().scene().id
            && self.targets == session.selection().entities()
    }

    pub fn exists_in(&self, session: &EditorSession) -> bool {
        if self.document != session.document().scene().id || self.targets.is_empty() {
            return false;
        }
        let rows = &session.preview_scene().entities;
        self.targets
            .iter()
            .all(|id| rows.binary_search_by_key(id, |row| row.id).is_ok())
    }

    pub fn has_component(&self, session: &EditorSession, component: &ComponentTypeId) -> bool {
        if !self.exists_in(session) {
            return false;
        }
        let rows = &session.preview_scene().entities;
        self.targets.iter().all(|id| {
            rows.binary_search_by_key(id, |row| row.id)
                .ok()
                .is_some_and(|index| rows[index].components.contains_key(component))
        })
    }

    pub fn rows<'session>(
        &self,
        session: &'session EditorSession,
    ) -> Option<Vec<&'session SceneEntity>> {
        if !self.matches(session) {
            return None;
        }
        let rows = &session.preview_scene().entities;
        self.targets
            .iter()
            .map(|id| rows.iter().find(|row| row.id == *id))
            .collect()
    }

    pub fn rename_command(
        &self,
        session: &EditorSession,
        expected: &str,
        name: String,
    ) -> Result<Option<EditorCommand>, &'static str> {
        let rows = self.rows(session).ok_or(SELECTION_CHANGED)?;
        let [row] = rows.as_slice() else {
            return Err(SELECTION_CHANGED);
        };
        if row.name != expected {
            return Err(CHANGED);
        }
        if name.trim().is_empty() {
            return Err("Enter a name that contains visible text.");
        }
        Ok((name != row.name).then_some(EditorCommand::Rename {
            entity: row.id,
            name,
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct FieldKey {
    pub component: ComponentTypeId,
    pub path: ComponentFieldPath,
    pub member: ComponentFieldPath,
}

#[derive(Clone, PartialEq)]
pub(super) struct FieldTarget {
    pub selection: Rc<Selection>,
    pub key: FieldKey,
    pub version: ComponentSchemaVersion,
    pub field: Rc<ComponentFieldSchema>,
    pub shape: ComponentValueShape,
}

impl FieldTarget {
    pub fn command(
        &self,
        session: &EditorSession,
        expected: &[Option<ComponentValue>],
        value: ComponentValue,
    ) -> Result<Option<EditorCommand>, String> {
        let rows = self.selection.rows(session).ok_or(SELECTION_CHANGED)?;
        let field = session
            .registry()
            .schema(&self.key.component)
            .filter(|schema| {
                schema.version() == self.version && schema.has_capability(ComponentCapability::Edit)
            })
            .and_then(|schema| {
                schema
                    .fields()
                    .iter()
                    .find(|field| *field == self.field.as_ref())
            })
            .filter(|field| field.has_capability(ComponentCapability::Edit))
            .ok_or(CHANGED)?;
        let roots = rows
            .iter()
            .map(|row| {
                let record = row.components.get(&self.key.component)?;
                (record.version() == self.version && record.value().is_some())
                    .then(|| field_value(record, field))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(CHANGED)?;
        let live = roots.iter().map(|root| project(*root, &self.key.member));
        if !live.eq(expected.iter().map(Option::as_ref)) {
            return Err(CHANGED.into());
        }
        self.shape
            .validate_value(&value)
            .map_err(|error| error.to_string())?;
        if expected
            .iter()
            .all(|current| current.as_ref() == Some(&value))
        {
            return Ok(None);
        }
        let path = ComponentFieldPath::new(
            self.key
                .path
                .segments()
                .iter()
                .chain(self.key.member.segments())
                .cloned(),
        );
        Ok(Some(EditorCommand::SetField {
            entities: self.selection.targets.clone(),
            component: self.key.component.clone(),
            path,
            value,
        }))
    }
}

pub(super) fn field_value<'record>(
    record: &'record SceneComponent,
    field: &'record ComponentFieldSchema,
) -> Option<&'record ComponentValue> {
    record
        .value()?
        .get_path(field.path())
        .ok()
        .or_else(|| field.default_value())
}

pub(super) fn project<'value>(
    value: Option<&'value ComponentValue>,
    member: &ComponentFieldPath,
) -> Option<&'value ComponentValue> {
    value.and_then(|root| root.get_path(member).ok())
}

pub(super) fn agreement(values: &[Option<ComponentValue>]) -> InspectorValue {
    match values.split_first() {
        Some((value, rest)) if rest.iter().any(|other| other != value) => InspectorValue::Mixed,
        Some((Some(value), _)) => InspectorValue::Shared(value.clone()),
        _ => InspectorValue::Missing,
    }
}

pub(super) struct ComponentRemoval {
    pub id: ComponentTypeId,
    present: Vec<(EntityUuid, ComponentSchemaVersion)>,
}

impl ComponentRemoval {
    pub fn new(id: ComponentTypeId, present: Vec<(EntityUuid, ComponentSchemaVersion)>) -> Self {
        Self { id, present }
    }

    pub fn command(&self, session: &EditorSession, selection: &Selection) -> Option<EditorCommand> {
        let rows = selection.rows(session)?;
        let present = rows
            .iter()
            .filter(|row| !session.is_inherited_entity(row.id))
            .filter_map(|row| {
                let record = row.components.get(&self.id)?;
                Some((row.id, record.version()))
            });
        if self.present.is_empty() || !present.eq(self.present.iter().copied()) {
            return None;
        }
        Some(EditorCommand::RemoveComponent {
            entities: self.present.iter().map(|(entity, _)| *entity).collect(),
            component: self.id.clone(),
        })
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct AddComponent {
    pub id: ComponentTypeId,
    pub label: String,
    version: ComponentSchemaVersion,
    initial: ComponentValue,
    missing: Vec<EntityUuid>,
}

impl AddComponent {
    pub fn options(session: &EditorSession, rows: &[&SceneEntity]) -> Vec<Self> {
        let mut options: Vec<_> = session
            .registry()
            .schemas()
            .filter(|schema| schema.has_capability(ComponentCapability::Edit))
            .filter_map(|schema| {
                let initial = schema.authoring_initial_value()?;
                let missing: Vec<_> = rows
                    .iter()
                    .filter(|row| {
                        !session.is_inherited_entity(row.id)
                            && !row.components.contains_key(schema.id())
                    })
                    .map(|row| row.id)
                    .collect();
                if missing.is_empty() {
                    return None;
                }
                let name = schema
                    .aliases()
                    .first()
                    .map_or(schema.id().as_str(), String::as_str);
                let label = if missing.len() == rows.len() {
                    name.to_owned()
                } else {
                    format!("{name} · {} missing", missing.len())
                };
                Some(Self {
                    id: schema.id().clone(),
                    label,
                    version: schema.version(),
                    initial: initial.clone(),
                    missing,
                })
            })
            .collect();
        options.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
        options
    }

    pub fn command(&self, session: &EditorSession, selection: &Selection) -> Option<EditorCommand> {
        let rows = selection.rows(session)?;
        let schema = session.registry().schema(&self.id)?;
        let missing = rows
            .iter()
            .filter(|row| {
                !session.is_inherited_entity(row.id) && !row.components.contains_key(&self.id)
            })
            .map(|row| row.id);
        if schema.version() != self.version
            || !schema.has_capability(ComponentCapability::Edit)
            || schema.authoring_initial_value() != Some(&self.initial)
            || !missing.eq(self.missing.iter().copied())
        {
            return None;
        }
        Some(EditorCommand::SetComponent {
            entities: self.missing.clone(),
            component: self.id.clone(),
            value: SceneComponent::known(self.version, self.initial.clone()),
        })
    }
}
