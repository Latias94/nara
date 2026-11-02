use std::{collections::BTreeMap, sync::Arc};

use nara_reflect::{ComponentPropertyPath, ComponentTypeId, ComponentValue, EntityReference};

use super::{
    AuthoringPropertyEdit, PrefabAuthoringSession, PreparedPrefabAuthoringEdit,
    PreparedSceneAuthoringEdit, SceneAuthoringHistoryLimits, SceneAuthoringRevision,
    SceneAuthoringSession, creation::allocate_entity_id, history_error_report, property,
};
use crate::{
    AuthoringSchemaBasis, PrefabDocument, SceneDocument, SceneEntityId, SceneEntityRecord,
    ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    ScenePlacement, ScenePlacementOwner,
    authoring_decode::{
        AuthoringComponentTemplate, AuthoringDocumentState, MAX_AUTHORING_EDIT_OPERATIONS,
        MAX_AUTHORING_FIELD_LOGICAL_BYTES, MAX_AUTHORING_FIELD_VALUE_NODES,
    },
};

#[derive(Debug)]
struct ConstructionScope;

/// One declaration in one draft, not a stored source ID or runtime entity.
#[derive(Debug, Clone)]
pub struct LocalConstructionEntity {
    scope: Arc<ConstructionScope>,
    index: usize,
}

impl PartialEq for LocalConstructionEntity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope) && self.index == other.index
    }
}

impl Eq for LocalConstructionEntity {}

/// An existing ID is relative to the target source; locals retain exact draft affinity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructionEntity {
    Existing(SceneEntityId),
    Local(LocalConstructionEntity),
}

impl From<SceneEntityId> for ConstructionEntity {
    fn from(entity: SceneEntityId) -> Self {
        Self::Existing(entity)
    }
}

impl From<&SceneEntityId> for ConstructionEntity {
    fn from(entity: &SceneEntityId) -> Self {
        Self::Existing(entity.clone())
    }
}

impl From<LocalConstructionEntity> for ConstructionEntity {
    fn from(entity: LocalConstructionEntity) -> Self {
        Self::Local(entity)
    }
}

impl From<&LocalConstructionEntity> for ConstructionEntity {
    fn from(entity: &LocalConstructionEntity) -> Self {
        Self::Local(entity.clone())
    }
}

/// Additive placement; local parents and sibling anchors must already be declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructionPlacement {
    Source(ScenePlacement),
    /// Appends to the named parent's children.
    ChildOf(ConstructionEntity),
    Before(ConstructionEntity),
    After(ConstructionEntity),
}

impl From<ScenePlacement> for ConstructionPlacement {
    fn from(placement: ScenePlacement) -> Self {
        Self::Source(placement)
    }
}

#[derive(Debug)]
enum Replacement {
    Value(ComponentValue),
    Reference(ConstructionEntity),
}

#[derive(Debug)]
struct PropertyWrite {
    entity: ConstructionEntity,
    component: ComponentTypeId,
    property: ComponentPropertyPath,
    replacement: Replacement,
}

/// Bounded additive declarations and ordered property writes for one exact source basis.
///
/// Appending does not validate templates or publish source state. Rejected appends leave earlier
/// intent usable. Prepare resolves templates and local IDs; it may still reject the entire draft.
#[derive(Debug)]
pub struct AuthoringConstructionDraft {
    revision: SceneAuthoringRevision,
    schema: AuthoringSchemaBasis,
    scope: Arc<ConstructionScope>,
    entities: Vec<ConstructionPlacement>,
    components: Vec<(ConstructionEntity, ComponentTypeId)>,
    writes: Vec<PropertyWrite>,
    write_nodes: usize,
    write_bytes: usize,
}

impl AuthoringConstructionDraft {
    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    pub fn create_entity(
        &mut self,
        placement: impl Into<ConstructionPlacement>,
    ) -> Result<LocalConstructionEntity, ScenePatchReport> {
        self.admit_declaration()?;
        let placement = placement.into();
        match &placement {
            ConstructionPlacement::Source(_) => {}
            ConstructionPlacement::ChildOf(entity)
            | ConstructionPlacement::Before(entity)
            | ConstructionPlacement::After(entity) => self.admit_entity(entity)?,
        }
        let entity = LocalConstructionEntity {
            scope: Arc::clone(&self.scope),
            index: self.entities.len(),
        };
        self.entities.push(placement);
        Ok(entity)
    }

