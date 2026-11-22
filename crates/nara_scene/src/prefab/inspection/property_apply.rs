use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget};
use nara_reflect::{
    __private::component_values_exactly_match, ComponentCapability, ComponentFieldPathSegment,
    ComponentPropertyStep,
};

use super::*;
use crate::{
    AuthoringPropertyEdit, AuthoringPropertyValue, PrefabAuthoringSession, PrefabDeclarationLimits,
    PrefabEntityOrigin, PrefabPropertyClaimRef, PrefabPropertyIntent, PreparedPrefabAuthoringEdit,
    SceneAuthoringRevision, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    SourceDocumentKey,
    authoring_decode::{retained_entity_reference_bytes, retained_prefab_target_bytes},
    provenance::provenance_string_bytes,
};

#[derive(Debug)]
pub struct ObservedPrefabPropertyApply {
    target: PrefabEntityTarget,
    record_target: Option<PrefabEntityTarget>,
    fact: AuthoringPrefabComponentFact,
    property: ComponentPropertyPath,
    basis: AuthoringSchemaBasis,
    value: ApplyValue,
    value_limits: ComponentValueMaterializationLimits,
}

#[derive(Debug)]
enum ApplyValue {
    Set {
        raw: ComponentValue,
        endpoint: Option<EntityReference>,
    },
    Absent {
        default: Option<ComponentValue>,
    },
}

impl ObservedPrefabPropertyApply {
    pub fn anchor(&self) -> &SceneEntityId {
        self.target.through[0].anchor.entity()
    }

    fn record_target(&self) -> &PrefabEntityTarget {
        self.record_target.as_ref().unwrap_or(&self.target)
    }

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let AuthoringPrefabRecordOrigin::SourceEntity {
            source: PrefabDeclarationSource::Prefab(source),
            entity,
        } = self.fact.record()
        else {
            return None;
        };
        let owner_bytes = match self.fact.owner() {
            AuthoringRecordOwnerClaim::Declared(owner) => provenance_string_bytes(owner.as_str())?,
            AuthoringRecordOwnerClaim::Unclaimed => 0,
        };
        let (value, endpoint_bytes) = match &self.value {
            ApplyValue::Set { raw, endpoint } => (
                Some(raw),
                endpoint
                    .as_ref()
                    .map_or(Some(0), retained_entity_reference_bytes)?,
            ),
            ApplyValue::Absent { default } => (default.as_ref(), 0),
        };
        let value_bytes = value.map_or(Some(0), |value| {
            let cost = value.cost();
            cost.nodes()
                .checked_mul(96)?
                .checked_add(cost.logical_bytes())
        })?;
        size_of::<Self>()
            .checked_add(retained_prefab_target_bytes(&self.target)?)?
            .checked_add(
                self.record_target
                    .as_ref()
                    .map_or(Some(0), retained_prefab_target_bytes)?,
            )?
            .checked_add(source.checked_retained_bytes()?)?
            .checked_add(provenance_string_bytes(entity.as_str())?)?
            .checked_add(provenance_string_bytes(self.fact.component().as_str())?)?
            .checked_add(owner_bytes)?
            .checked_add(self.property.logical_bytes().ok()?)?
            .checked_add(2 * size_of::<usize>())?
            .checked_add(
                self.property
                    .steps
                    .len()
                    .checked_mul(size_of::<ComponentPropertyStep>() + 4 * size_of::<usize>())?,
            )?
            .checked_add(value_bytes)?
            .checked_add(endpoint_bytes)
    }
}

#[derive(Debug)]
pub enum AuthoringPrefabPropertyApplyError {
    Read(AuthoringReadError),
    InvalidDestination,
    InvalidProperty,
    BudgetExceeded,
}

impl From<AuthoringReadError> for AuthoringPrefabPropertyApplyError {
    fn from(error: AuthoringReadError) -> Self {
        Self::Read(error)
    }
}

