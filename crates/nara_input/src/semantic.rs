use std::{any::Any, collections::BTreeMap, fmt, sync::Arc};

use nara_ecs::Resource;

use crate::{
    ActionContext, ActionId, ActionInputProvenance, ActionMap, ActionMaps, ActionPhase,
    ActionValue, ActionValueKind, AxisValue, ButtonTransitionPhase, InputConfigurationError,
    InputLimits, InputSeatId, InputTimelineEntry, MAX_BINDINGS_PER_INPUT,
};

/// Why a semantic action changed its aggregate held state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SemanticActionCause {
    PhysicalInput,
    ContextChanged,
    MappingChanged,
}

/// One aggregate action edge, independent of the number of physical bindings.
///
/// A context or mapping change has no physical event provenance. The corresponding terminal
/// edge therefore carries `None`, rather than attributing cancellation to the original press.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SemanticActionOutcome {
    pub seat: InputSeatId,
    pub action: ActionId,
    pub context: ActionContext,
    pub phase: ActionPhase,
    pub value: ActionValue,
    pub cause: SemanticActionCause,
    pub provenance: Option<ActionInputProvenance>,
}

/// Seat-local gameplay values and their frame-transient aggregate outcomes.
///
/// Buttons combine with OR; axis contributions sum in admitted-member order and clamp once.
/// `Started` marks an inactive-to-active aggregate, `Changed` a changed active value, and
/// `Released` an active-to-inactive value or active context/mapping cancellation. Opposing axis
/// contributions may remain physically held while their aggregate is zero.
/// Restoring a context never synthesizes activation: an old member must release or become neutral
/// before a fresh member may activate it. [`crate::ActionOutcomes`] exposes low-level binding edges.
///
/// Held state survives frames with zero or many fixed ticks. Outcomes last until frame cleanup;
/// durable commands and replay consumers must capture them before that boundary.
#[derive(Debug, Resource)]
pub struct SemanticActions {
    held: BTreeMap<ActionKey, HeldAction>,
    outcomes: Vec<SemanticActionOutcome>,
    contribution_count: usize,
    contribution_limit: usize,
}

impl Default for SemanticActions {
    fn default() -> Self {
        Self::with_limits(InputLimits::default()).expect("default semantic projection bounds fit")
    }
}

impl SemanticActions {
    pub(crate) fn with_limits(limits: InputLimits) -> Result<Self, InputConfigurationError> {
        let contribution_limit = limits
            .members()
            .checked_mul(MAX_BINDINGS_PER_INPUT)
            .ok_or(InputConfigurationError::CapacityOverflow)?;
        let outcome_limit = limits
            .events()
            .checked_mul(MAX_BINDINGS_PER_INPUT)
            .and_then(|count| count.checked_add(contribution_limit))
            .ok_or(InputConfigurationError::CapacityOverflow)?;
        let mut outcomes = Vec::new();
        outcomes
            .try_reserve_exact(outcome_limit)
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        Ok(Self {
            held: BTreeMap::new(),
            outcomes,
            contribution_count: 0,
            contribution_limit,
        })
    }

    #[must_use]
    pub fn pressed(&self, seat: InputSeatId, action: &ActionId, context: &ActionContext) -> bool {
        self.value(seat, action, context)
            .is_some_and(ActionValue::is_active)
    }

