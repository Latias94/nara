//! Interprets only retained value programs after structural composition has closed.

use std::collections::BTreeMap;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_reflect::__private::plan_declared_entity_references;
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPathSegment, ComponentSchema,
    ComponentTypeId, ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits,
    EntityReference, effective_component_value_owned,
};

use super::{Compiler, Program, Record, ScopeId};
use crate::diagnostics::{
    error as diagnostic_error, with_codec_error, with_migration_error, with_public_locator,
};
use crate::prefab::overlay::{components, properties, references};
use crate::prefab::{
    PrefabExpansionBudgetKind, PrefabExpansionWorkingSet, PrefabReferenceRemapError,
};
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabComponentIntent, PrefabEntityOrigin, PrefabEntityTarget, PrefabOverlayConflict,
    PrefabPropertyIntent, PrefabReferenceBinding, ResolvedPrefabSource, SceneComponentRecord,
    SceneDocument, SceneEntityId,
};

type Targets = BTreeMap<PrefabEntityTarget, SceneEntityId>;
type SourceNamespace = BTreeMap<SceneEntityId, SceneEntityId>;

#[derive(Default)]
struct ValueNamespaces {
    sources: BTreeMap<ScopeId, SourceNamespace>,
    targets: BTreeMap<ScopeId, Targets>,
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn materialize_components(
        &mut self,
        document: &mut SceneDocument,
    ) -> Result<(), PrefabOverlayConflict> {
        let mut namespaces = ValueNamespaces::default();

        for entity in &mut document.entities {
            let node = *self
                .locators
                .get(&entity.id)
                .ok_or(PrefabOverlayConflict::OwnerAdmission)?;
            if !entity.components.is_empty() {
                return Err(PrefabOverlayConflict::OwnerAdmission);
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
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            let mut usage = self.context.usage;
            usage.materialized_components = count;
            if !self
                .context
                .admit_working_usage(usage, Some(&entity.id), None)
            {
                return Err(PrefabOverlayConflict::WorkLimit);
            }
            self.context.usage.materialized_components = count;

            // Move the private program index aside so interpreting a record can borrow the
            // compiler mutably without copying its property suffix or any source payload.
            let programs = std::mem::take(&mut self.nodes[node].programs);
            let source_entity = self.nodes[node].origin.entity().clone();
            for (component, program) in &programs {
                let record =
                    self.materialize_program(component, program, &source_entity, &mut namespaces)?;
                entity.components.insert(component.clone(), record);
            }
            self.nodes[node].programs = programs;
        }
        Ok(())
    }

    fn materialize_program(
        &mut self,
        component: &ComponentTypeId,
        program: &Program,
        entity: &SceneEntityId,
        namespaces: &mut ValueNamespaces,
    ) -> Result<SceneComponentRecord, PrefabOverlayConflict> {
        let authority = self.context.authority;
        let original = self.component(program.record, component);
        #[cfg(feature = "serde")]
        let opaque = matches!(program.record, Record::Source { .. })
            && authority.unavailable_reason(component, original).is_some();
        #[cfg(not(feature = "serde"))]
        let opaque = false;

        if opaque && !program.properties.is_empty() {
            return Err(PrefabOverlayConflict::Component);
        }
        if !opaque && !authority.owner_matches(component, original) {
            return Err(PrefabOverlayConflict::OwnerAdmission);
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
            return Err(PrefabOverlayConflict::WorkLimit);
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
                return Err(PrefabOverlayConflict::WorkLimit);
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
        self.replace_value_cost(ComponentValueCost::ZERO, cost, entity)?;
        let mut record = self.component(program.record, component).clone();
        if opaque {
            return Ok(record);
        }

        if let Record::Source { scope, .. } = program.record {
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
                    return Err(PrefabOverlayConflict::Component);
                }
            };
            if authority.native_registry().is_some() && migrated.version != previous_version {
                self.context.source_upgrade_required = true;
            }
            record.version = migrated.version;
            record.value = migrated.value;
            let migrated_cost = self.bounded_value_cost(&record.value, cost)?;
            self.replace_value_cost(cost, migrated_cost, entity)?;
            let schema = authority.current_schema(component, &record)?;
            self.materialize_defaults(schema, &mut record, entity, 0)?;
            components::validate_value(
                schema,
                &record.value,
                self.value_program_limits(record.value.cost())?,
            )?;
            self.ensure_source_namespace(scope, namespaces)?;
            self.remap_source_references(
                schema,
                &mut record.value,
                &namespaces.sources[&scope],
                entity,
                component,
            )?;
        } else {
            let scope = match program.record {
                Record::Addition { scope, .. } | Record::Set { scope, .. } => scope,
                Record::Source { .. } => unreachable!("source records were handled above"),
            };
            self.ensure_value_targets(scope, namespaces)?;
            let targets = &namespaces.targets[&scope];
            let limits = self.value_program_limits(record.value.cost())?;
            let bindings = self.value_bindings(program.record, component);
            let growth = references::binding_growth(bindings, targets, limits)?;
            let binding_bytes = binding_work_bytes(bindings);
            self.admit(binding_bytes)?;
            self.admit(Some(growth))?;
            let schema = authority.current_schema(component, &record)?;
            self.materialize_defaults(schema, &mut record, entity, growth)?;

            let old_cost = record.value.cost();
            let limits = self.value_program_limits(old_cost)?;
            components::validate_value(schema, &record.value, limits)?;
            self.reserve_value_reference_work(schema, old_cost, limits)?;
            let predicted_bytes = old_cost
                .logical_bytes()
                .checked_add(growth)
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            self.context
                .preflight_materialized_value_cost(
                    old_cost.nodes(),
                    old_cost.nodes(),
                    old_cost.logical_bytes(),
                    predicted_bytes,
                    entity,
                )
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            let bound = references::bind_references(
                schema,
                &record.value,
                self.value_bindings(program.record, component),
                targets,
                limits,
            )?;
            components::validate_value(schema, &bound, limits)?;
            self.replace_value_cost(old_cost, bound.cost(), entity)?;
            record.value = bound;
        }

        let schema = authority.current_schema(component, &record)?;
        for &(scope, index) in &program.properties {
            let intent = &self
                .overlay(scope)
                .ok_or(PrefabOverlayConflict::OwnerAdmission)?
                .components[index];
            let PrefabComponentIntent::Properties {
                version, claims, ..
            } = intent
            else {
                return Err(PrefabOverlayConflict::OwnerAdmission);
            };
            if *version != record.version {
                return Err(PrefabOverlayConflict::Component);
            }
            let old_cost = record.value.cost();
            let limits = self.value_program_limits(old_cost)?;
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
            let (claim_bytes, claim_values) = claim_work.ok_or(PrefabOverlayConflict::WorkLimit)?;
            self.admit(Some(claim_bytes))?;
            self.ensure_value_targets(scope, namespaces)?;
            // apply_claims keeps an immutable base and an isolated candidate; its default
            // and binding plans are separately bounded before copying selected payloads.
            for _ in 0..2 {
                if !self
                    .context
                    .reserve_component_temporary(schema, old_cost, Some(entity))
                {
                    return Err(PrefabOverlayConflict::WorkLimit);
                }
            }
            self.reserve_value_reference_work(schema, old_cost, limits)?;
            self.reserve_value_reference_work(schema, claim_values, limits)?;
            let PrefabComponentIntent::Properties {
                version, claims, ..
            } = &self.overlay(scope).unwrap().components[index]
            else {
                return Err(PrefabOverlayConflict::OwnerAdmission);
            };
            let value = properties::apply_claims(
                schema,
                *version,
                &record.value,
                claims,
                &namespaces.targets[&scope],
                limits,
            )?;
            self.reserve_value_reference_work(schema, value.cost(), limits)?;
            references::validate_references(schema, &value, limits)?;
            self.replace_value_cost(old_cost, value.cost(), entity)?;
            record.value = value;
        }
        // The structural owner rewrites these internal references only after all suffixes
        // have run, so a later pin can replace a reference to an entity deleted outside.
        Ok(record)
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

    fn value_program_limits(
        &self,
        replaced: ComponentValueCost,
    ) -> Result<ComponentValueMaterializationLimits, PrefabOverlayConflict> {
        let usage = self.context.usage;
        let limits = self.context.materialization_limits();
        components::reserve_cost(
            limits,
            usage
                .materialized_value_nodes
                .checked_sub(replaced.nodes())
                .ok_or(PrefabOverlayConflict::WorkLimit)?,
            usage
                .materialized_value_bytes
                .checked_sub(replaced.logical_bytes())
                .ok_or(PrefabOverlayConflict::WorkLimit)?,
        )
    }

    fn bounded_value_cost(
        &self,
        value: &ComponentValue,
        replaced: ComponentValueCost,
    ) -> Result<ComponentValueCost, PrefabOverlayConflict> {
        let limits = self.value_program_limits(replaced)?;
        value
            .cost_up_to(limits.nodes().get(), limits.bytes().get())
            .ok_or(PrefabOverlayConflict::WorkLimit)
    }

    fn replace_value_cost(
        &mut self,
        original: ComponentValueCost,
        replacement: ComponentValueCost,
        entity: &SceneEntityId,
    ) -> Result<(), PrefabOverlayConflict> {
        let (nodes, bytes) = self
            .context
            .preflight_materialized_value_cost(
                original.nodes(),
                replacement.nodes(),
                original.logical_bytes(),
                replacement.logical_bytes(),
                entity,
            )
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
        self.context.usage.materialized_value_nodes = nodes;
        self.context.usage.materialized_value_bytes = bytes;
        Ok(())
    }

    fn materialize_defaults(
        &mut self,
        schema: &ComponentSchema,
        record: &mut SceneComponentRecord,
        entity: &SceneEntityId,
        binding_growth: usize,
    ) -> Result<(), PrefabOverlayConflict> {
        let old_cost = record.value.cost();
        self.context
            .preflight_materialized_value_cost(
                old_cost.nodes(),
                old_cost.nodes(),
                old_cost.logical_bytes(),
                old_cost
                    .logical_bytes()
                    .checked_add(binding_growth)
                    .ok_or(PrefabOverlayConflict::WorkLimit)?,
                entity,
            )
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
        let limits =
            components::reserve_cost(self.value_program_limits(old_cost)?, 0, binding_growth)?;
        if !self
            .context
            .reserve_component_temporary(schema, old_cost, Some(entity))
        {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        let value = std::mem::replace(&mut record.value, ComponentValue::Null);
        record.value = match effective_component_value_owned(schema, value, limits) {
            Ok(value) => value,
            Err(error) => {
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
                return Err(PrefabOverlayConflict::Component);
            }
        };
        let cost = self.bounded_value_cost(&record.value, old_cost)?;
        self.replace_value_cost(old_cost, cost, entity)
    }

    fn ensure_source_namespace(
        &mut self,
        scope: ScopeId,
        namespaces: &mut ValueNamespaces,
    ) -> Result<(), PrefabOverlayConflict> {
        if namespaces.sources.contains_key(&scope) {
            return Ok(());
        }
        let bytes = self.scopes[scope]
            .nodes
            .iter()
            .filter_map(|(origin, node)| match origin {
                PrefabEntityOrigin::SourceEntity(id) => {
                    Some((id, &self.value_ids[&self.nodes[*node].locator]))
                }
                PrefabEntityOrigin::InstanceAddition(_) => None,
            })
            .try_fold(128usize, |bytes, (id, locator)| {
                bytes
                    .checked_add(128)?
                    .checked_add(provenance_string_bytes(id.as_str())?)?
                    .checked_add(provenance_string_bytes(locator.as_str())?)
            });
        self.admit(bytes)?;
        let namespace = self.scopes[scope]
            .nodes
            .iter()
            .filter_map(|(origin, node)| match origin {
                PrefabEntityOrigin::SourceEntity(id) => Some((
                    id.clone(),
                    self.value_ids[&self.nodes[*node].locator].clone(),
                )),
                PrefabEntityOrigin::InstanceAddition(_) => None,
            })
            .collect();
        namespaces.sources.insert(scope, namespace);
        Ok(())
    }

    fn ensure_value_targets(
        &mut self,
        scope: ScopeId,
        namespaces: &mut ValueNamespaces,
    ) -> Result<(), PrefabOverlayConflict> {
        if !namespaces.targets.contains_key(&scope) {
            self.admit(Some(128))?;
            self.admit(self.target_index_bytes(scope))?;
            let mut targets = self.target_nodes(scope)?;
            self.admit(targets.values().try_fold(0usize, |bytes, locator| {
                bytes.checked_add(provenance_string_bytes(self.value_ids[locator].as_str())?)
            }))?;
            for value in targets.values_mut() {
                *value = self.value_ids[value].clone();
            }
            namespaces.targets.insert(scope, targets);
        }
        Ok(())
    }

    fn reserve_value_reference_work(
        &mut self,
        schema: &ComponentSchema,
        cost: ComponentValueCost,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<(), PrefabOverlayConflict> {
        let bytes = value_work_bytes(cost)
            .and_then(|bytes| bytes.checked_mul(limits.depth().get().saturating_add(2)))
            .and_then(|bytes| {
                schema.fields().iter().try_fold(bytes, |bytes, field| {
                    bytes
                        .checked_add(128)?
                        .checked_add(provenance_string_bytes(field.id().as_str())?)?
                        .checked_add(field.path().segments().len().checked_mul(96)?)
                })
            });
        self.admit(bytes)
    }

    fn remap_source_references(
        &mut self,
        schema: &ComponentSchema,
        value: &mut ComponentValue,
        namespace: &SourceNamespace,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> Result<(), PrefabOverlayConflict> {
        let cost = value.cost();
        let limits = self.value_program_limits(cost)?;
        self.reserve_value_reference_work(schema, cost, limits)?;
        let plan = plan_declared_entity_references::<PrefabReferenceRemapError>(
            schema,
            value,
            crate::validation::reference_traversal_limits(limits),
        )
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        let mut before = 0usize;
        let mut after = 0usize;
        plan.visit(value, |_, reference| {
            if let EntityReference::SceneLocal { entity } = reference {
                let mapped = namespace
                    .get(entity)
                    .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?;
                let overflow = || PrefabReferenceRemapError::MaterializedValue {
                    observed: usize::MAX,
                    maximum: limits.bytes().get(),
                };
                before = before
                    .checked_add(entity.as_str().len())
                    .ok_or_else(overflow)?;
                after = after
                    .checked_add(mapped.as_str().len())
                    .ok_or_else(overflow)?;
            }
            Ok(())
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        let bytes = cost
            .logical_bytes()
            .checked_sub(before)
            .and_then(|bytes| bytes.checked_add(after))
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
        self.admit(Some(after))?;
        let (nodes, bytes) = self
            .context
            .preflight_materialized_value_cost(
                cost.nodes(),
                cost.nodes(),
                cost.logical_bytes(),
                bytes,
                entity,
            )
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
        plan.rewrite_in_place(value, |_, reference| {
            Ok(match reference {
                EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                    entity: namespace
                        .get(entity)
                        .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                        .clone(),
                },
                EntityReference::Persistent { .. } => reference.clone(),
            })
        })
        .map_err(|error| self.source_value_reference_error(entity, component, &error))?;
        self.context.usage.materialized_value_nodes = nodes;
        self.context.usage.materialized_value_bytes = bytes;
        Ok(())
    }

    fn source_value_reference_error(
        &mut self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        error: &ComponentEntityReferenceRewriteError<PrefabReferenceRemapError>,
    ) -> PrefabOverlayConflict {
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
            } => PrefabOverlayConflict::WorkLimit,
            _ => PrefabOverlayConflict::Reference,
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
                .checked_add(binding.expected.as_str().len())?,
            |bytes, segment| {
                bytes.checked_add(96)?.checked_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
                })
            },
        )
    })
}
