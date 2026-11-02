//! Finite witnessed input endpoints that must outlive removal of the selected declaration.

use super::*;
use nara_identity::{AuthoredEntityReference, PrefabEntityOrigin};
use nara_reflect::{ComponentFieldSchema, ComponentValueShape};

pub(in crate::authoring::prefab_conversion::localize) struct SourceReferenceQueries {
    pub(in crate::authoring::prefab_conversion::localize) values: Vec<AuthoredEntityReference>,
    pub(in crate::authoring::prefab_conversion::localize) work_bytes: usize,
    pub(in crate::authoring::prefab_conversion::localize) reference_work: (usize, usize),
}

pub(in crate::authoring::prefab_conversion::localize) fn collect_source_reference_queries(
    source: &[SceneEntityRecord],
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    limits: EntityReferenceTraversalLimits,
    maximum: nara_core::ByteLimit,
) -> Result<SourceReferenceQueries, ScenePatchReport> {
    let mut inventory = QueryInventory {
        anchor,
        basis,
        limits,
        queries: BTreeSet::new(),
        work: ConversionWork::new(maximum),
        property_reads: nara_reflect::ComponentPropertyReadBudget::new(
            limits.nodes(),
            limits.bytes(),
        ),
        reference_work: (0, 0),
    };
    inventory.work.reserve(
        "source-reference-inventory",
        Some(std::mem::size_of::<QueryInventory<'_>>()),
    )?;
    for record in source {
        for (component, value) in &record.components {
            inventory.record(component, value)?;
        }
        if &record.id != anchor
            && let Some(instance) = &record.prefab
        {
            // Bound nested declaration storage before recursive traversal, without decoding
            // or loading any of its Prefab sources or unknown component payloads.
            prefab_instance_cost(
                instance,
                PrefabOverlayCostLimits {
                    nodes: ScenePatchApplyLimits::default().validation_work().get(),
                    depth: SceneFileLimits::default().shape().depth().get(),
                    ..PrefabOverlayCostLimits::UNBOUNDED
                },
            )
            .map_err(|_| copy_limit())?;
            inventory.instance(instance)?;
        }
    }
    inventory.work.reserve(
        "source-reference-query-slice",
        inventory
            .queries
            .len()
            .checked_mul(std::mem::size_of::<AuthoredEntityReference>()),
    )?;
    Ok(SourceReferenceQueries {
        values: inventory.queries.into_iter().collect(),
        work_bytes: inventory.work.used_bytes(),
        reference_work: inventory.reference_work,
    })
}

struct QueryInventory<'a> {
    anchor: &'a SceneEntityId,
    basis: &'a AuthoringSchemaBasis,
    limits: EntityReferenceTraversalLimits,
    queries: BTreeSet<AuthoredEntityReference>,
    work: ConversionWork,
    property_reads: nara_reflect::ComponentPropertyReadBudget,
    reference_work: (usize, usize),
}

fn admit_reference_work(
    total: &mut (usize, usize),
    limits: EntityReferenceTraversalLimits,
    work: &mut ConversionWork,
    nodes: usize,
    bytes: usize,
    allocated: usize,
) -> Result<(), ScenePatchReport> {
    let nodes = total
        .0
        .checked_add(nodes)
        .filter(|nodes| *nodes <= limits.nodes().get())
        .ok_or_else(copy_limit)?;
    let bytes = total
        .1
        .checked_add(bytes)
        .filter(|bytes| *bytes <= limits.bytes().get())
        .ok_or_else(copy_limit)?;
    work.reserve("source-reference-plan", Some(allocated))?;
    *total = (nodes, bytes);
    Ok(())
}

