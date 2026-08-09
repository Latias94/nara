use std::{fmt, sync::Arc};

use nara_ecs::Resource;
use nara_ui::{UiSurfaceId, advanced::UiFrame};
use nara_ui_render::{
    UiBackendSubmissionIdentity, UiPaintPacket, UiPresentationAttemptId,
    UiPresentationBackendGeneration, UiPresentationCapabilityGeneration, UiPresentationEpoch,
    UiPresentationRuntimeGeneration,
};

const MAX_ACTIVE_PRESENTATION_ATTEMPTS: usize = 16;
const INITIAL_CAPABILITY_GENERATION: u64 = 1;

pub(super) struct UiPresentationAttemptReservation {
    pub(super) attempt_id: UiPresentationAttemptId,
    pub(super) presentation_epoch: UiPresentationEpoch,
    pub(super) runtime_generation: UiPresentationRuntimeGeneration,
    pub(super) capability_generation: UiPresentationCapabilityGeneration,
}

pub(super) enum UiPresentationAttempt {
    Prepared {
        identity: UiBackendSubmissionIdentity,
        authority: nara_ui::__private::UiPreparedPresentationAuthority,
    },
    Submitted {
        identity: UiBackendSubmissionIdentity,
        authority: nara_ui::__private::UiPreparedPresentationAuthority,
    },
    Deferred {
        identity: UiBackendSubmissionIdentity,
        ui: nara_ui::__private::UiDeferredPresentationAuthority,
        backend: nara_ui_render::__private::UiDeferredBackendPresentation,
    },
}

impl UiPresentationAttempt {
    pub(super) fn prepared(
        identity: UiBackendSubmissionIdentity,
        authority: nara_ui::__private::UiPreparedPresentationAuthority,
    ) -> Self {
        Self::Prepared {
            identity,
            authority,
        }
    }

    pub(super) fn deferred(
        identity: UiBackendSubmissionIdentity,
        ui: nara_ui::__private::UiDeferredPresentationAuthority,
        backend: nara_ui_render::__private::UiDeferredBackendPresentation,
    ) -> Self {
        Self::Deferred {
            identity,
            ui,
            backend,
        }
    }

    pub(super) const fn identity(&self) -> UiBackendSubmissionIdentity {
        match self {
            Self::Prepared { identity, .. }
            | Self::Submitted { identity, .. }
            | Self::Deferred { identity, .. } => *identity,
        }
    }

    pub(super) const fn attempt_id(&self) -> UiPresentationAttemptId {
        self.identity().admission_id()
    }

    pub(super) const fn surface_id(&self) -> UiSurfaceId {
        self.identity().packet().surface_id()
    }

    pub(super) const fn is_submitted(&self) -> bool {
        matches!(self, Self::Submitted { .. })
    }

    pub(super) const fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred { .. })
    }

    pub(super) fn deferred_backend_generation(&self) -> Option<UiPresentationBackendGeneration> {
        match self {
            Self::Deferred { backend, .. } => Some(backend.backend_generation()),
            _ => None,
        }
    }
}

pub(super) struct UiPresentationAttemptInsertError<T> {
    error: UiPresentationAttemptLedgerError,
    payload: Box<T>,
}

impl<T> UiPresentationAttemptInsertError<T> {
    fn new(error: UiPresentationAttemptLedgerError, payload: T) -> Self {
        Self {
            error,
            payload: Box::new(payload),
        }
    }

    pub(super) fn into_parts(self) -> (UiPresentationAttemptLedgerError, T) {
        (self.error, *self.payload)
    }
}

#[derive(Resource)]
pub(super) struct UiPresentationAttemptLedger {
    next_attempt_id: u64,
    next_presentation_epoch: u64,
    capability_generation: UiPresentationCapabilityGeneration,
    active: Vec<UiPresentationAttempt>,
}

impl Default for UiPresentationAttemptLedger {
    fn default() -> Self {
        Self {
            next_attempt_id: 1,
            next_presentation_epoch: 1,
            capability_generation: UiPresentationCapabilityGeneration::new(
                INITIAL_CAPABILITY_GENERATION,
            )
            .expect("the initial UI capability generation is non-zero"),
            active: Vec::with_capacity(MAX_ACTIVE_PRESENTATION_ATTEMPTS),
        }
    }
}

