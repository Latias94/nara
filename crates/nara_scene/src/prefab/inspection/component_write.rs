use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::schema::component_values_exactly_match;

use super::*;
use crate::{
    PrefabComponentEdit, PrefabComponentSetMode, PrefabDeclarationLimits, PrefabEntityOrigin,
    PrefabInstance, SceneAuthoringRevision, SceneComponentRecord,
    authoring::prefab_anchor_instance,
    authoring_decode::{AuthoringComponentTemplate, retained_prefab_target_bytes},
    prefab::overlay::cost::{
        PrefabOverlayCost, PrefabOverlayCostLimits, prefab_component_intent_cost,
    },
    provenance::provenance_string_bytes,
};

/// A failure to select or prepare a component edit from a prefab inspection.
#[derive(Debug)]
pub enum AuthoringPrefabComponentEditError {
    InvalidComponent,
    BudgetExceeded,
    NoCurrentClaim,
    StaleObservedIntent,
}

/// A component edit bound to the inspected source revision and declaration route.
#[derive(Debug)]
pub struct ObservedPrefabComponentEdit {
    pub(crate) anchor: SceneEntityId,
    pub(crate) base: SourceDocumentKey,
    pub(crate) target: PrefabEntityTarget,
    pub(crate) component: ComponentTypeId,
    pub(crate) edit: PrefabComponentEdit,
    pub(crate) existing: Option<ObservedComponentDeclaration>,
    pub(crate) basis: AuthoringSchemaBasis,
    source: WorkingSourceObservation,
    selected: PrefabEntityTarget,
    value_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct ObservedComponentDeclaration {
    pub(crate) index: usize,
    pub(crate) intent: PrefabComponentIntent,
    cost: PrefabOverlayCost,
}

impl ObservedPrefabComponentEdit {
    pub const fn anchor(&self) -> &SceneEntityId {
        &self.anchor
    }

    pub const fn operation(&self) -> &PrefabComponentEdit {
        &self.edit
    }

    pub(crate) fn component_template<'a>(
        &'a self,
        value: &'a ComponentValue,
    ) -> Option<AuthoringComponentTemplate<'a>> {
        self.basis.component_template(&self.component, value)
    }

    pub(crate) fn matches_working_source(
        &self,
        input: SourceDocumentInput<'_>,
        revision: SceneAuthoringRevision,
    ) -> bool {
        self.source.revision_for(input) == Some(revision)
    }

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let existing_bytes = match &self.existing {
            Some(observed) => observed
                .cost
                .nodes
                .checked_mul(512)?
                .checked_add(observed.cost.logical_bytes())?,
            None => 0,
        };
        size_of::<Self>()
            .checked_add(provenance_string_bytes(self.anchor.as_str())?)?
            .checked_add(self.base.checked_retained_bytes()?)?
            .checked_add(retained_prefab_target_bytes(&self.target)?)?
            .checked_add(retained_prefab_target_bytes(&self.selected)?)?
            .checked_add(provenance_string_bytes(self.component.as_str())?)?
            .checked_add(self.value_bytes)?
            .checked_add(existing_bytes)
    }
}

