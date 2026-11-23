//! Read-only observations from the selected authoring compiler.

use std::{
    collections::BTreeMap,
    fmt,
    mem::size_of,
    ops::Bound,
    sync::{Arc, Weak},
};

use nara_asset::AssetRef;
use nara_diagnostic::DiagnosticReport;
use nara_identity::EntityReference;
use nara_reflect::{
    __private::validate_value_materialization_budget, ComponentFieldId, ComponentFieldPathError,
    ComponentFieldSchema, ComponentPropertyError, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueMaterializationLimits, ComponentValueRef, ComponentValueShape,
    ResolvedComponentProperty, resolve_component_property_in_root,
};

use super::{
    SelectedInstanceConsumer, SourceDocumentInput,
    try_compile_selected_authoring_instance_with_loader,
};
use crate::{
    AuthoringPropertyChildrenLimits, AuthoringPropertyChildrenPage,
    AuthoringPropertyChildrenPosition, AuthoringReadError, AuthoringRecordOwnerClaim,
    AuthoringRecordReadiness, AuthoringSchemaBasis, ExpandedSceneComponent, PrefabComponentIntent,
    PrefabDeclarationSource, PrefabEntityTarget, PrefabInstanceLocation, PrefabOverlaySlot,
    ResolvedPrefabSource, SceneAuthoringRevision, SceneCompilationFailure,
    SceneCompilationFailureKind, SceneCompilationOptions, SceneCompilationWorkingSet,
    SceneEntityId, SourceDocumentKey,
    authoring_decode::{property_children_in_schema, property_root},
    draft::SceneDraft,
};

mod component_write;
mod property_apply;
mod property_write;

pub use component_write::{AuthoringPrefabComponentEditError, ObservedPrefabComponentEdit};
pub use property_apply::{AuthoringPrefabPropertyApplyError, ObservedPrefabPropertyApply};
pub use property_write::{AuthoringPrefabPropertyWriteError, ObservedPrefabPropertyWrite};

