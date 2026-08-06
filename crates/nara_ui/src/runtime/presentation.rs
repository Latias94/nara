use std::{collections::BTreeSet, collections::VecDeque, sync::Arc};

use nara_ui_kernel::{
    UiFrameJournal, UiJournalError, UiKey, UiLayer, UiLimits, UiPresentationReceipt, UiRootStatus,
    UiRouteStamp, UiRouteTarget,
};

use super::candidate::{UiBehaviorSnapshot, UiCompiledCandidate};
use super::surface::UiSurfaceDescriptor;

#[derive(Clone, Debug)]
struct PresentedEntry {
    target: UiRouteTarget,
    behavior: Arc<UiBehaviorSnapshot>,
    descriptor: UiSurfaceDescriptor,
}

/// Exact frame, receipt, and behavior authority selected by one event-time route.
#[derive(Clone, Debug)]
pub(crate) struct UiPresentedSession {
    target: UiRouteTarget,
    latest: UiRouteTarget,
    behavior: Arc<UiBehaviorSnapshot>,
    descriptor: UiSurfaceDescriptor,
}

impl UiPresentedSession {
    pub(crate) const fn target(&self) -> &UiRouteTarget {
        &self.target
    }

    pub(crate) const fn latest(&self) -> &UiRouteTarget {
        &self.latest
    }

    pub(crate) fn behavior(&self) -> &Arc<UiBehaviorSnapshot> {
        &self.behavior
    }

    pub(crate) const fn descriptor(&self) -> UiSurfaceDescriptor {
        self.descriptor
    }

    pub(crate) fn blocks_unhandled_input(&self) -> bool {
        let frame = self.target.frame();
        frame.layouts().iter().rev().any(|layout| {
            layout.layer() == UiLayer::Modal
                && layout.visible()
                && frame.root_outcomes().iter().any(|outcome| {
                    outcome.key() == layout.root_key()
                        && outcome.status() == UiRootStatus::Published
                })
        })
    }
}

/// Per-surface presented authority retaining the current and two prior exact sessions.
#[derive(Clone, Debug)]
pub(super) struct UiPresentedHistory {
    journal: UiFrameJournal,
    entries: VecDeque<PresentedEntry>,
}

impl UiPresentedHistory {
    pub(super) fn with_limits(limits: UiLimits) -> Result<Self, UiJournalError> {
        Ok(Self {
            journal: UiFrameJournal::with_limits(limits)?,
            entries: VecDeque::new(),
        })
    }

    pub(super) fn submit(
        &mut self,
        candidate: &UiCompiledCandidate,
        descriptor: UiSurfaceDescriptor,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, UiJournalError> {
        let receipt = self
            .journal
            .submit(Arc::clone(candidate.frame()), presentation_epoch)?;
        let target = self.journal.resolve(UiRouteStamp::new(
            candidate.frame().environment().surface_id(),
            presentation_epoch,
        ))?;
        self.entries.push_front(PresentedEntry {
            target,
            behavior: Arc::clone(candidate.behavior()),
            descriptor,
        });
        self.entries.truncate(3);
        Ok(receipt)
    }

    pub(super) fn resolve(
        &self,
        route: UiRouteStamp,
    ) -> Result<Option<UiPresentedSession>, UiJournalError> {
        let target = self.journal.resolve(route)?;
        let latest = self.journal.latest(route.surface_id())?;
        let presented = self
            .entries
            .iter()
            .find(|entry| {
                entry.target.receipt() == target.receipt()
                    && Arc::ptr_eq(entry.target.frame(), target.frame())
            })
            .map(|entry| UiPresentedSession {
                target,
                latest,
                behavior: Arc::clone(&entry.behavior),
                descriptor: entry.descriptor,
            });
        Ok(presented)
    }

    pub(super) const fn journal(&self) -> &UiFrameJournal {
        &self.journal
    }

    pub(super) fn retained_button_keys(&self) -> BTreeSet<UiKey> {
        self.entries
            .iter()
            .flat_map(|entry| entry.behavior.keys().cloned())
            .collect()
    }
}
