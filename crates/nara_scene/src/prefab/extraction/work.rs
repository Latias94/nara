//! One cumulative allowance for a cut's source construction and semantic interpretation.

use std::{borrow::Cow, mem::size_of};

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    __private::{
        ComponentPropertyAdmissionError, resolve_component_property_schema_claim_with_admission,
        validate_component_property_value_with_admission,
    },
    ComponentCapability, ComponentCodecError, ComponentEntityReferenceRewriteError,
    ComponentPropertyError, ComponentPropertyPath, ComponentPropertyReadBudget, ComponentTypeId,
    ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits,
    EntityReferenceTraversalLimits,
};

use super::{SourceCut, SourceDocumentInput};
use crate::{
    AuthoringSchemaBasis, PrefabComponentIntent, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstanceHop, PrefabPropertyIntent, SceneCompilationLimits, SceneCompilationOptions,
    SceneCompilationWorkingSet, SceneComponentRecord, SceneEntityPlacement, ScenePatchApplyLimits,
};

#[cfg(test)]
#[path = "work/readiness_tests.rs"]
mod readiness_tests;

#[derive(Debug)]
pub(crate) enum ExtractionError {
    Capacity,
    InvalidSource,
    SavedGuardRequiresRelocation,
    UnknownSchema,
    AdapterUnavailable,
    VersionMismatch,
    OwnerMismatch,
    Value(ComponentCodecError),
    Property(ComponentPropertyError),
    Reference(Box<ComponentEntityReferenceRewriteError<ExtractionError>>),
    Compilation {
        diagnostics: nara_diagnostic::DiagnosticReport,
        failure: Option<crate::SceneCompilationFailure>,
        omissions: Vec<crate::prefab::AuthoringSceneCompilationOmission>,
    },
}

/// Logical work/ownership admission, not allocator isolation or measured RSS. Structural capture's
/// debit remains in the same allowance; discarded construction does not refund cumulative work.
pub(crate) struct ExtractionWork {
    limits: SceneCompilationLimits,
    nodes: usize,
    bytes: usize,
    reference_bytes: usize,
    pub(super) property_reads: ComponentPropertyReadBudget,
}

impl ExtractionWork {
    pub(super) fn from_cut(
        limits: SceneCompilationLimits,
        cut: &SourceCut<'_>,
        captured_work_bytes: Option<usize>,
    ) -> Result<Self, ExtractionError> {
        let mut work = Self::new(limits, captured_work_bytes)?;
        work.settle_compilation(Some(0), cut.reference_work)?;
        Ok(work)
    }

    pub(crate) fn new(
        limits: SceneCompilationLimits,
        captured_work_bytes: Option<usize>,
    ) -> Result<Self, ExtractionError> {
        let bytes = captured_work_bytes
            .filter(|bytes| *bytes <= limits.working_bytes().get())
            .ok_or(ExtractionError::Capacity)?;
        Ok(Self {
            limits,
            nodes: 0,
            bytes,
            reference_bytes: 0,
            property_reads: ComponentPropertyReadBudget::new(
                limits.reference_work_nodes(),
                limits.reference_work_bytes(),
            ),
        })
    }

    pub(crate) const fn usage(&self) -> (usize, usize) {
        (self.nodes, self.bytes)
    }

    /// Each invocation of the original compiler receives only this operation's residual
    /// allowance. Completed and failed invocations both settle their actual cumulative work.
    pub(super) fn compiler_options(
        &self,
        max_depth: usize,
    ) -> Result<SceneCompilationOptions, ExtractionError> {
        Ok(SceneCompilationOptions {
            max_depth,
            limits: self
                .limits
                .with_working_bytes(
                    ByteLimit::new(self.limits.working_bytes().get().saturating_sub(self.bytes))
                        .ok_or(ExtractionError::Capacity)?,
                )
                .with_reference_work_nodes(
                    ItemLimit::new(
                        self.limits
                            .reference_work_nodes()
                            .get()
                            .saturating_sub(self.nodes),
                    )
                    .ok_or(ExtractionError::Capacity)?,
                )
                .with_reference_work_bytes(
                    ByteLimit::new(
                        self.limits
                            .reference_work_bytes()
                            .get()
                            .saturating_sub(self.reference_bytes),
                    )
                    .ok_or(ExtractionError::Capacity)?,
                ),
        })
    }

