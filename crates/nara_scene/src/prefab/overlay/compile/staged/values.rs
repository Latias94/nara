//! Interprets only retained value programs after structural composition has closed.

use std::collections::BTreeMap;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_reflect::__private::{
    effective_component_value_owned_with_default_overrides, validate_value_materialization_budget,
};
use nara_reflect::{
    ComponentCodecError, ComponentEntityReferenceRewriteError, ComponentFieldPathSegment,
    ComponentSchema, ComponentTypeId, ComponentValue, ComponentValueCost,
    ComponentValueMaterializationLimits,
};

use super::reference_program::{ReferenceProgram, ReferencePrograms};
use super::{Compiler, Program, Record, instance_from_inputs};
use crate::diagnostics::{
    error as diagnostic_error, with_codec_error, with_migration_error, with_public_locator,
};
use crate::prefab::overlay::{components, properties};
use crate::prefab::{
    PrefabExpansionBudgetKind, PrefabExpansionWorkingSet, PrefabReferenceRemapError,
};
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabComponentIntent, PrefabExpansionFailure, PrefabExpansionFailureKind, PrefabOverlaySlot,
    PrefabPropertyIntent, ResolvedPrefabSource, SceneComponentRecord, SceneDocument, SceneEntityId,
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

        let limits = self.value_program_limits();
        let admitted = (|| {
            let cost = bounded_value_cost(&original.value, limits)?;
            for default in original.contextual_defaults.values() {
                validate_value_materialization_budget(default, limits)?;
            }
            if !opaque
                && !original.contextual_defaults.is_empty()
                && let Some(schema) = authority.schema(component)
            {
                crate::document::validate_contextual_defaults(schema, original, limits)?;
            }
            let metadata = provenance_string_bytes(component.as_str())
                .and_then(|bytes| {
                    bytes.checked_add(
                        original
                            .owner()
                            .map_or(Some(0), |owner| provenance_string_bytes(owner.as_str()))?,
                    )
                })
                .and_then(|bytes| bytes.checked_add(size_of::<SceneComponentRecord>()))
                .and_then(|bytes| bytes.checked_add(contextual_defaults_work_bytes(original)?));
            Ok::<_, ComponentCodecError>((cost, metadata))
        })();
        let (cost, metadata) =
            admitted.map_err(|error| self.normalization_failure(&error, entity))?;
        self.admit(metadata)?;
        // Every attempt pays for the complete authored clone, including hidden defaults.
        // Only final lowering charges the materialized value counters.
        self.admit(value_work_bytes(cost))?;
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

        let record_scope = self.record_reference_scope(program.record)?;
        if matches!(program.record, Record::Source { .. }) {
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
            bounded_value_cost(&record.value, limits)
                .map_err(|error| self.normalization_failure(&error, entity))?;
        }
        let schema = authority.current_schema(component, &record)?;
        self.materialize_defaults(schema, &mut record, entity)?;
        components::validate_value(schema, &record.value, self.value_program_limits())?;
        let declaration = match program.record {
            Record::Source { .. } => None,
            Record::Addition { scope, index } => {
                Some((scope, PrefabOverlaySlot::Addition { index }))
            }
            Record::Set { scope, index } => Some((scope, PrefabOverlaySlot::Component { index })),
        };
        let mut references = self.record_reference_origins(
            schema,
            &record.value,
            record_scope,
            declaration,
            entity,
            component,
        )?;
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
            let limits = self.value_program_limits();
            let claim_count = claims.len();
            let claim_work = (|| {
                let mut bytes = Some(0usize);
                let mut values = ComponentValueCost::ZERO;
                for claim in claims {
                    bytes = bytes.and_then(|bytes| {
                        bytes
                            .checked_add(128)?
                            .checked_add(claim.path.logical_bytes().ok()?)
                    });
                    let cost = match &claim.intent {
                        PrefabPropertyIntent::Set(value) => {
                            let cost = bounded_value_cost(value, limits)?;
                            bytes = bytes.and_then(|bytes| {
                                bytes.checked_add(value_work_bytes(cost)?.checked_mul(3)?)
                            });
                            cost
                        }
                        PrefabPropertyIntent::Absent => record
                            .contextual_defaults
                            .get(&claim.path.root)
                            .or_else(|| {
                                schema
                                    .fields()
                                    .iter()
                                    .find(|field| field.id() == &claim.path.root)
                                    .and_then(|field| field.default_value())
                            })
                            .map(|value| bounded_value_cost(value, limits))
                            .transpose()?
                            .unwrap_or_default(),
                    };
                    values = values.saturating_add(cost);
                }
                Ok::<_, ComponentCodecError>((bytes, values))
            })();
            let (claim_bytes, claim_values) =
                claim_work.map_err(|error| self.normalization_failure(&error, entity))?;
            self.admit(claim_bytes)?;
            // prepare_claims keeps an immutable base and an isolated candidate; its default
            // and reference plans are separately bounded before copying selected payloads.
            for _ in 0..2 {
                self.reserve_program_temporary(schema, &record, old_cost, entity)?;
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
                &record.contextual_defaults,
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
                record_scope,
                scope,
                index,
            )?;
            record.value = prepared.value;
            references.plan = None;
        }
        let limits = self.value_program_limits();
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

    pub(super) fn value_program_limits(&self) -> ComponentValueMaterializationLimits {
        // Authored routes and defaults can exceed the final flat value. Each owned copy
        // still pays nodes * 96 + logical bytes against the cumulative working allowance.
        let working = self.context.options.limits.working_bytes();
        ComponentValueMaterializationLimits::new(
            ItemLimit::new((working.get() / 96).max(1))
                .expect("a working allowance produces a non-zero node ceiling"),
            working,
            self.context.materialization_limits().depth(),
        )
    }

    fn materialize_defaults(
        &mut self,
        schema: &ComponentSchema,
        record: &mut SceneComponentRecord,
        entity: &SceneEntityId,
    ) -> Result<(), PrefabExpansionFailure> {
        let old_cost = record.value.cost();
        let limits = self.value_program_limits();
        crate::document::validate_contextual_defaults(schema, record, limits)
            .map_err(|error| self.normalization_failure(&error, entity))?;
        self.reserve_program_temporary(schema, record, old_cost, entity)?;
        let value = std::mem::replace(&mut record.value, ComponentValue::Null);
        record.value = match effective_component_value_owned_with_default_overrides(
            schema,
            value,
            &record.contextual_defaults,
            limits,
        ) {
            Ok(value) => value,
            Err(error) => {
                if self.normalization_failure(&error, entity)
                    == PrefabExpansionFailureKind::WorkLimit
                {
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
        Ok(())
    }

    fn reserve_program_temporary(
        &mut self,
        schema: &ComponentSchema,
        record: &SceneComponentRecord,
        value: ComponentValueCost,
        entity: &SceneEntityId,
    ) -> Result<(), PrefabExpansionFailure> {
        let limits = self.value_program_limits();
        let mut bytes = value_work_bytes(value);
        for field in schema.fields() {
            if let Some(default) = record
                .contextual_defaults
                .get(field.id())
                .or_else(|| field.default_value())
            {
                let cost = bounded_value_cost(default, limits)
                    .map_err(|error| self.normalization_failure(&error, entity))?;
                bytes = bytes
                    .and_then(|bytes| bytes.checked_add(value_work_bytes(cost)?.checked_mul(3)?));
            }
        }
        self.context
            .reserve_scratch_bytes(bytes, Some(entity), None)
            .then_some(())
            .ok_or_else(|| PrefabExpansionFailureKind::WorkLimit.into())
    }

    fn normalization_failure(
        &mut self,
        error: &ComponentCodecError,
        entity: &SceneEntityId,
    ) -> PrefabExpansionFailureKind {
        let budget = match error {
            ComponentCodecError::MaterializationNodeLimit { observed, .. } => Some((
                PrefabExpansionBudgetKind::WorkingBytes,
                observed.saturating_mul(96),
                self.context.options.limits.working_bytes().get(),
            )),
            ComponentCodecError::MaterializationByteLimit { observed, .. } => Some((
                PrefabExpansionBudgetKind::WorkingBytes,
                *observed,
                self.context.options.limits.working_bytes().get(),
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
            PrefabExpansionFailureKind::WorkLimit
        } else {
            PrefabExpansionFailureKind::Component
        }
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

// Reuse Reflect's complete source-value walker before derived Clone can recurse.
fn bounded_value_cost(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValueCost, ComponentCodecError> {
    validate_value_materialization_budget(value, limits)?;
    Ok(value.cost())
}

fn contextual_defaults_work_bytes(record: &SceneComponentRecord) -> Option<usize> {
    record
        .contextual_defaults
        .iter()
        .try_fold(0usize, |bytes, (field, value)| {
            let cost = value.cost();
            bytes
                .checked_add(128)?
                .checked_add(provenance_string_bytes(field.as_str())?)?
                .checked_add(value_work_bytes(cost)?)
        })
}