/// A working-source inspection can prepare edits against its captured document allocation.
#[doc(hidden)]
pub enum PrefabInspectionInput<'a> {
    WorkingScene(&'a Arc<crate::SceneDocument>, SceneAuthoringRevision),
    WorkingPrefab(&'a Arc<crate::PrefabDocument>, SceneAuthoringRevision),
    Borrowed(SourceDocumentInput<'a>),
}

#[derive(Debug, Clone)]
pub(crate) enum WorkingSourceObservation {
    Scene(Weak<crate::SceneDocument>, SceneAuthoringRevision),
    Prefab(Weak<crate::PrefabDocument>, SceneAuthoringRevision),
}

impl WorkingSourceObservation {
    fn revision_for(&self, input: SourceDocumentInput<'_>) -> Option<SceneAuthoringRevision> {
        match (self, input) {
            (Self::Scene(original, revision), SourceDocumentInput::Scene(current))
                if std::ptr::eq(original.as_ptr(), current) =>
            {
                Some(*revision)
            }
            (Self::Prefab(original, revision), SourceDocumentInput::Prefab(current))
                if std::ptr::eq(original.as_ptr(), current) =>
            {
                Some(*revision)
            }
            _ => None,
        }
    }
}

/// Compiles an actual source's selected directly owned Prefab instance for authoring reads.
/// The loader receives only requests made by structural and surviving effective-value demand.
pub fn try_inspect_authoring_prefab_instance_with_loader<F, E>(
    input: PrefabInspectionInput<'_>,
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    options: SceneCompilationOptions,
    loader: F,
) -> Result<AuthoringPrefabInstanceSnapshot, AuthoringPrefabInspectionError<E>>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let (source, working_source) = match input {
        PrefabInspectionInput::WorkingScene(document, revision) => (
            SourceDocumentInput::Scene(document),
            Some(WorkingSourceObservation::Scene(
                Arc::downgrade(document),
                revision,
            )),
        ),
        PrefabInspectionInput::WorkingPrefab(document, revision) => (
            SourceDocumentInput::Prefab(document),
            Some(WorkingSourceObservation::Prefab(
                Arc::downgrade(document),
                revision,
            )),
        ),
        PrefabInspectionInput::Borrowed(source) => (source, None),
    };
    let compiled = try_compile_selected_authoring_instance_with_loader(
        source,
        anchor,
        SelectedInstanceConsumer::Inspection,
        basis,
        options,
        None,
        loader,
    )
    .map_err(AuthoringPrefabInspectionError::Loader)?;
    let Some(data) = compiled.inspection else {
        return Err(AuthoringPrefabInspectionError::Compilation {
            failure: compiled
                .failure
                .unwrap_or_else(|| SceneCompilationFailureKind::OwnerAdmission.into()),
            diagnostics: compiled.diagnostics,
        });
    };
    Ok(AuthoringPrefabInstanceSnapshot {
        data,
        basis: basis.clone(),
        diagnostics: compiled.diagnostics,
        working_source,
    })
}

/// A selected compilation refusal or the original actual-demand loader error.
#[derive(Debug)]
pub enum AuthoringPrefabInspectionError<E> {
    Loader(E),
    Compilation {
        failure: SceneCompilationFailure,
        diagnostics: DiagnosticReport,
    },
}

/// Immutable effective records and declaration observations under one exact authoring basis.
pub struct AuthoringPrefabInstanceSnapshot {
    data: CapturedPrefabInspection,
    basis: AuthoringSchemaBasis,
    diagnostics: DiagnosticReport,
    working_source: Option<WorkingSourceObservation>,
}

impl fmt::Debug for AuthoringPrefabInstanceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringPrefabInstanceSnapshot")
            .field("targets", &self.data.targets.len())
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

impl AuthoringPrefabInstanceSnapshot {
    pub const fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.basis
    }

    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Logical storage retained by this observation, charging its payload backing. The exact
    /// Schema publication and the Host's acquired source evidence have their own owners.
    pub fn checked_retained_bytes(&self) -> Option<usize> {
        size_of::<Self>()
            .checked_add(self.data.retained_bytes)?
            .checked_add(self.diagnostics.retained_bytes())
    }

    /// Surviving routes relative to the containing source, in authored target order. The first
    /// hop enters the selected anchor, including when a route shares its root-bound entity.
    pub fn targets(&self) -> impl ExactSizeIterator<Item = &PrefabEntityTarget> {
        self.data.targets.keys()
    }

    pub fn targets_after<'a>(
        &'a self,
        after: Option<&PrefabEntityTarget>,
    ) -> impl Iterator<Item = &'a PrefabEntityTarget> + 'a {
        let lower = after.map_or(Bound::Unbounded, Bound::Excluded);
        self.data
            .targets
            .range((lower, Bound::Unbounded))
            .map(|(target, _)| target)
    }

    /// Every retained root alias resolves to its actual merged component record inventory.
    pub fn target(&self, target: &PrefabEntityTarget) -> Option<AuthoringPrefabTargetView<'_>> {
        let entity = self.data.targets.get(target)?;
        let components = self.data.entities.get(entity)?;
        Some(AuthoringPrefabTargetView {
            snapshot: self,
            components,
            component_overrides: self.data.component_overrides.get(entity),
        })
    }
}

#[derive(Clone, Copy)]
pub struct AuthoringPrefabTargetView<'a> {
    snapshot: &'a AuthoringPrefabInstanceSnapshot,
    components: &'a BTreeMap<ComponentTypeId, InspectedComponent>,
    component_overrides: Option<&'a BTreeMap<ComponentTypeId, InspectedPrefabComponentOverride>>,
}

impl fmt::Debug for AuthoringPrefabTargetView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringPrefabTargetView")
            .field("components", &self.components.len())
            .finish_non_exhaustive()
    }
}

