use nara_reflect::{
    __private::component_values_exactly_match, ComponentCapability, ComponentFieldPathSegment,
    ComponentPropertyStep,
};

use super::*;
use crate::{
    PrefabDeclarationError, PrefabDeclarationLimits, PrefabEntityOrigin, PrefabPropertyClaimRef,
    PrefabPropertyIntent, SourceDocumentKey,
    authoring::{admit_property_write, prefab_anchor_instance},
    authoring_decode::retained_prefab_target_bytes,
    provenance::provenance_string_bytes,
};

#[derive(Debug)]
pub struct ObservedPrefabPropertyWrite {
    pub(crate) anchor: SceneEntityId,
    pub(crate) base: SourceDocumentKey,
    pub(crate) target: PrefabEntityTarget,
    pub(crate) selected: PrefabEntityTarget,
    pub(crate) component: ComponentTypeId,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) existing_intent: Option<usize>,
    pub(crate) existing_claim: Option<usize>,
    pub(crate) property: ComponentPropertyPath,
    pub(crate) intent: PrefabPropertyIntent,
    pub(crate) basis: AuthoringSchemaBasis,
    pub(crate) source: WorkingSourceObservation,
    pub(crate) value_bytes: usize,
}

impl ObservedPrefabPropertyWrite {
    pub const fn anchor(&self) -> &SceneEntityId {
        &self.anchor
    }

    pub const fn kind(&self) -> AuthoringPrefabPropertyClaimKind {
        match &self.intent {
            PrefabPropertyIntent::Set(_) => AuthoringPrefabPropertyClaimKind::Set,
            PrefabPropertyIntent::Absent => AuthoringPrefabPropertyClaimKind::Absent,
        }
    }

    pub(crate) fn matches_working_source(
        &self,
        input: SourceDocumentInput<'_>,
        revision: SceneAuthoringRevision,
    ) -> bool {
        self.source.revision_for(input) == Some(revision)
    }

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        size_of::<Self>()
            .checked_add(provenance_string_bytes(self.anchor.as_str())?)?
            .checked_add(self.base.checked_retained_bytes()?)?
            .checked_add(retained_prefab_target_bytes(&self.target)?)?
            .checked_add(retained_prefab_target_bytes(&self.selected)?)?
            .checked_add(provenance_string_bytes(self.component.as_str())?)?
            .checked_add(self.property.logical_bytes().ok()?)?
            .checked_add(
                self.property
                    .steps
                    .len()
                    .checked_mul(size_of::<ComponentPropertyStep>())?,
            )?
            .checked_add(self.value_bytes)
    }
}

#[derive(Debug)]
pub enum AuthoringPrefabPropertyWriteError {
    Read(AuthoringReadError),
    Declaration(Box<crate::ScenePatchReport>),
    InvalidProperty,
    BudgetExceeded,
    StaleObservedIntent,
}

impl From<AuthoringReadError> for AuthoringPrefabPropertyWriteError {
    fn from(error: AuthoringReadError) -> Self {
        Self::Read(error)
    }
}

