use std::collections::BTreeSet;

use nara_ui_kernel::UiSurfaceId;

use super::UiAdapterError;

const MAX_RETIRED_SURFACE_IDENTITIES: usize = 4_096;

/// Bounded non-reuse authority for Host-allocated UI surface lifetimes.
#[derive(Debug, Default)]
pub(super) struct UiSurfaceIdentityLedger {
    retired: BTreeSet<UiSurfaceId>,
    exhausted: bool,
}

impl UiSurfaceIdentityLedger {
    pub(super) fn validate_new(&self, surface_id: UiSurfaceId) -> Result<(), UiAdapterError> {
        if self.exhausted {
            return Err(UiAdapterError::SurfaceIdentitySpaceExhausted);
        }
        if self.retired.contains(&surface_id) {
            return Err(UiAdapterError::RetiredSurface {
                surface: surface_id.get(),
            });
        }
        Ok(())
    }

    pub(super) fn retire(&mut self, surface_id: UiSurfaceId) {
        if self.exhausted || self.retired.contains(&surface_id) {
            return;
        }
        if self.retired.len() == MAX_RETIRED_SURFACE_IDENTITIES {
            self.exhausted = true;
            return;
        }
        self.retired.insert(surface_id);
    }

    pub(super) fn is_retired(&self, surface_id: UiSurfaceId) -> bool {
        self.exhausted || self.retired.contains(&surface_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_ceiling_fails_closed_without_unbounded_tombstones() {
        let mut ledger = UiSurfaceIdentityLedger::default();
        for value in 1..=MAX_RETIRED_SURFACE_IDENTITIES as u64 {
            let surface = UiSurfaceId::new(value).unwrap();
            ledger.retire(surface);
            assert!(ledger.is_retired(surface));
        }

        let overflow = UiSurfaceId::new(MAX_RETIRED_SURFACE_IDENTITIES as u64 + 1).unwrap();
        ledger.retire(overflow);
        assert!(ledger.is_retired(overflow));
        assert!(matches!(
            ledger.validate_new(UiSurfaceId::new(u64::MAX).unwrap()),
            Err(UiAdapterError::SurfaceIdentitySpaceExhausted)
        ));
        assert_eq!(ledger.retired.len(), MAX_RETIRED_SURFACE_IDENTITIES);
    }
}