    /// Requests the source owner's exact complete creation template, not caller-supplied records.
    pub fn add_component(
        &mut self,
        entity: impl Into<ConstructionEntity>,
        component: ComponentTypeId,
    ) -> Result<(), ScenePatchReport> {
        if component.as_str().len() > ComponentTypeId::MAX_BYTES {
            return Err(rejected());
        }
        self.admit_declaration()?;
        let entity = entity.into();
        self.admit_entity(&entity)?;
        self.components.push((entity, component));
        Ok(())
    }

    pub fn set_property(
        &mut self,
        entity: impl Into<ConstructionEntity>,
        component: ComponentTypeId,
        property: ComponentPropertyPath,
        value: ComponentValue,
    ) -> Result<(), ScenePatchReport> {
        self.push_write(PropertyWrite {
            entity: entity.into(),
            component,
            property,
            replacement: Replacement::Value(value),
        })
    }

    /// Replaces one Schema-selected leaf with a source-local reference after local resolution.
    pub fn set_entity_reference(
        &mut self,
        entity: impl Into<ConstructionEntity>,
        component: ComponentTypeId,
        property: ComponentPropertyPath,
        target: impl Into<ConstructionEntity>,
    ) -> Result<(), ScenePatchReport> {
        let target = target.into();
        self.admit_entity(&target)?;
        self.push_write(PropertyWrite {
            entity: entity.into(),
            component,
            property,
            replacement: Replacement::Reference(target),
        })
    }

    fn admit_declaration(&self) -> Result<(), ScenePatchReport> {
        if self.entities.len() + self.components.len() + self.writes.len()
            >= MAX_AUTHORING_EDIT_OPERATIONS
        {
            return Err(rejected());
        }
        Ok(())
    }

    fn admit_entity(&self, entity: &ConstructionEntity) -> Result<(), ScenePatchReport> {
        if let ConstructionEntity::Local(local) = entity
            && (!Arc::ptr_eq(&self.scope, &local.scope) || local.index >= self.entities.len())
        {
            return Err(foreign_local());
        }
        Ok(())
    }

    fn push_write(&mut self, write: PropertyWrite) -> Result<(), ScenePatchReport> {
        if write.component.as_str().len() > ComponentTypeId::MAX_BYTES {
            return Err(rejected());
        }
        self.admit_entity(&write.entity)?;
        self.admit_declaration()?;
        let bytes = self
            .write_bytes
            .saturating_add(write.property.logical_bytes().map_err(|_| rejected())?);
        if bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES {
            return Err(rejected());
        }
        let remaining_nodes = MAX_AUTHORING_FIELD_VALUE_NODES.saturating_sub(self.write_nodes);
        let remaining_bytes = MAX_AUTHORING_FIELD_LOGICAL_BYTES.saturating_sub(bytes);
        let (nodes, value_bytes) = match &write.replacement {
            Replacement::Value(value) => {
                let cost = value
                    .cost_up_to(remaining_nodes, remaining_bytes)
                    .ok_or_else(rejected)?;
                (cost.nodes(), cost.logical_bytes())
            }
            Replacement::Reference(ConstructionEntity::Existing(entity)) => {
                let value = ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: entity.clone(),
                });
                let cost = value
                    .cost_up_to(remaining_nodes, remaining_bytes)
                    .ok_or_else(rejected)?;
                (cost.nodes(), cost.logical_bytes())
            }
            Replacement::Reference(ConstructionEntity::Local(_)) => {
                if remaining_nodes == 0 {
                    return Err(rejected());
                }
                // The local handle retains no ID bytes; lowering charges the resolved value again.
                (1, 0)
            }
        };
        self.write_nodes += nodes;
        self.write_bytes = bytes + value_bytes;
        self.writes.push(write);
        Ok(())
    }
}

/// The fixed mapping chosen by one preparation; it never reallocates IDs at commit.
#[derive(Debug, Clone)]
pub struct ConstructionEntityMap {
    scope: Arc<ConstructionScope>,
    entities: Vec<SceneEntityId>,
}

