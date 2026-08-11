use std::collections::BTreeMap;

use nara_ui_kernel::{UiSurfaceId, UiSurfaceRetirementToken};

use super::UiAdapterError;

const MAX_RETIRED_SURFACE_IDENTITIES: usize = 4_096;

/// Bounded non-reuse authority for Host-allocated UI surface lifetimes.
#[derive(Debug, Default)]
pub(super) struct UiSurfaceIdentityLedger {
    retired: BTreeMap<UiSurfaceId, UiSurfaceRetirementToken>,
    exhausted: bool,
}

impl UiSurfaceIdentityLedger {
    pub(super) fn validate_new(&self, surface_id: UiSurfaceId) -> Result<(), UiAdapterError> {
        if self.exhausted {
            return Err(UiAdapterError::SurfaceIdentitySpaceExhausted);
        }
        if self.retired.contains_key(&surface_id) {
            return Err(UiAdapterError::RetiredSurface {
                surface: surface_id.get(),
            });
        }
        Ok(())
    }

    pub(super) fn retire(&mut self, retirement: UiSurfaceRetirementToken) {
        let surface_id = retirement.surface_id();
        if self.exhausted || self.retired.contains_key(&surface_id) {
            return;
        }
        if self.retired.len() == MAX_RETIRED_SURFACE_IDENTITIES {
            self.exhausted = true;
            return;
        }
        self.retired.insert(surface_id, retirement);
    }

    pub(super) fn is_retired(&self, surface_id: UiSurfaceId) -> bool {
        self.exhausted || self.retired.contains_key(&surface_id)
    }

    pub(super) fn retirement(&self, surface_id: UiSurfaceId) -> Option<UiSurfaceRetirementToken> {
        self.retired.get(&surface_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retirement(surface: UiSurfaceId) -> UiSurfaceRetirementToken {
        let mut journal = nara_ui_kernel::UiFrameJournal::default();
        journal.install_surface(surface, 1).unwrap();
        journal.retire_surface(surface, 2).unwrap().unwrap()
    }

    #[test]
    fn retirement_ceiling_fails_closed_without_unbounded_tombstones() {
        let mut ledger = UiSurfaceIdentityLedger::default();
        for value in 1..=MAX_RETIRED_SURFACE_IDENTITIES as u64 {
            let surface = UiSurfaceId::new(value).unwrap();
            ledger.retire(retirement(surface));
            assert!(ledger.is_retired(surface));
            assert_eq!(
                ledger
                    .retirement(surface)
                    .map(UiSurfaceRetirementToken::retirement_generation)
                    .map(nara_ui_kernel::UiRevision::get),
                Some(2)
            );
        }

        let overflow = UiSurfaceId::new(MAX_RETIRED_SURFACE_IDENTITIES as u64 + 1).unwrap();
        ledger.retire(retirement(overflow));
        assert!(ledger.is_retired(overflow));
        assert!(matches!(
            ledger.validate_new(UiSurfaceId::new(u64::MAX).unwrap()),
            Err(UiAdapterError::SurfaceIdentitySpaceExhausted)
        ));
        assert_eq!(ledger.retired.len(), MAX_RETIRED_SURFACE_IDENTITIES);
    }
}
