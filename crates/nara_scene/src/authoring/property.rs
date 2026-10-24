use std::collections::{BTreeMap, BTreeSet};

use nara_reflect::{
    ComponentFieldId, ComponentPropertyError, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, replace_component_property,
};

use super::{
    PrefabAuthoringSession, PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit,
    SceneAuthoringRevision, SceneAuthoringSession, history_error_report,
};
use crate::{
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    authoring_decode::{
        AuthoringDocumentState, MAX_AUTHORING_FIELD_LOGICAL_BYTES, MAX_AUTHORING_FIELD_VALUE_NODES,
        MAX_AUTHORING_FIELD_WRITES,
    },
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
    /// Prepares ordered property replacements, coalescing roots into one existing history entry.
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
        self.prepare_authoring_edit(base_revision, &patch)
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
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

pub(super) fn lower<'a>(
    entities: &'a [SceneEntityRecord],
    state: &'a AuthoringDocumentState,
    edits: &[AuthoringPropertyEdit],
    staged: &BTreeMap<(SceneEntityId, ComponentTypeId), StagedPropertyRecord<'a>>,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    // Bound original work, not just the coalesced patch: many writes to one root still cost work.
    if edits.is_empty() || edits.len() > MAX_AUTHORING_FIELD_WRITES {
        return Err(rejected());
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for (operation_index, edit) in edits.iter().enumerate() {
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

    let mut candidates =
        BTreeMap::<(SceneEntityId, ComponentTypeId), (&ComponentSchema, ComponentValue)>::new();
    let mut touched = BTreeSet::<(SceneEntityId, ComponentTypeId, ComponentFieldId)>::new();
    for (operation_index, edit) in edits.iter().enumerate() {
        let key = (edit.entity.clone(), edit.component.clone());
        let (schema, candidate) = match candidates.entry(key) {
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
                        value: &record.value,
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
                        operation_index,
                    ));
                }
                entry.insert((schema, target.value.clone()))
            }
        };
        replace_component_property(
            schema,
            edit.component_version,
            candidate,
            &edit.property,
            edit.value.clone(),
        )
        .map_err(|error| property_rejection(&error, schema, operation_index))?;
        touched.insert((
            edit.entity.clone(),
            edit.component.clone(),
            edit.property.root.clone(),
        ));
    }

    let mut operations = Vec::with_capacity(touched.len());
    for (entity, component, root) in touched {
        let (schema, candidate) = candidates
            .get(&(entity.clone(), component.clone()))
            .ok_or_else(rejected)?;
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == &root)
            .ok_or_else(rejected)?;
        let value = candidate
            .get_path(field.path())
            .map_err(|_| rejected())?
            .clone();
        operations.push(ScenePatchOperation::SetField {
            entity,
            component,
            component_version: schema.version(),
            field: root,
            value,
        });
    }
    Ok(ScenePatchDocument::new(operations))
}

fn rejected() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-property-edit-rejected",
        "Property edits require bounded replacements of current Available source targets",
    )
}

fn rejected_at(operation_index: usize) -> ScenePatchReport {
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(rejection_diagnostic(operation_index));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn rejection_diagnostic(operation_index: usize) -> nara_diagnostic::Diagnostic {
    crate::diagnostics::with_public_u64(
        crate::diagnostics::error(
            "scene.authoring-property-edit-rejected",
            "Property edit was rejected before document publication",
        ),
        "operation-index",
        u64::try_from(operation_index).unwrap_or(u64::MAX),
    )
}

fn property_rejection(
    error: &ComponentPropertyError,
    schema: &ComponentSchema,
    operation_index: usize,
) -> ScenePatchReport {
    let (kind, step) = match error {
        ComponentPropertyError::PathDepthLimit => ("path-depth-limit", None),
        ComponentPropertyError::PathByteLimit => ("path-byte-limit", None),
        ComponentPropertyError::InvalidFieldId(_) => ("invalid-field-id", None),
        ComponentPropertyError::VersionMismatch { .. } => ("version-mismatch", None),
        ComponentPropertyError::InvalidSchema(_) => ("invalid-schema", None),
        ComponentPropertyError::ValueBudget(_) => ("value-budget", None),
        ComponentPropertyError::InvalidValue(_) => ("invalid-value", None),
        ComponentPropertyError::UnknownRoot(_) => ("unknown-root", None),
        ComponentPropertyError::RetiredField(_) => ("retired-field", None),
        ComponentPropertyError::RetiredMember(_) => ("retired-member", None),
        ComponentPropertyError::ValueUnavailable(_) => ("value-unavailable", None),
        ComponentPropertyError::OptionalAbsent { step } => ("optional-absent", Some(*step)),
        ComponentPropertyError::ExpectedRecord { step } => ("expected-record", Some(*step)),
        ComponentPropertyError::MemberNotFound { step, .. } => ("member-not-found", Some(*step)),
        ComponentPropertyError::ExpectedList { step } => ("expected-list", Some(*step)),
        ComponentPropertyError::UnkeyedList { step } => ("unkeyed-list", Some(*step)),
        ComponentPropertyError::ListIdentityMismatch { step, .. } => {
            ("list-identity-mismatch", Some(*step))
        }
        ComponentPropertyError::KeyTypeMismatch { step, .. } => ("key-type-mismatch", Some(*step)),
        ComponentPropertyError::InvalidListItemKey { step, .. } => {
            ("invalid-list-item-key", Some(*step))
        }
        ComponentPropertyError::ListItemNotFound { step, .. } => {
            ("list-item-not-found", Some(*step))
        }
        ComponentPropertyError::AmbiguousListItem { step, .. } => {
            ("ambiguous-list-item", Some(*step))
        }
        ComponentPropertyError::ExpectedMap { step } => ("expected-map", Some(*step)),
        ComponentPropertyError::MapEntryNotFound { step, .. } => {
            ("map-entry-not-found", Some(*step))
        }
        ComponentPropertyError::InvalidReplacementShape(_) => ("invalid-replacement-shape", None),
        ComponentPropertyError::InvalidReplacement(_) => ("invalid-replacement", None),
        ComponentPropertyError::StorageIndexOverflow => ("storage-index-overflow", None),
    };
    let mut diagnostic = crate::diagnostics::with_public_identifier(
        rejection_diagnostic(operation_index),
        "property-error-kind",
        kind,
    );
    if let Some(step) = step {
        diagnostic = crate::diagnostics::with_public_u64(
            diagnostic,
            "property-step-index",
            u64::try_from(step).unwrap_or(u64::MAX),
        );
    }
    if let ComponentPropertyError::InvalidValue(codec)
    | ComponentPropertyError::InvalidReplacement(codec) = error
    {
        diagnostic = crate::diagnostics::with_codec_error(diagnostic, codec, Some(schema));
    }
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(diagnostic);
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}