    #[must_use]
    pub fn value(
        &self,
        seat: InputSeatId,
        action: &ActionId,
        context: &ActionContext,
    ) -> Option<ActionValue> {
        self.held
            .get(&ActionKey {
                seat,
                action: action.clone(),
                context: context.clone(),
            })
            .map(|held| held.value)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[SemanticActionOutcome] {
        &self.outcomes
    }

    pub(crate) fn clear_outcomes(&mut self) {
        self.outcomes.clear();
    }

    pub(crate) fn reconcile(&mut self, maps: &ActionMaps, staging: &ActionStaging) {
        self.held.retain(|key, held| {
            let current = maps.get(key.seat).zip(staging.revision_for_seat(key.seat));
            let cause = if current
                .as_ref()
                .is_none_or(|(_, revision)| !Arc::ptr_eq(&held.mapping, &revision.mapping))
            {
                SemanticActionCause::MappingChanged
            } else {
                let (action_map, revision) = current.expect("an unchanged mapping is installed");
                if action_map.is_context_enabled(&key.context)
                    && action_map
                        .definition()
                        .context_index(&key.context)
                        .is_some_and(|slot| {
                            same_context_lifetime(
                                held.context.as_ref(),
                                revision.contexts[slot].as_ref(),
                            )
                        })
                {
                    return true;
                }
                SemanticActionCause::ContextChanged
            };
            self.contribution_count -= held.contributions.len();
            if held.value.is_active() {
                assert!(self.outcomes.len() < self.outcomes.capacity());
                self.outcomes.push(SemanticActionOutcome {
                    seat: key.seat,
                    action: key.action.clone(),
                    context: key.context.clone(),
                    phase: ActionPhase::Released,
                    value: neutral_value(held.value.kind()),
                    cause,
                    provenance: None,
                });
            }
            false
        });
    }

    pub(crate) fn apply(
        &mut self,
        maps: &ActionMaps,
        staging: &ActionStaging,
        staged_revision: &StagedActionRevision,
        entry: InputTimelineEntry,
    ) {
        let Some(input) = entry.action_input() else {
            return;
        };
        let press_sequence = entry
            .press_sequence()
            .expect("an admitted button edge names its original member");
        let provenance = entry.action_provenance();
        let axis = entry.axis_value();
        let phase = entry.action_transition().map(|(_, phase)| phase);
        if axis == Some(AxisValue::ZERO) || phase == Some(ButtonTransitionPhase::Released) {
            self.held.retain(|key, held| {
                if key.seat != entry.route().seat()
                    || held.contributions.remove(&press_sequence).is_none()
                {
                    return true;
                }
                self.contribution_count -= 1;
                let previous = held.value;
                held.value = held.aggregate();
                if let Some(phase) = value_transition(previous, held.value) {
                    assert!(self.outcomes.len() < self.outcomes.capacity());
                    self.outcomes.push(SemanticActionOutcome {
                        seat: key.seat,
                        action: key.action.clone(),
                        context: key.context.clone(),
                        phase,
                        value: held.value,
                        cause: SemanticActionCause::PhysicalInput,
                        provenance: Some(provenance),
                    });
                }
                !held.contributions.is_empty()
            });
            return;
        }

        let seat = entry.route().seat();
        let Some((action_map, revision)) = maps.get(seat).zip(staging.revision_for_seat(seat))
        else {
            return;
        };

        if !Arc::ptr_eq(&staged_revision.mapping, &revision.mapping) {
            return;
        }
        for index in action_map.binding_indices_for_input(input) {
            let Some(binding) = action_map.binding_at(index) else {
                continue;
            };
            let Some(slot) = action_map.definition().binding_context_index(index) else {
                continue;
            };
            if !action_map.is_context_enabled(&binding.context)
                || !same_context_lifetime(
                    staged_revision.contexts[slot].as_ref(),
                    revision.contexts[slot].as_ref(),
                )
            {
                continue;
            }
            let key = ActionKey {
                seat,
                action: binding.action.clone(),
                context: binding.context.clone(),
            };
            if axis.is_some()
                && entry.sequence() != press_sequence
                && !self
                    .held
                    .get(&key)
                    .is_some_and(|held| held.contributions.contains_key(&press_sequence))
            {
                continue;
            }
            let value = match axis {
                Some(value) => binding.value_for_axis(value),
                None => binding.value_for_button(ButtonTransitionPhase::Pressed),
            };
            let Some(value) = value else {
                continue;
            };
            let held = self.held.entry(key.clone()).or_insert_with(|| HeldAction {
                mapping: Arc::clone(&revision.mapping),
                context: revision.contexts[slot].clone(),
                contributions: BTreeMap::new(),
                value: neutral_value(binding.value_kind()),
                retention_owner: revision.retention_owner.clone(),
            });
            let previous = held.value;
            if !held.contributions.contains_key(&press_sequence) {
                assert!(self.contribution_count < self.contribution_limit);
                self.contribution_count += 1;
            }
            held.contributions.insert(press_sequence, value);
            held.value = held.aggregate();
            if let Some(phase) = value_transition(previous, held.value) {
                assert!(self.outcomes.len() < self.outcomes.capacity());
                self.outcomes.push(SemanticActionOutcome {
                    seat,
                    action: key.action,
                    context: key.context,
                    phase,
                    value: held.value,
                    cause: SemanticActionCause::PhysicalInput,
                    provenance: Some(provenance),
                });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ActionKey {
    seat: InputSeatId,
    action: ActionId,
    context: ActionContext,
}

struct HeldAction {
    mapping: Arc<()>,
    context: Option<Arc<()>>,
    // Each globally unique press sequence identifies an admitted Input owner member.
    contributions: BTreeMap<u64, ActionValue>,
    value: ActionValue,
    // Only the original state ticket follows these identities until release or reconciliation.
    retention_owner: Option<Arc<dyn Any + Send + Sync>>,
}

fn neutral_value(kind: ActionValueKind) -> ActionValue {
    match kind {
        ActionValueKind::Button => ActionValue::released(),
        ActionValueKind::Axis => ActionValue::Axis(AxisValue::ZERO),
    }
}

fn value_transition(previous: ActionValue, next: ActionValue) -> Option<ActionPhase> {
    if previous == next {
        None
    } else if !previous.is_active() && next.is_active() {
        Some(ActionPhase::Started)
    } else if previous.is_active() && !next.is_active() {
        Some(ActionPhase::Released)
    } else {
        Some(ActionPhase::Changed)
    }
}

impl HeldAction {
    fn aggregate(&self) -> ActionValue {
        match self.value.kind() {
            ActionValueKind::Button => {
                ActionValue::Button(self.contributions.values().any(|value| value.is_active()))
            }
            ActionValueKind::Axis => {
                let sum = self
                    .contributions
                    .values()
                    .map(|value| match value {
                        ActionValue::Axis(value) => f64::from(value.get()),
                        ActionValue::Button(pressed) => {
                            if *pressed {
                                1.0
                            } else {
                                0.0
                            }
                        }
                    })
                    .sum::<f64>();
                ActionValue::Axis(
                    AxisValue::new(sum.clamp(-1.0, 1.0) as f32)
                        .expect("bounded finite axis contributions produce a normalized sum"),
                )
            }
        }
    }
}

impl fmt::Debug for HeldAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeldAction")
            .field("mapping", &self.mapping)
            .field("context", &self.context)
            .field("contributions", &self.contributions)
            .field("has_retention_owner", &self.retention_owner.is_some())
            .finish()
    }
}

/// A configuration lifetime with one slot per admitted context, independent of binding custody.
#[derive(Clone)]
pub(crate) struct ActionMapRevision {
    mapping: Arc<()>,
    contexts: Box<[Option<Arc<()>>]>,
    retention_owner: Option<Arc<dyn Any + Send + Sync>>,
}

impl fmt::Debug for ActionMapRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActionMapRevision")
            .field("mapping", &self.mapping)
            .field("contexts", &self.contexts)
            .field("has_retention_owner", &self.retention_owner.is_some())
            .finish()
    }
}

impl ActionMapRevision {
    #[cfg(test)]
    pub(crate) fn new(context_count: usize) -> Self {
        Self::new_with_retention_owner(context_count, None)
    }

