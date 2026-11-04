use std::sync::Arc;

use nara_render::RenderTarget;
use nara_text::FontFace;
use nara_ui_kernel::{
    __private::{UiFrameJournalAuthority, new_hosted_frame_journal},
    UiFrame, UiFrameJournal, UiJournalError, UiLimits, UiPresentationReceipt, UiSurfaceId,
    UiSurfaceRetirementToken,
};
use thiserror::Error;

use crate::{
    UiBackendSubmissionIdentity, UiPaintPacket, UiPaintPacketIdentity, UiPreparedImages,
    UiPresentationAttemptId, UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
    UiPresentationCapabilityGeneration, UiPresentationContentError, UiPresentationEpoch,
    UiPresentationIntegrationError, UiPresentationLimits, UiPresentationOutcome,
    UiPresentationPlanner, UiPresentationPlannerCreateError, UiPresentationRuntimeGeneration,
    UiPresentationWorkStats,
    integration::{
        UiBackendPresentationFrame, UiBackendSubmission, UiCompletedBackendOutcome,
        UiPresentationOutcomes,
    },
    packet::{UiDeferredBackendPresentation, UiPreparedBackendSubmission},
};

/// Settled observations from one surface's original backend outcome owner.
#[derive(Debug)]
pub enum UiSurfacePresentationEvent {
    Presented(UiPresentationReceipt),
    Deferred {
        attempt: UiPresentationAttemptIdentity,
        message: String,
    },
    Terminal(UiPresentationOutcome),
    Discarded {
        attempt: Option<UiPresentationAttemptIdentity>,
    },
}

