//! Value-local reference provenance, discarded before the compiled document is published.

use std::collections::BTreeMap;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_identity::AuthoredEntityReference;
use nara_reflect::__private::{
    DeclaredEntityReferencePlan, plan_declared_entity_references,
    plan_declared_value_entity_references,
};
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentSchema, ComponentTypeId, ComponentValue,
    ComponentValueMaterializationLimits, EntityReference,
};

use super::inputs::{InputFailure, ReferenceResolution, ResolvedReference, authored_clone_bytes};
use super::{Compiler, NodeId, ScopeId};
use crate::prefab::overlay::properties::PropertyReferenceReplacement;
use crate::prefab::{PrefabReferenceRemapError, SceneCompilationWorkingSet};
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabOverlaySlot, ResolvedPrefabSource, SceneCompilationFailure, SceneCompilationFailureKind,
    SceneComponentRecord, SceneEntityId,
};

pub(super) struct ScopedReference {
    scope: ScopeId,
    target: AuthoredEntityReference,
    declaration: Option<(ScopeId, PrefabOverlaySlot)>,
    pub(super) resolved: Option<ResolvedReference>,
}

pub(super) type ReferenceOrigins = BTreeMap<ComponentFieldPath, ScopedReference>;

pub(super) struct ReferenceProgram {
    pub(super) origins: ReferenceOrigins,
    pub(super) plan: Option<DeclaredEntityReferencePlan>,
}

pub(super) struct ComponentReferencePrograms {
    pub(super) value: ReferenceProgram,
    pub(super) contextual_defaults: BTreeMap<ComponentFieldId, ReferenceProgram>,
}