    pub(super) fn settle_compilation(
        &mut self,
        bytes: Option<usize>,
        references: (usize, usize),
    ) -> Result<(), ExtractionError> {
        self.bytes = self
            .bytes
            .checked_add(bytes.unwrap_or(usize::MAX))
            .unwrap_or(usize::MAX);
        self.nodes = self.nodes.saturating_add(references.0);
        self.reference_bytes = self.reference_bytes.saturating_add(references.1);
        if self.bytes > self.limits.working_bytes().get()
            || self.nodes > self.limits.reference_work_nodes().get()
            || self.reference_bytes > self.limits.reference_work_bytes().get()
        {
            return Err(ExtractionError::Capacity);
        }
        Ok(())
    }

    /// Raw walkers settle work already performed, including prefixes of failed attempts.
    /// An exceeded allowance remains spent so retry cannot repeat that prefix for free.
    pub(super) fn settle_raw_progress(
        &mut self,
        nodes: usize,
        logical_bytes: usize,
    ) -> Result<(), ExtractionError> {
        self.settle_compilation(Some(logical_bytes), (nodes, logical_bytes))
    }

    /// Resolver metadata is admitted while the compiler's actual suspended allocation and
    /// traversal remain live. Only the temporary child debit is removed on return; resolver
    /// work remains charged, and the resumed compiler receives the tightened residual limits.
    pub(super) fn with_suspended_compilation<T>(
        &mut self,
        suspended: &mut SceneCompilationWorkingSet,
        resolve: impl FnOnce(&mut Self) -> Result<T, ExtractionError>,
    ) -> Result<T, ExtractionError> {
        let bytes = suspended
            .checked_work_bytes()
            .ok_or(ExtractionError::Capacity)?;
        let references = suspended.reference_work;
        let live_bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= self.limits.working_bytes().get())
            .ok_or(ExtractionError::Capacity)?;
        let live_nodes = self
            .nodes
            .checked_add(references.0)
            .filter(|nodes| *nodes <= self.limits.reference_work_nodes().get())
            .ok_or(ExtractionError::Capacity)?;
        let live_references = self
            .reference_bytes
            .checked_add(references.1)
            .filter(|bytes| *bytes <= self.limits.reference_work_bytes().get())
            .ok_or(ExtractionError::Capacity)?;
        (self.bytes, self.nodes, self.reference_bytes) = (live_bytes, live_nodes, live_references);
        let result = resolve(self);
        self.bytes -= bytes;
        self.nodes -= references.0;
        self.reference_bytes -= references.1;
        let residual = self.compiler_options(0)?;
        suspended.restrict_working_bytes(residual.limits.working_bytes());
        suspended.restrict_reference_work(
            residual.limits.reference_work_nodes(),
            residual.limits.reference_work_bytes(),
        );
        result
    }

    pub(crate) fn admit(&mut self, bytes: Option<usize>) -> Result<(), ExtractionError> {
        self.visit(1, 0, bytes.ok_or(ExtractionError::Capacity)?)
    }

    pub(crate) fn visit(
        &mut self,
        nodes: usize,
        bytes: usize,
        allocated: usize,
    ) -> Result<(), ExtractionError> {
        let next_nodes = self
            .nodes
            .checked_add(nodes)
            .filter(|nodes| *nodes <= self.limits.reference_work_nodes().get())
            .ok_or(ExtractionError::Capacity)?;
        let next_bytes = self
            .bytes
            .checked_add(bytes)
            .and_then(|bytes| bytes.checked_add(allocated))
            .filter(|bytes| *bytes <= self.limits.working_bytes().get())
            .ok_or(ExtractionError::Capacity)?;
        let reference_bytes = self
            .reference_bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= self.limits.reference_work_bytes().get())
            .ok_or(ExtractionError::Capacity)?;
        (self.nodes, self.bytes, self.reference_bytes) = (next_nodes, next_bytes, reference_bytes);
        Ok(())
    }

    pub(super) fn traversal(&self) -> Result<EntityReferenceTraversalLimits, ExtractionError> {
        Ok(EntityReferenceTraversalLimits::new(
            ItemLimit::new(
                self.limits
                    .reference_work_nodes()
                    .get()
                    .saturating_sub(self.nodes),
            )
            .ok_or(ExtractionError::Capacity)?,
            ByteLimit::new(
                self.limits
                    .working_bytes()
                    .get()
                    .saturating_sub(self.bytes)
                    .min(
                        self.limits
                            .reference_work_bytes()
                            .get()
                            .saturating_sub(self.reference_bytes),
                    ),
            )
            .ok_or(ExtractionError::Capacity)?,
            self.limits.materialization_limits().depth(),
        ))
    }

    pub(super) fn materialization(
        &self,
    ) -> Result<ComponentValueMaterializationLimits, ExtractionError> {
        let remaining = self.traversal()?;
        Ok(ComponentValueMaterializationLimits::new(
            ItemLimit::new(
                remaining
                    .nodes()
                    .get()
                    .min(self.limits.materialized_value_nodes().get()),
            )
            .ok_or(ExtractionError::Capacity)?,
            ByteLimit::new(
                remaining
                    .bytes()
                    .get()
                    .min(self.limits.materialized_value_bytes().get()),
            )
            .ok_or(ExtractionError::Capacity)?,
            remaining.depth(),
        ))
    }

    pub(super) fn value(
        &mut self,
        value: &ComponentValue,
    ) -> Result<ComponentValueCost, ExtractionError> {
        let remaining = self.traversal()?;
        let (result, visited) = value.cost_up_to_with_admission_measured(
            remaining.nodes().get(),
            remaining.bytes().get(),
            |bytes| self.admit(bytes),
        );
        self.settle_raw_progress(visited.nodes(), visited.logical_bytes())?;
        result?.ok_or(ExtractionError::Capacity)
    }

    pub(super) fn own_reference(
        &mut self,
        reference: Cow<'_, EntityReference>,
    ) -> Result<EntityReference, ExtractionError> {
        match reference {
            Cow::Owned(reference) => Ok(reference),
            Cow::Borrowed(reference) => {
                let hops = match reference {
                    EntityReference::Authored { reference } => reference.through.len(),
                    _ => 0,
                };
                self.admit(
                    hops.checked_add(1)
                        .and_then(|nodes| nodes.checked_mul(size_of::<PrefabInstanceHop>())),
                )?;
                self.admit(reference_clone_bytes(reference))?;
                Ok(reference.clone())
            }
        }
    }

    pub(super) fn placement_copy(
        &mut self,
        placement: &SceneEntityPlacement,
    ) -> Result<(), ExtractionError> {
        let remaining = self.traversal()?;
        // The placement cost walk borrows target routes and allocates no iterator frames.
        let cost = crate::prefab::scene_placements_cost(
            std::slice::from_ref(placement),
            crate::prefab::PrefabOverlayCostLimits {
                nodes: remaining.nodes().get(),
                logical_bytes: remaining.bytes().get(),
                ..crate::prefab::PrefabOverlayCostLimits::UNBOUNDED
            },
        )
        .map_err(|_| ExtractionError::Capacity)?;
        self.visit(cost.nodes, cost.logical_bytes(), 0)?;
        self.admit(
            cost.nodes
                .checked_mul(512)
                .and_then(|bytes| bytes.checked_add(cost.logical_bytes())),
        )
    }

    pub(super) fn target_lookup(
        &mut self,
        target: &PrefabEntityTarget,
        entries: usize,
    ) -> Result<(), ExtractionError> {
        let levels = (usize::BITS - entries.leading_zeros()) as usize + 1;
        self.visit(
            target
                .through
                .len()
                .saturating_add(1)
                .saturating_mul(levels),
            0,
            0,
        )?;
        let bytes =
            target
                .through
                .iter()
                .try_fold(target.entity.entity().as_str().len(), |bytes, hop| {
                    bytes
                        .checked_add(hop.anchor.entity().as_str().len())?
                        .checked_add(hop.expected_base.checked_retained_bytes()?)
                });
        self.admit(bytes.and_then(|bytes| bytes.checked_mul(levels)))
    }

    /// The entire raw tree, including dormant metadata, is admitted before its recursive Clone.
    pub(super) fn source_copy(
        &mut self,
        source: SourceDocumentInput<'_>,
    ) -> Result<(), ExtractionError> {
        self.admit_source_copies(source, 1)
    }

    /// Encoding overlaps canonical input copies and serializer storage. Output buffer growth
    /// is admitted separately by the canonical codec under this same cumulative allowance.
    pub(crate) fn admit_source_encoding(
        &mut self,
        source: SourceDocumentInput<'_>,
    ) -> Result<(), ExtractionError> {
        self.admit_source_copies(source, 4)
    }

    fn admit_source_copies(
        &mut self,
        source: SourceDocumentInput<'_>,
        copies: usize,
    ) -> Result<(), ExtractionError> {
        let remaining = self.traversal()?;
        let limits = ScenePatchApplyLimits::default()
            .with_validation_work(remaining.nodes())
            .with_validation_value_nodes(remaining.nodes())
            .with_validation_value_bytes(remaining.bytes());
        let mut observer = SourceWorkObserver {
            work: self,
            refusal: None,
        };
        let cost = crate::patch::bounded_source_cost_with_observer(source, limits, &mut observer);
        if let Some(error) = observer.refusal {
            return Err(error);
        }
        let cost = cost.ok_or(ExtractionError::Capacity)?;
        self.admit(
            cost.operation_count()
                .checked_mul(512)
                .and_then(|bytes| bytes.checked_add(cost.value_cost().nodes().checked_mul(96)?))
                .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
                .and_then(|bytes| bytes.checked_mul(copies)),
        )
    }

    /// Observes the actual borrowed source under this operation's residual allowance. The raw
    /// walker includes dormant additions, Set records and nested instance claims. Schema and
    /// property declarations share their existing owners; no source copy or manifest is built.
    pub(crate) fn source_has_unavailable_records(
        &mut self,
        source: SourceDocumentInput<'_>,
        basis: &AuthoringSchemaBasis,
    ) -> Result<bool, ExtractionError> {
        let remaining = self.traversal()?;
        let limits = ScenePatchApplyLimits::default()
            .with_validation_work(remaining.nodes())
            .with_validation_value_nodes(remaining.nodes())
            .with_validation_value_bytes(remaining.bytes());
        let mut observer = SourceReadinessObserver {
            work: self,
            basis,
            unavailable: false,
            refusal: None,
        };
        let cost = crate::patch::bounded_source_cost_with_observer(source, limits, &mut observer);
        if let Some(error) = observer.refusal {
            return Err(error);
        }
        let unavailable = observer.unavailable;
        cost.ok_or(ExtractionError::Capacity)?;
        Ok(unavailable)
    }

    pub(super) fn source_intent_unavailable(
        &mut self,
        intent: &PrefabComponentIntent,
        basis: &AuthoringSchemaBasis,
    ) -> Result<bool, ExtractionError> {
        let (component, version, claims) = match intent {
            // The complete record is observed separately by the raw walker.
            PrefabComponentIntent::Set { .. } => return Ok(false),
            PrefabComponentIntent::Suppress { component, .. } => (component, None, None),
            PrefabComponentIntent::Properties {
                component,
                version,
                claims,
                ..
            } => (component, Some(*version), Some(claims.as_slice())),
        };
        let schema = match super::payload::require_schema(component, version, basis, self) {
            Ok(schema) => schema,
            Err(ExtractionError::Capacity) => return Err(ExtractionError::Capacity),
            Err(_) => return Ok(true),
        };
        let Some(claims) = claims else {
            return Ok(false);
        };
        // The original declaration-conflict checker owns prefix/duplicate semantics. Admit
        // its borrowed pointer index and complete comparisons before sorting it.
        let comparisons = (usize::BITS - claims.len().leading_zeros()) as usize + 1;
        for claim in claims {
            let Some(bytes) = source_property_result(
                claim
                    .path
                    .logical_bytes()
                    .map_err(ComponentPropertyAdmissionError::Property),
            )?
            else {
                return Ok(true);
            };
            self.visit(
                comparisons,
                bytes
                    .checked_mul(comparisons)
                    .ok_or(ExtractionError::Capacity)?,
                0,
            )?;
        }
        self.admit(
            claims
                .len()
                .checked_mul(size_of::<&ComponentPropertyPath>()),
        )?;
        if crate::prefab::validate_claim_paths(claims).is_err() {
            return Ok(true);
        }
        for claim in claims {
            let mut reads = self.property_reads;
            let selected = resolve_component_property_schema_claim_with_admission(
                schema,
                schema.version(),
                &claim.path,
                &mut reads,
                |nodes, bytes, allocated| self.visit(nodes, bytes, allocated),
            );
            self.property_reads = reads;
            let Some(selected) = source_property_result(selected)? else {
                return Ok(true);
            };
            match &claim.intent {
                PrefabPropertyIntent::Set(value) => {
                    let checked = validate_component_property_value_with_admission(
                        selected.shape,
                        &claim.path,
                        value,
                        self.materialization()?,
                        |nodes, bytes, allocated| self.visit(nodes, bytes, allocated),
                    );
                    if source_property_result(checked)?.is_none() {
                        return Ok(true);
                    }
                }
                PrefabPropertyIntent::Absent => {
                    // These are the compiler's declaration-only Absent restrictions. Its
                    // actual target/default environment remains with the original compiler.
                    if !claim.path.steps.is_empty()
                        || (selected.field.required && selected.field.default_value().is_none())
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

struct SourceWorkObserver<'a> {
    work: &'a mut ExtractionWork,
    refusal: Option<ExtractionError>,
}

impl crate::patch::SourceCostObserver for SourceWorkObserver<'_> {
    fn admit_scratch(&mut self, bytes: Option<usize>) -> bool {
        match self.work.admit(bytes) {
            Ok(()) => true,
            Err(error) => {
                self.refusal = Some(error);
                false
            }
        }
    }

    fn progress(&mut self, nodes: usize, logical_bytes: usize) -> bool {
        match self.work.settle_raw_progress(nodes, logical_bytes) {
            Ok(()) => true,
            Err(error) => {
                self.refusal = Some(error);
                false
            }
        }
    }
}

struct SourceReadinessObserver<'a> {
    work: &'a mut ExtractionWork,
    basis: &'a AuthoringSchemaBasis,
    unavailable: bool,
    refusal: Option<ExtractionError>,
}

impl SourceReadinessObserver<'_> {
    fn observe(&mut self, result: Result<bool, ExtractionError>) -> bool {
        match result {
            Ok(unavailable) => {
                self.unavailable |= unavailable;
                true
            }
            Err(error) => {
                self.refusal = Some(error);
                false
            }
        }
    }
}