impl<'a> AuthoringPrefabTargetView<'a> {
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    pub fn components(&self) -> impl ExactSizeIterator<Item = AuthoringPrefabComponentView<'a>> {
        let snapshot = self.snapshot;
        self.components
            .values()
            .map(move |component| AuthoringPrefabComponentView {
                snapshot,
                component,
            })
    }

    pub fn components_after(
        &self,
        after: Option<&ComponentTypeId>,
    ) -> impl Iterator<Item = AuthoringPrefabComponentView<'a>> + 'a {
        let snapshot = self.snapshot;
        let lower = after.map_or(Bound::Unbounded, Bound::Excluded);
        self.components
            .range((lower, Bound::Unbounded))
            .map(move |(_, component)| AuthoringPrefabComponentView {
                snapshot,
                component,
            })
    }

    /// Lists the selected layer's declarations, including suppression of absent effective records.
    pub fn component_overrides(
        &self,
    ) -> impl Iterator<Item = (&'a ComponentTypeId, AuthoringPrefabComponentOverride<'a>)> + 'a
    {
        self.component_overrides.into_iter().flat_map(|overrides| {
            overrides.iter().map(|(component, declaration)| {
                (component, AuthoringPrefabComponentOverride { declaration })
            })
        })
    }

    /// Reports the selected layer's declaration for this component, if any.
    pub fn component_override(
        &self,
        component: &ComponentTypeId,
    ) -> Option<AuthoringPrefabComponentOverride<'a>> {
        self.component_overrides
            .and_then(|overrides| overrides.get(component))
            .map(|declaration| AuthoringPrefabComponentOverride { declaration })
    }

    pub fn component(
        &self,
        id: &ComponentTypeId,
    ) -> Result<AuthoringPrefabComponentView<'a>, AuthoringReadError> {
        let component = self
            .components
            .get(id)
            .ok_or(AuthoringReadError::ComponentNotFound)?;
        Ok(AuthoringPrefabComponentView {
            snapshot: self.snapshot,
            component,
        })
    }
}

/// Physical declaration of the complete record selected by the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringPrefabRecordOrigin {
    SourceEntity {
        source: PrefabDeclarationSource,
        entity: SceneEntityId,
    },
    Overlay {
        location: PrefabInstanceLocation,
        slot: PrefabOverlaySlot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringPrefabComponentFact {
    pub(crate) component: ComponentTypeId,
    pub(crate) owner: AuthoringRecordOwnerClaim,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) readiness: AuthoringRecordReadiness,
    pub(crate) record: AuthoringPrefabRecordOrigin,
}

impl AuthoringPrefabComponentFact {
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    pub const fn owner(&self) -> &AuthoringRecordOwnerClaim {
        &self.owner
    }

    pub const fn version(&self) -> ComponentSchemaVersion {
        self.version
    }

    pub const fn readiness(&self) -> AuthoringRecordReadiness {
        self.readiness
    }

    pub const fn record(&self) -> &AuthoringPrefabRecordOrigin {
        &self.record
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPrefabPropertyClaimKind {
    Set,
    Absent,
}

/// The authored component declaration in the selected instance layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPrefabComponentOverrideKind {
    Added,
    Replaced,
    Suppressed,
    Properties,
}

/// One declaration and the origin of the record beneath its selected instance layer.
#[derive(Debug, Clone, Copy)]
pub struct AuthoringPrefabComponentOverride<'a> {
    declaration: &'a InspectedPrefabComponentOverride,
}

impl<'a> AuthoringPrefabComponentOverride<'a> {
    #[must_use]
    pub const fn kind(self) -> AuthoringPrefabComponentOverrideKind {
        self.declaration.kind
    }

    /// Returns the physical source of the record before this declaration was applied.
    #[must_use]
    pub fn underlying_record(self) -> Option<&'a AuthoringPrefabRecordOrigin> {
        self.declaration
            .underlying
            .as_ref()
            .map(|underlying| &underlying.origin)
    }
}

/// One claim in the surviving complete record's ordered compiler program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringPrefabPropertyClaim {
    pub path: ComponentPropertyPath,
    pub location: PrefabInstanceLocation,
    pub slot: PrefabOverlaySlot,
    pub kind: AuthoringPrefabPropertyClaimKind,
}

#[derive(Clone, Copy)]
pub struct AuthoringPrefabComponentView<'a> {
    snapshot: &'a AuthoringPrefabInstanceSnapshot,
    component: &'a InspectedComponent,
}

impl fmt::Debug for AuthoringPrefabComponentView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.fact(), formatter)
    }
}

