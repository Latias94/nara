//! Final surviving locators and provenance, after structural and value demand close.

use super::inputs::ResolvedReference;
use super::reference_program::ReferencePrograms;
use super::*;
use crate::prefab::{PrefabReferenceRemapError, accumulate_generated_identifier_bytes};
use nara_identity::EntityReference;

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn materialize(
        &mut self,
        mut document: SceneDocument,
        references: ReferencePrograms,
    ) -> Result<SceneDocument, PrefabExpansionFailure> {
        self.admit(self.nodes.len().checked_mul(256))?;
        self.admit(self.root.entities.iter().try_fold(0usize, |bytes, record| {
            bytes
                .checked_add(256)?
                .checked_add(provenance_string_bytes(record.id.as_str())?.checked_mul(3)?)
        }))?;
        self.context
            .reserved_entity_ids
            .extend(self.root.entities.iter().map(|record| record.id.clone()));
        self.admit(document.entities.iter().try_fold(0usize, |bytes, record| {
            bytes
                .checked_add(size_of::<SceneEntityRecord>())?
                .checked_add(provenance_string_bytes(record.id.as_str())?.checked_mul(2)?)
        }))?;
        let projected = SceneDocument {
            roots: Vec::new(),
            entities: document
                .entities
                .iter()
                .filter(|record| self.nodes[self.locators[&record.id]].scope != 0)
                .map(|record| SceneEntityRecord::new(record.id.clone()))
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
        self.admit(
            document
                .entities
                .iter()
                .try_fold(0usize, |bytes, record| {
                    let bytes = bytes.checked_add(256)?.checked_add(
                        provenance_string_bytes(identifiers[&record.id].as_str())?
                            .checked_mul(2)?,
                    )?;
                    record.children.iter().try_fold(bytes, |bytes, child| {
                        bytes.checked_add(provenance_string_bytes(identifiers[child].as_str())?)
                    })
                })
                .and_then(|bytes| {
                    document.roots.iter().try_fold(bytes, |bytes, root| {
                        bytes.checked_add(provenance_string_bytes(identifiers[root].as_str())?)
                    })
                }),
        )?;
        for record in &mut document.entities {
            let node = self.locators[&record.id];
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
                if self.nodes[self.locators[child]].scope != 0 {
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
                    let cost = payload.value.cost();
                    let (nodes, bytes) = self
                        .context
                        .preflight_materialized_value_cost(
                            0,
                            cost.nodes(),
                            0,
                            cost.logical_bytes(),
                            self.nodes[node].origin.entity(),
                        )
                        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                    self.context.usage.materialized_value_nodes = nodes;
                    self.context.usage.materialized_value_bytes = bytes;
                    continue;
                }
                let reference_program = references
                    .get(&node)
                    .and_then(|components| components.get(component))
                    .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
                let origins = &reference_program.origins;
                let plan = reference_program
                    .plan
                    .as_ref()
                    .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
                let mut old_nodes = 0usize;
                let mut old_bytes = 0usize;
                let mut new_nodes = 0usize;
                let mut new_bytes = 0usize;
                plan.visit(&payload.value, |path, reference| {
                    if matches!(
                        reference,
                        EntityReference::SceneLocal { .. } | EntityReference::Authored { .. }
                    ) {
                        let overflow = || PrefabReferenceRemapError::MaterializedValue {
                            observed: usize::MAX,
                            maximum: limits.materialized_value_bytes().get(),
                        };
                        let original = reference.cost();
                        let (replacement_nodes, replacement_bytes) = match origins
                            .get(path)
                            .and_then(|origin| origin.resolved.as_ref())
                            .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                        {
                            ResolvedReference::Local(target) => {
                                let mapped = identifiers
                                    .get(&self.nodes[*target].locator)
                                    .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?;
                                if self.nodes[*target].scope != 0 {
                                    accumulate_generated_identifier_bytes(
                                        self.context.usage.generated_identifier_bytes,
                                        &mut generated,
                                        mapped.as_str().len(),
                                        limits.single_generated_identifier_bytes().get(),
                                        limits.generated_identifier_bytes().get(),
                                    )?;
                                }
                                (1, mapped.as_str().len())
                            }
                            ResolvedReference::Persistent(reference) => {
                                let cost = reference.cost();
                                (cost.nodes(), cost.logical_bytes())
                            }
                        };
                        old_nodes = old_nodes
                            .checked_add(original.nodes())
                            .ok_or_else(overflow)?;
                        old_bytes = old_bytes
                            .checked_add(original.logical_bytes())
                            .ok_or_else(overflow)?;
                        new_nodes = new_nodes
                            .checked_add(replacement_nodes)
                            .ok_or_else(overflow)?;
                        new_bytes = new_bytes
                            .checked_add(replacement_bytes)
                            .ok_or_else(overflow)?;
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
                let nodes = cost
                    .nodes()
                    .checked_sub(old_nodes)
                    .and_then(|nodes| nodes.checked_add(new_nodes))
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                let bytes = cost
                    .logical_bytes()
                    .checked_sub(old_bytes)
                    .and_then(|bytes| bytes.checked_add(new_bytes))
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                // Admit the selected flat tree against the one aggregate output allowance.
                // Its cost does not depend on whether a leaf came from a default or a pin.
                let (nodes, bytes) = self
                    .context
                    .preflight_materialized_value_cost(
                        0,
                        nodes,
                        0,
                        bytes,
                        self.nodes[node].origin.entity(),
                    )
                    .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
                self.admit(
                    new_nodes
                        .checked_mul(96)
                        .and_then(|bytes| bytes.checked_add(new_bytes)),
                )?;
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
                plan.rewrite_in_place(&mut payload.value, |path, reference| {
                    Ok::<_, PrefabReferenceRemapError>(match reference {
                        EntityReference::SceneLocal { .. } | EntityReference::Authored { .. } => {
                            match origins
                                .get(path)
                                .and_then(|origin| origin.resolved.as_ref())
                                .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                            {
                                ResolvedReference::Local(target) => EntityReference::SceneLocal {
                                    entity: identifiers
                                        .get(&self.nodes[*target].locator)
                                        .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                                        .clone(),
                                },
                                ResolvedReference::Persistent(reference) => reference.clone(),
                            }
                        }
                        EntityReference::Persistent { .. } => reference.clone(),
                    })
                })
                .map_err(|_| PrefabExpansionFailureKind::Reference)?;
                self.context.usage.materialized_value_nodes = nodes;
                self.context.usage.materialized_value_bytes = bytes;
                // Contextual declarations have already supplied defaults and property Absent.
                payload.contextual_defaults.clear();
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
