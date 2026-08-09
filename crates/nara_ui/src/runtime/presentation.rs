use std::{collections::VecDeque, sync::Arc};

use super::candidate::{UiBehaviorSnapshot, UiCompiledCandidate};
use super::surface::UiSurfaceSlot;
use nara_ui_kernel::__private::UiFrameJournalAuthority;
use nara_ui_kernel::{
    UiFrameJournal, UiJournalError, UiKey, UiLimits, UiPresentationReceipt, UiRouteStamp,
    UiRouteTarget, UiSurfaceRetirementToken,
};

#[derive(Clone, Debug)]
struct PresentedEntry {
    target: UiRouteTarget,
    behavior: Arc<UiBehaviorSnapshot>,
    slot: UiSurfaceSlot,
}

/// Exact frame, receipt, and behavior authority selected by one event-time route.
#[derive(Clone, Debug)]
pub(crate) struct UiPresentedSession {
    target: UiRouteTarget,
    latest: UiRouteTarget,
    behavior: Arc<UiBehaviorSnapshot>,
    latest_behavior: Arc<UiBehaviorSnapshot>,
    slot: UiSurfaceSlot,
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

    pub(crate) const fn slot(&self) -> UiSurfaceSlot {
        self.slot
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
    pub(super) fn with_limits(
        limits: UiLimits,
        slot: UiSurfaceSlot,
    ) -> Result<(Self, UiFrameJournalAuthority), UiJournalError> {
        let (mut journal, authority) = nara_ui_kernel::__private::new_hosted_frame_journal(limits)?;
        journal.install_surface(slot.surface_id(), slot.slot_generation())?;
        Ok((
            Self {
                journal,
                entries: VecDeque::new(),
            },
            authority,
        ))
    }

    pub(super) fn submit(
        &mut self,
        authority: &UiFrameJournalAuthority,
        candidate: &UiCompiledCandidate,
        slot: UiSurfaceSlot,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, UiJournalError> {
        let receipt = authority.submit(
            &mut self.journal,
            Arc::clone(candidate.frame()),
            presentation_epoch,
        )?;
        let target = self.journal.resolve(UiRouteStamp::from_receipt(receipt))?;
        self.entries.push_front(PresentedEntry {
            target,
            behavior: Arc::clone(candidate.behavior()),
            slot,
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
                slot: entry.slot,
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

    pub(super) fn latest_session(&self) -> Result<Option<UiPresentedSession>, UiJournalError> {
        let Some(entry) = self.entries.front() else {
            return Ok(None);
        };
        self.resolve(UiRouteStamp::from_receipt(entry.target.receipt()))
    }

    pub(super) fn advance_slot(
        &mut self,
        previous: UiSurfaceSlot,
        next: UiSurfaceSlot,
    ) -> Result<(), UiJournalError> {
        self.journal.advance_surface_slot(
            previous.surface_id(),
            previous.slot_generation(),
            next.slot_generation(),
        )?;
        self.entries.clear();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn receipt(&self, presentation_epoch: u64) -> Option<UiPresentationReceipt> {
        self.entries
            .iter()
            .map(|entry| entry.target.receipt())
            .find(|receipt| receipt.presentation_epoch() == presentation_epoch)
    }

    pub(super) fn retire(
        &mut self,
        surface_id: nara_ui_kernel::UiSurfaceId,
        retirement_generation: u64,
    ) -> Result<Option<UiSurfaceRetirementToken>, UiJournalError> {
        self.entries.clear();
        self.journal
            .retire_surface(surface_id, retirement_generation)
    }
}

fn frame_blocks_unhandled_input(frame: &nara_ui_kernel::UiFrame) -> bool {
    nara_ui_kernel::__private::frame_blocks_unhandled_input(frame)
}
