use std::{
    collections::{BTreeMap, BTreeSet},
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
#[derive(Debug, Default, Clone, Resource)]
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
                    || !same_context_lifetime(
                        held.context.as_ref(),
                        revision.contexts.get(&key.context),
                    )
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
            if !action_map.is_context_enabled(&binding.context)
                || !same_context_lifetime(
                    staged_revision.contexts.get(&binding.context),
                    revision.contexts.get(&binding.context),
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
                context: revision.contexts.get(&binding.context).cloned(),
                contributions: BTreeSet::new(),
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

#[derive(Debug, Clone)]
struct HeldAction {
    mapping: Arc<()>,
    context: Option<Arc<()>>,
    contributions: BTreeSet<PhysicalContribution>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ActionMapRevision {
    mapping: Arc<()>,
    contexts: BTreeMap<ActionContext, Arc<()>>,
}

impl ActionMapRevision {
    pub(crate) fn mapping_changed(revision: &mut Arc<Self>) {
        Arc::make_mut(revision).mapping = Arc::new(());
    }

    pub(crate) fn context_changed(revision: &mut Arc<Self>, context: ActionContext) {
        Arc::make_mut(revision)
            .contexts
            .insert(context, Arc::new(()));
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct StagedActionRevision {
    mapping: Arc<()>,
    contexts: BTreeMap<ActionContext, Arc<()>>,
}

#[derive(Debug, Default, Resource)]
pub(crate) struct ActionStaging {
    observed: Option<Arc<ActionMapRevision>>,
    current: Arc<StagedActionRevision>,
}

impl ActionStaging {
    pub(crate) fn observe(&mut self, action_map: &ActionMap) -> Arc<StagedActionRevision> {
        if self
            .observed
            .as_ref()
            .is_some_and(|observed| Arc::ptr_eq(observed, &action_map.revision))
        {
            return Arc::clone(&self.current);
        }
        if self
            .observed
            .as_ref()
            .is_none_or(|observed| !Arc::ptr_eq(&observed.mapping, &action_map.revision.mapping))
        {
            self.current = Arc::new(StagedActionRevision::default());
        } else if let Some(observed) = &self.observed {
            let contexts = action_map
                .bindings()
                .iter()
                .map(|binding| &binding.context)
                .collect::<BTreeSet<_>>();
            for context in contexts {
                if !same_context_lifetime(
                    observed.contexts.get(context),
                    action_map.revision.contexts.get(context),
                ) {
                    Arc::make_mut(&mut self.current)
                        .contexts
                        .insert(context.clone(), Arc::new(()));
                }
            }
        }
        self.observed = Some(Arc::clone(&action_map.revision));
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
