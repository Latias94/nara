use nara_reflect::{
    __private::component_values_exactly_match, ComponentCapability, ComponentPropertyStep,
};

use super::*;
use crate::{
    PrefabDeclarationLimits, PrefabEntityOrigin, PrefabInstanceHop, SourceDocumentKey,
    authoring_decode::retained_prefab_target_bytes, provenance::provenance_string_bytes,
};

#[derive(Debug)]
pub struct ObservedPrefabPropertySet {
    pub(crate) anchor: SceneEntityId,
    pub(crate) base: SourceDocumentKey,
    pub(crate) target: PrefabEntityTarget,
    pub(crate) observed_target: Option<PrefabEntityTarget>,
    pub(crate) component: ComponentTypeId,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) existing_intent: Option<usize>,
    pub(crate) property: ComponentPropertyPath,
    pub(crate) value: ComponentValue,
    pub(crate) basis: AuthoringSchemaBasis,
}

impl ObservedPrefabPropertySet {
    pub const fn anchor(&self) -> &SceneEntityId {
        &self.anchor
    }

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let value_bytes = match &self.value {
            ComponentValue::String(value) => value.capacity(),
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
pub enum AuthoringPrefabPropertySetError {
    Read(AuthoringReadError),
    InvalidProperty,
    BudgetExceeded,
}

impl From<AuthoringReadError> for AuthoringPrefabPropertySetError {
    fn from(error: AuthoringReadError) -> Self {
        Self::Read(error)
    }
}

impl AuthoringPrefabInstanceSnapshot {
    /// Resolves a visible scalar to its writable instance layer and original declaration group.
    #[doc(hidden)]
    pub fn plan_property_set(
        &self,
        entity: &PrefabEntityTarget,
        component: &ComponentTypeId,
        property: &ComponentPropertyPath,
        value: ComponentValue,
        limits: PrefabDeclarationLimits,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<ObservedPrefabPropertySet, AuthoringPrefabPropertySetError> {
        use AuthoringPrefabPropertySetError as Error;

        let max_bytes = limits.max_logical_bytes();
        let target_bytes = entity
            .logical_bytes_up_to(max_bytes)
            .ok_or(Error::BudgetExceeded)?;
        let property_bytes = property.logical_bytes().map_err(AuthoringReadError::from)?;
        let value_room = max_bytes
            .checked_sub(target_bytes)
            .and_then(|room| room.checked_sub(property_bytes))
            .and_then(|room| room.checked_sub(component.as_str().len()))
            .ok_or(Error::BudgetExceeded)?;
        if entity.through.len() > limits.max_depth() || property.steps.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        let value_nodes = limits
            .max_nodes()
            .checked_sub(2 + 2 * entity.through.len() + property.steps.len())
            .filter(|remaining| *remaining > 0)
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
        let current = selected
            .resolve_value(field, property, read)?
            .ok_or(Error::InvalidProperty)?;
        let mut leaf = current.shape;
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
        let (value_room, value_nodes) = if write_target != entity {
            let bytes = write_target
                .logical_bytes_up_to(value_room)
                .ok_or(Error::BudgetExceeded)?;
            let nodes = value_nodes
                .checked_sub(1 + 2 * write_target.through.len())
                .ok_or(Error::BudgetExceeded)?;
            (value_room - bytes, nodes)
        } else {
            (value_room, value_nodes)
        };
        if write_target.through.len() > limits.max_depth() {
            return Err(Error::BudgetExceeded);
        }
        value
            .cost_up_to(value_nodes, value_room)
            .ok_or(Error::BudgetExceeded)?;
        current
            .shape
            .validate_value(&value)
            .map_err(|_| Error::InvalidProperty)?;

        Ok(ObservedPrefabPropertySet {
            anchor: anchor.clone(),
            base: hop.expected_base.clone(),
            target: PrefabEntityTarget {
                through: through.to_vec(),
                entity: write_target.entity.clone(),
            },
            observed_target: (write_target != entity).then(|| entity.clone()),
            component: component.clone(),
            version: selected.fact().version(),
            existing_intent: selected.component.selected_overlay_intent,
            property: property.clone(),
            value,
            basis: self.basis.clone(),
        })
    }

    /// Confirms that the prepared source composes the exact Set at the original address.
    #[doc(hidden)]
    pub fn verifies_property_set(
        &self,
        edit: &ObservedPrefabPropertySet,
        read: &mut ComponentPropertyReadBudget,
    ) -> Result<bool, AuthoringPrefabPropertySetError> {
        use AuthoringPrefabPropertySetError as Error;

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
        let AuthoringPrefabPropertyValue::Present(value) = property.value else {
            return Ok(false);
        };
        let same_value = value
            .value
            .as_value()
            .is_some_and(|actual| component_values_exactly_match(actual, &edit.value));
        Ok(same_value
            && property.revert_claim().is_some_and(|claim| {
                in_selected_layer(claim, &edit.anchor)
                    && claim.kind == AuthoringPrefabPropertyClaimKind::Set
            }))
    }
}

fn in_selected_layer(claim: &AuthoringPrefabPropertyClaim, anchor: &SceneEntityId) -> bool {
    claim.location.source() == &PrefabDeclarationSource::Containing
        && &claim.location.path().anchor == anchor
        && claim.location.path().steps.is_empty()
}