    pub(crate) fn new_with_retention_owner(
        context_count: usize,
        retention_owner: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Self {
        Self {
            mapping: Arc::new(()),
            contexts: vec![None; context_count].into_boxed_slice(),
            retention_owner,
        }
    }

    /// The receiving map validates the declared slot before changing its context state.
    pub(crate) fn context_changed(revision: &mut Arc<Self>, slot: usize) {
        Arc::make_mut(revision).contexts[slot] = Some(Arc::new(()));
    }
}

/// Queued lifetime observations retain their original state allowance, without bindings or assets.
#[derive(Default, Clone)]
pub(crate) struct StagedActionRevision {
    mapping: Arc<()>,
    contexts: Box<[Option<Arc<()>>]>,
    retention_owner: Option<Arc<dyn Any + Send + Sync>>,
}

impl fmt::Debug for StagedActionRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StagedActionRevision")
            .field("mapping", &self.mapping)
            .field("contexts", &self.contexts)
            .field("has_retention_owner", &self.retention_owner.is_some())
            .finish()
    }
}

impl StagedActionRevision {
    fn new(context_count: usize, retention_owner: Option<Arc<dyn Any + Send + Sync>>) -> Self {
        Self {
            mapping: Arc::new(()),
            contexts: vec![None; context_count].into_boxed_slice(),
            retention_owner,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct MapStaging {
    observed: Option<Arc<ActionMapRevision>>,
    current: Arc<StagedActionRevision>,
}

impl MapStaging {
    pub(super) fn observe(&mut self, action_map: &ActionMap) -> Arc<StagedActionRevision> {
        debug_assert_eq!(
            action_map.revision.contexts.len(),
            action_map.definition().contexts().len(),
        );
        self.observe_revision(&action_map.revision)
    }

    fn observe_revision(&mut self, revision: &Arc<ActionMapRevision>) -> Arc<StagedActionRevision> {
        if self
            .observed
            .as_ref()
            .is_some_and(|observed| Arc::ptr_eq(observed, revision))
        {
            return Arc::clone(&self.current);
        }
        if self
            .observed
            .as_ref()
            .is_none_or(|observed| !Arc::ptr_eq(&observed.mapping, &revision.mapping))
        {
            self.current = Arc::new(StagedActionRevision::new(
                revision.contexts.len(),
                revision.retention_owner.clone(),
            ));
        } else if let Some(observed) = &self.observed {
            debug_assert_eq!(observed.contexts.len(), revision.contexts.len());
            for (slot, (previous, next)) in observed
                .contexts
                .iter()
                .zip(revision.contexts.iter())
                .enumerate()
            {
                if !same_context_lifetime(previous.as_ref(), next.as_ref()) {
                    // Issue a fresh observed lifetime even when an older configuration returns.
                    Arc::make_mut(&mut self.current).contexts[slot] = Some(Arc::new(()));
                }
            }
        }
        self.observed = Some(Arc::clone(revision));
        Arc::clone(&self.current)
    }
}

#[derive(Debug, Default, Resource)]
pub(crate) struct ActionStaging {
    maps: BTreeMap<InputSeatId, MapStaging>,
}

impl ActionStaging {
    pub(crate) fn observe_maps(&mut self, maps: &ActionMaps) {
        self.maps.retain(|seat, _| maps.get(*seat).is_some());
        for (seat, map) in maps.iter() {
            self.maps.entry(seat).or_default().observe(map);
        }
    }

    pub(crate) fn revision_for_seat(
        &self,
        seat: InputSeatId,
    ) -> Option<&Arc<StagedActionRevision>> {
        self.maps.get(&seat).map(|map| &map.current)
    }
}

fn same_context_lifetime(left: Option<&Arc<()>>, right: Option<&Arc<()>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionMapRevision, ActionStaging, MapStaging, SemanticActions, same_context_lifetime,
    };
    use crate::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
        ActionMapLimits, ActionMaps, ButtonDriverInput, InputLimits, InputSeatId, InputTimeline,
        InputWindowId, KeyCode,
    };
    use nara_core::ByteLimit;
    use std::sync::Arc;

    #[test]
    fn unchanged_revision_shares_the_staged_snapshot() {
        let revision = Arc::new(ActionMapRevision::new(2));
        let mut staging = MapStaging::default();
        let first = staging.observe_revision(&revision);
        let repeated = staging.observe_revision(&revision);

        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(first.contexts.len(), 2);
    }

    #[test]
    fn restoring_context_configuration_cannot_restore_an_observed_lifetime() {
        let mut original = Arc::new(ActionMapRevision::new(2));
        let mut staging = MapStaging::default();
        staging.observe_revision(&original);
        ActionMapRevision::context_changed(&mut original, 1);
        let first = staging.observe_revision(&original);

        let mut changed = Arc::clone(&original);
        ActionMapRevision::context_changed(&mut changed, 0);
        let exited = staging.observe_revision(&changed);
        let restored = staging.observe_revision(&original);

        assert!(Arc::ptr_eq(&first.mapping, &restored.mapping));
        assert!(first.contexts[0].is_none());
        assert!(first.contexts[1].is_some());
        assert!(!same_context_lifetime(
            first.contexts[0].as_ref(),
            restored.contexts[0].as_ref(),
        ));
        assert!(!same_context_lifetime(
            exited.contexts[0].as_ref(),
            restored.contexts[0].as_ref(),
        ));
        assert!(same_context_lifetime(
            first.contexts[1].as_ref(),
            restored.contexts[1].as_ref(),
        ));
    }

    #[test]
    fn restoring_mapping_configuration_issues_a_fresh_staged_layout() {
        let original = Arc::new(ActionMapRevision::new(2));
        let replacement = Arc::new(ActionMapRevision::new(1));
        let mut staging = MapStaging::default();
        let first = staging.observe_revision(&original);
        let changed = staging.observe_revision(&replacement);
        let restored = staging.observe_revision(&original);

        assert!(!Arc::ptr_eq(&first.mapping, &changed.mapping));
        assert!(!Arc::ptr_eq(&first.mapping, &restored.mapping));
        assert!(!Arc::ptr_eq(&changed.mapping, &restored.mapping));
        assert_eq!(first.contexts.len(), 2);
        assert_eq!(changed.contexts.len(), 1);
        assert_eq!(restored.contexts.len(), 2);
    }

    #[test]
    fn context_snapshots_retain_state_after_the_map_and_staging_drop() {
        let owner = Arc::new(());
        let retained = Arc::downgrade(&owner);
        let mut revision = Arc::new(ActionMapRevision::new_with_retention_owner(2, Some(owner)));
        let mut staging = MapStaging::default();
        let first = staging.observe_revision(&revision);
        ActionMapRevision::context_changed(&mut revision, 0);
        let changed = staging.observe_revision(&revision);

        drop(revision);
        drop(staging);
        assert!(retained.upgrade().is_some());
        drop(first);
        assert!(retained.upgrade().is_some());
        drop(changed);
        assert!(retained.upgrade().is_none());
    }

    #[test]
    fn mapping_snapshots_retain_their_own_state_owner() {
        let original_owner = Arc::new(());
        let original_retained = Arc::downgrade(&original_owner);
        let original = Arc::new(ActionMapRevision::new_with_retention_owner(
            2,
            Some(original_owner),
        ));
        let replacement_owner = Arc::new(());
        let replacement_retained = Arc::downgrade(&replacement_owner);
        let replacement = Arc::new(ActionMapRevision::new_with_retention_owner(
            1,
            Some(replacement_owner),
        ));
        let mut staging = MapStaging::default();
        let first = staging.observe_revision(&original);
        let changed = staging.observe_revision(&replacement);

        drop(original);
        drop(replacement);
        drop(staging);
        assert!(original_retained.upgrade().is_some());
        assert!(replacement_retained.upgrade().is_some());
        drop(first);
        assert!(original_retained.upgrade().is_none());
        assert!(replacement_retained.upgrade().is_some());
        drop(changed);
        assert!(replacement_retained.upgrade().is_none());
    }

    #[test]
    fn held_identities_retain_only_the_state_ticket_after_map_and_snapshots_drop() {
        let action = ActionId::new("move").unwrap();
        let context = ActionContext::gameplay();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder.declare_context(context.clone(), true).unwrap();
        builder
            .bind(ActionBinding::key(action.clone(), KeyCode::Space))
            .unwrap();
        let definition_owner = Arc::new(builder.compile().unwrap());
        let definition_retained = Arc::downgrade(&definition_owner);
        let state_owner = Arc::new(());
        let state_retained = Arc::downgrade(&state_owner);
        let map = ActionMap::new_with_retention_owner(
            ActionMapBindings::from_owner(definition_owner),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
            state_owner,
        )
        .unwrap();
        let mut maps = ActionMaps::new(map);
        let mut staging = ActionStaging::default();
        staging.observe_maps(&maps);
        let map = maps.get_mut(InputSeatId::PRIMARY).unwrap();
        map.disable_context(&context).unwrap();
        map.enable_context(&context).unwrap();
        staging.observe_maps(&maps);
        let revision = staging
            .revision_for_seat(InputSeatId::PRIMARY)
            .unwrap()
            .clone();
        assert!(revision.contexts[0].is_some());
        let limits = InputLimits::new(16, 1, 1, 1).unwrap();
        let mut timeline = InputTimeline::with_limits(limits).unwrap();
        timeline
            .admit_keyboard(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Space),
            )
            .unwrap();
        let entry = *timeline.entries().last().unwrap();
        let mut semantic = SemanticActions::with_limits(limits).unwrap();
        semantic.apply(&maps, &staging, &revision, entry);
        assert!(semantic.pressed(InputSeatId::PRIMARY, &action, &context));

        drop(revision);
        drop(staging);
        drop(maps);
        assert!(definition_retained.upgrade().is_none());
        assert!(state_retained.upgrade().is_some());
        drop(semantic);
        assert!(state_retained.upgrade().is_none());
    }

