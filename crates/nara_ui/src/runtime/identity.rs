use std::collections::BTreeMap;

use nara_ui_kernel::{UiSurfaceId, UiSurfaceRetirementToken};

use super::UiAdapterError;

const MAX_RETIRED_SURFACE_IDENTITIES: usize = 4_096;

/// Bounded non-reuse authority for Host-allocated UI surface lifetimes.
#[derive(Debug, Default)]
pub(super) struct UiSurfaceIdentityLedger {
    retired: BTreeMap<UiSurfaceId, UiSurfaceRetirementToken>,
}

impl UiSurfaceIdentityLedger {
    pub(super) fn validate_new(
        &self,
        surface_id: UiSurfaceId,
        active_surfaces: usize,
    ) -> Result<(), UiAdapterError> {
        if self.retired.contains_key(&surface_id) {
            return Err(UiAdapterError::RetiredSurface {
                surface: surface_id.get(),
            });
        }
        if self.retired.len().saturating_add(active_surfaces) >= MAX_RETIRED_SURFACE_IDENTITIES {
            return Err(UiAdapterError::SurfaceIdentitySpaceExhausted);
        }
        Ok(())
    }

    pub(super) fn commit_retirement(&mut self, retirement: UiSurfaceRetirementToken) {
        let surface_id = retirement.surface_id();
        debug_assert!(
            self.retired.len() < MAX_RETIRED_SURFACE_IDENTITIES,
            "surface installation reserves one exact retirement record"
        );
        let replaced = self.retired.insert(surface_id, retirement);
        debug_assert!(
            replaced.is_none(),
            "a surface lifetime retires exactly once"
        );
    }

    pub(super) fn is_retired(&self, surface_id: UiSurfaceId) -> bool {
        self.retired.contains_key(&surface_id)
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
            ledger.commit_retirement(retirement(surface));
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
        assert!(matches!(
            ledger.validate_new(overflow, 0),
            Err(UiAdapterError::SurfaceIdentitySpaceExhausted)
        ));
        assert!(!ledger.is_retired(overflow));
        assert_eq!(ledger.retirement(overflow), None);
        assert_eq!(ledger.retired.len(), MAX_RETIRED_SURFACE_IDENTITIES);
    }

    #[test]
    fn installation_reserves_exact_retirement_evidence_for_every_active_surface() {
        let mut ledger = UiSurfaceIdentityLedger::default();
        for value in 1..MAX_RETIRED_SURFACE_IDENTITIES as u64 {
            let surface = UiSurfaceId::new(value).unwrap();
            ledger.commit_retirement(retirement(surface));
        }

        let reserved = UiSurfaceId::new(MAX_RETIRED_SURFACE_IDENTITIES as u64).unwrap();
        ledger.validate_new(reserved, 0).unwrap();
        let unreserved = UiSurfaceId::new(MAX_RETIRED_SURFACE_IDENTITIES as u64 + 1).unwrap();
        assert!(matches!(
            ledger.validate_new(unreserved, 1),
            Err(UiAdapterError::SurfaceIdentitySpaceExhausted)
        ));

        ledger.commit_retirement(retirement(reserved));
        assert_eq!(ledger.retirement(reserved), Some(retirement(reserved)));
    }
}
