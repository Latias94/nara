use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use nara_ecs::Resource;

use crate::{
    ActionContext, ActionId, ActionInputProvenance, ActionMap, ActionPhase, ActionValue,
    ButtonInputEndpoint, ButtonTransitionPhase, InputBinding,
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
    pub action: ActionId,
    pub context: ActionContext,
    pub phase: ActionPhase,
    pub value: ActionValue,
    pub cause: SemanticActionCause,
    pub provenance: Option<ActionInputProvenance>,
}

/// Gameplay-facing held actions and their frame-transient aggregate edges.
///
/// `Started` is emitted when the first admitted physical binding becomes held. `Released` is
/// emitted when the final contribution releases, or once when its context/mapping expires.
/// Restoring a context never synthesizes a press: inputs already held must release and be pressed
/// again. [`crate::ActionOutcomes`] separately exposes low-level per-binding edges.
///
/// Held state survives frames with zero or many fixed ticks. Outcomes last until frame cleanup;
/// durable commands and replay consumers must capture them before that boundary.
#[derive(Debug, Default, Resource)]
pub struct SemanticActions {
    held: BTreeMap<ActionKey, HeldAction>,
    outcomes: Vec<SemanticActionOutcome>,
}

impl SemanticActions {
    #[must_use]
    pub fn pressed(&self, action: &ActionId, context: &ActionContext) -> bool {
        self.held.contains_key(&ActionKey {
            action: action.clone(),
            context: context.clone(),
        })
    }

    #[must_use]
    pub fn as_slice(&self) -> &[SemanticActionOutcome] {
        &self.outcomes
    }

    pub(crate) fn clear_outcomes(&mut self) {
        self.outcomes.clear();
    }

    pub(crate) fn reconcile(&mut self, action_map: &ActionMap, revision: &StagedActionRevision) {
        let expired = self
            .held
            .iter()
            .filter_map(|(key, held)| {
                let cause = if !Arc::ptr_eq(&held.mapping, &revision.mapping) {
                    SemanticActionCause::MappingChanged
                } else if !action_map.is_context_enabled(&key.context)
                    || !action_map
                        .definition()
                        .context_index(&key.context)
                        .is_some_and(|slot| {
                            same_context_lifetime(
                                held.context.as_ref(),
                                revision.contexts[slot].as_ref(),
                            )
                        })
                {
                    SemanticActionCause::ContextChanged
                } else {
                    return None;
                };
                Some((key.clone(), cause))
            })
            .collect::<Vec<_>>();
        for (key, cause) in expired {
            self.held.remove(&key);
            self.outcomes.push(SemanticActionOutcome {
                action: key.action,
                context: key.context,
                phase: ActionPhase::Released,
                value: ActionValue::released(),
                cause,
                provenance: None,
            });
        }
    }

    pub(crate) fn apply(
        &mut self,
        action_map: &ActionMap,
        revision: &StagedActionRevision,
        staged_revision: &StagedActionRevision,
        input: InputBinding,
        phase: ButtonTransitionPhase,
        provenance: ActionInputProvenance,
    ) {
        let physical = PhysicalContribution {
            input,
            endpoint: if provenance.sequence().is_some() {
                ButtonInputEndpoint::from_route(provenance.route())
            } else {
                ButtonInputEndpoint::Synthetic
            },
        };
        if phase == ButtonTransitionPhase::Released {
            let ended = self
                .held
                .iter_mut()
                .filter_map(|(key, held)| {
                    (held.contributions.remove(&physical) && held.contributions.is_empty())
                        .then(|| key.clone())
                })
                .collect::<Vec<_>>();
            for key in ended {
                self.held.remove(&key);
                self.outcomes.push(SemanticActionOutcome {
                    action: key.action,
                    context: key.context,
                    phase: ActionPhase::Released,
                    value: ActionValue::released(),
                    cause: SemanticActionCause::PhysicalInput,
                    provenance: Some(provenance),
                });
            }
            return;
        }

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
                action: binding.action.clone(),
                context: binding.context.clone(),
            };
            let held = self.held.entry(key.clone()).or_insert_with(|| HeldAction {
                mapping: Arc::clone(&revision.mapping),
                context: revision.contexts[slot].clone(),
                contributions: BTreeSet::new(),
                retention_owner: revision.retention_owner.clone(),
            });
            let started = held.contributions.is_empty();
            held.contributions.insert(physical);
            if started {
                self.outcomes.push(SemanticActionOutcome {
                    action: key.action,
                    context: key.context,
                    phase: ActionPhase::Started,
                    value: ActionValue::pressed(),
                    cause: SemanticActionCause::PhysicalInput,
                    provenance: Some(provenance),
                });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ActionKey {
    action: ActionId,
    context: ActionContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PhysicalContribution {
    input: InputBinding,
    endpoint: ButtonInputEndpoint,
}

struct HeldAction {
    mapping: Arc<()>,
    context: Option<Arc<()>>,
    contributions: BTreeSet<PhysicalContribution>,
    // Only the original state ticket follows these identities until release or reconciliation.
    retention_owner: Option<Arc<dyn Any + Send + Sync>>,
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

#[derive(Debug, Default, Resource)]
pub(crate) struct ActionStaging {
    observed: Option<Arc<ActionMapRevision>>,
    current: Arc<StagedActionRevision>,
}

impl ActionStaging {
    pub(crate) fn observe(&mut self, action_map: &ActionMap) -> Arc<StagedActionRevision> {
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

fn same_context_lifetime(left: Option<&Arc<()>>, right: Option<&Arc<()>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionMapRevision, ActionStaging, SemanticActions, same_context_lifetime};
    use crate::{
        ActionBinding, ActionContext, ActionId, ActionInputProvenance, ActionMap,
        ActionMapBindings, ActionMapBuilder, ActionMapLimits, ButtonTransitionPhase, InputBinding,
        KeyCode,
    };
    use nara_core::ByteLimit;
    use std::sync::Arc;

    #[test]
    fn unchanged_revision_shares_the_staged_snapshot() {
        let revision = Arc::new(ActionMapRevision::new(2));
        let mut staging = ActionStaging::default();
        let first = staging.observe_revision(&revision);
        let repeated = staging.observe_revision(&revision);

        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(first.contexts.len(), 2);
    }

    #[test]
    fn restoring_context_configuration_cannot_restore_an_observed_lifetime() {
        let mut original = Arc::new(ActionMapRevision::new(2));
        let mut staging = ActionStaging::default();
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
        let mut staging = ActionStaging::default();
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
        let mut staging = ActionStaging::default();
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
        let mut staging = ActionStaging::default();
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
        let mut map = ActionMap::new_with_retention_owner(
            ActionMapBindings::from_owner(definition_owner),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
            state_owner,
        )
        .unwrap();
        let mut staging = ActionStaging::default();
        staging.observe(&map);
        map.disable_context(context.clone()).unwrap();
        map.enable_context(&context).unwrap();
        let revision = staging.observe(&map);
        assert!(revision.contexts[0].is_some());
        let mut semantic = SemanticActions::default();
        semantic.apply(
            &map,
            &revision,
            &revision,
            InputBinding::Key(KeyCode::Space),
            ButtonTransitionPhase::Pressed,
            ActionInputProvenance::synthetic(),
        );
        assert!(semantic.pressed(&action, &context));

        drop(revision);
        drop(staging);
        drop(map);
        assert!(definition_retained.upgrade().is_none());
        assert!(state_retained.upgrade().is_some());
        drop(semantic);
        assert!(state_retained.upgrade().is_none());
    }
}
