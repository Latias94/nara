//! Value-local reference provenance, discarded before the compiled document is published.

use std::collections::BTreeMap;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_reflect::__private::{DeclaredEntityReferencePlan, plan_declared_entity_references};
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPath, ComponentFieldPathSegment,
    ComponentSchema, ComponentTypeId, ComponentValue, ComponentValueMaterializationLimits,
    EntityReference,
};

use super::{Compiler, NodeId, ScopeId, target_clone_bytes};
use crate::prefab::overlay::properties::PropertyReferenceReplacement;
use crate::prefab::{PrefabExpansionWorkingSet, PrefabReferenceRemapError};
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabEntityTarget, PrefabExpansionFailure, PrefabExpansionFailureKind, PrefabOverlaySlot,
    ResolvedPrefabSource, SceneEntityId,
};

pub(super) struct ScopedReference {
    scope: ScopeId,
    target: PrefabEntityTarget,
    slot: Option<PrefabOverlaySlot>,
    pub(super) resolved: Option<NodeId>,
}

pub(super) type ReferenceOrigins = BTreeMap<ComponentFieldPath, ScopedReference>;

pub(super) struct ReferenceProgram {
    pub(super) origins: ReferenceOrigins,
    pub(super) plan: Option<DeclaredEntityReferencePlan>,
}

pub(super) type ReferencePrograms = BTreeMap<NodeId, BTreeMap<ComponentTypeId, ReferenceProgram>>;

