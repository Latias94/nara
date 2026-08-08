use std::{collections::VecDeque, sync::Arc};

use super::candidate::{UiBehaviorSnapshot, UiCompiledCandidate};
use super::surface::UiSurfaceDescriptor;
use nara_ui_kernel::{
    UiFrameJournal, UiJournalError, UiKey, UiLimits, UiPresentationReceipt, UiRouteStamp,
    UiRouteTarget,
};

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
    latest_behavior: Arc<UiBehaviorSnapshot>,
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

    pub(crate) fn latest_behavior(&self) -> &Arc<UiBehaviorSnapshot> {
        &self.latest_behavior
    }

    pub(crate) const fn descriptor(&self) -> UiSurfaceDescriptor {
        self.descriptor
    }

    pub(crate) fn blocks_unhandled_input(&self) -> bool {
        frame_blocks_unhandled_input(self.target.frame())
            || frame_blocks_unhandled_input(self.latest.frame())
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
        let presented = self.entries.iter().find(|entry| {
            entry.target.receipt() == target.receipt()
                && Arc::ptr_eq(entry.target.frame(), target.frame())
        });
        let latest_presented = self.entries.iter().find(|entry| {
            entry.target.receipt() == latest.receipt()
                && Arc::ptr_eq(entry.target.frame(), latest.frame())
        });
        let presented = presented
            .zip(latest_presented)
            .map(|(entry, latest_entry)| UiPresentedSession {
                target,
                latest,
                behavior: Arc::clone(&entry.behavior),
                latest_behavior: Arc::clone(&latest_entry.behavior),
                descriptor: entry.descriptor,
            });
        Ok(presented)
    }

    pub(super) const fn journal(&self) -> &UiFrameJournal {
        &self.journal
    }

    pub(super) fn behavior_invalidations(&self, next: &UiBehaviorSnapshot) -> Vec<UiKey> {
        self.entries.front().map_or_else(Vec::new, |latest| {
            latest.behavior.pointer_invalidations(next)
        })
    }

    pub(super) fn latest_target(&self) -> Option<&UiRouteTarget> {
        self.entries.front().map(|entry| &entry.target)
    }
}

fn frame_blocks_unhandled_input(frame: &nara_ui_kernel::UiFrame) -> bool {
    nara_ui_kernel::__private::frame_blocks_unhandled_input(frame)
}