impl QueryInventory<'_> {
    fn reference(&mut self, reference: &EntityReference) -> Result<(), ScenePatchReport> {
        let EntityReference::Authored { reference: query } = reference else {
            return Ok(());
        };
        if !matches!(query.target, AuthoredReferenceTarget::Input(_))
            || !query.through.first().is_some_and(|hop| {
                matches!(&hop.anchor, PrefabEntityOrigin::SourceEntity(entity) if entity == self.anchor)
            }) || self.queries.contains(query)
        { return Ok(()); }
        let cost = reference.cost();
        self.work.reserve(
            "source-reference-query",
            cost.nodes()
                .checked_mul(96)
                .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
                .and_then(|bytes| bytes.checked_add(128)),
        )?;
        self.queries.insert(query.clone());
        Ok(())
    }

    fn plan(
        &mut self,
        value: &ComponentValue,
        plan: &DeclaredEntityReferencePlan,
    ) -> Result<(), ScenePatchReport> {
        plan.visit(value, |_, reference| self.reference(reference))
            .map_err(reference_error)
    }

    fn record(
        &mut self,
        component: &ComponentTypeId,
        record: &crate::SceneComponentRecord,
    ) -> Result<(), ScenePatchReport> {
        let Some(schema) = understood_schema(component, record, self.basis) else {
            return Ok(());
        };
        let plan = plan_declared_entity_references_with_admission::<ScenePatchReport, _>(
            schema,
            &record.value,
            self.limits,
            |nodes, bytes, allocated| {
                admit_reference_work(
                    &mut self.reference_work,
                    self.limits,
                    &mut self.work,
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(plan_error)?;
        self.plan(&record.value, &plan)?;
        for (id, value) in &record.contextual_defaults {
            let field = schema
                .fields()
                .iter()
                .find(|field| field.id() == id)
                .ok_or_else(|| {
                    refused("Conversion requires the declared contextual default field")
                })?;
            self.payload(value, field, field.shape())?;
        }
        Ok(())
    }

    fn payload(
        &mut self,
        value: &ComponentValue,
        field: &ComponentFieldSchema,
        shape: &ComponentValueShape,
    ) -> Result<(), ScenePatchReport> {
        let plan = plan_declared_value_entity_references_with_admission::<ScenePatchReport, _>(
            field,
            shape,
            value,
            self.limits,
            |nodes, bytes, allocated| {
                admit_reference_work(
                    &mut self.reference_work,
                    self.limits,
                    &mut self.work,
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(plan_error)?;
        self.plan(value, &plan)
    }

    fn instance(&mut self, instance: &crate::PrefabInstance) -> Result<(), ScenePatchReport> {
        self.work.reserve("source-reference-instance", Some(128))?;
        if let Some(arguments) = &instance.arguments {
            for reference in arguments.values.values() {
                let cost = reference.cost();
                admit_reference_work(
                    &mut self.reference_work,
                    self.limits,
                    &mut self.work,
                    cost.nodes(),
                    cost.logical_bytes(),
                    0,
                )?;
                self.reference(reference)?;
            }
        }
        let Some(overlay) = &instance.overrides else {
            return Ok(());
        };
        for addition in &overlay.additions {
            for (component, record) in &addition.components {
                self.record(component, record)?;
            }
            if let Some(instance) = &addition.prefab {
                self.instance(instance)?;
            }
        }
        for claim in &overlay.instances {
            if let Some(instance) = &claim.value {
                self.instance(instance)?;
            }
        }
        for intent in &overlay.components {
            match intent {
                crate::PrefabComponentIntent::Set {
                    component, record, ..
                } => self.record(component, record)?,
                crate::PrefabComponentIntent::Properties {
                    component,
                    version,
                    claims,
                    ..
                } => {
                    let Some(schema) = self.basis.schema(component).filter(|schema| {
                        schema.version() == *version
                            && self.basis.has_authoring_adapter(component)
                            && schema.has_capability(nara_reflect::ComponentCapability::Scene)
                    }) else {
                        continue;
                    };
                    for claim in claims {
                        let crate::PrefabPropertyIntent::Set(value) = &claim.intent else {
                            continue;
                        };
                        let selected = nara_reflect::__private::resolve_component_property_schema_claim_with_admission(
                            schema, *version, &claim.path, &mut self.property_reads,
                            |nodes, bytes, allocated| admit_reference_work(
                                &mut self.reference_work, self.limits, &mut self.work, nodes, bytes, allocated,
                            ),
                        );
                        let selected = match selected {
                            Ok(selected) => selected,
                            Err(
                                nara_reflect::__private::ComponentPropertyAdmissionError::Property(
                                    _,
                                ),
                            ) => continue,
                            Err(
                                nara_reflect::__private::ComponentPropertyAdmissionError::Admission(
                                    report,
                                ),
                            ) => return Err(report),
                        };
                        self.payload(value, selected.field, selected.shape)?;
                    }
                }
                crate::PrefabComponentIntent::Suppress { .. } => {}
            }
        }
        Ok(())
    }
}