enum ReferenceResolution {
    Entered,
    Invalid(PrefabExpansionFailureKind),
    Node(NodeId),
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn source_reference_origins(
        &mut self,
        schema: &ComponentSchema,
        value: &ComponentValue,
        scope: ScopeId,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<ReferenceProgram, PrefabExpansionFailure> {
        let cost = value.cost();
        let limits = self.value_program_limits(cost)?;
        self.reserve_value_reference_work(schema, cost, limits)?;
        let plan = plan_declared_entity_references::<PrefabReferenceRemapError>(
            schema,
            value,
            crate::validation::reference_traversal_limits(limits),
        )
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        let mut bytes = Some(0usize);
        plan.visit(value, |path, reference| {
            if let EntityReference::SceneLocal { entity } = reference {
                bytes = bytes.and_then(|bytes| {
                    bytes
                        .checked_add(origin_entry_bytes(path)?)?
                        .checked_add(size_of::<PrefabEntityTarget>())?
                        .checked_add(provenance_string_bytes(entity.as_str())?)
                });
            }
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        self.admit(bytes)?;
        let mut origins = BTreeMap::new();
        plan.visit(value, |path, reference| {
            if let EntityReference::SceneLocal { entity } = reference {
                origins.insert(
                    path.clone(),
                    ScopedReference {
                        scope,
                        target: PrefabEntityTarget::source(entity.clone()),
                        slot: None,
                        resolved: None,
                    },
                );
            }
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        Ok(ReferenceProgram {
            origins,
            plan: Some(plan),
        })
    }

    pub(super) fn scope_reference_bindings(
        &mut self,
        bindings: BTreeMap<ComponentFieldPath, PrefabEntityTarget>,
        scope: ScopeId,
        slot: Option<PrefabOverlaySlot>,
    ) -> Result<ReferenceProgram, PrefabExpansionFailure> {
        self.admit(
            bindings
                .len()
                .checked_mul(size_of::<ScopedReference>() + 128),
        )?;
        let origins = bindings
            .into_iter()
            .map(|(path, target)| {
                (
                    path,
                    ScopedReference {
                        scope,
                        target,
                        slot,
                        resolved: None,
                    },
                )
            })
            .collect();
        Ok(ReferenceProgram {
            origins,
            plan: None,
        })
    }

    pub(super) fn replace_reference_origins(
        &mut self,
        origins: &mut ReferenceOrigins,
        replacements: Vec<PropertyReferenceReplacement>,
        scope: ScopeId,
        component_index: usize,
    ) -> Result<(), PrefabExpansionFailure> {
        for (claim_index, replacement) in replacements.into_iter().enumerate() {
            // The same Reflect operation selected this actual storage prefix. Equality
            // of bare values cannot preserve the old declaration's reference meaning.
            self.admit(
                origins
                    .keys()
                    .try_fold(0usize, |bytes, path| {
                        bytes.checked_add(origin_entry_bytes(path)?)
                    })
                    .and_then(|bytes| {
                        replacement
                            .references
                            .keys()
                            .try_fold(bytes, |bytes, path| {
                                bytes
                                    .checked_add(origin_entry_bytes(&replacement.path)?)?
                                    .checked_add(path_storage_bytes(path)?)
                            })
                    }),
            )?;
            origins.retain(|path, _| !path.segments().starts_with(replacement.path.segments()));
            for (path, target) in replacement.references {
                let path = ComponentFieldPath::new(
                    replacement
                        .path
                        .segments()
                        .iter()
                        .chain(path.segments())
                        .cloned(),
                );
                origins.insert(
                    path,
                    ScopedReference {
                        scope,
                        target,
                        slot: Some(PrefabOverlaySlot::Property {
                            component_index,
                            claim_index,
                        }),
                        resolved: None,
                    },
                );
            }
        }
        Ok(())
    }

    pub(super) fn validate_reference_program(
        &mut self,
        schema: &ComponentSchema,
        value: &ComponentValue,
        program: &mut ReferenceProgram,
        limits: ComponentValueMaterializationLimits,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<(), PrefabExpansionFailure> {
        let plan = match program.plan.take() {
            Some(plan) => plan,
            None => {
                self.reserve_value_reference_work(schema, value.cost(), limits)?;
                plan_declared_entity_references::<PrefabReferenceRemapError>(
                    schema,
                    value,
                    crate::validation::reference_traversal_limits(limits),
                )
                .map_err(|error| self.source_value_reference_error(entity, component, &error))?
            }
        };
        let mut local_count = 0usize;
        let mut complete = true;
        plan.visit(value, |path, reference| {
            match reference {
                EntityReference::SceneLocal { .. } => {
                    local_count = local_count.saturating_add(1);
                    complete &= program.origins.contains_key(path);
                }
                EntityReference::Persistent { .. } => {
                    complete &= !program.origins.contains_key(path);
                }
            }
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        if !complete || local_count != program.origins.len() {
            return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
        }
        program.plan = Some(plan);
        Ok(())
    }

    /// Returns false as soon as one new occurrence invalidates the current candidate.
    pub(super) fn close_reference_demands(
        &mut self,
        programs: &mut ReferencePrograms,
    ) -> Result<bool, PrefabExpansionFailure> {
        self.admit(programs.values().try_fold(0usize, |bytes, components| {
            components.values().try_fold(bytes, |bytes, program| {
                program.origins.values().try_fold(bytes, |bytes, origin| {
                    bytes.checked_add(target_clone_bytes(&origin.target)?)
                })
            })
        }))?;
        // Do not reject a stale leaf before another surviving leaf has had a chance to
        // open a scope and trigger structural/program recomposition.
        for components in programs.values() {
            for program in components.values() {
                for origin in program.origins.values() {
                    if matches!(
                        self.reference_target(origin.scope, &origin.target)?,
                        ReferenceResolution::Entered
                    ) {
                        return Ok(false);
                    }
                }
            }
        }
        for (node, components) in programs {
            for (component, program) in components {
                for (path, origin) in &mut program.origins {
                    let target = match self.reference_target(origin.scope, &origin.target)? {
                        ReferenceResolution::Node(target) => target,
                        ReferenceResolution::Invalid(reason) => {
                            return Err(self
                                .reference_target_failure(*node, component, path, origin, reason));
                        }
                        ReferenceResolution::Entered => {
                            return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
                        }
                    };
                    if !self.scopes[origin.scope].output.contains(&target) {
                        return Err(self.reference_target_failure(
                            *node,
                            component,
                            path,
                            origin,
                            PrefabExpansionFailureKind::MissingTarget,
                        ));
                    }
                    if !self.scopes[0].output.contains(&target) {
                        return Err(self.reference_target_failure(
                            *node,
                            component,
                            path,
                            origin,
                            PrefabExpansionFailureKind::Reference,
                        ));
                    }
                    origin.resolved = Some(target);
                }
            }
        }
        Ok(true)
    }

    fn reference_target(
        &mut self,
        scope: ScopeId,
        target: &PrefabEntityTarget,
    ) -> Result<ReferenceResolution, PrefabExpansionFailure> {
        let mut current = scope;
        for hop in &target.through {
            let Some(&anchor) = self.scopes[current].nodes.get(&hop.anchor) else {
                return Ok(ReferenceResolution::Invalid(
                    PrefabExpansionFailureKind::MissingTarget,
                ));
            };
            let previous = self.scopes.len();
            let entered = self.open_slot(anchor)?;
            if previous != self.scopes.len() {
                return Ok(ReferenceResolution::Entered);
            }
            let Some(entered) = entered else {
                return Ok(ReferenceResolution::Invalid(
                    PrefabExpansionFailureKind::MissingTarget,
                ));
            };
            current = entered;
            if self.scopes[current]
                .source
                .as_ref()
                .is_none_or(|source| source.key() != &hop.expected_base)
            {
                return Ok(ReferenceResolution::Invalid(
                    PrefabExpansionFailureKind::SourceChanged,
                ));
            }
        }
        Ok(match self.scopes[current].nodes.get(&target.entity) {
            Some(&node) => ReferenceResolution::Node(node),
            None => ReferenceResolution::Invalid(PrefabExpansionFailureKind::MissingTarget),
        })
    }

    fn reference_target_failure(
        &mut self,
        node: NodeId,
        component: &ComponentTypeId,
        path: &ComponentFieldPath,
        origin: &ScopedReference,
        reason: PrefabExpansionFailureKind,
    ) -> PrefabExpansionFailure {
        let failure = if reason == PrefabExpansionFailureKind::Reference
            || (origin.slot.is_none() && reason == PrefabExpansionFailureKind::MissingTarget)
        {
            let bytes = path_storage_bytes(path).and_then(|bytes| {
                bytes.checked_add(provenance_string_bytes(
                    self.nodes[node].origin.entity().as_str(),
                )?)
            });
            if let Err(failure) = self.admit(bytes) {
                return failure;
            }
            let entity = self.nodes[node].origin.entity().clone();
            let error = ComponentEntityReferenceRewriteError::Rewrite {
                path: path.clone(),
                error: PrefabReferenceRemapError::MissingSourceEntity,
            };
            self.source_value_reference_error(&entity, component, &error)
                .into()
        } else {
            reason.into()
        };
        if origin.slot.is_some() {
            self.scope_failure(origin.scope, origin.slot, failure)
        } else {
            failure
        }
    }
}

fn origin_entry_bytes(path: &ComponentFieldPath) -> Option<usize> {
    size_of::<ScopedReference>()
        .checked_add(128)?
        .checked_add(path_storage_bytes(path)?)
}

fn path_storage_bytes(path: &ComponentFieldPath) -> Option<usize> {
    path.segments()
        .iter()
        .try_fold(size_of::<ComponentFieldPath>(), |bytes, segment| {
            bytes
                .checked_add(size_of::<ComponentFieldPathSegment>())?
                .checked_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => 0,
                })
        })
}