impl From<AuthoringPrefabValueError> for AuthoringPrefabPropertyApplyError {
    fn from(error: AuthoringPrefabValueError) -> Self {
        match error {
            AuthoringPrefabValueError::BudgetExceeded => Self::BudgetExceeded,
            AuthoringPrefabValueError::InconsistentSnapshot => Self::InvalidProperty,
        }
    }
}

impl AuthoringPrefabInstanceSnapshot {
    /// Observes promotion of the exact stored claim used for the containing source's Revert.
    #[doc(hidden)]
    pub fn plan_property_apply(
        &self,
        target: &PrefabEntityTarget,
        component: &ComponentTypeId,
        property: &ComponentPropertyPath,
        claim: PrefabPropertyClaimRef<'_>,
        destination: &SourceDocumentKey,
        limits: PrefabDeclarationLimits,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<ObservedPrefabPropertyApply, AuthoringPrefabPropertyApplyError> {
        use AuthoringPrefabPropertyApplyError as Error;

        let mut value_limits = value_limits(target, component, property, limits)?;
        let hop = target.through.first().ok_or(Error::InvalidProperty)?;
        let PrefabEntityOrigin::SourceEntity(anchor) = &hop.anchor else {
            return Err(Error::InvalidProperty);
        };
        let selected = self
            .target(target)
            .ok_or(Error::InvalidProperty)?
            .component(component)?;
        let fact = selected.fact();
        let schema = selected.schema()?;
        let field = property_root(schema, property)?;
        let apply = selected
            .component
            .property_apply
            .as_deref()
            .ok_or(Error::InvalidDestination)?;
        let record_target = &apply.record_target;
        if record_target.through.last().map(|hop| &hop.expected_base) != Some(destination) {
            return Err(Error::InvalidDestination);
        }
        let separate_record_target = (record_target != target).then_some(record_target);
        if let Some(record_target) = separate_record_target {
            value_limits = reserve_target(record_target, value_limits)?;
        }
        if claim.component() != component
            || claim.version() != fact.version()
            || claim.claim().path != *property
            || apply.claim_target.as_ref() != Some(claim.target)
            || !schema.has_capability(ComponentCapability::Edit)
            || !field.has_capability(ComponentCapability::Edit)
        {
            return Err(Error::InvalidProperty);
        }
        let observed = selected.property_value(property, read)?;
        let controlling = observed.revert_claim().ok_or(Error::InvalidProperty)?;
        if controlling.slot != claim.slot()
            || &controlling.location.path().anchor != anchor
            || !controlling.location.path().steps.is_empty()
        {
            return Err(Error::InvalidProperty);
        }
        let value = match (&claim.claim().intent, controlling.kind, observed.value) {
            (
                PrefabPropertyIntent::Set(raw),
                AuthoringPrefabPropertyClaimKind::Set,
                AuthoringPrefabPropertyValue::Present(effective),
            ) => {
                let mut shape = observed.shape;
                while let ComponentValueShape::Optional { value } = shape {
                    shape = value;
                }
                if !matches!(
                    shape,
                    ComponentValueShape::Scalar { .. } | ComponentValueShape::EntityRef
                ) {
                    return Err(Error::InvalidProperty);
                }
                let cost = validate_value_materialization_budget(raw, value_limits)
                    .map_err(|_| Error::BudgetExceeded)?;
                let endpoint = match raw {
                    ComponentValue::EntityReference(EntityReference::Authored { reference }) => {
                        if reference.through.len() > limits.max_depth() {
                            return Err(Error::BudgetExceeded);
                        }
                        if !matches!(reference.target, AuthoredReferenceTarget::Entity(_))
                            || !reference.through.starts_with(&record_target.through)
                            || !apply.portable_reference_claims.contains(&claim.slot())
                        {
                            return Err(Error::InvalidProperty);
                        }
                        let endpoint =
                            effective_reference(&effective)?.ok_or(Error::InvalidProperty)?;
                        let remaining = value_limits.remaining(cost.nodes(), cost.logical_bytes());
                        endpoint
                            .cost_up_to(remaining.nodes(), remaining.bytes())
                            .ok_or(Error::BudgetExceeded)?;
                        Some(endpoint.clone())
                    }
                    _ if matches_effective_value(effective, raw)? => None,
                    _ => return Err(Error::InvalidProperty),
                };
                ApplyValue::Set {
                    raw: raw.clone(),
                    endpoint,
                }
            }
            (PrefabPropertyIntent::Absent, AuthoringPrefabPropertyClaimKind::Absent, value)
                if property.steps.is_empty()
                    && (!field.is_required() || field.default_value().is_some())
                    && matches!(
                        field.path().segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    ) =>
            {
                let default = match value {
                    AuthoringPrefabPropertyValue::Missing => None,
                    AuthoringPrefabPropertyValue::Defaulted(value) => {
                        Some(value.copy_value(value_limits)?)
                    }
                    AuthoringPrefabPropertyValue::Present(_) => return Err(Error::InvalidProperty),
                };
                ApplyValue::Absent { default }
            }
            _ => return Err(Error::InvalidProperty),
        };

        Ok(ObservedPrefabPropertyApply {
            target: target.clone(),
            record_target: separate_record_target.cloned(),
            fact: fact.clone(),
            property: property.clone(),
            basis: self.basis.clone(),
            value,
            value_limits,
        })
    }

    /// Confirms that the same source record now supplies the unchanged effective property.
    #[doc(hidden)]
    pub fn verifies_property_apply(
        &self,
        edit: &ObservedPrefabPropertyApply,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<bool, AuthoringPrefabPropertyApplyError> {
        if self.basis != edit.basis {
            return Ok(false);
        }
        let Some(selected) = self
            .target(&edit.target)
            .and_then(|target| target.component(edit.fact.component()).ok())
        else {
            return Ok(false);
        };
        if selected.fact() != &edit.fact
            || selected
                .component
                .property_apply
                .as_deref()
                .is_none_or(|apply| &apply.record_target != edit.record_target())
        {
            return Ok(false);
        }
        let observed = selected.property_value(&edit.property, read)?;
        if observed.controlling_claim().is_some() {
            return Ok(false);
        }
        match (&edit.value, observed.value) {
            (ApplyValue::Set { raw, endpoint }, AuthoringPrefabPropertyValue::Present(value)) => {
                match endpoint {
                    Some(expected) => Ok(effective_reference(&value)? == Some(expected)),
                    None => matches_effective_value(value, raw),
                }
            }
            (ApplyValue::Absent { default: None }, AuthoringPrefabPropertyValue::Missing) => {
                Ok(true)
            }
            (
                ApplyValue::Absent {
                    default: Some(expected),
                },
                AuthoringPrefabPropertyValue::Defaulted(value),
            ) => {
                let actual = value.copy_value(edit.value_limits)?;
                Ok(component_values_exactly_match(&actual, expected))
            }
            _ => Ok(false),
        }
    }
}

impl PrefabAuthoringSession {
    /// Prepares the destination's ordinary edit on the supplied revision and observed Schema.
    /// The caller binds this session to the inspected canonical source and prepares the Revert.
    #[doc(hidden)]
    pub fn prepare_prefab_property_apply(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabPropertyApply,
    ) -> Result<Option<PreparedPrefabAuthoringEdit>, ScenePatchReport> {
        if self.revision() != base_revision
            || self.authoring_schema_basis().as_ref() != Some(&edit.basis)
        {
            return Err(rejected(
                "Prefab property promotion requires the exact destination revision and Schema admission",
            ));
        }
        let record_target = edit.record_target();
        let entity = record_target.entity.entity();
        let component = edit.fact.component();
        let destination = self
            .authoring_entity_view(entity)
            .and_then(|entity| entity.component(component))
            .map_err(|_| {
                rejected("Prefab property promotion requires an available direct component")
            })?;
        let fact = destination.fact();
        if !fact.readiness().is_available()
            || fact.owner() != edit.fact.owner()
            || fact.version() != edit.fact.version()
        {
            return Err(rejected(
                "Prefab property promotion requires the observed owner and component version",
            ));
        }
        match &edit.value {
            ApplyValue::Set { raw, .. } => {
                let value = match raw {
                    ComponentValue::EntityReference(EntityReference::Authored { reference }) => {
                        let through = reference
                            .through
                            .as_slice()
                            .strip_prefix(record_target.through.as_slice())
                            .ok_or_else(|| {
                                rejected("Prefab property promotion requires a destination-local reference route")
                            })?
                            .to_vec();
                        ComponentValue::EntityReference(EntityReference::Authored {
                            reference: AuthoredEntityReference {
                                through,
                                target: reference.target.clone(),
                            },
                        })
                    }
                    _ => raw.clone(),
                };
                let prepared = self.prepare_authoring_property_edit(
                    base_revision,
                    &[AuthoringPropertyEdit {
                        entity: entity.clone(),
                        component: component.clone(),
                        component_version: fact.version(),
                        property: edit.property.clone(),
                        value,
                    }],
                )?;
                Ok((!prepared.patch().is_empty()).then_some(prepared))
            }
            ApplyValue::Absent { .. } => {
                let current = destination
                    .property_root_value(&edit.property.root)
                    .map_err(|_| {
                        rejected(
                            "Prefab property promotion requires an admitted field storage path",
                        )
                    })?;
                if !matches!(current, AuthoringPropertyValue::Present(_)) {
                    return Ok(None);
                }
                let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version: fact.version(),
                    field: edit.property.root.clone(),
                }]);
                self.prepare_authoring_edit(base_revision, &patch).map(Some)
            }
        }
    }
}