impl<'a> AuthoringPrefabComponentView<'a> {
    pub const fn fact(&self) -> &'a AuthoringPrefabComponentFact {
        &self.component.fact
    }

    pub fn root_fields(&self) -> Result<&'a [ComponentFieldSchema], AuthoringReadError> {
        Ok(self.schema()?.fields())
    }

    /// Applied claims in evaluation order, from the innermost surviving layer outwards.
    pub fn property_claims(&self) -> &'a [AuthoringPrefabPropertyClaim] {
        &self.component.claims
    }

    pub fn property_value(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<AuthoringPrefabProperty<'a>, AuthoringReadError> {
        let field = property_root(self.schema()?, property)?;
        let value = self.resolve_value(field, property, budget)?;
        let mut controlling_claim = None;
        let mut has_descendant_claim = false;
        for claim in self.property_claims().iter().rev() {
            budget.observe_entry(claim.path.logical_bytes()?)?;
            if claim.path.root != property.root {
                continue;
            }
            if property.steps.starts_with(&claim.path.steps) {
                controlling_claim = Some(claim);
                break;
            }
            has_descendant_claim |= claim.path.steps.starts_with(&property.steps);
        }
        let defaulted = controlling_claim.map_or_else(
            || {
                self.component
                    .defaulted_roots
                    .binary_search(&property.root)
                    .is_ok()
            },
            |claim| claim.kind == AuthoringPrefabPropertyClaimKind::Absent,
        );
        let revert_claim = controlling_claim.filter(|claim| {
            claim.path == *property
                && claim.location.source() == &PrefabDeclarationSource::Containing
        });
        let (shape, value) = match value {
            Some(ResolvedComponentProperty { shape, value }) => {
                let value = AuthoringPrefabValue {
                    value,
                    references: &self.snapshot.data.references,
                };
                let presence = if defaulted {
                    AuthoringPrefabPropertyValue::Defaulted(value)
                } else {
                    AuthoringPrefabPropertyValue::Present(value)
                };
                (shape, presence)
            }
            None => (field.shape(), AuthoringPrefabPropertyValue::Missing),
        };
        Ok(AuthoringPrefabProperty {
            shape,
            value,
            record: self.fact().record(),
            controlling_claim,
            revert_claim,
            has_descendant_claim,
        })
    }

    pub fn property_children(
        &self,
        property: &ComponentPropertyPath,
        position: AuthoringPropertyChildrenPosition<'_>,
        limits: AuthoringPropertyChildrenLimits,
    ) -> Result<AuthoringPropertyChildrenPage<'a>, AuthoringReadError> {
        property_children_in_schema(
            || {
                let field = property_root(self.schema()?, property)?;
                let root = self
                    .component
                    .payload
                    .as_ref()
                    .ok_or(AuthoringReadError::NotAdmitted)
                    .and_then(|record| match record.value().get_path(field.path()) {
                        Ok(value) => Ok((Some(value), None)),
                        Err(
                            error @ (ComponentFieldPathError::MissingField { .. }
                            | ComponentFieldPathError::IndexOutOfBounds { .. }),
                        ) => {
                            if property.steps.is_empty() {
                                Ok((None, None))
                            } else {
                                Err(ComponentPropertyError::ValueUnavailable(error).into())
                            }
                        }
                        Err(_) => Err(AuthoringReadError::InvalidFieldPath),
                    });
                Ok((field, root))
            },
            property,
            position,
            limits,
        )
    }

    fn schema(&self) -> Result<&'a ComponentSchema, AuthoringReadError> {
        let readiness = self.fact().readiness();
        if !readiness.is_available() {
            return Err(AuthoringReadError::UnavailableComponent { readiness });
        }
        self.snapshot
            .basis
            .schema(self.fact().component())
            .ok_or(AuthoringReadError::NotAdmitted)
    }

    fn resolve_value(
        &self,
        field: &'a ComponentFieldSchema,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<Option<ResolvedComponentProperty<'a, 'a>>, AuthoringReadError> {
        let record = self
            .component
            .payload
            .as_ref()
            .ok_or(AuthoringReadError::NotAdmitted)?;
        match record.value().get_path(field.path()) {
            Ok(root) => Ok(Some(resolve_component_property_in_root(
                field, root, property, budget,
            )?)),
            Err(
                error @ (ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. }),
            ) => {
                if property.steps.is_empty() {
                    Ok(None)
                } else {
                    Err(ComponentPropertyError::ValueUnavailable(error).into())
                }
            }
            Err(_) => Err(AuthoringReadError::InvalidFieldPath),
        }
    }
}