impl PartialEq for ConstructionEntityMap {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope) && self.entities == other.entities
    }
}

impl Eq for ConstructionEntityMap {}

impl ConstructionEntityMap {
    #[must_use]
    pub fn entity(&self, local: &LocalConstructionEntity) -> Option<&SceneEntityId> {
        Arc::ptr_eq(&self.scope, &local.scope)
            .then(|| self.entities.get(local.index))
            .flatten()
    }

    fn resolve(&self, entity: &ConstructionEntity) -> Result<SceneEntityId, ScenePatchReport> {
        match entity {
            ConstructionEntity::Existing(entity) => Ok(entity.clone()),
            ConstructionEntity::Local(local) => {
                self.entity(local).cloned().ok_or_else(foreign_local)
            }
        }
    }
}

/// A successful source publication and its exact local mapping, not proof of later existence.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedAuthoringConstruction {
    entities: ConstructionEntityMap,
    report: ScenePatchReport,
}

impl CommittedAuthoringConstruction {
    #[must_use]
    pub fn entity(&self, local: &LocalConstructionEntity) -> Option<&SceneEntityId> {
        self.entities.entity(local)
    }

    #[must_use]
    pub const fn report(&self) -> &ScenePatchReport {
        &self.report
    }

    #[must_use]
    pub fn into_parts(self) -> (ConstructionEntityMap, ScenePatchReport) {
        (self.entities, self.report)
    }
}

macro_rules! construction_session {
    ($session:ty, $prepared:ident, $edit:ty, $document:ty) => {
        /// Unpublished construction retaining the existing nominal prepared edit and one mapping.
        #[derive(Debug)]
        pub struct $prepared {
            edit: $edit,
            entities: ConstructionEntityMap,
        }

        impl $prepared {
            #[must_use]
            pub fn base_revision(&self) -> SceneAuthoringRevision {
                self.edit.base_revision()
            }

            #[must_use]
            pub fn document(&self) -> &$document {
                self.edit.document()
            }

            #[must_use]
            pub fn patch(&self) -> &ScenePatchDocument {
                self.edit.patch()
            }

            #[must_use]
            pub fn inverse(&self) -> &ScenePatchDocument {
                self.edit.inverse()
            }

            #[must_use]
            pub fn entity(&self, local: &LocalConstructionEntity) -> Option<&SceneEntityId> {
                self.entities.entity(local)
            }
        }

        impl $session {
            pub fn begin_authoring_construction(
                &self,
                base_revision: SceneAuthoringRevision,
            ) -> Result<AuthoringConstructionDraft, ScenePatchReport> {
                if self.revision() != base_revision {
                    return Err(super::prepared::stale_basis());
                }
                let state = self.authority.published().ok_or_else(rejected)?;
                Ok(AuthoringConstructionDraft {
                    revision: base_revision,
                    schema: state.schema_basis(),
                    scope: Arc::new(ConstructionScope),
                    entities: Vec::new(),
                    components: Vec::new(),
                    writes: Vec::new(),
                    write_nodes: 0,
                    write_bytes: 0,
                })
            }

            pub fn prepare_authoring_construction(
                &self,
                draft: &AuthoringConstructionDraft,
            ) -> Result<$prepared, ScenePatchReport> {
                if self.revision() != draft.revision {
                    return Err(super::prepared::stale_basis());
                }
                let state = self.authority.published().ok_or_else(rejected)?;
                if state.schema_basis() != draft.schema {
                    return Err(super::prepared::stale_basis());
                }
                let (patch, entities) = lower(
                    draft,
                    &self.document.roots,
                    &self.document.entities,
                    state,
                    self.history.limits,
                )?;
                let edit = self.prepare_authoring_edit(draft.revision, &patch)?;
                Ok($prepared { edit, entities })
            }

            pub fn commit_authoring_construction(
                &mut self,
                prepared: $prepared,
            ) -> Result<CommittedAuthoringConstruction, ScenePatchReport> {
                let report = self.commit_authoring_edit(prepared.edit);
                if report.applied {
                    Ok(CommittedAuthoringConstruction {
                        entities: prepared.entities,
                        report,
                    })
                } else {
                    Err(report)
                }
            }
        }
    };
}