#[derive(Debug, Error)]
pub enum UiSurfacePresentationError {
    #[error(transparent)]
    Journal(#[from] UiJournalError),
    #[error(transparent)]
    PlannerCreate(#[from] UiPresentationPlannerCreateError),
    #[error(transparent)]
    Content(#[from] UiPresentationContentError),
    #[error(transparent)]
    Integration(#[from] UiPresentationIntegrationError),
    #[error("UI frame surface {actual:?} does not match presenter surface {expected:?}")]
    WrongSurface {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error("UI frame slot generation {actual} does not match presenter slot {expected}")]
    WrongSlot { expected: u64, actual: u64 },
    #[error("the UI surface still owns an unsettled presentation candidate or backend result")]
    Busy,
    #[error("the UI surface presenter has closed admission")]
    Closed,
    #[error("the UI surface presentation identity was exhausted")]
    IdentityExhausted,
    #[error("the UI surface has no deferred presentation to retry")]
    NoRetry,
    #[error("the UI surface outcome does not match its exact staged presentation")]
    ExactMismatch,
}

/// Finite presentation adapter for one immutable surface, slot and render target lifetime.
///
/// A backend frame carries the existing move-only submission and outcome permit. Return its
/// completion with [`Self::return_outcome`], then call [`Self::drain`] to publish accepted input
/// geometry. The journal remains at its last accepted presentation throughout preparation and
/// deferral. Retire this presenter before releasing its target, and drain it to quiescence.
///
/// Publication installs the exact accepted frame and its routing receipt. The staged journal
/// contains frame routing history only. Interaction sessions remain owned by the frontend and
/// are never cloned or restored by this presenter, so publication preserves interaction changes
/// made while a backend attempt was outstanding.
///
/// The backend is a trusted integration in the same process. It must issue
/// [`presented`](crate::advanced::UiSubmittedBackendSubmission::presented) only after actual
/// presentation. Frontends receive read-only journal access; the adapter retains its exclusive
/// submission authority and stages receipts internally.
pub struct UiSurfacePresenter {
    surface: UiSurfaceId,
    slot_generation: u64,
    target: RenderTarget,
    planner: UiPresentationPlanner,
    journal: UiFrameJournal,
    authority: UiFrameJournalAuthority,
    outcomes: UiPresentationOutcomes,
    sequence: u64,
    candidate: Option<Candidate>,
    closing: bool,
}

struct Candidate {
    identity: UiBackendSubmissionIdentity,
    packet: Arc<UiPaintPacket>,
    journal: UiFrameJournal,
    receipt: UiPresentationReceipt,
    backend: CandidateBackend,
}

enum CandidateBackend {
    Pending,
    Deferred(UiDeferredBackendPresentation),
    Ready(UiPreparedBackendSubmission),
}

impl UiSurfacePresenter {
    /// Creates a presenter for one Host-allocated surface lifetime.
    ///
    /// The Host must never reuse `surface` across recreated presenters, including replacements
    /// for the same slot or render target. Surface identity allocation and non-reuse belong to
    /// the Host; this adapter keeps no global identity registry.
    pub fn with_fonts(
        surface: UiSurfaceId,
        slot_generation: u64,
        target: RenderTarget,
        fonts: Vec<FontFace>,
        limits: UiLimits,
        presentation_limits: UiPresentationLimits,
    ) -> Result<Self, UiSurfacePresentationError> {
        let (mut journal, authority) = new_hosted_frame_journal(limits)?;
        journal.install_surface(surface, slot_generation)?;
        Ok(Self {
            surface,
            slot_generation,
            target,
            planner: UiPresentationPlanner::with_fonts(fonts, presentation_limits)?,
            journal,
            authority,
            outcomes: UiPresentationOutcomes::default(),
            sequence: 0,
            candidate: None,
            closing: false,
        })
    }

    /// Read-only routing authority for frames whose original backend acceptance was drained.
    #[must_use]
    pub const fn journal(&self) -> &UiFrameJournal {
        &self.journal
    }

    /// Cumulative work observations from the original presentation planner.
    #[must_use]
    pub fn work_stats(&self) -> UiPresentationWorkStats {
        self.planner.work_stats()
    }

    /// Stages one exact journal/receipt pair before its backend submission can leave the owner.
    pub fn prepare(
        &mut self,
        frame: Arc<UiFrame>,
        images: &UiPreparedImages,
    ) -> Result<UiBackendPresentationFrame, UiSurfacePresentationError> {
        self.check_open()?;
        if !self.is_quiescent() {
            return Err(UiSurfacePresentationError::Busy);
        }
        let environment = frame.environment();
        if environment.surface_id() != self.surface {
            return Err(UiSurfacePresentationError::WrongSurface {
                expected: self.surface,
                actual: environment.surface_id(),
            });
        }
        if environment.slot_generation().get() != self.slot_generation {
            return Err(UiSurfacePresentationError::WrongSlot {
                expected: self.slot_generation,
                actual: environment.slot_generation().get(),
            });
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(UiSurfacePresentationError::IdentityExhausted)?;
        let admission = UiPresentationAttemptId::new(sequence)
            .ok_or(UiSurfacePresentationError::IdentityExhausted)?;
        let epoch = UiPresentationEpoch::new(sequence)
            .ok_or(UiSurfacePresentationError::IdentityExhausted)?;
        let runtime = UiPresentationRuntimeGeneration::new(self.slot_generation)
            .ok_or(UiSurfacePresentationError::IdentityExhausted)?;
        let capability = UiPresentationCapabilityGeneration::new(self.slot_generation)
            .ok_or(UiSurfacePresentationError::IdentityExhausted)?;
        let mut journal = self.journal.clone();
        let receipt = self
            .authority
            .submit(&mut journal, Arc::clone(&frame), sequence)?;
        let packet = self.planner.plan(Arc::clone(&frame), self.target, images)?;
        if !Arc::ptr_eq(&packet.frame(), &frame)
            || packet.identity() != UiPaintPacketIdentity::from_frame(&frame)
            || packet.target() != self.target
        {
            let _ = self.planner.abort_surface_target(self.surface, self.target);
            return Err(UiSurfacePresentationError::ExactMismatch);
        }
        let prepared = UiPreparedBackendSubmission::new(
            admission,
            Arc::clone(&packet),
            epoch,
            runtime,
            capability,
        );
        self.sequence = sequence;
        self.candidate = Some(Candidate {
            identity: prepared.identity(),
            packet,
            journal,
            receipt,
            backend: CandidateBackend::Ready(prepared),
        });
        let result = self.submit_candidate();
        if result.is_err() {
            self.abort_candidate();
        }
        result
    }

    /// Returns only original permits to this presenter, including after logical close.
    pub fn return_outcome(
        &mut self,
        completed: UiCompletedBackendOutcome,
    ) -> Result<(), UiCompletedBackendOutcome> {
        self.outcomes.try_commit_backend(completed)
    }

    /// Consumes original one-shot acceptance before installing the complete staged journal.
    pub fn drain(&mut self) -> Result<Vec<UiSurfacePresentationEvent>, UiSurfacePresentationError> {
        let outcomes = self.outcomes.drain()?;
        let mut events = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            let attempt = outcome.attempt();
            if self.closing {
                self.abort_candidate();
                events.push(UiSurfacePresentationEvent::Discarded { attempt });
                continue;
            }
            let Some(mut candidate) = self.candidate.take() else {
                return Err(UiSurfacePresentationError::ExactMismatch);
            };
            let matches = attempt
                == Some(UiPresentationAttemptIdentity::backend(candidate.identity))
                && matches!(&candidate.backend, CandidateBackend::Pending)
                && candidate.identity.matches_receipt(candidate.receipt)
                && match &outcome {
                    UiPresentationOutcome::Presented(commit) => {
                        Arc::ptr_eq(commit.packet(), &candidate.packet)
                    }
                    UiPresentationOutcome::Deferred { retry, .. } => {
                        Arc::ptr_eq(retry.packet(), &candidate.packet)
                    }
                    _ => true,
                };
            if !matches {
                let _ = self
                    .planner
                    .abort_planned(UiPresentationAttemptIdentity::backend(candidate.identity));
                return Err(UiSurfacePresentationError::ExactMismatch);
            }
            match outcome {
                UiPresentationOutcome::Presented(commit) => {
                    let accepted = commit.accept();
                    self.planner
                        .commit_presented(accepted.identity(), Arc::clone(accepted.packet()));
                    self.journal = candidate.journal;
                    events.push(UiSurfacePresentationEvent::Presented(candidate.receipt));
                }
                UiPresentationOutcome::Deferred { retry, message } => {
                    let attempt = retry.attempt();
                    candidate.backend = CandidateBackend::Deferred(retry);
                    self.candidate = Some(candidate);
                    events.push(UiSurfacePresentationEvent::Deferred { attempt, message });
                }
                terminal => {
                    let _ = self
                        .planner
                        .abort_planned(UiPresentationAttemptIdentity::backend(candidate.identity));
                    if matches!(
                        terminal.fault_domain(),
                        Some(
                            UiPresentationBackendFaultDomain::Target
                                | UiPresentationBackendFaultDomain::Device
                        )
                    ) {
                        self.authority.invalidate_surface_presentation(
                            &mut self.journal,
                            self.surface,
                            self.slot_generation,
                        )?;
                        self.closing = true;
                        self.outcomes.begin_close();
                    }
                    events.push(UiSurfacePresentationEvent::Terminal(terminal));
                }
            }
        }
        self.retire_settled_target();
        Ok(events)
    }

    /// Retries the original deferred attempt, including its required backend generation.
    pub fn retry(&mut self) -> Result<UiBackendPresentationFrame, UiSurfacePresentationError> {
        self.check_open()?;
        if self.outcomes.has_unsettled_backend_results() {
            return Err(UiSurfacePresentationError::Busy);
        }
        self.submit_candidate()
    }

    /// Immediately revokes input routes while retaining every outstanding original permit.
    pub fn retire(
        &mut self,
        retirement_generation: u64,
    ) -> Result<Option<UiSurfaceRetirementToken>, UiSurfacePresentationError> {
        let token = self
            .journal
            .retire_surface(self.surface, retirement_generation)?;
        self.closing = true;
        self.outcomes.begin_close();
        if self
            .candidate
            .as_ref()
            .is_some_and(|candidate| !matches!(&candidate.backend, CandidateBackend::Pending))
        {
            self.abort_candidate();
        }
        self.retire_settled_target();
        Ok(token)
    }

    /// Includes unreturned permits, buffered results and retained deferred candidates.
    #[must_use]
    pub fn is_quiescent(&self) -> bool {
        self.candidate.is_none() && !self.outcomes.has_unsettled_backend_results()
    }

    fn check_open(&self) -> Result<(), UiSurfacePresentationError> {
        if self.closing {
            Err(UiSurfacePresentationError::Closed)
        } else {
            Ok(())
        }
    }

    fn submit_candidate(
        &mut self,
    ) -> Result<UiBackendPresentationFrame, UiSurfacePresentationError> {
        let mut candidate = self
            .candidate
            .take()
            .ok_or(UiSurfacePresentationError::NoRetry)?;
        let prepared = match std::mem::replace(&mut candidate.backend, CandidateBackend::Pending) {
            CandidateBackend::Deferred(retry) => retry.into_retry(),
            CandidateBackend::Ready(prepared) => prepared,
            CandidateBackend::Pending => {
                self.candidate = Some(candidate);
                return Err(UiSurfacePresentationError::NoRetry);
            }
        };
        if self
            .planner
            .replace_backend_submissions(std::slice::from_ref(&prepared), &[])
            .is_err()
        {
            let _ = self
                .planner
                .abort_planned(UiPresentationAttemptIdentity::backend(candidate.identity));
            return Err(UiSurfacePresentationError::ExactMismatch);
        }
        let mut permits = match self
            .outcomes
            .reserve_backend(std::slice::from_ref(&prepared))
        {
            Ok(permits) => permits,
            Err(error) => {
                candidate.backend = CandidateBackend::Ready(prepared);
                self.candidate = Some(candidate);
                return Err(error.into());
            }
        };
        self.candidate = Some(candidate);
        let permit = permits
            .pop()
            .ok_or(UiSurfacePresentationError::ExactMismatch)?;
        let packet = Arc::clone(prepared.packet());
        let submission = UiBackendSubmission::pair(prepared, permit)?;
        Ok(UiBackendPresentationFrame::from_parts(
            vec![packet],
            vec![submission],
        ))
    }

    fn abort_candidate(&mut self) {
        if let Some(candidate) = self.candidate.take() {
            let _ = self
                .planner
                .abort_planned(UiPresentationAttemptIdentity::backend(candidate.identity));
        }
    }

    fn retire_settled_target(&mut self) {
        if self.closing && self.is_quiescent() {
            let _ = self
                .planner
                .retire_surface_target(self.surface, self.target);
        }
    }
}