/// An effective value at a stable property selector, with the complete record and nearest
/// controlling claim. Claims on strict descendants remain visible in `property_claims`.
#[derive(Debug, Clone, Copy)]
pub struct AuthoringPrefabProperty<'a> {
    pub shape: &'a ComponentValueShape,
    pub value: AuthoringPrefabPropertyValue<'a>,
    pub record: &'a AuthoringPrefabRecordOrigin,
    controlling_claim: Option<&'a AuthoringPrefabPropertyClaim>,
    revert_claim: Option<&'a AuthoringPrefabPropertyClaim>,
    has_descendant_claim: bool,
}

impl<'a> AuthoringPrefabProperty<'a> {
    /// The last applied claim covering this exact selector or an ancestor of it.
    pub const fn controlling_claim(&self) -> Option<&'a AuthoringPrefabPropertyClaim> {
        self.controlling_claim
    }

    /// The exact applied claim at this selector in the containing document, for prepared Revert.
    pub const fn revert_claim(&self) -> Option<&'a AuthoringPrefabPropertyClaim> {
        self.revert_claim
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AuthoringPrefabPropertyValue<'a> {
    Present(AuthoringPrefabValue<'a>),
    Defaulted(AuthoringPrefabValue<'a>),
    Missing,
}

/// An opaque value with compiler-resolved reference endpoints in the containing source.
#[derive(Clone, Copy)]
pub struct AuthoringPrefabValue<'a> {
    value: ComponentValueRef<'a>,
    references: &'a BTreeMap<SceneEntityId, EntityReference>,
}

impl fmt::Debug for AuthoringPrefabValue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringPrefabValue")
            .field("kind", &self.value.kind())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPrefabValueError {
    BudgetExceeded,
    InconsistentSnapshot,
}

impl fmt::Display for AuthoringPrefabValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BudgetExceeded => "selected Prefab value exceeded the copy allowance",
            Self::InconsistentSnapshot => {
                "selected Prefab value has an inconsistent reference projection"
            }
        })
    }
}

impl std::error::Error for AuthoringPrefabValueError {}

pub(crate) fn copy_value_with_references<E>(
    value: ComponentValueRef<'_>,
    rewrite: &mut impl FnMut(&EntityReference) -> Result<EntityReference, E>,
) -> Result<ComponentValue, E> {
    if let Some(reference) = value.as_entity_reference() {
        return rewrite(reference).map(ComponentValue::EntityReference);
    }
    if let Some(fields) = value.as_map() {
        return fields
            .iter()
            .map(|(key, child)| Ok((key.to_owned(), copy_value_with_references(child, rewrite)?)))
            .collect::<Result<_, _>>()
            .map(ComponentValue::Map);
    }
    if let Some(items) = value.as_list() {
        return items
            .iter()
            .map(|child| copy_value_with_references(child, rewrite))
            .collect::<Result<_, _>>()
            .map(ComponentValue::List);
    }
    Ok(value.to_owned())
}

