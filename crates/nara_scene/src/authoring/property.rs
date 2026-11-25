use std::collections::{BTreeMap, BTreeSet};

use nara_reflect::{
    ComponentFieldId, ComponentFieldPathError, ComponentPropertyError, ComponentPropertyPath,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    schema::exact_component_value_witness,
};

use super::{
    AuthoringHistoryScope, PrefabAuthoringSession, PreparedPrefabAuthoringEdit,
    PreparedSceneAuthoringEdit, SceneAuthoringRevision, SceneAuthoringSession,
    history_error_report,
};
use crate::{
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    authoring_decode::{
        AuthoringDocumentState, MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_LOGICAL_BYTES,
        MAX_AUTHORING_FIELD_VALUE_NODES, MAX_AUTHORING_FIELD_WRITES,
    },
    patch::{DefaultPropertyDirection, DefaultPropertyEdit},
};

/// One current-version replacement relative to an Available direct component in one document.
///
/// Property selectors are durable data; the session/revision and admitted Schema supply mutation
/// authority. A batch resolves in order against its evolving private values, including key edits.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringPropertyEdit {
    pub entity: SceneEntityId,
    pub component: ComponentTypeId,
    pub component_version: ComponentSchemaVersion,
    pub property: ComponentPropertyPath,
    pub value: ComponentValue,
}

#[derive(Clone, Copy)]
pub(super) struct StagedPropertyRecord<'a> {
    pub(super) schema: &'a ComponentSchema,
    pub(super) version: ComponentSchemaVersion,
    pub(super) value: &'a ComponentValue,
}

impl SceneAuthoringSession {
    /// Prepares ordered property replacements in one source candidate and history entry.
    pub fn prepare_authoring_property_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        let state = self.authority.published().ok_or_else(rejected)?;
        let patch = lower(&self.document.entities, state, edits, &BTreeMap::new())?;
        self.prepare_scoped_authoring_edit(base_revision, &patch, AuthoringHistoryScope::Ordinary)
    }
}

impl PrefabAuthoringSession {
    /// Uses the same lowering and final validation as Scene editing, retaining nominal Prefab data.
    pub fn prepare_authoring_property_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        let state = self.authority.published().ok_or_else(rejected)?;
        let patch = lower(&self.document.entities, state, edits, &BTreeMap::new())?;
        self.prepare_scoped_authoring_edit(base_revision, &patch, AuthoringHistoryScope::Ordinary)
    }
}

pub(super) fn lower<'a>(
    entities: &'a [SceneEntityRecord],
    state: &'a AuthoringDocumentState,
    edits: &[AuthoringPropertyEdit],
    staged: &BTreeMap<(SceneEntityId, ComponentTypeId), StagedPropertyRecord<'a>>,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    lower_with_write_limit(entities, state, edits, staged, MAX_AUTHORING_FIELD_WRITES)
}

pub(super) fn lower_construction<'a>(
    entities: &'a [SceneEntityRecord],
    state: &'a AuthoringDocumentState,
    edits: &[AuthoringPropertyEdit],
    staged: &BTreeMap<(SceneEntityId, ComponentTypeId), StagedPropertyRecord<'a>>,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    lower_with_write_limit(
        entities,
        state,
        edits,
        staged,
        MAX_AUTHORING_EDIT_OPERATIONS,
    )
}

