//! Final surviving locators and provenance, after structural and value demand close.

use nara_identity::EntityReference;
use nara_reflect::__private::plan_declared_entity_references;

use super::*;
use crate::prefab::{PrefabReferenceRemapError, accumulate_generated_identifier_bytes};

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn materialize(
        &mut self,
        mut document: SceneDocument,
    ) -> Result<SceneDocument, PrefabExpansionFailure> {
        self.admit(self.nodes.len().checked_mul(256))?;
        self.admit(self.root.entities.iter().try_fold(0usize, |bytes, record| {
            bytes
                .checked_add(128)?
                .checked_add(provenance_string_bytes(record.id.as_str())?.checked_mul(2)?)
        }))?;
        self.context
            .reserved_entity_ids
            .extend(self.root.entities.iter().map(|record| record.id.clone()));
        let projected = SceneDocument {
            roots: Vec::new(),
            entities: document
                .entities
                .iter()
                .filter(|record| self.nodes[self.locators[&record.id]].scope != 0)
                .cloned()
                .collect(),
        };
        let mut identifiers = self
            .context
            .allocate_projected_entity_ids(&projected)
            .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
        for &node in self.scopes[0].nodes.values() {
            identifiers.insert(
                self.nodes[node].locator.clone(),
                self.nodes[node].origin.entity().clone(),
            );
        }
        self.admit(self.nodes.iter().try_fold(0usize, |bytes, node| {
            bytes
                .checked_add(256)?
                .checked_add(provenance_string_bytes(node.locator.as_str())?)?
                .checked_add(
                    identifiers
                        .get(&node.locator)
                        .map_or(Some(64), |id| provenance_string_bytes(id.as_str()))?
                        .checked_mul(6)?,
                )
        }))?;
        let mut used = identifiers.values().cloned().collect::<BTreeSet<_>>();
        let mut ordinal = 0usize;
        for node in &self.nodes {
            let value_id = if let Some(id) = identifiers.get(&node.locator) {
                id.clone()
            } else {
                loop {
                    let id = SceneEntityId::new(format!("discarded_{ordinal}"))
                        .expect("valid unpublished reference locator");
                    ordinal = ordinal
                        .checked_add(1)
                        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                    if used.insert(id.clone()) {
                        break id;
                    }
                }
            };
            self.value_ids.insert(node.locator.clone(), value_id);
        }
        self.materialize_components(&mut document)?;
        let node_index = self
            .nodes
            .iter()
            .enumerate()
            .map(|(id, node)| (node.locator.clone(), id))
            .collect::<BTreeMap<_, _>>();
        let reference_nodes = self
            .value_ids
            .iter()
            .map(|(locator, value_id)| (value_id.clone(), node_index[locator]))
            .collect::<BTreeMap<_, _>>();
        let references = identifiers
            .iter()
            .map(|(locator, id)| (self.value_ids[locator].clone(), id.clone()))
            .collect::<BTreeMap<_, _>>();
        for record in &mut document.entities {
            let node = node_index[&record.id];
            let id = &identifiers[&record.id];
            let path_cost = self.projection_cost(node)?;
            if self.nodes[node].scope != 0
                && !self.context.reserve_provenance_bytes(
                    provenance_entry_bytes(id, self.nodes[node].origin.entity())
                        .and_then(|bytes| bytes.checked_add(path_cost)),
                    Some(id),
                    None,
                )
            {
                return Err(PrefabExpansionFailureKind::WorkLimit.into());
            }
            let provenance = if self.nodes[node].scope == 0 {
                SceneEntityProvenance::local(self.nodes[node].origin.entity().clone())
            } else {
                SceneEntityProvenance::projected(
                    self.nodes[node].origin.entity().clone(),
                    self.projection(node)?,
                )
            };
            self.context.provenance.insert(id.clone(), provenance);
            let mut generated = 0;
            let limits = self.context.options.limits;
            for child in &record.children {
                if self.nodes[node_index[child]].scope != 0 {
                    accumulate_generated_identifier_bytes(
                        self.context.usage.generated_identifier_bytes,
                        &mut generated,
                        identifiers[child].as_str().len(),
                        limits.single_generated_identifier_bytes().get(),
                        limits.generated_identifier_bytes().get(),
                    )
                    .map_err(|error| {
                        self.context.fail_prefab_remap_budget(error, Some(id));
                        PrefabExpansionFailureKind::WorkLimit
                    })?;
                }
            }
            for (component, payload) in &mut record.components {
                if !self.context.authority.can_interpret(component, payload) {
                    continue;
                }
                let schema = self
                    .context
                    .authority
                    .schema(component)
                    .ok_or(PrefabExpansionFailureKind::Component)?;
                if !self
                    .context
                    .reserve_component_temporary(schema, payload.value.cost(), Some(id))
                {
                    return Err(PrefabExpansionFailureKind::WorkLimit.into());
                }
                let plan = plan_declared_entity_references::<PrefabReferenceRemapError>(
                    schema,
                    &payload.value,
                    self.context.prefab_reference_traversal_limits(),
                )
                .map_err(|error| {
                    self.context.push_prefab_reference_rewrite_error(
                        id,
                        component.as_str(),
                        &error,
                    );
                    PrefabExpansionFailureKind::Reference
                })?;
                let mut old_bytes = 0usize;
                let mut new_bytes = 0usize;
                plan.visit(&payload.value, |_, reference| {
                    if let EntityReference::SceneLocal { entity } = reference {
                        let mapped = references
                            .get(entity)
                            .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?;
                        if self.nodes[reference_nodes[entity]].scope != 0 {
                            accumulate_generated_identifier_bytes(
                                self.context.usage.generated_identifier_bytes,
                                &mut generated,
                                mapped.as_str().len(),
                                limits.single_generated_identifier_bytes().get(),
                                limits.generated_identifier_bytes().get(),
                            )?;
                        }
                        old_bytes = old_bytes.saturating_add(entity.as_str().len());
                        new_bytes = new_bytes.saturating_add(mapped.as_str().len());
                    }
                    Ok(())
                })
                .map_err(|error| {
                    if let nara_reflect::ComponentEntityReferenceRewriteError::Rewrite {
                        error, ..
                    } = &error
                        && self.context.fail_prefab_remap_budget(*error, Some(id))
                    {
                        return PrefabExpansionFailureKind::WorkLimit;
                    }
                    self.context.push_prefab_reference_rewrite_error(
                        id,
                        component.as_str(),
                        &error,
                    );
                    PrefabExpansionFailureKind::Reference
                })?;
                let cost = payload.value.cost();
                let bytes = cost
                    .logical_bytes()
                    .checked_sub(old_bytes)
                    .and_then(|bytes| bytes.checked_add(new_bytes))
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                let (nodes, bytes) = self
                    .context
                    .preflight_materialized_value_cost(
                        cost.nodes(),
                        cost.nodes(),
                        cost.logical_bytes(),
                        bytes,
                        id,
                    )
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                let mut usage = self.context.usage;
                usage.materialized_value_nodes = nodes;
                usage.materialized_value_bytes = bytes;
                usage.generated_identifier_bytes = usage
                    .generated_identifier_bytes
                    .checked_add(generated)
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                if !self.context.admit_working_usage(usage, Some(id), None) {
                    return Err(PrefabExpansionFailureKind::WorkLimit.into());
                }
                plan.rewrite_in_place(&mut payload.value, |_, reference| {
                    Ok::<_, PrefabReferenceRemapError>(match reference {
                        EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                            entity: references
                                .get(entity)
                                .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                                .clone(),
                        },
                        EntityReference::Persistent { .. } => reference.clone(),
                    })
                })
                .map_err(|_| PrefabExpansionFailureKind::Reference)?;
                self.context.usage.materialized_value_nodes = nodes;
                self.context.usage.materialized_value_bytes = bytes;
            }
            let mut usage = self.context.usage;
            usage.generated_identifier_bytes = usage
                .generated_identifier_bytes
                .checked_add(generated)
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
            if !self.context.admit_working_usage(usage, Some(id), None) {
                return Err(PrefabExpansionFailureKind::WorkLimit.into());
            }
            self.context.usage = usage;
            record.id = id.clone();
            for child in &mut record.children {
                *child = identifiers[child].clone();
            }
        }
        for root in &mut document.roots {
            *root = identifiers[root].clone();
        }
        Ok(document)
    }

    fn projection_cost(&self, mut node: NodeId) -> Result<usize, PrefabExpansionFailure> {
        if self.nodes[node].scope == 0 {
            return Ok(0);
        }
        let mut bytes = 0usize;
        loop {
            let scope = &self.scopes[self.nodes[node].scope];
            if scope.anchor.is_none() {
                return bytes
                    .checked_add(
                        provenance_string_bytes(self.nodes[node].origin.entity().as_str())
                            .ok_or(PrefabExpansionFailureKind::WorkLimit)?,
                    )
                    .ok_or_else(|| PrefabExpansionFailureKind::WorkLimit.into());
            }
            let step = match &self.nodes[node].origin {
                PrefabEntityOrigin::SourceEntity(entity) => {
                    PrefabProjectionStep::source_allocation_bytes(
                        scope.request.as_ref().unwrap(),
                        scope.source.as_ref().unwrap().key(),
                        entity,
                    )
                }
                PrefabEntityOrigin::InstanceAddition(entity) => {
                    PrefabProjectionStep::addition_allocation_bytes(entity)
                }
            }
            .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
            bytes = bytes
                .checked_add(step)
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
            node = scope.anchor.unwrap();
        }
    }

    fn projection(&self, mut node: NodeId) -> Result<PrefabProjectionPath, PrefabExpansionFailure> {
        let mut steps = Vec::new();
        loop {
            let scope = &self.scopes[self.nodes[node].scope];
            let Some(anchor) = scope.anchor else {
                steps.reverse();
                return Ok(PrefabProjectionPath::from_resolved_steps(
                    self.nodes[node].origin.entity().clone(),
                    steps,
                ));
            };
            steps.push(match &self.nodes[node].origin {
                PrefabEntityOrigin::SourceEntity(entity) => PrefabProjectionStep::SourceEntity {
                    source_document: scope.request.as_ref().unwrap().clone(),
                    canonical_source: scope.source.as_ref().unwrap().key().clone(),
                    entity: entity.clone(),
                },
                PrefabEntityOrigin::InstanceAddition(entity) => {
                    PrefabProjectionStep::InstanceAddition {
                        entity: entity.clone(),
                    }
                }
            });
            node = anchor;
        }
    }
}