impl AuthoringPrefabValue<'_> {
    /// Copies the selected value under the explicit limit. Any local reference in the copy is an
    /// `Authored` route rooted in the original containing document, including nested leaves.
    pub fn copy_value(
        self,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<ComponentValue, AuthoringPrefabValueError> {
        let raw = validate_value_materialization_budget(self.value, limits)
            .map_err(|_| AuthoringPrefabValueError::BudgetExceeded)?;
        let (mut nodes, mut bytes) = (raw.nodes(), raw.logical_bytes());
        // The compiler already admitted and resolved these leaves. Measure the actual
        // replacement cost without allocating a storage path for each reference.
        self.adjust_reference_cost(self.value, &mut nodes, &mut bytes)?;
        if nodes > limits.nodes() || bytes > limits.bytes() {
            return Err(AuthoringPrefabValueError::BudgetExceeded);
        }
        copy_value_with_references(self.value, &mut |reference| {
            self.authored_reference(reference).cloned()
        })
    }

    fn adjust_reference_cost(
        &self,
        value: ComponentValueRef<'_>,
        nodes: &mut usize,
        bytes: &mut usize,
    ) -> Result<(), AuthoringPrefabValueError> {
        use AuthoringPrefabValueError::BudgetExceeded;

        if let Some(reference) = value.as_entity_reference() {
            let before = reference.cost();
            let after = self.authored_reference(reference)?.cost();
            *nodes = nodes
                .checked_sub(before.nodes())
                .and_then(|nodes| nodes.checked_add(after.nodes()))
                .ok_or(BudgetExceeded)?;
            *bytes = bytes
                .checked_sub(before.logical_bytes())
                .and_then(|bytes| bytes.checked_add(after.logical_bytes()))
                .ok_or(BudgetExceeded)?;
        } else if let Some(fields) = value.as_map() {
            for child in fields.values() {
                self.adjust_reference_cost(child, nodes, bytes)?;
            }
        } else if let Some(items) = value.as_list() {
            for child in items {
                self.adjust_reference_cost(child, nodes, bytes)?;
            }
        }
        Ok(())
    }

    fn authored_reference<'a>(
        &'a self,
        reference: &'a EntityReference,
    ) -> Result<&'a EntityReference, AuthoringPrefabValueError> {
        match reference {
            EntityReference::SceneLocal { entity } => self
                .references
                .get(entity)
                .ok_or(AuthoringPrefabValueError::InconsistentSnapshot),
            EntityReference::Persistent { .. } => Ok(reference),
            EntityReference::Authored { .. } => {
                Err(AuthoringPrefabValueError::InconsistentSnapshot)
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CapturedPrefabInspection {
    pub(crate) targets: BTreeMap<PrefabEntityTarget, SceneEntityId>,
    pub(crate) entities: BTreeMap<SceneEntityId, BTreeMap<ComponentTypeId, InspectedComponent>>,
    pub(crate) component_overrides:
        BTreeMap<SceneEntityId, BTreeMap<ComponentTypeId, InspectedPrefabComponentOverride>>,
    pub(crate) references: BTreeMap<SceneEntityId, EntityReference>,
    pub(crate) retained_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct InspectedPrefabComponentOverride {
    pub(crate) index: usize,
    pub(crate) kind: AuthoringPrefabComponentOverrideKind,
    pub(crate) underlying: Option<InspectedUnderlyingComponent>,
}

#[derive(Debug)]
pub(crate) struct InspectedUnderlyingComponent {
    pub(crate) origin: AuthoringPrefabRecordOrigin,
    pub(crate) ownership: InspectedComponentOwnership,
}

impl AuthoringPrefabComponentOverrideKind {
    pub(crate) const fn of(intent: &PrefabComponentIntent) -> Self {
        match intent {
            PrefabComponentIntent::Set {
                mode: crate::PrefabComponentSetMode::Add,
                ..
            } => Self::Added,
            PrefabComponentIntent::Set {
                mode: crate::PrefabComponentSetMode::Replace,
                ..
            } => Self::Replaced,
            PrefabComponentIntent::Suppress { .. } => Self::Suppressed,
            PrefabComponentIntent::Properties { .. } => Self::Properties,
        }
    }
}

#[derive(Debug)]
pub(crate) struct InspectedComponent {
    pub(crate) fact: AuthoringPrefabComponentFact,
    pub(crate) claims: Vec<AuthoringPrefabPropertyClaim>,
    pub(crate) defaulted_roots: Vec<ComponentFieldId>,
    pub(crate) payload: Option<ExpandedSceneComponent>,
    pub(crate) selected_overlay_intent: Option<usize>,
    pub(crate) ownership: InspectedComponentOwnership,
    pub(crate) property_apply: Option<Box<InspectedPrefabPropertyApply>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InspectedComponentOwnership {
    LocalRecord,
    SelectedOverride,
    Inherited,
}

#[derive(Debug)]
pub(crate) struct InspectedPrefabPropertyApply {
    pub(crate) record_target: PrefabEntityTarget,
    pub(crate) claim_target: Option<PrefabEntityTarget>,
    pub(crate) reference_ownership: BTreeMap<PrefabOverlaySlot, bool>,
}

impl CapturedPrefabInspection {
    pub(crate) fn retain_admitted_payloads(
        &mut self,
        document: SceneDraft,
    ) -> Result<(), SceneCompilationFailure> {
        for entity in document.entities {
            let Some(observed) = self.entities.get_mut(&entity.id) else {
                continue;
            };
            for (id, record) in entity.components {
                let component = observed
                    .get_mut(&id)
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
                component.payload = Some(record);
            }
            if observed.values().any(|component| {
                component.fact.readiness.is_available() != component.payload.is_some()
            }) {
                return Err(SceneCompilationFailureKind::OwnerAdmission.into());
            }
        }
        Ok(())
    }
}