    #[test]
    fn release_removes_its_admitted_member_when_raw_labels_repeat() {
        let limits = InputLimits::new(24, 1, 1, 2).unwrap();
        let mut timeline = InputTimeline::with_limits(limits).unwrap();
        for edge in [
            ButtonDriverInput::Press(KeyCode::Space),
            ButtonDriverInput::Release(KeyCode::Space),
            ButtonDriverInput::Press(KeyCode::Space),
            ButtonDriverInput::Release(KeyCode::Space),
        ] {
            timeline
                .admit_keyboard(InputWindowId::UNSPECIFIED, InputSeatId::PRIMARY, edge)
                .unwrap();
        }
        let entries = timeline.entries().copied().collect::<Vec<_>>();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        let context = ActionContext::gameplay();
        let action = ActionId::new("activate").unwrap();
        builder.declare_context(context.clone(), true).unwrap();
        builder
            .bind(ActionBinding::key(action.clone(), KeyCode::Space))
            .unwrap();
        let map = ActionMap::new(
            builder.compile().unwrap().into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        let maps = ActionMaps::new(map);
        let mut staging = ActionStaging::default();
        staging.observe_maps(&maps);
        let revision = staging
            .revision_for_seat(InputSeatId::PRIMARY)
            .unwrap()
            .clone();
        let mut semantic = SemanticActions::with_limits(limits).unwrap();
        // Apply both distinct admitted presses, then end the first member. The same raw endpoint
        // and binding must not make the older release remove the later member's contribution.
        for index in [0, 2, 1] {
            let entry = entries[index];
            semantic.apply(&maps, &staging, &revision, entry);
        }
        assert!(semantic.pressed(InputSeatId::PRIMARY, &action, &context));
        assert_eq!(semantic.contribution_count, 1);
        assert_eq!(semantic.as_slice().len(), 1);

        let release = entries[3];
        semantic.apply(&maps, &staging, &revision, release);
        assert!(!semantic.pressed(InputSeatId::PRIMARY, &action, &context));
        assert_eq!(semantic.contribution_count, 0);
        assert_eq!(semantic.as_slice().len(), 2);
        assert_eq!(
            semantic.as_slice()[1].provenance,
            Some(release.action_provenance())
        );
    }

    #[test]
    fn maximum_fanout_and_context_cancellation_fit_derived_projection_bounds() {
        let limits = InputLimits::new(32, 2, 1, 2).unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        let context = ActionContext::gameplay();
        builder.declare_context(context.clone(), true).unwrap();
        for index in 0..crate::MAX_BINDINGS_PER_INPUT {
            builder
                .bind(ActionBinding::key(
                    ActionId::new(format!("action-{index}")).unwrap(),
                    KeyCode::Space,
                ))
                .unwrap();
        }
        let map = ActionMap::new(
            builder.compile().unwrap().into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        let mut maps = ActionMaps::new(map);
        let mut staging = ActionStaging::default();
        staging.observe_maps(&maps);
        let revision = staging
            .revision_for_seat(InputSeatId::PRIMARY)
            .unwrap()
            .clone();
        let mut timeline = InputTimeline::with_limits(limits).unwrap();
        let other = InputWindowId::new(99);
        timeline
            .connect_endpoint(other, InputSeatId::PRIMARY)
            .unwrap();
        for window in [InputWindowId::UNSPECIFIED, other] {
            timeline
                .admit_keyboard(
                    window,
                    InputSeatId::PRIMARY,
                    ButtonDriverInput::Press(KeyCode::Space),
                )
                .unwrap();
        }
        let mut semantic = SemanticActions::with_limits(limits).unwrap();
        let output_capacity = semantic.outcomes.capacity();
        for entry in timeline.entries().copied() {
            semantic.apply(&maps, &staging, &revision, entry);
        }
        assert_eq!(
            semantic.contribution_count,
            limits.members() * crate::MAX_BINDINGS_PER_INPUT
        );
        assert_eq!(semantic.contribution_count, semantic.contribution_limit);
        assert_eq!(semantic.held.len(), crate::MAX_BINDINGS_PER_INPUT);
        assert_eq!(semantic.as_slice().len(), crate::MAX_BINDINGS_PER_INPUT);

        semantic.clear_outcomes();
        maps.get_mut(InputSeatId::PRIMARY)
            .unwrap()
            .disable_context(&context)
            .unwrap();
        staging.observe_maps(&maps);
        semantic.reconcile(&maps, &staging);
        assert!(semantic.held.is_empty());
        assert_eq!(semantic.contribution_count, 0);
        assert_eq!(semantic.as_slice().len(), crate::MAX_BINDINGS_PER_INPUT);
        assert!(semantic.as_slice().iter().all(|outcome| {
            outcome.phase == crate::ActionPhase::Released
                && outcome.cause == super::SemanticActionCause::ContextChanged
                && outcome.provenance.is_none()
        }));
        assert_eq!(semantic.outcomes.capacity(), output_capacity);
    }
}