impl UiPresentationAttemptLedger {
    pub(super) fn reserve(
        &mut self,
        surface_id: UiSurfaceId,
        runtime_generation: UiPresentationRuntimeGeneration,
    ) -> Result<UiPresentationAttemptReservation, UiPresentationAttemptLedgerError> {
        if self.active.len() >= MAX_ACTIVE_PRESENTATION_ATTEMPTS {
            return Err(UiPresentationAttemptLedgerError::CapacityExceeded);
        }
        if self
            .active
            .iter()
            .any(|attempt| attempt.surface_id() == surface_id)
        {
            return Err(UiPresentationAttemptLedgerError::SurfaceAlreadyActive { surface_id });
        }
        let next_attempt_id = self
            .next_attempt_id
            .checked_add(1)
            .ok_or(UiPresentationAttemptLedgerError::AttemptIdentityExhausted)?;
        let next_presentation_epoch = self
            .next_presentation_epoch
            .checked_add(1)
            .ok_or(UiPresentationAttemptLedgerError::PresentationEpochExhausted)?;
        let attempt_id = UiPresentationAttemptId::new(self.next_attempt_id)
            .expect("the attempt ledger starts at a non-zero identity");
        let presentation_epoch = UiPresentationEpoch::new(self.next_presentation_epoch)
            .expect("the presentation ledger starts at a non-zero epoch");
        self.next_attempt_id = next_attempt_id;
        self.next_presentation_epoch = next_presentation_epoch;
        Ok(UiPresentationAttemptReservation {
            attempt_id,
            presentation_epoch,
            runtime_generation,
            capability_generation: self.capability_generation,
        })
    }

    fn preflight_insert(
        &self,
        identity: UiBackendSubmissionIdentity,
    ) -> Result<(), UiPresentationAttemptLedgerError> {
        if self.active.len() >= MAX_ACTIVE_PRESENTATION_ATTEMPTS {
            return Err(UiPresentationAttemptLedgerError::CapacityExceeded);
        }
        if self
            .active
            .iter()
            .any(|active| active.attempt_id() == identity.admission_id())
        {
            return Err(UiPresentationAttemptLedgerError::DuplicateAttempt {
                attempt_id: identity.admission_id(),
            });
        }
        if self
            .active
            .iter()
            .any(|active| active.surface_id() == identity.packet().surface_id())
        {
            return Err(UiPresentationAttemptLedgerError::SurfaceAlreadyActive {
                surface_id: identity.packet().surface_id(),
            });
        }
        Ok(())
    }

    pub(super) fn try_insert_prepared(
        &mut self,
        identity: UiBackendSubmissionIdentity,
        authority: nara_ui::__private::UiPreparedPresentationAuthority,
    ) -> Result<
        (),
        UiPresentationAttemptInsertError<nara_ui::__private::UiPreparedPresentationAuthority>,
    > {
        if let Err(error) = self.preflight_insert(identity) {
            return Err(UiPresentationAttemptInsertError::new(error, authority));
        }
        self.active
            .push(UiPresentationAttempt::prepared(identity, authority));
        Ok(())
    }

    pub(super) fn try_insert_deferred(
        &mut self,
        identity: UiBackendSubmissionIdentity,
        ui: nara_ui::__private::UiDeferredPresentationAuthority,
        backend: nara_ui_render::__private::UiDeferredBackendPresentation,
    ) -> Result<
        (),
        UiPresentationAttemptInsertError<(
            nara_ui::__private::UiDeferredPresentationAuthority,
            nara_ui_render::__private::UiDeferredBackendPresentation,
        )>,
    > {
        if let Err(error) = self.preflight_insert(identity) {
            return Err(UiPresentationAttemptInsertError::new(error, (ui, backend)));
        }
        self.active
            .push(UiPresentationAttempt::deferred(identity, ui, backend));
        Ok(())
    }

    pub(super) fn take(
        &mut self,
        attempt_id: UiPresentationAttemptId,
    ) -> Option<UiPresentationAttempt> {
        self.active
            .iter()
            .position(|attempt| attempt.attempt_id() == attempt_id)
            .map(|index| self.active.swap_remove(index))
    }

    pub(super) fn take_submitted(
        &mut self,
        identity: UiBackendSubmissionIdentity,
        packet: &Arc<UiPaintPacket>,
    ) -> Option<nara_ui::__private::UiPreparedPresentationAuthority> {
        let index = self.active.iter().position(|attempt| {
            attempt.identity() == identity
                && matches!(
                    attempt,
                    UiPresentationAttempt::Submitted { authority, .. }
                        if same_frame(authority.frame(), packet)
                )
        })?;
        match self.active.swap_remove(index) {
            UiPresentationAttempt::Submitted { authority, .. } => Some(authority),
            attempt => {
                self.active.push(attempt);
                None
            }
        }
    }