impl AuthoringPrefabInstanceSnapshot {
    pub(crate) fn component_apply_declaration<'a>(
        &self,
        source: SourceDocumentInput<'a>,
        revision: SceneAuthoringRevision,
        selected: &'a PrefabEntityTarget,
        component: &ComponentTypeId,
        limits: PrefabDeclarationLimits,
    ) -> Result<
        (
            &'a SceneEntityId,
            usize,
            &'a PrefabComponentIntent,
            PrefabOverlayCost,
        ),
        AuthoringPrefabComponentEditError,
    > {
        use AuthoringPrefabComponentEditError as Error;
        if self
            .working_source
            .as_ref()
            .and_then(|original| original.revision_for(source))
            != Some(revision)
        {
            return Err(Error::StaleObservedIntent);
        }
        let Some(PrefabEntityOrigin::SourceEntity(anchor)) =
            selected.through.first().map(|hop| &hop.anchor)
        else {
            return Err(Error::InvalidComponent);
        };
        let bytes = remaining_selector_bytes(selected, component, limits)?;
        let view = self.target(selected).ok_or(Error::InvalidComponent)?;
        let declaration = view
            .component_overrides
            .and_then(|entries| entries.get(component))
            .ok_or(Error::NoCurrentClaim)?;
        let instance = instance_at(source, anchor).ok_or(Error::StaleObservedIntent)?;
        let intent = declaration.stored_intent(instance, component)?;
        let cost = declaration_copy_cost(intent, limits, bytes)?;
        Ok((anchor, declaration.index, intent, cost))
    }

    /// Selects a complete component edit against the captured working source and its revision.
    pub fn plan_component_edit(
        &self,
        source: SourceDocumentInput<'_>,
        selected: &PrefabEntityTarget,
        component: &ComponentTypeId,
        edit: PrefabComponentEdit,
        limits: PrefabDeclarationLimits,
    ) -> Result<ObservedPrefabComponentEdit, AuthoringPrefabComponentEditError> {
        use AuthoringPrefabComponentEditError as Error;
        let working_source = self
            .working_source
            .as_ref()
            .filter(|original| original.revision_for(source).is_some())
            .ok_or(Error::StaleObservedIntent)?;
        let (hop, through) = selected
            .through
            .split_first()
            .ok_or(Error::InvalidComponent)?;
        let PrefabEntityOrigin::SourceEntity(anchor) = &hop.anchor else {
            return Err(Error::InvalidComponent);
        };
        let bytes = remaining_selector_bytes(selected, component, limits)?;
        let entity = self
            .data
            .targets
            .get(selected)
            .ok_or(Error::InvalidComponent)?;
        let target_view = self.target(selected).ok_or(Error::InvalidComponent)?;
        let current = target_view.components.get(component);
        let observation = target_view
            .component_overrides
            .and_then(|overrides| overrides.get(component));
        let instance = instance_at(source, anchor).ok_or(Error::StaleObservedIntent)?;
        let declared = observation
            .map(|observation| {
                let intent = observation.stored_intent(instance, component)?;
                Ok((observation.index, intent))
            })
            .transpose()?;

        let schema = self
            .basis
            .schema(component)
            .ok_or(Error::InvalidComponent)?;
        let underlying = observation.map_or_else(
            || current.map(|record| record.ownership),
            |entry| entry.underlying.as_ref().map(|record| record.ownership),
        );
        if !matches!(&edit, PrefabComponentEdit::Revert)
            && (!self.basis.has_authoring_adapter(component)
                || !crate::authoring_decode::component_allows_composition(schema)
                || underlying == Some(InspectedComponentOwnership::LocalRecord)
                || current.is_some_and(|current| {
                    !current.fact.readiness().is_available()
                        || current.ownership == InspectedComponentOwnership::LocalRecord
                }))
        {
            return Err(Error::InvalidComponent);
        }
        let value_bytes = match &edit {
            PrefabComponentEdit::Set { mode, value } => {
                if (*mode == PrefabComponentSetMode::Replace) != underlying.is_some() {
                    return Err(Error::InvalidComponent);
                }
                let cost = validate_value_materialization_budget(
                    value,
                    ComponentValueMaterializationLimits::new(
                        ItemLimit::new(limits.max_nodes()).ok_or(Error::BudgetExceeded)?,
                        ByteLimit::new(bytes).ok_or(Error::BudgetExceeded)?,
                        DepthLimit::new(limits.max_depth()).ok_or(Error::BudgetExceeded)?,
                    ),
                )
                .map_err(|_| Error::BudgetExceeded)?;
                cost.nodes()
                    .checked_mul(96)
                    .and_then(|nodes| nodes.checked_add(cost.logical_bytes()))
                    .ok_or(Error::BudgetExceeded)?
            }
            PrefabComponentEdit::Suppress if underlying.is_none() => {
                return Err(Error::InvalidComponent);
            }
            PrefabComponentEdit::Revert if declared.is_none() => return Err(Error::NoCurrentClaim),
            _ => 0,
        };

        let existing = declared
            .map(|(index, intent)| {
                let cost = declaration_copy_cost(intent, limits, bytes)?;
                Ok(ObservedComponentDeclaration {
                    index,
                    intent: intent.clone(),
                    cost,
                })
            })
            .transpose()?;

        let target = if let Some(previous) = &existing {
            previous.intent.address().0.clone()
        } else if through.is_empty()
            && matches!(&selected.entity, PrefabEntityOrigin::InstanceAddition(_))
        {
            let mut route = None;
            for (index, (candidate, id)) in self.data.targets.iter().enumerate() {
                if index >= limits.max_nodes() {
                    return Err(Error::BudgetExceeded);
                }
                if id == entity && candidate.through.len() > 1 {
                    candidate
                        .through
                        .get(1..)
                        .filter(|steps| steps.len() < limits.max_depth())
                        .ok_or(Error::BudgetExceeded)?;
                    candidate
                        .logical_bytes_up_to(bytes)
                        .ok_or(Error::BudgetExceeded)?;
                    route = Some(candidate);
                    break;
                }
            }
            let route = route.ok_or(Error::InvalidComponent)?;
            PrefabEntityTarget {
                through: route.through[1..].to_vec(),
                entity: route.entity.clone(),
            }
        } else {
            PrefabEntityTarget {
                through: through.to_vec(),
                entity: selected.entity.clone(),
            }
        };

        Ok(ObservedPrefabComponentEdit {
            anchor: anchor.clone(),
            base: hop.expected_base.clone(),
            target,
            component: component.clone(),
            edit: edit.clone(),
            existing,
            source: working_source.clone(),
            basis: self.basis.clone(),
            selected: selected.clone(),
            value_bytes,
        })
    }

    /// Confirms the stored declaration and the compiled result at the originally selected route.
    pub fn verifies_component_edit(
        &self,
        source: SourceDocumentInput<'_>,
        edit: &ObservedPrefabComponentEdit,
    ) -> bool {
        if self.basis != edit.basis {
            return false;
        }
        let Some(target) = self.target(&edit.selected) else {
            return false;
        };
        let current = target.components.get(&edit.component);
        let declaration = target
            .component_overrides
            .and_then(|entries| entries.get(&edit.component));
        let stored = declaration.and_then(|observed| {
            instance_at(source, &edit.anchor)?
                .overrides
                .as_ref()?
                .components
                .get(observed.index)
        });
        if stored.is_some_and(|intent| intent.address() != (&edit.target, &edit.component)) {
            return false;
        }
        match &edit.edit {
            PrefabComponentEdit::Set { mode, value } => {
                let Some(PrefabComponentIntent::Set {
                    mode: stored_mode,
                    record,
                    ..
                }) = stored
                else {
                    return false;
                };
                let Some(template) = edit.component_template(value) else {
                    return false;
                };
                *mode == *stored_mode
                    && record_matches_template(record, template)
                    && current.is_some_and(|entry| {
                        entry.fact.readiness().is_available() && matches!(entry.fact.record(),
                            AuthoringPrefabRecordOrigin::Overlay { location, slot: PrefabOverlaySlot::Component { index } }
                                if location.source() == &PrefabDeclarationSource::Containing
                                    && location.path().anchor == edit.anchor && location.path().steps.is_empty()
                                    && declaration.is_some_and(|declaration| declaration.index == *index)
                        )
                    })
            }
            PrefabComponentEdit::Suppress => {
                current.is_none() && matches!(stored, Some(PrefabComponentIntent::Suppress { .. }))
            }
            PrefabComponentEdit::Revert => {
                declaration.is_none()
                    && current.is_none_or(|entry| entry.fact.readiness().is_available())
            }
        }
    }
}

