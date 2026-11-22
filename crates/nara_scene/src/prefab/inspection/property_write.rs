use nara_reflect::{
    __private::component_values_exactly_match, ComponentCapability, ComponentFieldPathSegment,
    ComponentPropertyStep,
};

use super::*;
use crate::{
    PrefabDeclarationLimits, PrefabEntityOrigin, PrefabInstanceHop, PrefabPropertyIntent,
    SourceDocumentKey, authoring_decode::retained_prefab_target_bytes,
    provenance::provenance_string_bytes,
};

#[derive(Debug)]
pub struct ObservedPrefabPropertyWrite {
    pub(crate) anchor: SceneEntityId,
    pub(crate) base: SourceDocumentKey,
    pub(crate) target: PrefabEntityTarget,
    pub(crate) observed_target: Option<PrefabEntityTarget>,
    pub(crate) component: ComponentTypeId,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) existing_intent: Option<usize>,
    pub(crate) property: ComponentPropertyPath,
    pub(crate) intent: PrefabPropertyIntent,
    pub(crate) basis: AuthoringSchemaBasis,
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

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let value_bytes = match &self.intent {
            PrefabPropertyIntent::Set(ComponentValue::String(value)) => value.capacity(),
            _ => 0,
        };
        let target = retained_prefab_target_bytes(&self.target)?;
        let observed_and_scratch = self.observed_target.as_ref().map_or_else(
            || {
                Some(
                    target
                        .checked_add(size_of::<PrefabInstanceHop>())?
                        .checked_add(provenance_string_bytes(self.anchor.as_str())?)?
                        .checked_add(self.base.checked_retained_bytes()?)?,
                )
            },
            retained_prefab_target_bytes,
        )?;
        size_of::<Self>()
            .checked_add(provenance_string_bytes(self.anchor.as_str())?)?
            .checked_add(self.base.checked_retained_bytes()?)?
            .checked_add(target)?
            .checked_add(observed_and_scratch)?
            .checked_add(provenance_string_bytes(self.component.as_str())?)?
            .checked_add(self.property.logical_bytes().ok()?)?
            .checked_add(
                self.property
                    .steps
                    .len()
                    .checked_mul(size_of::<ComponentPropertyStep>())?,
            )?
            .checked_add(value_bytes)
    }
}

