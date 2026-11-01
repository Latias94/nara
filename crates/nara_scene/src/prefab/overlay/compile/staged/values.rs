//! Interprets only retained value programs after structural composition has closed.

use std::collections::BTreeMap;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_reflect::{
    ComponentCodecError, ComponentEntityReferenceRewriteError, ComponentFieldPathSegment,
    ComponentSchema, ComponentTypeId, ComponentValue, ComponentValueCost,
    ComponentValueMaterializationLimits, effective_component_value_owned,
};

use super::reference_program::{ReferenceProgram, ReferencePrograms};
use super::{Compiler, Program, Record, instance_from_inputs};
use crate::diagnostics::{
    error as diagnostic_error, with_codec_error, with_migration_error, with_public_locator,
};
use crate::prefab::overlay::{components, properties, references};
use crate::prefab::{
    PrefabExpansionBudgetKind, PrefabExpansionWorkingSet, PrefabReferenceRemapError,
};
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabComponentIntent, PrefabExpansionFailure, PrefabExpansionFailureKind, PrefabOverlaySlot,
    PrefabPropertyIntent, PrefabReferenceBinding, ResolvedPrefabSource, SceneComponentRecord,
    SceneDocument, SceneEntityId,
};

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn materialize_components(
        &mut self,
        document: &mut SceneDocument,
    ) -> Result<ReferencePrograms, PrefabExpansionFailure> {
        let mut references = BTreeMap::new();

        for entity in &mut document.entities {
            let node = *self
                .locators
                .get(&entity.id)
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
            if !self.selects_values(node) {
                continue;
            }
            if !entity.components.is_empty() {
                return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
            }
            let count = self
                .context
                .observe_item_budget(
                    PrefabExpansionBudgetKind::MaterializedComponents,
                    self.context.usage.materialized_components,
                    self.nodes[node].programs.len(),
                    self.context.options.limits.materialized_components(),
                    Some(&entity.id),
                    None,
                )
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
            let mut usage = self.context.usage;
            usage.materialized_components = count;
            if !self
                .context
                .admit_working_usage(usage, Some(&entity.id), None)
            {
                return Err(PrefabExpansionFailureKind::WorkLimit.into());
            }
            self.context.usage.materialized_components = count;

            self.admit(
                self.nodes[node]
                    .programs
                    .keys()
                    .try_fold(128usize, |bytes, component| {
                        bytes
                            .checked_add(256)?
                            .checked_add(provenance_string_bytes(component.as_str())?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(provenance_string_bytes(
                            self.nodes[node].origin.entity().as_str(),
                        )?)
                    }),
            )?;
            // Move the private program index only after admission. Interpreting a record
            // can then borrow the compiler without copying its suffix or source payload.
            let programs = std::mem::take(&mut self.nodes[node].programs);
            let source_entity = self.nodes[node].origin.entity().clone();
            let mut component_references = BTreeMap::new();
            let result = (|| {
                for (component, program) in &programs {
                    let (record, origins) = self
                        .materialize_program(component, program, &source_entity)
                        .map_err(|failure| self.record_failure(program.record, failure))?;
                    component_references.insert(component.clone(), origins);
                    entity.components.insert(component.clone(), record);
                }
                Ok::<_, PrefabExpansionFailure>(())
            })();
            self.nodes[node].programs = programs;
            result?;
            references.insert(node, component_references);
        }
        Ok(references)
    }

    fn materialize_program(
        &mut self,
        component: &ComponentTypeId,
        program: &Program,
        entity: &SceneEntityId,
    ) -> Result<(SceneComponentRecord, ReferenceProgram), PrefabExpansionFailure> {
        let authority = self.context.authority;
        let original = self.component(program.record, component);
        #[cfg(feature = "serde")]
        let opaque = matches!(program.record, Record::Source { .. })
            && authority.unavailable_reason(component, original).is_some();
        #[cfg(not(feature = "serde"))]
        let opaque = false;

        if opaque && !program.properties.is_empty() {
            return Err(PrefabExpansionFailureKind::Component.into());
        }
        if !opaque && authority.schema(component).is_none() {
            self.context.diagnostics.push(with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.unknown-component",
                        "Component type is not registered",
                    ),
                    "entity-id",
                    entity.as_str(),
                ),
                "component-id",
                component.as_str(),
            ));
            return Err(PrefabExpansionFailureKind::Component.into());
        }
        if !opaque && !authority.owner_matches(component, original) {
            return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
        }
        if !matches!(program.record, Record::Source { .. }) {
            authority.current_schema(component, original)?;
        }

        let limits = self.context.materialization_limits();
        let mut visited = 0;
        if let Err((kind, observed, maximum)) =
            admit_value_depth(&original.value, limits, 1, &mut visited)
        {
            self.context
                .fail_budget(kind, observed, maximum, Some(entity), None);
            return Err(PrefabExpansionFailureKind::WorkLimit.into());
        }
        let cost = match original
            .value
            .cost_up_to(limits.nodes().get(), limits.bytes().get())
        {
            Some(cost) => cost,
            None => {
                let (kind, observed, maximum) =
                    match original.value.cost_up_to(limits.nodes().get(), usize::MAX) {
                        Some(cost) => (
                            PrefabExpansionBudgetKind::MaterializedValueBytes,
                            cost.logical_bytes(),
                            limits.bytes().get(),
                        ),
                        None => (
                            PrefabExpansionBudgetKind::MaterializedValueNodes,
                            limits.nodes().get().saturating_add(1),
                            limits.nodes().get(),
                        ),
                    };
                self.context
                    .fail_budget(kind, observed, maximum, Some(entity), None);
                return Err(PrefabExpansionFailureKind::WorkLimit.into());
            }
        };
        let metadata = provenance_string_bytes(component.as_str())
            .and_then(|bytes| {
                bytes.checked_add(
                    original
                        .owner()
                        .map_or(Some(0), |owner| provenance_string_bytes(owner.as_str()))?,
                )
            })
            .and_then(|bytes| bytes.checked_add(size_of::<SceneComponentRecord>()));
        self.admit(metadata)?;
        // Every attempt pays for this clone before allocation, including attempts whose
        // materialized counters are later released when reference demand opens a scope.
        self.admit(value_work_bytes(cost))?;
        self.replace_value_cost(ComponentValueCost::ZERO, cost, entity)?;
        let mut record = self.component(program.record, component).clone();
        if opaque {
            return Ok((
                record,
                ReferenceProgram {
                    origins: BTreeMap::new(),
                    plan: None,
                },
            ));
        }

        let mut references = if let Record::Source { scope, .. } = program.record {
            let previous_version = record.version;
            let value = std::mem::replace(&mut record.value, ComponentValue::Null);
            let migrated = match authority.migrate_owned(component, &record, value) {
                Ok(migrated) => migrated,
                Err(error) => {
                    self.context.diagnostics.push(with_migration_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-component-migration-failed",
                                    "Prefab component migration failed before publication",
                                ),
                                "entity-id",
                                entity.as_str(),
                            ),
                            "component-id",
                            component.as_str(),
                        ),
                        &error,
                    ));
                    return Err(PrefabExpansionFailureKind::Component.into());
                }
            };
            if authority.native_snapshot().is_some() && migrated.version != previous_version {
                self.context.source_upgrade_required = true;
            }
            record.version = migrated.version;
            record.value = migrated.value;
            let migrated_cost = self.bounded_value_cost(&record.value, cost)?;
            self.replace_value_cost(cost, migrated_cost, entity)?;
            let schema = authority.current_schema(component, &record)?;
            self.materialize_defaults(schema, &mut record, entity)?;
            components::validate_value(
                schema,
                &record.value,
                self.value_program_limits(record.value.cost())?,
            )?;
            self.source_reference_origins(schema, &record.value, scope, entity, component)?
        } else {
            let (scope, slot) = match program.record {
                Record::Addition { scope, index } => (scope, PrefabOverlaySlot::Addition { index }),
                Record::Set { scope, index } => (scope, PrefabOverlaySlot::Component { index }),
                Record::Source { .. } => unreachable!("source records were handled above"),
            };
            let bindings = self.value_bindings(program.record, component);
            let binding_bytes = binding_work_bytes(bindings);
            self.admit(binding_bytes)?;
            let schema = authority.current_schema(component, &record)?;
            self.materialize_defaults(schema, &mut record, entity)?;

            let old_cost = record.value.cost();
            let limits = self.value_program_limits(old_cost)?;
            components::validate_value(schema, &record.value, limits)?;
            self.reserve_value_reference_work(schema, old_cost, limits)?;
            let bindings = references::plan_bindings(
                schema,
                &record.value,
                self.value_bindings(program.record, component),
                limits,
            )?;
            self.scope_reference_bindings(bindings, scope, Some(slot))?
        };

        let schema = authority.current_schema(component, &record)?;
        for &(scope, index) in &program.properties {
            let intent = &self
                .overlay(scope)
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?
                .components[index];
            let PrefabComponentIntent::Properties {
                version, claims, ..
            } = intent
            else {
                return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
            };
            if *version != record.version {
                return Err(self.scope_failure(
                    scope,
                    Some(PrefabOverlaySlot::Component { index }),
                    PrefabExpansionFailureKind::Component.into(),
                ));
            }
            let old_cost = record.value.cost();
            let limits = self.value_program_limits(old_cost)?;
            let claim_count = claims.len();
            let claim_work = claims.iter().try_fold(
                (0usize, ComponentValueCost::ZERO),
                |(bytes, values), claim| {
                    let bytes = bytes
                        .checked_add(128)?
                        .checked_add(claim.path.logical_bytes().ok()?)?
                        .checked_add(binding_work_bytes(&claim.bindings)?)?;
                    match &claim.intent {
                        PrefabPropertyIntent::Set(value) => {
                            let cost =
                                value.cost_up_to(limits.nodes().get(), limits.bytes().get())?;
                            Some((
                                bytes.checked_add(value_work_bytes(cost)?.checked_mul(3)?)?,
                                values.saturating_add(cost),
                            ))
                        }
                        PrefabPropertyIntent::Absent => {
                            let cost = schema
                                .fields()
                                .iter()
                                .find(|field| field.id() == &claim.path.root)
                                .and_then(|field| field.default_value())
                                .map(ComponentValue::cost)
                                .unwrap_or_default();
                            Some((bytes, values.saturating_add(cost)))
                        }
                    }
                },
            );
            let (claim_bytes, claim_values) =
                claim_work.ok_or(PrefabExpansionFailureKind::WorkLimit)?;
            self.admit(Some(claim_bytes))?;
            // prepare_claims keeps an immutable base and an isolated candidate; its default
            // and binding plans are separately bounded before copying selected payloads.
            for _ in 0..2 {
                if !self
                    .context
                    .reserve_component_temporary(schema, old_cost, Some(entity))
                {
                    return Err(PrefabExpansionFailureKind::WorkLimit.into());
                }
            }
            self.reserve_value_reference_work(schema, old_cost, limits)?;
            self.reserve_value_reference_work(schema, claim_values, limits)?;
            let site = self.scopes[scope]
                .site
                .as_ref()
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
            let overlay = instance_from_inputs(self.root.entities, &self.scopes, site)
                .and_then(|instance| instance.overrides.as_ref())
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
            let PrefabComponentIntent::Properties {
                version, claims, ..
            } = &overlay.components[index]
            else {
                return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
            };
            let prepared = properties::prepare_claims(
                schema,
                *version,
                &record.value,
                claims,
                limits,
                |bytes| {
                    self.context
                        .reserve_scratch_bytes(Some(bytes), Some(entity), None)
                        .then_some(())
                        .ok_or(PrefabExpansionFailureKind::WorkLimit)
                },
            )
            .map_err(|reason| {
                self.scope_failure(
                    scope,
                    Some(PrefabOverlaySlot::Component { index }),
                    reason.into(),
                )
            })?;
            if prepared.replacements.len() != claim_count {
                return Err(PrefabExpansionFailureKind::OwnerAdmission.into());
            }
            self.replace_reference_origins(
                &mut references.origins,
                prepared.replacements,
                scope,
                index,
            )?;
            self.replace_value_cost(old_cost, prepared.value.cost(), entity)?;
            record.value = prepared.value;
            references.plan = None;
        }
        let limits = self.value_program_limits(record.value.cost())?;
        self.validate_reference_program(
            schema,
            &record.value,
            &mut references,
            limits,
            entity,
            component,
        )?;
        Ok((record, references))
    }

    fn value_bindings(
        &self,
        record: Record,
        component: &ComponentTypeId,
    ) -> &[PrefabReferenceBinding] {
        match record {
            Record::Source { .. } => &[],
            Record::Addition { scope, index } => self.overlay(scope).unwrap().additions[index]
                .bindings
                .get(component)
                .map_or(&[], Vec::as_slice),
            Record::Set { scope, index } => match &self.overlay(scope).unwrap().components[index] {
                PrefabComponentIntent::Set { bindings, .. } => bindings,
                _ => unreachable!("a Set value program retains its declaration"),
            },
        }
    }

    pub(super) fn value_program_limits(
        &self,
        replaced: ComponentValueCost,
    ) -> Result<ComponentValueMaterializationLimits, PrefabExpansionFailure> {
        let usage = self.context.usage;
        let limits = self.context.materialization_limits();
        Ok(components::reserve_cost(
            limits,
            usage
                .materialized_value_nodes
                .checked_sub(replaced.nodes())
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?,
            usage
                .materialized_value_bytes
                .checked_sub(replaced.logical_bytes())
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?,
        )?)
    }

    fn bounded_value_cost(
        &self,
        value: &ComponentValue,
        replaced: ComponentValueCost,
    ) -> Result<ComponentValueCost, PrefabExpansionFailure> {
        let limits = self.value_program_limits(replaced)?;
        value
            .cost_up_to(limits.nodes().get(), limits.bytes().get())
            .ok_or_else(|| PrefabExpansionFailureKind::WorkLimit.into())
    }

    fn replace_value_cost(
        &mut self,
        original: ComponentValueCost,
        replacement: ComponentValueCost,
        entity: &SceneEntityId,
    ) -> Result<(), PrefabExpansionFailure> {
        let (nodes, bytes) = self
            .context
            .preflight_materialized_value_cost(
                original.nodes(),
                replacement.nodes(),
                original.logical_bytes(),
                replacement.logical_bytes(),
                entity,
            )
            .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
        self.context.usage.materialized_value_nodes = nodes;
        self.context.usage.materialized_value_bytes = bytes;
        Ok(())
    }

    fn materialize_defaults(
        &mut self,
        schema: &ComponentSchema,
        record: &mut SceneComponentRecord,
        entity: &SceneEntityId,
    ) -> Result<(), PrefabExpansionFailure> {
        let old_cost = record.value.cost();
        let limits = self.value_program_limits(old_cost)?;
        if !self
            .context
            .reserve_component_temporary(schema, old_cost, Some(entity))
        {
            return Err(PrefabExpansionFailureKind::WorkLimit.into());
        }
        let value = std::mem::replace(&mut record.value, ComponentValue::Null);
        record.value = match effective_component_value_owned(schema, value, limits) {
            Ok(value) => value,
            Err(error) => {
                let budget = match &error {
                    ComponentCodecError::MaterializationNodeLimit { observed, maximum } => Some((
                        PrefabExpansionBudgetKind::MaterializedValueNodes,
                        *observed,
                        *maximum,
                    )),
                    ComponentCodecError::MaterializationByteLimit { observed, maximum } => Some((
                        PrefabExpansionBudgetKind::MaterializedValueBytes,
                        *observed,
                        *maximum,
                    )),
                    ComponentCodecError::MaterializationDepthLimit { observed, maximum } => Some((
                        PrefabExpansionBudgetKind::MaterializedValueDepth,
                        *observed,
                        *maximum,
                    )),
                    _ => None,
                };
                if let Some((kind, observed, maximum)) = budget {
                    self.context
                        .fail_budget(kind, observed, maximum, Some(entity), None);
                    return Err(PrefabExpansionFailureKind::WorkLimit.into());
                }
                self.context.diagnostics.push(with_codec_error(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.prefab-default-materialization-failed",
                                "Prefab component defaults exceeded their admitted interpretation",
                            ),
                            "entity-id",
                            entity.as_str(),
                        ),
                        "component-id",
                        schema.id().as_str(),
                    ),
                    &error,
                    Some(schema),
                ));
                return Err(PrefabExpansionFailureKind::Component.into());
            }
        };
        let cost = self.bounded_value_cost(&record.value, old_cost)?;
        self.replace_value_cost(old_cost, cost, entity)
    }

    pub(super) fn reserve_value_reference_work(
        &mut self,
        schema: &ComponentSchema,
        cost: ComponentValueCost,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<(), PrefabExpansionFailure> {
        let bytes = value_work_bytes(cost)
            .and_then(|bytes| bytes.checked_mul(limits.depth().get().saturating_add(2)))
            .and_then(|bytes| {
                schema.fields().iter().try_fold(bytes, |bytes, field| {
                    bytes
                        .checked_add(128)?
                        .checked_add(provenance_string_bytes(field.id().as_str())?)?
                        .checked_add(field.path().segments().iter().try_fold(
                            0usize,
                            |bytes, segment| {
                                bytes.checked_add(96)?.checked_add(match segment {
                                    ComponentFieldPathSegment::Field(name) => name.len(),
                                    ComponentFieldPathSegment::Index(_) => 0,
                                })
                            },
                        )?)
                })
            });
        self.admit(bytes)
    }

    pub(super) fn source_value_reference_error(
        &mut self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        error: &ComponentEntityReferenceRewriteError<PrefabReferenceRemapError>,
    ) -> PrefabExpansionFailureKind {
        self.context
            .push_prefab_reference_rewrite_error(entity, component.as_str(), error);
        match error {
            ComponentEntityReferenceRewriteError::NodeLimit { .. }
            | ComponentEntityReferenceRewriteError::ByteLimit { .. }
            | ComponentEntityReferenceRewriteError::DepthLimit { .. }
            | ComponentEntityReferenceRewriteError::PathIndexOverflow
            | ComponentEntityReferenceRewriteError::Rewrite {
                error: PrefabReferenceRemapError::MaterializedValue { .. },
                ..
            } => PrefabExpansionFailureKind::WorkLimit,
            _ => PrefabExpansionFailureKind::Reference,
        }
    }
}