impl crate::patch::SourceCostObserver for SourceReadinessObserver<'_> {
    fn admit_scratch(&mut self, bytes: Option<usize>) -> bool {
        let result = self.work.admit(bytes).map(|()| false);
        self.observe(result)
    }

    fn progress(&mut self, nodes: usize, logical_bytes: usize) -> bool {
        let result = self
            .work
            .settle_raw_progress(nodes, logical_bytes)
            .map(|()| false);
        self.observe(result)
    }

    fn record(&mut self, component: &ComponentTypeId, record: &SceneComponentRecord) -> bool {
        let result = self.work.visit(
            1,
            component
                .as_str()
                .len()
                .saturating_add(record.owner().map_or(0, |owner| owner.as_str().len())),
            0,
        );
        let result = result.map(|()| {
            crate::schema_authority::SchemaAuthority::Authoring(self.basis)
                .unavailable_reason(component, record)
                .is_some()
                || self
                    .basis
                    .schema(component)
                    .is_some_and(|schema| !schema.has_capability(ComponentCapability::Scene))
        });
        self.observe(result)
    }

    fn component_intent(&mut self, intent: &PrefabComponentIntent) -> bool {
        let result = self.work.source_intent_unavailable(intent, self.basis);
        self.observe(result)
    }
}

fn source_property_result<T>(
    result: Result<T, ComponentPropertyAdmissionError<ExtractionError>>,
) -> Result<Option<T>, ExtractionError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(ComponentPropertyAdmissionError::Admission(error)) => Err(error),
        Err(ComponentPropertyAdmissionError::Property(
            ComponentPropertyError::ReadBudgetExceeded
            | ComponentPropertyError::PathDepthLimit
            | ComponentPropertyError::PathByteLimit
            | ComponentPropertyError::InvalidValue(
                ComponentCodecError::MaterializationNodeLimit { .. }
                | ComponentCodecError::MaterializationByteLimit { .. }
                | ComponentCodecError::MaterializationDepthLimit { .. },
            ),
        )) => Err(ExtractionError::Capacity),
        Err(ComponentPropertyAdmissionError::Property(_)) => Ok(None),
    }
}

/// A caller admits the hop scan from its O(1) length before consulting this allocation model.
pub(super) fn reference_clone_bytes(reference: &EntityReference) -> Option<usize> {
    let bytes = size_of::<EntityReference>();
    match reference {
        EntityReference::SceneLocal { entity } => bytes.checked_add(entity.as_str().len()),
        EntityReference::Persistent { .. } => bytes.checked_add(reference.cost().logical_bytes()),
        EntityReference::Authored { reference } => {
            let origin = |origin: &PrefabEntityOrigin| {
                size_of::<PrefabEntityOrigin>().checked_add(origin.entity().as_str().len())
            };
            let endpoint = match &reference.target {
                AuthoredReferenceTarget::Entity(entity) => origin(entity)?,
                AuthoredReferenceTarget::Input(input) => input.as_str().len(),
            };
            reference
                .through
                .iter()
                .try_fold(bytes.checked_add(endpoint)?, |bytes, hop| {
                    bytes
                        .checked_add(size_of::<PrefabInstanceHop>())?
                        .checked_add(origin(&hop.anchor)?)?
                        .checked_add(hop.expected_base.checked_retained_bytes()?)
                })
        }
    }
}