fn value_limits(
    target: &PrefabEntityTarget,
    component: &ComponentTypeId,
    property: &ComponentPropertyPath,
    limits: PrefabDeclarationLimits,
) -> Result<ComponentValueMaterializationLimits, AuthoringPrefabPropertyApplyError> {
    use AuthoringPrefabPropertyApplyError::BudgetExceeded;

    if property.steps.len() > limits.max_depth() {
        return Err(BudgetExceeded);
    }
    let nodes = 1 + property.steps.len();
    let bytes = property
        .logical_bytes()
        .map_err(AuthoringReadError::from)?
        .saturating_add(component.as_str().len());
    if nodes > limits.max_nodes() || bytes > limits.max_logical_bytes() {
        return Err(BudgetExceeded);
    }
    reserve_target(
        target,
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(limits.max_nodes()).ok_or(BudgetExceeded)?,
            ByteLimit::new(limits.max_logical_bytes()).ok_or(BudgetExceeded)?,
            DepthLimit::new(limits.max_depth()).ok_or(BudgetExceeded)?,
        )
        .remaining(nodes, bytes),
    )
}

fn reserve_target(
    target: &PrefabEntityTarget,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValueMaterializationLimits, AuthoringPrefabPropertyApplyError> {
    use AuthoringPrefabPropertyApplyError::BudgetExceeded;

    if target.through.len() > limits.depth().get() {
        return Err(BudgetExceeded);
    }
    let nodes = 1 + 2 * target.through.len();
    if nodes > limits.nodes() {
        return Err(BudgetExceeded);
    }
    let bytes = target
        .logical_bytes_up_to(limits.bytes())
        .ok_or(BudgetExceeded)?;
    Ok(limits.remaining(nodes, bytes))
}

fn matches_effective_value(
    value: AuthoringPrefabValue<'_>,
    expected: &ComponentValue,
) -> Result<bool, AuthoringPrefabPropertyApplyError> {
    if let ComponentValue::EntityReference(expected) = expected {
        return Ok(effective_reference(&value)? == Some(expected));
    }
    Ok(value
        .value
        .as_value()
        .is_some_and(|actual| component_values_exactly_match(actual, expected)))
}

fn effective_reference<'a>(
    value: &'a AuthoringPrefabValue<'_>,
) -> Result<Option<&'a EntityReference>, AuthoringPrefabValueError> {
    value
        .value
        .as_entity_reference()
        .map(|reference| value.authored_reference(reference))
        .transpose()
}

fn rejected(summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(crate::diagnostics::error(
        "scene.authoring-prefab-property-apply-rejected",
        summary,
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

#[cfg(test)]
mod tests;