fn value_work_bytes(cost: ComponentValueCost) -> Option<usize> {
    cost.nodes()
        .checked_mul(96)?
        .checked_add(cost.logical_bytes())
}

// Check borrowed input before derived Clone can recurse. The recursion itself is bounded
// by the admitted depth, and the node ceiling also bounds flat programmatic inputs.
fn admit_value_depth(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
    depth: usize,
    visited: &mut usize,
) -> Result<(), (PrefabExpansionBudgetKind, usize, usize)> {
    if depth > limits.depth().get() {
        return Err((
            PrefabExpansionBudgetKind::MaterializedValueDepth,
            depth,
            limits.depth().get(),
        ));
    }
    *visited = visited.saturating_add(1);
    if *visited > limits.nodes().get() {
        return Err((
            PrefabExpansionBudgetKind::MaterializedValueNodes,
            *visited,
            limits.nodes().get(),
        ));
    }
    match value {
        ComponentValue::List(values) => {
            for value in values {
                admit_value_depth(value, limits, depth + 1, visited)?;
            }
        }
        ComponentValue::Map(values) => {
            for value in values.values() {
                admit_value_depth(value, limits, depth + 1, visited)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn binding_work_bytes(bindings: &[PrefabReferenceBinding]) -> Option<usize> {
    bindings.iter().try_fold(0usize, |bytes, binding| {
        binding.slot.segments().iter().try_fold(
            bytes
                .checked_add(128)?
                .checked_add(binding.expected.as_str().len())?
                .checked_add(super::target_clone_bytes(&binding.target)?)?,
            |bytes, segment| {
                bytes.checked_add(96)?.checked_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
                })
            },
        )
    })
}