pub(super) type ReferencePrograms =
    BTreeMap<NodeId, BTreeMap<ComponentTypeId, ComponentReferencePrograms>>;

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn record_reference_origins(
        &mut self,
        schema: &ComponentSchema,
        value: &ComponentValue,
        scope: ScopeId,
        declaration: Option<(ScopeId, PrefabOverlaySlot)>,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<ReferenceProgram, SceneCompilationFailure> {
        let cost = value.cost();
        let limits = self.value_program_limits();
        self.reserve_value_reference_work(schema, cost, limits)?;
        let plan = plan_declared_entity_references::<PrefabReferenceRemapError>(
            schema,
            value,
            crate::validation::reference_traversal_limits(limits),
        )
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        self.scoped_reference_program(plan, value, scope, declaration, entity, component)
    }

    /// Conversion must carry hidden lexical defaults into the new source environment too.
    /// Ordinary activation keeps these dormant and does not acquire their source demand.
    pub(super) fn contextual_default_reference_programs(
        &mut self,
        schema: &ComponentSchema,
        record: &SceneComponentRecord,
        scope: ScopeId,
        declaration: Option<(ScopeId, PrefabOverlaySlot)>,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<BTreeMap<ComponentFieldId, ReferenceProgram>, SceneCompilationFailure> {
        let mut programs = BTreeMap::new();
        if self.selected_anchor.is_none() {
            return Ok(programs);
        }
        for (id, value) in &record.contextual_defaults {
            let field = schema
                .fields()
                .iter()
                .find(|field| field.id() == id)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let limits = self.value_program_limits();
            self.reserve_value_reference_work(schema, value.cost(), limits)?;
            self.admit(
                size_of::<ReferenceProgram>()
                    .checked_add(128)
                    .and_then(|bytes| bytes.checked_add(provenance_string_bytes(id.as_str())?)),
            )?;
            let plan = plan_declared_value_entity_references::<PrefabReferenceRemapError>(
                field,
                field.shape(),
                value,
                crate::validation::reference_traversal_limits(limits),
            )
            .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
            let program =
                self.scoped_reference_program(plan, value, scope, declaration, entity, component)?;
            programs.insert(id.clone(), program);
        }
        Ok(programs)
    }

    fn scoped_reference_program(
        &mut self,
        plan: DeclaredEntityReferencePlan,
        value: &ComponentValue,
        scope: ScopeId,
        declaration: Option<(ScopeId, PrefabOverlaySlot)>,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<ReferenceProgram, SceneCompilationFailure> {
        let mut bytes = Some(0usize);
        plan.visit(value, |path, reference| {
            let target_bytes = match reference {
                EntityReference::SceneLocal { entity } => provenance_string_bytes(entity.as_str())
                    .and_then(|bytes| bytes.checked_add(size_of::<AuthoredEntityReference>())),
                EntityReference::Authored { reference } => authored_clone_bytes(reference),
                EntityReference::Persistent { .. } => return Ok(()),
            };
            bytes = bytes.and_then(|bytes| {
                bytes
                    .checked_add(origin_entry_bytes(path)?)?
                    .checked_add(target_bytes?)
            });
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        self.admit(bytes)?;
        let mut origins = BTreeMap::new();
        plan.visit(value, |path, reference| {
            let target = match reference {
                EntityReference::SceneLocal { entity } => {
                    AuthoredEntityReference::source(entity.clone())
                }
                EntityReference::Authored { reference } => reference.clone(),
                EntityReference::Persistent { .. } => return Ok(()),
            };
            origins.insert(
                path.clone(),
                ScopedReference {
                    scope,
                    target,
                    declaration,
                    resolved: None,
                },
            );
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        Ok(ReferenceProgram {
            origins,
            plan: Some(plan),
        })
    }

    pub(super) fn replace_reference_origins(
        &mut self,
        origins: &mut ReferenceOrigins,
        replacements: Vec<PropertyReferenceReplacement>,
        record_scope: ScopeId,
        claim_scope: ScopeId,
        component_index: usize,
    ) -> Result<(), SceneCompilationFailure> {
        let lexical_claim_scope = self.overlay_reference_scope(claim_scope)?;
        for (claim_index, replacement) in replacements.into_iter().enumerate() {
            // The same Reflect operation selected this actual storage prefix. Equality
            // of bare values cannot preserve the old declaration's reference meaning.
            self.admit(
                replacement
                    .references
                    .keys()
                    .try_fold(0usize, |bytes, path| {
                        bytes
                            .checked_add(origin_entry_bytes(&replacement.path)?)?
                            .checked_add(path_storage_bytes(path)?)
                    }),
            )?;
            origins.retain(|path, _| !path.segments().starts_with(replacement.path.segments()));
            let scope = if replacement.inherited_default {
                record_scope
            } else {
                lexical_claim_scope
            };
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
                        declaration: Some((
                            claim_scope,
                            PrefabOverlaySlot::Property {
                                component_index,
                                claim_index,
                            },
                        )),
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
    ) -> Result<(), SceneCompilationFailure> {
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
        let mut scoped_count = 0usize;
        let mut complete = true;
        plan.visit(value, |path, reference| {
            match reference {
                EntityReference::SceneLocal { .. } | EntityReference::Authored { .. } => {
                    scoped_count = scoped_count.saturating_add(1);
                    complete &= program.origins.contains_key(path);
                }
                EntityReference::Persistent { .. } => {
                    complete &= !program.origins.contains_key(path);
                }
            }
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        if !complete || scoped_count != program.origins.len() {
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        program.plan = Some(plan);
        Ok(())
    }

    /// Returns false as soon as one new occurrence invalidates the current candidate.
    pub(super) fn close_reference_demands(
        &mut self,
        programs: &mut ReferencePrograms,
    ) -> Result<bool, SceneCompilationFailure> {
        // Do not reject a stale leaf before another surviving leaf has had a chance to
        // open a scope and trigger structural/program recomposition.
        for components in programs.values() {
            for programs in components.values() {
                for origin in programs.value.origins.values().chain(
                    programs
                        .contextual_defaults
                        .values()
                        .flat_map(|program| program.origins.values()),
                ) {
                    if matches!(
                        self.reference_target(origin.scope, &origin.target)?,
                        ReferenceResolution::Entered
                    ) {
                        return Ok(false);
                    }
                }
            }
        }
        // Source-conversion queries share this discovery round. No stale value or query may
        // reject before another witnessed reference opens a scope and requires recomposition.
        for query in self.root.reference_queries {
            if matches!(
                self.reference_target(0, query)?,
                ReferenceResolution::Entered
            ) {
                return Ok(false);
            }
        }
        for (node, components) in programs {
            for (component, programs) in components {
                for (path, origin) in programs.value.origins.iter_mut().chain(
                    programs
                        .contextual_defaults
                        .values_mut()
                        .flat_map(|program| program.origins.iter_mut()),
                ) {
                    let (resolved, declaring_scope) =
                        match self.reference_target(origin.scope, &origin.target)? {
                            ReferenceResolution::Resolved {
                                reference,
                                declaring_scope,
                            } => (reference, declaring_scope),
                            ReferenceResolution::Invalid { reason, input } => {
                                return Err(self.reference_target_failure(
                                    *node, component, path, origin, reason, input,
                                ));
                            }
                            ReferenceResolution::Entered => {
                                return Ok(false);
                            }
                        };
                    if let ResolvedReference::Local(target) = &resolved {
                        if !self.scopes[declaring_scope].output.contains(target) {
                            return Err(self.reference_target_failure(
                                *node,
                                component,
                                path,
                                origin,
                                SceneCompilationFailureKind::MissingTarget,
                                None,
                            ));
                        }
                        if !self.scopes[0].output.contains(target) {
                            return Err(self.reference_target_failure(
                                *node,
                                component,
                                path,
                                origin,
                                SceneCompilationFailureKind::Reference,
                                None,
                            ));
                        }
                    }
                    origin.resolved = Some(resolved);
                }
            }
        }
        Ok(true)
    }

    fn reference_target_failure(
        &mut self,
        node: NodeId,
        component: &ComponentTypeId,
        path: &ComponentFieldPath,
        origin: &ScopedReference,
        reason: SceneCompilationFailureKind,
        input: Option<(ScopeId, InputFailure)>,
    ) -> SceneCompilationFailure {
        let failure = if let Some((scope, failure)) = input {
            self.push_input_failure(failure);
            self.scope_failure(scope, None, reason.into())
        } else if reason == SceneCompilationFailureKind::Reference
            || (origin.declaration.is_none()
                && reason == SceneCompilationFailureKind::MissingTarget)
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
        if let Some((scope, slot)) = origin.declaration {
            self.scope_failure(scope, Some(slot), failure)
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