construction_session!(
    SceneAuthoringSession,
    PreparedSceneAuthoringConstruction,
    PreparedSceneAuthoringEdit,
    SceneDocument
);
construction_session!(
    PrefabAuthoringSession,
    PreparedPrefabAuthoringConstruction,
    PreparedPrefabAuthoringEdit,
    PrefabDocument
);

fn lower(
    draft: &AuthoringConstructionDraft,
    roots: &[SceneEntityId],
    original: &[SceneEntityRecord],
    state: &AuthoringDocumentState,
    history_limits: SceneAuthoringHistoryLimits,
) -> Result<(ScenePatchDocument, ConstructionEntityMap), ScenePatchReport> {
    let definitions = draft
        .components
        .iter()
        .map(|(_, component)| {
            state
                .initial_component_template(component)
                .ok_or_else(rejected)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut entities = ConstructionEntityMap {
        scope: Arc::clone(&draft.scope),
        entities: Vec::with_capacity(draft.entities.len()),
    };
    for _ in &draft.entities {
        let entity = allocate_entity_id(|id| {
            original.iter().any(|record| &record.id == id) || entities.entities.contains(id)
        })?;
        entities.entities.push(entity);
    }
    let mut operations = Vec::new();
    let mut placements = BTreeMap::<SceneEntityId, ScenePlacementOwner>::new();
    for (index, placement) in draft.entities.iter().enumerate() {
        let placement = match placement {
            ConstructionPlacement::Source(placement) => placement.clone(),
            ConstructionPlacement::ChildOf(parent) => {
                ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: entities.resolve(parent)?,
                })
            }
            ConstructionPlacement::Before(sibling) | ConstructionPlacement::After(sibling) => {
                let sibling = entities.resolve(sibling)?;
                let owner = match placements.entry(sibling.clone()) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.get().clone(),
                    std::collections::btree_map::Entry::Vacant(entry) => entry
                        .insert(
                            crate::patch::placement_owner(roots, original, &sibling)
                                .ok_or_else(rejected)?,
                        )
                        .clone(),
                };
                match placement {
                    ConstructionPlacement::Before(_) => ScenePlacement::before(owner, sibling),
                    _ => ScenePlacement::after(owner, sibling),
                }
            }
        };
        let entity = entities.entities[index].clone();
        placements.insert(entity.clone(), placement.owner.clone());
        operations.push(ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(entity),
            placement,
        });
    }
    let mut templates = Vec::with_capacity(draft.components.len());
    for ((entity, component), template) in draft.components.iter().zip(definitions) {
        let entity = entities.resolve(entity)?;
        templates.push((entity, component.clone(), template));
    }
    let mut patch = ScenePatchDocument::new(operations);
    // Admit every retained default and its declaration before property lowering can copy an
    // initial value, including defaults hidden by that value's explicit Nulls.
    admit_template_payloads(
        [&patch],
        templates
            .iter()
            .map(|(entity, _, template)| (entity, *template)),
        history_limits,
        ScenePatchApplyLimits::default(),
    )?;
    let staged = templates
        .iter()
        .map(|(entity, component, template)| {
            (
                (entity.clone(), component.clone()),
                property::StagedPropertyRecord {
                    schema: template.schema,
                    version: template.schema.version(),
                    value: template.value,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut writes = Vec::with_capacity(draft.writes.len());
    for write in &draft.writes {
        let entity = entities.resolve(&write.entity)?;
        let schema = state
            .available_component_schema(&entity, &write.component)
            .or_else(|| {
                staged
                    .get(&(entity.clone(), write.component.clone()))
                    .map(|entry| entry.schema)
            })
            .ok_or_else(rejected)?;
        let value = match &write.replacement {
            Replacement::Value(value) => value.clone(),
            Replacement::Reference(target) => {
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: entities.resolve(target)?,
                })
            }
        };
        writes.push(AuthoringPropertyEdit {
            entity,
            component: write.component.clone(),
            component_version: schema.version(),
            property: write.property.clone(),
            value,
        });
    }
    let fields = ScenePatchDocument::new(if writes.is_empty() {
        Vec::new()
    } else if draft.entities.is_empty() && draft.components.is_empty() {
        // A construction-shaped request with no additive declarations is an ordinary edit.
        // Check its original write count before coalescing repeated roots.
        property::lower(original, state, &writes, &staged)?.operations
    } else {
        property::lower_construction(original, state, &writes, &staged)?.operations
    });
    drop(staged);
    // Coalesced writes now have their exact replay cost. Admit the complete transaction before
    // materializing the records, rather than giving their hidden defaults a separate allowance.
    admit_template_payloads(
        [&patch, &fields],
        templates
            .iter()
            .map(|(entity, _, template)| (entity, *template)),
        history_limits,
        ScenePatchApplyLimits::default(),
    )?;
    for (entity, component, template) in templates {
        patch.operations.push(ScenePatchOperation::AddComponent {
            entity,
            component,
            value: template.to_record().map_err(|_| rejected())?,
        });
    }
    patch.operations.extend(fields.operations);
    Ok((patch, entities))
}

fn admit_template_payloads<'a>(
    patches: impl IntoIterator<Item = &'a ScenePatchDocument>,
    templates: impl IntoIterator<Item = (&'a SceneEntityId, AuthoringComponentTemplate<'a>)>,
    history: SceneAuthoringHistoryLimits,
    replay: ScenePatchApplyLimits,
) -> Result<(), ScenePatchReport> {
    let refused = || {
        history_error_report(
            "scene.authoring-construction-template-copy-limit",
            "Construction template payloads exceed the source's replay or history copy budget",
        )
    };
    let operation_limit = history
        .max_entry_operations()
        .min(replay.validation_work().get());
    let node_limit = history
        .max_entry_value_nodes()
        .min(replay.validation_value_nodes().get());
    let byte_limit = history
        .max_entry_logical_bytes()
        .min(replay.validation_value_bytes().get());
    let mut operations = 0usize;
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for patch in patches {
        let cost = crate::patch::bounded_scene_patch_cost(patch, replay).ok_or_else(refused)?;
        operations = operations
            .checked_add(cost.operation_count())
            .ok_or_else(refused)?;
        nodes = nodes
            .checked_add(cost.value_cost().nodes())
            .ok_or_else(refused)?;
        bytes = bytes
            .checked_add(cost.logical_bytes())
            .ok_or_else(refused)?;
        if operations > operation_limit || nodes > node_limit || bytes > byte_limit {
            return Err(refused());
        }
    }
    for (entity, template) in templates {
        operations = operations.checked_add(1).ok_or_else(refused)?;
        bytes = bytes
            .checked_add(entity.as_str().len())
            .and_then(|bytes| bytes.checked_add(template.schema.id().as_str().len()))
            .and_then(|bytes| bytes.checked_add(template.owner.as_str().len()))
            .ok_or_else(refused)?;
        let values = std::iter::once(Ok((None, template.value))).chain(
            template
                .contextual_defaults()
                .map(|entry| entry.map(|(field, value)| (Some(field), value))),
        );
        for entry in values {
            let (field, value) = entry.map_err(|_| rejected())?;
            if let Some(field) = field {
                // ScenePatchCost::add_patch_record_cost counts each retained contextual
                // declaration in operation_count, independently of its payload nodes.
                operations = operations.checked_add(1).ok_or_else(refused)?;
                bytes = bytes
                    .checked_add(field.as_str().len())
                    .ok_or_else(refused)?;
            }
            if operations > operation_limit || bytes > byte_limit {
                return Err(refused());
            }
            let cost = value
                .cost_up_to(node_limit.saturating_sub(nodes), byte_limit - bytes)
                .ok_or_else(refused)?;
            nodes += cost.nodes();
            bytes += cost.logical_bytes();
        }
    }
    Ok(())
}

