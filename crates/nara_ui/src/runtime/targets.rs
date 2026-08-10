use std::collections::BTreeMap;

use nara_input::InputWindowId;
use nara_ui_kernel::UiSurfaceId;

use super::{UiAdapterError, surface::UiSurfaceSlot};

/// Private Host-side ownership index for the current one-surface-per-window baseline.
#[derive(Default)]
pub(super) struct UiSurfaceTargetIndex {
    owners: BTreeMap<InputWindowId, UiSurfaceId>,
}

pub(super) struct UiPreparedTargetBinding {
    surface_id: UiSurfaceId,
    previous: Option<InputWindowId>,
    next: InputWindowId,
}

pub(super) struct UiPreparedTargetRelease {
    surface_id: UiSurfaceId,
    target: InputWindowId,
}

impl UiSurfaceTargetIndex {
    pub(super) fn prepare_install(
        &self,
        slot: UiSurfaceSlot,
    ) -> Result<UiPreparedTargetBinding, UiAdapterError> {
        self.prepare_binding(slot.surface_id(), None, slot.endpoint().window())
    }

    pub(super) fn prepare_transition(
        &self,
        previous: UiSurfaceSlot,
        next: UiSurfaceSlot,
    ) -> Result<UiPreparedTargetBinding, UiAdapterError> {
        let previous_target = previous.endpoint().window();
        self.validate_owner(previous.surface_id(), previous_target)?;
        self.prepare_binding(
            previous.surface_id(),
            Some(previous_target),
            next.endpoint().window(),
        )
    }

    pub(super) fn prepare_release(
        &self,
        slot: UiSurfaceSlot,
    ) -> Result<UiPreparedTargetRelease, UiAdapterError> {
        let target = slot.endpoint().window();
        self.validate_owner(slot.surface_id(), target)?;
        Ok(UiPreparedTargetRelease {
            surface_id: slot.surface_id(),
            target,
        })
    }

    pub(super) fn commit_binding(&mut self, binding: UiPreparedTargetBinding) {
        if binding.previous == Some(binding.next) {
            debug_assert_eq!(
                self.owners.get(&binding.next).copied(),
                Some(binding.surface_id)
            );
            return;
        }
        if let Some(previous) = binding.previous
            && previous != binding.next
        {
            let removed = self.owners.remove(&previous);
            debug_assert_eq!(removed, Some(binding.surface_id));
        }
        let replaced = self.owners.insert(binding.next, binding.surface_id);
        debug_assert!(replaced.is_none() || replaced == Some(binding.surface_id));
    }

    pub(super) fn commit_release(&mut self, release: UiPreparedTargetRelease) {
        let removed = self.owners.remove(&release.target);
        debug_assert_eq!(removed, Some(release.surface_id));
    }

    fn prepare_binding(
        &self,
        surface_id: UiSurfaceId,
        previous: Option<InputWindowId>,
        next: InputWindowId,
    ) -> Result<UiPreparedTargetBinding, UiAdapterError> {
        if let Some(owner) = self.owners.get(&next).copied()
            && owner != surface_id
        {
            return Err(UiAdapterError::PresentationTargetConflict {
                window: next.get(),
                surface: owner.get(),
            });
        }
        Ok(UiPreparedTargetBinding {
            surface_id,
            previous,
            next,
        })
    }

    fn validate_owner(
        &self,
        surface_id: UiSurfaceId,
        target: InputWindowId,
    ) -> Result<(), UiAdapterError> {
        let actual = self.owners.get(&target).copied();
        if actual == Some(surface_id) {
            return Ok(());
        }
        Err(UiAdapterError::PresentationTargetOwnerMismatch {
            window: target.get(),
            expected_surface: surface_id.get(),
            actual_surface: actual.map(UiSurfaceId::get),
        })
    }
}
