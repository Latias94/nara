//! Final surviving locators and provenance, after structural and value demand close.

use super::inputs::ResolvedReference;
use super::reference_program::{ReferenceProgram, ReferencePrograms};
use super::*;
use crate::prefab::{PrefabReferenceRemapError, accumulate_generated_identifier_bytes};
use nara_identity::EntityReference;

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn materialize(
        &mut self,
        mut document: SceneDocument,
        references: ReferencePrograms,
    ) -> Result<StagedSceneOutput, SceneCompilationFailure> {
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
            .ok_or(SceneCompilationFailureKind::WorkLimit)?;
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
        let mut source_defaults = Vec::new();
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
                return Err(SceneCompilationFailureKind::WorkLimit.into());
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
                        SceneCompilationFailureKind::WorkLimit
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
                        .ok_or(SceneCompilationFailureKind::WorkLimit)?;
                    self.context.usage.materialized_value_nodes = nodes;
                    self.context.usage.materialized_value_bytes = bytes;
                    continue;
                }
                let component_programs = references
                    .get(&node)
                    .and_then(|components| components.get(component))
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
                let reference_program = &component_programs.value;
                let origins = &reference_program.origins;
                let plan = reference_program
                    .plan
                    .as_ref()
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
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
                        return SceneCompilationFailureKind::WorkLimit;
                    }
                    self.context.push_prefab_reference_rewrite_error(
                        id,
                        component.as_str(),
                        &error,
                    );
                    SceneCompilationFailureKind::Reference
                })?;
                let cost = payload.value.cost();
                let nodes = cost
                    .nodes()
                    .checked_sub(old_nodes)
                    .and_then(|nodes| nodes.checked_add(new_nodes))
                    .ok_or(SceneCompilationFailureKind::WorkLimit)?;
                let bytes = cost
                    .logical_bytes()
                    .checked_sub(old_bytes)
                    .and_then(|bytes| bytes.checked_add(new_bytes))
                    .ok_or(SceneCompilationFailureKind::WorkLimit)?;
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
                    .ok_or(SceneCompilationFailureKind::WorkLimit)?;
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
                    .ok_or(SceneCompilationFailureKind::WorkLimit)?;
                if !self.context.admit_working_usage(usage, Some(id), None) {
                    return Err(SceneCompilationFailureKind::WorkLimit.into());
                }
                plan.rewrite_in_place(&mut payload.value, |path, reference| {
                    materialized_reference(
                        reference,
                        path,
                        reference_program,
                        &self.nodes,
                        &identifiers,
                    )
                })
                .map_err(|_| SceneCompilationFailureKind::Reference)?;
                self.context.usage.materialized_value_nodes = nodes;
                self.context.usage.materialized_value_bytes = bytes;
                if !component_programs.contextual_defaults.is_empty() {
                    self.materialize_source_defaults(
                        &mut payload.contextual_defaults,
                        &component_programs.contextual_defaults,
                        &identifiers,
                        id,
                        component,
                    )?;
                    // Admit a possible Vec growth before retaining the source-only snapshot.
                    if source_defaults.len() == source_defaults.capacity() {
                        let capacity = source_defaults
                            .len()
                            .saturating_add(1)
                            .max(source_defaults.capacity().saturating_mul(2));
                        self.admit(capacity.checked_mul(size_of::<MaterializedSourceDefaults>()))?;
                        source_defaults.reserve_exact(capacity - source_defaults.len());
                    }
                    self.admit(provenance_string_bytes(id.as_str()).and_then(|bytes| {
                        bytes.checked_add(provenance_string_bytes(component.as_str())?)
                    }))?;
                    source_defaults.push(MaterializedSourceDefaults {
                        entity: id.clone(),
                        component: component.clone(),
                        values: std::mem::take(&mut payload.contextual_defaults),
                    });
                } else {
                    // Native-flat output contains no contextual declarations. Conversion keeps
                    // its separately admitted snapshots outside the effective-value validator.
                    payload.contextual_defaults.clear();
                }
            }
            let mut usage = self.context.usage;
            usage.generated_identifier_bytes = usage
                .generated_identifier_bytes
                .checked_add(generated)
                .ok_or(SceneCompilationFailureKind::WorkLimit)?;
            if !self.context.admit_working_usage(usage, Some(id), None) {
                return Err(SceneCompilationFailureKind::WorkLimit.into());
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
        Ok(StagedSceneOutput {
            document,
            source_defaults,
        })
    }

    fn materialize_source_defaults(
        &mut self,
        defaults: &mut BTreeMap<nara_reflect::ComponentFieldId, nara_reflect::ComponentValue>,
        programs: &BTreeMap<nara_reflect::ComponentFieldId, ReferenceProgram>,
        identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
        entity: &SceneEntityId,
        component: &nara_reflect::ComponentTypeId,
    ) -> Result<(), SceneCompilationFailure> {
        for (field, value) in defaults {
            let program = programs
                .get(field)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let plan = program
                .plan
                .as_ref()
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let mut bytes = Some(0usize);
            plan.visit(value, |path, reference| {
                let replacement = match reference {
                    EntityReference::SceneLocal { .. } | EntityReference::Authored { .. } => {
                        match program
                            .origins
                            .get(path)
                            .and_then(|origin| origin.resolved.as_ref())
                            .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                        {
                            ResolvedReference::Local(target) => provenance_string_bytes(
                                identifiers
                                    .get(&self.nodes[*target].locator)
                                    .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                                    .as_str(),
                            ),
                            ResolvedReference::Persistent(reference) => {
                                reference.cost().nodes().checked_mul(96).and_then(|bytes| {
                                    bytes.checked_add(reference.cost().logical_bytes())
                                })
                            }
                        }
                    }
                    EntityReference::Persistent { .. } => Some(0),
                };
                bytes = bytes.and_then(|bytes| bytes.checked_add(replacement?));
                Ok(())
            })
            .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
            self.admit(bytes)?;
            plan.rewrite_in_place(value, |path, reference| {
                materialized_reference(reference, path, program, &self.nodes, identifiers)
            })
            .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        }
        Ok(())
    }

    fn projection_cost(&self, mut node: NodeId) -> Result<usize, SceneCompilationFailure> {
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
                            .ok_or(SceneCompilationFailureKind::WorkLimit)?,
                    )
                    .ok_or_else(|| SceneCompilationFailureKind::WorkLimit.into());
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
            .ok_or(SceneCompilationFailureKind::WorkLimit)?;
            bytes = bytes
                .checked_add(step)
                .ok_or(SceneCompilationFailureKind::WorkLimit)?;
            node = scope.anchor.unwrap();
        }
    }

    fn projection(
        &self,
        mut node: NodeId,
    ) -> Result<PrefabProjectionPath, SceneCompilationFailure> {
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

fn materialized_reference(
    reference: &EntityReference,
    path: &nara_reflect::ComponentFieldPath,
    program: &ReferenceProgram,
    nodes: &[Node],
    identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
) -> Result<EntityReference, PrefabReferenceRemapError> {
    Ok(match reference {
        EntityReference::SceneLocal { .. } | EntityReference::Authored { .. } => {
            match program
                .origins
                .get(path)
                .and_then(|origin| origin.resolved.as_ref())
                .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
            {
                ResolvedReference::Local(target) => EntityReference::SceneLocal {
                    entity: identifiers
                        .get(&nodes[*target].locator)
                        .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                        .clone(),
                },
                ResolvedReference::Persistent(reference) => reference.clone(),
            }
        }
        EntityReference::Persistent { .. } => reference.clone(),
    })
}
