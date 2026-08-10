use std::{collections::BTreeMap, sync::Arc};

use nara_ui_kernel::{UiFrame, UiInteractionSession, UiKey};
use nara_ui_widgets::{UiButton, UiButtonState};
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(super) enum UiBehaviorSnapshotError {
    #[error("actionable semantic node {key:?} has no exact controlled Button owner")]
    ActionOwnerMissing { key: UiKey },
}

/// Immutable behavior authority compiled alongside one immutable frame candidate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UiBehaviorSnapshot {
    buttons: BTreeMap<UiKey, UiButton>,
}

impl UiBehaviorSnapshot {
    pub(super) fn from_compiled_frame(
        frame: &UiFrame,
        mut buttons: BTreeMap<UiKey, UiButton>,
    ) -> Result<Self, UiBehaviorSnapshotError> {
        for semantic in frame
            .semantics()
            .iter()
            .filter(|semantic| !semantic.actions().is_empty())
        {
            let owned = buttons.get(semantic.key()).is_some_and(|button| {
                semantic.actions().len() == 1 && &semantic.actions()[0] == button.action()
            });
            if !owned {
                return Err(UiBehaviorSnapshotError::ActionOwnerMissing {
                    key: semantic.key().clone(),
                });
            }
        }
        buttons.retain(|key, button| {
            frame.semantics().iter().any(|semantic| {
                semantic.key() == key
                    && semantic.actions().len() == 1
                    && &semantic.actions()[0] == button.action()
            })
        });
        Ok(Self { buttons })
    }

    pub(crate) fn button(&self, key: &UiKey) -> Option<&UiButton> {
        self.buttons.get(key)
    }

    pub(crate) fn pointer_invalidations(&self, next: &Self) -> Vec<UiKey> {
        self.buttons
            .iter()
            .filter(|(key, previous)| {
                next.buttons.get(*key).is_none_or(|next| {
                    previous.action() != next.action() || previous.enabled() != next.enabled()
                })
            })
            .map(|(key, _)| key.clone())
            .collect()
    }
}

/// One atomically replaceable frame-and-behavior candidate for a surface.
#[derive(Clone, Debug)]
pub(super) struct UiCompiledCandidate {
    frame: Arc<UiFrame>,
    behavior: Arc<UiBehaviorSnapshot>,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
}

/// Lightweight immutable facts retained across one backend deferral.
#[derive(Clone, Debug)]
pub(super) struct UiPresentationFacts {
    frame: Arc<UiFrame>,
    behavior: Arc<UiBehaviorSnapshot>,
}

impl UiPresentationFacts {
    pub(super) fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    pub(super) fn matches_candidate(&self, candidate: &UiCompiledCandidate) -> bool {
        self.behavior.as_ref() == candidate.behavior.as_ref()
            && frame_presentation_facts_match(self.frame.as_ref(), candidate.frame.as_ref())
    }
}

impl UiCompiledCandidate {
    pub(super) const fn new(
        frame: Arc<UiFrame>,
        behavior: Arc<UiBehaviorSnapshot>,
        interaction: UiInteractionSession,
        reducers: BTreeMap<UiKey, UiButtonState>,
    ) -> Self {
        Self {
            frame,
            behavior,
            interaction,
            reducers,
        }
    }

    pub(super) fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    pub(super) fn behavior(&self) -> &Arc<UiBehaviorSnapshot> {
        &self.behavior
    }

    pub(super) const fn interaction(&self) -> &UiInteractionSession {
        &self.interaction
    }

    pub(super) const fn reducers(&self) -> &BTreeMap<UiKey, UiButtonState> {
        &self.reducers
    }

    pub(super) fn presentation_facts(&self) -> UiPresentationFacts {
        UiPresentationFacts {
            frame: Arc::clone(&self.frame),
            behavior: Arc::clone(&self.behavior),
        }
    }

    pub(super) fn rebind_presentation_identity(&self, retained: &UiPresentationFacts) -> Self {
        debug_assert!(retained.matches_candidate(self));
        Self::new(
            Arc::clone(&retained.frame),
            Arc::clone(&retained.behavior),
            self.interaction.clone(),
            self.reducers.clone(),
        )
    }
}

fn frame_presentation_facts_match(previous: &UiFrame, latest: &UiFrame) -> bool {
    previous.environment() == latest.environment()
        && previous.root_outcomes() == latest.root_outcomes()
        && previous.layouts() == latest.layouts()
        && previous.paint() == latest.paint()
        && previous.hit_regions() == latest.hit_regions()
        && previous.focus_order() == latest.focus_order()
        && previous.semantics() == latest.semantics()
        && previous.diagnostics() == latest.diagnostics()
}

/// Interaction and reducer facts staged while a candidate is reprojected for exact visual parity.
pub(super) struct UiCandidateStaging {
    probe: Arc<UiFrame>,
    behavior: Arc<UiBehaviorSnapshot>,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
}

impl UiCandidateStaging {
    pub(super) const fn new(
        probe: Arc<UiFrame>,
        behavior: Arc<UiBehaviorSnapshot>,
        interaction: UiInteractionSession,
        reducers: BTreeMap<UiKey, UiButtonState>,
    ) -> Self {
        Self {
            probe,
            behavior,
            interaction,
            reducers,
        }
    }

    pub(super) const fn probe(&self) -> &Arc<UiFrame> {
        &self.probe
    }

    pub(super) const fn behavior(&self) -> &Arc<UiBehaviorSnapshot> {
        &self.behavior
    }

    pub(super) const fn reducers(&self) -> &BTreeMap<UiKey, UiButtonState> {
        &self.reducers
    }

    pub(super) fn requires_reprojection(&self, presented: &BTreeMap<UiKey, UiButtonState>) -> bool {
        self.behavior.buttons.keys().any(|key| {
            presented.get(key).copied().unwrap_or_default()
                != self.reducers.get(key).copied().unwrap_or_default()
        })
    }

    pub(super) fn into_candidate(self) -> UiCompiledCandidate {
        UiCompiledCandidate::new(self.probe, self.behavior, self.interaction, self.reducers)
    }

    pub(super) fn into_parts(self) -> (UiInteractionSession, BTreeMap<UiKey, UiButtonState>) {
        (self.interaction, self.reducers)
    }
}