fn rejected() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-construction-rejected",
        "Construction requires bounded additive declarations and admitted source templates/properties",
    )
}

fn foreign_local() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-construction-foreign-local",
        "Construction local handles require their exact originating draft and declaration",
    )
}

#[cfg(test)]
mod contextual_default_tests;

#[cfg(test)]
mod limit_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use nara_core::{ByteLimit, ItemLimit};

    #[test]
    fn committed_mapping_equality_retains_construction_scope_and_resolved_ids() {
        let scope = Arc::new(ConstructionScope);
        let entity = SceneEntityId::new("created").unwrap();
        let mapping = ConstructionEntityMap {
            scope: Arc::clone(&scope),
            entities: vec![entity.clone()],
        };
        let local = LocalConstructionEntity {
            scope: Arc::clone(&scope),
            index: 0,
        };
        let cloned = mapping.clone();
        assert_eq!(mapping, cloned);
        assert_eq!(cloned.entity(&local), Some(&entity));

        let foreign = ConstructionEntityMap {
            scope: Arc::new(ConstructionScope),
            entities: vec![entity],
        };
        assert_ne!(mapping, foreign);
        assert!(foreign.entity(&local).is_none());

        let different_ids = ConstructionEntityMap {
            scope,
            entities: vec![SceneEntityId::new("other").unwrap()],
        };
        assert_ne!(mapping, different_ids);
    }

    #[test]
    fn cloned_committed_receipt_preserves_local_resolution_and_report_equality() {
        let scope = Arc::new(ConstructionScope);
        let local = LocalConstructionEntity {
            scope: Arc::clone(&scope),
            index: 0,
        };
        let entity = SceneEntityId::new("created").unwrap();
        let committed = CommittedAuthoringConstruction {
            entities: ConstructionEntityMap {
                scope,
                entities: vec![entity.clone()],
            },
            report: ScenePatchReport {
                applied: true,
                inverse: Some(ScenePatchDocument::new([])),
                diagnostics: Default::default(),
            },
        };
        let mut cloned = committed.clone();
        assert_eq!(cloned, committed);
        assert_eq!(cloned.entity(&local), Some(&entity));
        cloned.report.applied = false;
        assert_ne!(cloned, committed);
    }

    #[test]
    fn borrowed_template_payload_gate_keeps_exact_independent_history_and_replay_limits() {
        let value = ComponentValue::String("abcd".into());
        let schema = nara_reflect::ComponentSchema::new(
            ComponentTypeId::new("c"),
            "Component",
            nara_reflect::ComponentSchemaVersion::ONE,
        );
        let owner = nara_reflect::PersistentComponentSchemaOwnerId::try_new("o").unwrap();
        let entity = SceneEntityId::new("e").unwrap();
        let template = AuthoringComponentTemplate {
            schema: &schema,
            value: &value,
            owner: &owner,
        };
        let copies = [(&entity, template); 2];
        // Each record retains its entity/component/owner locators plus four payload bytes.
        let history = SceneAuthoringHistoryLimits::new(8, 16, 2, 14, 16, 2, 14).unwrap();
        let replay = ScenePatchApplyLimits::default()
            .with_validation_value_nodes(ItemLimit::new(2).unwrap())
            .with_validation_value_bytes(ByteLimit::new(14).unwrap());
        assert!(admit_template_payloads([], copies, history, replay).is_ok());
        assert!(admit_template_payloads([], [(&entity, template); 3], history, replay).is_err());
        let fewer_nodes = replay.with_validation_value_nodes(ItemLimit::new(1).unwrap());
        assert!(admit_template_payloads([], copies, history, fewer_nodes).is_err());
        let fewer_bytes = replay.with_validation_value_bytes(ByteLimit::new(13).unwrap());
        assert!(admit_template_payloads([], copies, history, fewer_bytes).is_err());
        let history = SceneAuthoringHistoryLimits::new(8, 16, 2, 13, 16, 2, 13).unwrap();
        assert!(admit_template_payloads([], copies, history, replay).is_err());
        let history = SceneAuthoringHistoryLimits::new(8, 32, 8, 32, 16, 2, 13).unwrap();
        assert!(
            admit_template_payloads([], copies, history, ScenePatchApplyLimits::default()).is_err()
        );
        let history = SceneAuthoringHistoryLimits::new(8, 32, 8, 32, 16, 1, 14).unwrap();
        assert!(
            admit_template_payloads([], copies, history, ScenePatchApplyLimits::default()).is_err()
        );
    }
}