#[derive(Debug)]
pub enum AuthoringPrefabPropertyWriteError {
    Read(AuthoringReadError),
    InvalidProperty,
    BudgetExceeded,
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
        entity: &PrefabEntityTarget,
        component: &ComponentTypeId,
        property: &ComponentPropertyPath,
        intent: PrefabPropertyIntent,
        limits: PrefabDeclarationLimits,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<ObservedPrefabPropertyWrite, AuthoringPrefabPropertyWriteError> {
        use AuthoringPrefabPropertyWriteError as Error;

        let max_bytes = limits.max_logical_bytes();
        let target_bytes = entity
            .logical_bytes_up_to(max_bytes)
            .ok_or(Error::BudgetExceeded)?;
        let property_bytes = property.logical_bytes().map_err(AuthoringReadError::from)?;
        let remaining_bytes = max_bytes
            .checked_sub(target_bytes)
            .and_then(|room| room.checked_sub(property_bytes))
            .and_then(|room| room.checked_sub(component.as_str().len()))
            .ok_or(Error::BudgetExceeded)?;
        if entity.through.len() > limits.max_depth() || property.steps.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        let remaining_nodes = limits
            .max_nodes()
            .checked_sub(2 + 2 * entity.through.len() + property.steps.len())
            .filter(|remaining| *remaining > 0 || matches!(&intent, PrefabPropertyIntent::Absent))
            .ok_or(Error::BudgetExceeded)?;

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
        if !selected.component.accepts_selected_overlay_properties
            || !schema.has_capability(ComponentCapability::Edit)
            || !field.has_capability(ComponentCapability::Edit)
        {
            return Err(Error::InvalidProperty);
        }
        let replacement = match &intent {
            PrefabPropertyIntent::Set(value) => {
                let shape = selected
                    .resolve_value(field, property, read)?
                    .ok_or(Error::InvalidProperty)?
                    .shape;
                let mut leaf = shape;
                while let ComponentValueShape::Optional { value } = leaf {
                    leaf = value;
                }
                if !matches!(leaf, ComponentValueShape::Scalar { .. })
                    || matches!(
                        value,
                        ComponentValue::List(_)
                            | ComponentValue::Map(_)
                            | ComponentValue::EntityReference(_)
                    )
                {
                    return Err(Error::InvalidProperty);
                }
                Some((shape, value))
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
        let through = &write_target.through[1..];
        let via_alias = write_target != entity;
        let (remaining_bytes, remaining_nodes) = if via_alias {
            let bytes = write_target
                .logical_bytes_up_to(remaining_bytes)
                .ok_or(Error::BudgetExceeded)?;
            let nodes = remaining_nodes
                .checked_sub(1 + 2 * write_target.through.len())
                .ok_or(Error::BudgetExceeded)?;
            (remaining_bytes - bytes, nodes)
        } else {
            (remaining_bytes, remaining_nodes)
        };
        if write_target.through.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        if let Some((shape, value)) = replacement {
            value
                .cost_up_to(remaining_nodes, remaining_bytes)
                .ok_or(Error::BudgetExceeded)?;
            shape
                .validate_value(value)
                .map_err(|_| Error::InvalidProperty)?;
        }

        Ok(ObservedPrefabPropertyWrite {
            anchor: anchor.clone(),
            base: hop.expected_base.clone(),
            target: PrefabEntityTarget {
                through: through.to_vec(),
                entity: write_target.entity.clone(),
            },
            observed_target: via_alias.then(|| entity.clone()),
            component: component.clone(),
            version: selected.fact().version(),
            existing_intent: selected.component.selected_overlay_intent,
            property: property.clone(),
            intent,
            basis: self.basis.clone(),
        })
    }

    /// Confirms that the prepared source composes the exact write at the original address.
    #[doc(hidden)]
    pub fn verifies_property_write(
        &self,
        edit: &ObservedPrefabPropertyWrite,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<bool, AuthoringPrefabPropertyWriteError> {
        use AuthoringPrefabPropertyWriteError as Error;

        let route;
        let target = if let Some(target) = &edit.observed_target {
            target
        } else {
            let mut through = Vec::with_capacity(edit.target.through.len() + 1);
            through.push(PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(edit.anchor.clone()),
                expected_base: edit.base.clone(),
            });
            through.extend(edit.target.through.iter().cloned());
            route = PrefabEntityTarget {
                through,
                entity: edit.target.entity.clone(),
            };
            &route
        };
        let property = self
            .target(target)
            .ok_or(Error::InvalidProperty)?
            .component(&edit.component)?
            .property_value(&edit.property, read)?;
        if !property.revert_claim().is_some_and(|claim| {
            in_selected_layer(claim, &edit.anchor) && claim.kind == edit.kind()
        }) {
            return Ok(false);
        }
        Ok(match (&edit.intent, property.value) {
            (PrefabPropertyIntent::Set(expected), AuthoringPrefabPropertyValue::Present(value)) => {
                value
                    .value
                    .as_value()
                    .is_some_and(|actual| component_values_exactly_match(actual, expected))
            }
            (
                PrefabPropertyIntent::Absent,
                AuthoringPrefabPropertyValue::Missing | AuthoringPrefabPropertyValue::Defaulted(_),
            ) => true,
            _ => false,
        })
    }
}

fn in_selected_layer(claim: &AuthoringPrefabPropertyClaim, anchor: &SceneEntityId) -> bool {
    claim.location.source() == &PrefabDeclarationSource::Containing
        && &claim.location.path().anchor == anchor
        && claim.location.path().steps.is_empty()
}