fn lower_with_write_limit<'a>(
    entities: &'a [SceneEntityRecord],
    state: &'a AuthoringDocumentState,
    edits: &[AuthoringPropertyEdit],
    staged: &BTreeMap<(SceneEntityId, ComponentTypeId), StagedPropertyRecord<'a>>,
    maximum_writes: usize,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    // Bound original work, not just the coalesced patch: many writes to one root still cost work.
    if edits.is_empty() || edits.len() > maximum_writes {
        return Err(rejected());
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for (operation_index, edit) in edits.iter().enumerate() {
        bytes = bytes
            .saturating_add(edit.entity.as_str().len())
            .saturating_add(edit.component.as_str().len());
        if bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES {
            return Err(rejected_at(operation_index));
        }
        let selector_bytes = edit
            .property
            .logical_bytes()
            .map_err(|_| rejected_at(operation_index))?;
        bytes = bytes.saturating_add(selector_bytes);
        if bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES {
            return Err(rejected_at(operation_index));
        }
        let cost = edit
            .value
            .cost_up_to(
                MAX_AUTHORING_FIELD_VALUE_NODES.saturating_sub(nodes),
                MAX_AUTHORING_FIELD_LOGICAL_BYTES.saturating_sub(bytes),
            )
            .ok_or_else(|| rejected_at(operation_index))?;
        nodes = nodes.saturating_add(cost.nodes());
        bytes = bytes.saturating_add(cost.logical_bytes());
        if bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES {
            return Err(rejected_at(operation_index));
        }
    }

    let mut candidates = BTreeMap::<
        (SceneEntityId, ComponentTypeId),
        (
            &ComponentSchema,
            ComponentValue,
            Option<&BTreeMap<ComponentFieldId, ComponentValue>>,
        ),
    >::new();
    let root_updates = edits
        .iter()
        .filter(|edit| edit.property.steps.is_empty())
        .map(|edit| (&edit.entity, &edit.component, &edit.property.root))
        .collect::<BTreeSet<_>>();
    let mut nested_updates = Vec::new();
    let limits = state.property_staging_limits();
    for (operation_index, edit) in edits.iter().enumerate() {
        let key = (edit.entity.clone(), edit.component.clone());
        let (schema, candidate, contextual_defaults) = match candidates.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let original = entities
                    .iter()
                    .find(|entity| entity.id == edit.entity)
                    .and_then(|entity| entity.components.get(&edit.component));
                let target = match original {
                    Some(record) => StagedPropertyRecord {
                        schema: state
                            .available_component_schema(&edit.entity, &edit.component)
                            .ok_or_else(|| rejected_at(operation_index))?,
                        version: record.version,
                        value: record.value(),
                    },
                    None => *staged
                        .get(entry.key())
                        .ok_or_else(|| rejected_at(operation_index))?,
                };
                let schema = target.schema;
                if target.version != edit.component_version {
                    return Err(property_rejection(
                        &ComponentPropertyError::VersionMismatch {
                            expected: target.version,
                            actual: edit.component_version,
                        },
                        schema,
                        Some(operation_index),
                    ));
                }
                entry.insert((
                    schema,
                    target.value.clone(),
                    original.map(|record| &record.contextual_defaults),
                ))
            }
        };
        let target = (&edit.entity, &edit.component, &edit.property.root);
        let reject = |error| property_rejection(&error, schema, Some(operation_index));
        let materialized = initialize_missing_root(schema, candidate, *contextual_defaults, edit)
            .map_err(reject)?;
        edit.property
            .replace_in_candidate(
                schema,
                edit.component_version,
                candidate,
                edit.value.clone(),
                limits,
            )
            .map_err(reject)?;
        if !root_updates.contains(&target) {
            nested_updates.push(match materialized {
                Some(default) => ScenePatchOperation::DefaultProperty(DefaultPropertyEdit {
                    entity: edit.entity.clone(),
                    component: edit.component.clone(),
                    component_version: edit.component_version,
                    property: edit.property.clone(),
                    value: edit.value.clone(),
                    default_witness: exact_component_value_witness(default),
                    direction: DefaultPropertyDirection::Materialize,
                }),
                None => ScenePatchOperation::ReplaceProperty {
                    entity: edit.entity.clone(),
                    component: edit.component.clone(),
                    component_version: edit.component_version,
                    property: edit.property.clone(),
                    value: edit.value.clone(),
                },
            });
        }
    }

    let mut operations = Vec::with_capacity(root_updates.len() + nested_updates.len());
    for (entity, component, root) in root_updates {
        let (schema, candidate, _) = candidates
            .get(&(entity.clone(), component.clone()))
            .ok_or_else(rejected)?;
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == root)
            .ok_or_else(rejected)?;
        let value = candidate
            .get_path(field.path())
            .map_err(|_| rejected())?
            .clone();
        operations.push(ScenePatchOperation::SetField {
            entity: entity.clone(),
            component: component.clone(),
            component_version: schema.version(),
            field: root.clone(),
            value,
        });
    }
    operations.extend(nested_updates);
    Ok(ScenePatchDocument::new(operations))
}

fn initialize_missing_root<'a>(
    schema: &'a ComponentSchema,
    candidate: &mut ComponentValue,
    contextual_defaults: Option<&'a BTreeMap<ComponentFieldId, ComponentValue>>,
    edit: &AuthoringPropertyEdit,
) -> Result<Option<&'a ComponentValue>, ComponentPropertyError> {
    let Some(field) = schema
        .fields()
        .iter()
        .find(|field| field.id() == &edit.property.root)
    else {
        return Ok(None);
    };
    if matches!(
        candidate.get_path(field.path()),
        Err(ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. })
    ) {
        let default = if edit.property.steps.is_empty() {
            None
        } else {
            let Some(default) = contextual_defaults
                .and_then(|defaults| defaults.get(field.id()))
                .or(field.default_value())
            else {
                return Ok(None);
            };
            Some(default)
        };
        candidate
            .set_path(field.path(), default.unwrap_or(&edit.value).clone())
            .map_err(ComponentPropertyError::ValueUnavailable)?;
        return Ok(default);
    }
    Ok(None)
}

fn rejected() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-property-edit-rejected",
        "Property edits require bounded replacements of current Available source targets",
    )
}

fn rejected_at(operation_index: usize) -> ScenePatchReport {
    let mut diagnostics = nara_core::diagnostic::DiagnosticReport::default();
    diagnostics.push(rejection_diagnostic(Some(operation_index)));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn rejection_diagnostic(operation_index: Option<usize>) -> nara_core::diagnostic::ReportDiagnostic {
    let diagnostic = crate::diagnostics::error(
        "scene.authoring-property-edit-rejected",
        "Property edit was rejected before document publication",
    );
    if let Some(index) = operation_index {
        crate::diagnostics::with_public_u64(
            diagnostic,
            "operation-index",
            u64::try_from(index).unwrap_or(u64::MAX),
        )
    } else {
        diagnostic
    }
}

pub(crate) fn property_rejection(
    error: &ComponentPropertyError,
    schema: &ComponentSchema,
    operation_index: Option<usize>,
) -> ScenePatchReport {
    let mut diagnostics = nara_core::diagnostic::DiagnosticReport::default();
    diagnostics.push(crate::patch::with_property_error(
        rejection_diagnostic(operation_index),
        error,
        schema,
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}