impl AuthoringPrefabInstanceSnapshot {
    /// Resolves a property write to its writable instance layer and original declaration group.
    #[doc(hidden)]
    pub fn plan_property_write(
        &self,
        input: SourceDocumentInput<'_>,
        entity: &PrefabEntityTarget,
        component: &ComponentTypeId,
        property: &ComponentPropertyPath,
        intent: PrefabPropertyIntent,
        limits: PrefabDeclarationLimits,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<ObservedPrefabPropertyWrite, AuthoringPrefabPropertyWriteError> {
        use AuthoringPrefabPropertyWriteError as Error;

        let source = self
            .working_source
            .as_ref()
            .filter(|source| source.revision_for(input).is_some())
            .ok_or(Error::StaleObservedIntent)?;
        let max_bytes = limits.max_logical_bytes();
        let target_bytes = entity
            .logical_bytes_up_to(max_bytes)
            .ok_or(Error::BudgetExceeded)?;
        let property_bytes = property.logical_bytes().map_err(AuthoringReadError::from)?;
        max_bytes
            .checked_sub(target_bytes)
            .and_then(|room| room.checked_sub(property_bytes))
            .and_then(|room| room.checked_sub(component.as_str().len()))
            .ok_or(Error::BudgetExceeded)?;
        if entity.through.len() > limits.max_depth() || property.steps.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        let (hop, through) = entity.through.split_first().ok_or(Error::InvalidProperty)?;
        let PrefabEntityOrigin::SourceEntity(anchor) = &hop.anchor else {
            return Err(Error::InvalidProperty);
        };
        let materialized = self
            .data
            .targets
            .get(entity)
            .ok_or(Error::InvalidProperty)?;
        let selected = self
            .target(entity)
            .ok_or(Error::InvalidProperty)?
            .component(component)?;
        let schema = selected.schema()?;
        let field = property_root(schema, property)?;
        if selected.component.ownership != InspectedComponentOwnership::Inherited
            || !schema.has_capability(ComponentCapability::Edit)
            || !field.has_capability(ComponentCapability::Edit)
        {
            return Err(Error::InvalidProperty);
        }
        let replacement = match &intent {
            PrefabPropertyIntent::Set(_) => {
                let resolved = selected.resolve_value(field, property, read)?;
                let shape = resolved.map_or(field.shape(), |selected| selected.shape);
                let mut leaf = shape;
                while let ComponentValueShape::Optional { value } = leaf {
                    leaf = value;
                }
                if matches!(leaf, ComponentValueShape::Opaque { .. })
                    || (resolved.is_none()
                        && field.is_required()
                        && field.default_value().is_none())
                {
                    return Err(Error::InvalidProperty);
                }
                Some(shape)
            }
            PrefabPropertyIntent::Absent => {
                if !property.steps.is_empty()
                    || (field.is_required() && field.default_value().is_none())
                    || !matches!(
                        field.path().segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    )
                {
                    return Err(Error::InvalidProperty);
                }
                None
            }
        };
        let write_target = if through.is_empty()
            && matches!(entity.entity, PrefabEntityOrigin::InstanceAddition(_))
            && selected.component.selected_overlay_intent.is_none()
        {
            let mut route = None;
            for (candidate, id) in &self.data.targets {
                read.observe_entry(0).map_err(AuthoringReadError::from)?;
                if id == materialized && candidate.through.len() > 1 {
                    route = Some(candidate);
                    break;
                }
            }
            route.ok_or(Error::InvalidProperty)?
        } else {
            entity
        };
        if write_target.through.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        write_target
            .logical_bytes_up_to(max_bytes)
            .ok_or(Error::BudgetExceeded)?;
        let mut observed = ObservedPrefabPropertyWrite {
            anchor: anchor.clone(),
            base: hop.expected_base.clone(),
            target: PrefabEntityTarget {
                through: write_target.through[1..].to_vec(),
                entity: write_target.entity.clone(),
            },
            selected: entity.clone(),
            component: component.clone(),
            version: selected.fact().version(),
            existing_intent: selected.component.selected_overlay_intent,
            existing_claim: None,
            property: property.clone(),
            intent,
            basis: self.basis.clone(),
            source: source.clone(),
            value_bytes: 0,
        };
        let (group_target, exact_claim, cost) =
            admit_property_write(source_entities(input), &observed, limits).map_err(|error| {
                match error {
                    PrefabDeclarationError::BudgetExceeded => Error::BudgetExceeded,
                    PrefabDeclarationError::AlreadyWritten
                    | PrefabDeclarationError::ConflictingClaim => {
                        Error::Declaration(Box::new(error.report()))
                    }
                    _ => Error::StaleObservedIntent,
                }
            })?;
        if let (Some(shape), PrefabPropertyIntent::Set(value)) = (replacement, &observed.intent) {
            shape
                .validate_value(value)
                .map_err(|_| Error::InvalidProperty)?;
        }
        if let Some(target) = group_target {
            observed.target = target.clone();
        }
        observed.existing_claim = exact_claim;
        observed.intent = observed.intent.clone();
        observed.value_bytes = cost
            .nodes()
            .checked_mul(96)
            .and_then(|nodes| nodes.checked_add(cost.logical_bytes()))
            .ok_or(Error::BudgetExceeded)?;
        Ok(observed)
    }

    /// Confirms the write at the original address using the same canonical prepared source that
    /// produced this snapshot. References preserve their authored spelling in that source and
    /// use the compiler's resolved endpoint in the effective record.
    #[doc(hidden)]
    pub fn verifies_property_write(
        &self,
        source: SourceDocumentInput<'_>,
        edit: &ObservedPrefabPropertyWrite,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<bool, AuthoringPrefabPropertyWriteError> {
        use AuthoringPrefabPropertyWriteError as Error;

        if self.basis != edit.basis {
            return Ok(false);
        }
        let property = self
            .target(&edit.selected)
            .ok_or(Error::InvalidProperty)?
            .component(&edit.component)?
            .property_value(&edit.property, read)?;
        let Some(claim) = property
            .revert_claim()
            .filter(|claim| in_selected_layer(claim, &edit.anchor) && claim.kind == edit.kind())
        else {
            return Ok(false);
        };
        let Some(stored) = stored_property_claim(source, &edit.anchor, claim.slot) else {
            return Ok(false);
        };
        let PrefabOverlaySlot::Property {
            component_index,
            claim_index,
        } = claim.slot
        else {
            return Ok(false);
        };
        if edit
            .existing_intent
            .is_some_and(|index| index != component_index)
            || edit
                .existing_claim
                .is_some_and(|index| index != claim_index)
            || stored.target != &edit.target
            || stored.component() != &edit.component
            || stored.version() != edit.version
            || stored.claim().path != edit.property
        {
            return Ok(false);
        }
        Ok(
            match (&edit.intent, &stored.claim().intent, property.value) {
                (
                    PrefabPropertyIntent::Set(expected),
                    PrefabPropertyIntent::Set(actual),
                    AuthoringPrefabPropertyValue::Present(_),
                ) => component_values_exactly_match(actual, expected),
                (
                    PrefabPropertyIntent::Absent,
                    PrefabPropertyIntent::Absent,
                    AuthoringPrefabPropertyValue::Missing
                    | AuthoringPrefabPropertyValue::Defaulted(_),
                ) => true,
                _ => false,
            },
        )
    }
}

fn stored_property_claim<'a>(
    source: SourceDocumentInput<'a>,
    anchor: &SceneEntityId,
    slot: PrefabOverlaySlot,
) -> Option<PrefabPropertyClaimRef<'a>> {
    let overlay = prefab_anchor_instance(source_entities(source), anchor)
        .ok()?
        .as_ref()?
        .overrides
        .as_ref()?;
    overlay.property_claim_at(slot)
}

fn source_entities(source: SourceDocumentInput<'_>) -> &[crate::SceneEntityRecord] {
    match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    }
}

fn in_selected_layer(claim: &AuthoringPrefabPropertyClaim, anchor: &SceneEntityId) -> bool {
    claim.location.source() == &PrefabDeclarationSource::Containing
        && &claim.location.path().anchor == anchor
        && claim.location.path().steps.is_empty()
}