    pub(super) fn take_deferred(
        &mut self,
        attempt_id: UiPresentationAttemptId,
    ) -> Option<(
        UiBackendSubmissionIdentity,
        nara_ui::__private::UiDeferredPresentationAuthority,
        nara_ui_render::__private::UiDeferredBackendPresentation,
    )> {
        let index = self
            .active
            .iter()
            .position(|attempt| attempt.attempt_id() == attempt_id && attempt.is_deferred())?;
        match self.active.swap_remove(index) {
            UiPresentationAttempt::Deferred {
                identity,
                ui,
                backend,
            } => Some((identity, ui, backend)),
            attempt => {
                self.active.push(attempt);
                None
            }
        }
    }

    pub(super) fn mark_submitted(
        &mut self,
        attempt_id: UiPresentationAttemptId,
    ) -> Result<(), UiPresentationAttemptLedgerError> {
        let index = self
            .active
            .iter()
            .position(|attempt| attempt.attempt_id() == attempt_id)
            .ok_or(UiPresentationAttemptLedgerError::AttemptNotFound { attempt_id })?;
        let attempt = self.active.swap_remove(index);
        match attempt {
            UiPresentationAttempt::Prepared {
                identity,
                authority,
            } => {
                self.active.push(UiPresentationAttempt::Submitted {
                    identity,
                    authority,
                });
                Ok(())
            }
            attempt => {
                self.active.push(attempt);
                Err(UiPresentationAttemptLedgerError::InvalidTransition { attempt_id })
            }
        }
    }

    pub(super) fn submitted_ids(&self) -> Vec<UiPresentationAttemptId> {
        self.active
            .iter()
            .filter(|attempt| attempt.is_submitted())
            .map(UiPresentationAttempt::attempt_id)
            .collect()
    }

    pub(super) fn deferred_ids(&self) -> Vec<UiPresentationAttemptId> {
        self.active
            .iter()
            .filter(|attempt| attempt.is_deferred())
            .map(UiPresentationAttempt::attempt_id)
            .collect()
    }

    pub(super) fn deferred_ids_for_backend(
        &self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Vec<UiPresentationAttemptId> {
        self.active
            .iter()
            .filter(|attempt| attempt.deferred_backend_generation() == Some(backend_generation))
            .map(UiPresentationAttempt::attempt_id)
            .collect()
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

fn same_frame(frame: &Arc<UiFrame>, packet: &Arc<UiPaintPacket>) -> bool {
    Arc::ptr_eq(frame, &packet.frame())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UiPresentationAttemptLedgerError {
    CapacityExceeded,
    SurfaceAlreadyActive { surface_id: UiSurfaceId },
    DuplicateAttempt { attempt_id: UiPresentationAttemptId },
    AttemptIdentityExhausted,
    PresentationEpochExhausted,
    AttemptNotFound { attempt_id: UiPresentationAttemptId },
    InvalidTransition { attempt_id: UiPresentationAttemptId },
}

impl fmt::Display for UiPresentationAttemptLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded => write!(
                formatter,
                "UI presentation attempt capacity of {MAX_ACTIVE_PRESENTATION_ATTEMPTS} was exceeded"
            ),
            Self::SurfaceAlreadyActive { surface_id } => write!(
                formatter,
                "UI surface {} already owns an active presentation attempt",
                surface_id.get()
            ),
            Self::DuplicateAttempt { attempt_id } => write!(
                formatter,
                "UI presentation attempt {} is already active",
                attempt_id.get()
            ),
            Self::AttemptIdentityExhausted => {
                formatter.write_str("UI presentation attempt identity space was exhausted")
            }
            Self::PresentationEpochExhausted => {
                formatter.write_str("UI presentation epoch space was exhausted")
            }
            Self::AttemptNotFound { attempt_id } => write!(
                formatter,
                "UI presentation attempt {} was not found",
                attempt_id.get()
            ),
            Self::InvalidTransition { attempt_id } => write!(
                formatter,
                "UI presentation attempt {} was not prepared for backend submission",
                attempt_id.get()
            ),
        }
    }
}