impl InspectedPrefabComponentOverride {
    fn stored_intent<'a>(
        &self,
        instance: &'a PrefabInstance,
        component: &ComponentTypeId,
    ) -> Result<&'a PrefabComponentIntent, AuthoringPrefabComponentEditError> {
        instance
            .overrides
            .as_ref()
            .and_then(|overlay| overlay.components.get(self.index))
            .filter(|intent| {
                intent.address().1 == component
                    && AuthoringPrefabComponentOverrideKind::of(intent) == self.kind
            })
            .ok_or(AuthoringPrefabComponentEditError::StaleObservedIntent)
    }
}

fn declaration_copy_cost(
    intent: &PrefabComponentIntent,
    limits: PrefabDeclarationLimits,
    bytes: usize,
) -> Result<PrefabOverlayCost, AuthoringPrefabComponentEditError> {
    prefab_component_intent_cost(
        intent,
        PrefabOverlayCostLimits {
            nodes: limits.max_nodes(),
            depth: limits.max_depth(),
            logical_bytes: bytes,
            ..PrefabOverlayCostLimits::UNBOUNDED
        },
    )
    .map_err(|_| AuthoringPrefabComponentEditError::BudgetExceeded)
}

fn record_matches_template(
    record: &SceneComponentRecord,
    template: AuthoringComponentTemplate<'_>,
) -> bool {
    if record.version != template.schema.version()
        || record.owner() != Some(template.owner)
        || !component_values_exactly_match(record.value(), template.value)
    {
        return false;
    }
    let mut defaults = template.contextual_defaults();
    record.contextual_defaults.len() == defaults.len()
        && defaults.all(|(field, expected)| {
            record
                .contextual_defaults
                .get(field)
                .is_some_and(|value| component_values_exactly_match(value, expected))
        })
}

fn instance_at<'a>(
    source: SourceDocumentInput<'a>,
    anchor: &SceneEntityId,
) -> Option<&'a PrefabInstance> {
    let entities = match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    };
    prefab_anchor_instance(entities, anchor).ok()?.as_ref()
}

fn remaining_selector_bytes(
    target: &PrefabEntityTarget,
    component: &ComponentTypeId,
    limits: PrefabDeclarationLimits,
) -> Result<usize, AuthoringPrefabComponentEditError> {
    use AuthoringPrefabComponentEditError::BudgetExceeded;
    if target.through.len() > limits.max_depth() {
        return Err(BudgetExceeded);
    }
    let bytes = target
        .logical_bytes_up_to(limits.max_logical_bytes())
        .ok_or(BudgetExceeded)?;
    limits
        .max_logical_bytes()
        .checked_sub(bytes)
        .and_then(|bytes| bytes.checked_sub(component.as_str().len()))
        .ok_or(BudgetExceeded)
}
