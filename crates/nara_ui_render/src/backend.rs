use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use nara_render::RenderTarget;
use thiserror::Error;

use crate::packet::{
    UiAcceptedBackendPresentation, UiPreparedBackendSubmission, UiSubmittedBackendPresentation,
};
use crate::{
    OneShotPresentationCommit, UiBackendSubmissionIdentity, UiPaintPacket, UiPresentationAttemptId,
    UiPresentationAttemptIdentity, UiPresentationBackendGeneration, UiPresentationOutcome,
};

#[doc(hidden)]
#[derive(Debug, Default)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct UiPresentationOutcomes {
    outcomes: Vec<UiPresentationOutcome>,
    backend: Option<Arc<Mutex<BackendOutcomeBatch>>>,
    pub(super) receiver: UiBackendOutcomeReceiver,
}

#[derive(Debug)]
struct BackendOutcomeBatch {
    slots: Vec<BackendOutcomeSlot>,
    outcomes: Vec<UiPresentationOutcome>,
    pending: usize,
}

#[derive(Debug)]
struct BackendOutcomeSlot {
    identity: UiBackendSubmissionIdentity,
    returned: bool,
}

#[derive(Debug, Default)]
struct BackendOutcomeTracking {
    current: Option<Arc<Mutex<BackendOutcomeBatch>>>,
    closing: bool,
}

/// Original backend-result custody retained by the Runtime's logical close consumer.
///
/// This receiver retains CPU outcomes through removal of the World ingress resource. Receiving
/// every backend result does not itself settle UI/Input authority; the original consumer does.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct UiBackendOutcomeReceiver {
    tracking: Arc<Mutex<BackendOutcomeTracking>>,
}

impl UiBackendOutcomeReceiver {
    #[cfg(feature = "runtime")]
    pub(super) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tracking, &other.tracking)
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.tracking
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closing
    }

    pub fn begin_close(&self) {
        self.tracking
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closing = true;
    }

    #[must_use]
    pub fn pending_backend_count(&self) -> usize {
        self.current().map_or(0, |owner| {
            owner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pending
        })
    }

    /// Backend transport remains outstanding until original results have also been drained.
    #[must_use]
    pub fn has_unsettled_backend_results(&self) -> bool {
        self.current().is_some_and(|owner| {
            let batch = owner.lock().unwrap_or_else(|error| error.into_inner());
            batch.pending != 0 || !batch.outcomes.is_empty()
        })
    }

    pub fn drain(&self) -> Result<Vec<UiPresentationOutcome>, UiPresentationIntegrationError> {
        let Some(owner) = self.current() else {
            return Ok(Vec::new());
        };
        let mut batch = owner.lock().unwrap_or_else(|error| error.into_inner());
        if batch.pending != 0 {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: batch.pending,
                },
            );
        }
        Ok(std::mem::take(&mut batch.outcomes))
    }

    fn current(&self) -> Option<Arc<Mutex<BackendOutcomeBatch>>> {
        self.tracking
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .current
            .clone()
    }
}

/// Move-only permission to publish exactly one backend outcome for one admitted submission.
#[doc(hidden)]
#[derive(Debug)]
pub struct UiBackendOutcomePermit {
    identity: UiBackendSubmissionIdentity,
    owner: Arc<Mutex<BackendOutcomeBatch>>,
    slot: usize,
    abandoned: Option<UiPresentationOutcome>,
}

impl UiBackendOutcomePermit {
    #[must_use]
    pub const fn identity(&self) -> UiBackendSubmissionIdentity {
        self.identity
    }
}

impl Drop for UiBackendOutcomePermit {
    fn drop(&mut self) {
        let Some(abandoned) = self.abandoned.take() else {
            return;
        };
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        let slot = &mut owner.slots[self.slot];
        debug_assert_eq!(slot.identity, self.identity);
        debug_assert!(!slot.returned);
        // The slot and fallback were admitted before the permit left its original owner.
        // Returning an abandoned permission needs neither a live World nor an allocation.
        slot.returned = true;
        owner.outcomes.push(abandoned);
        owner.pending -= 1;
    }
}

/// One exact backend outcome whose storage was reserved before submission.
#[derive(Debug)]
pub struct UiCompletedBackendOutcome {
    permit: UiBackendOutcomePermit,
    outcome: UiPresentationOutcome,
}

/// One prepared backend submission paired with its pre-submit outcome permission.
/// Dropping it returns an aborted outcome to its original owner.
#[derive(Debug)]
pub struct UiBackendSubmission {
    submission: UiPreparedBackendSubmission,
    permit: UiBackendOutcomePermit,
}

/// One backend-generation-bound submission retaining its exact outcome permission.
#[derive(Debug)]
pub struct UiSubmittedBackendSubmission {
    submission: UiSubmittedBackendPresentation,
    permit: UiBackendOutcomePermit,
}

impl UiSubmittedBackendSubmission {
    #[must_use]
    pub fn submission(&self) -> &UiSubmittedBackendPresentation {
        &self.submission
    }

    /// Completes this exact attempt after the trusted backend actually presents its packet.
    /// Binding a backend generation alone does not establish presentation.
    #[must_use]
    pub fn presented(self) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::Presented(OneShotPresentationCommit::issue(submission)),
        }
    }

    #[must_use]
    pub fn deferred(self, message: String) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::Deferred {
                retry: submission.defer(),
                message,
            },
        }
    }

    #[must_use]
    pub fn backend_fault(
        self,
        domain: crate::UiPresentationBackendFaultDomain,
        message: String,
    ) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        let backend_generation = submission.backend_generation();
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::BackendFault {
                attempt: Some(submission.into_attempt()),
                backend_generation: Some(backend_generation),
                domain,
                message,
            },
        }
    }

    #[must_use]
    pub fn superseded(
        self,
        successor: UiPresentationAttemptIdentity,
        message: String,
    ) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::Superseded {
                attempt: submission.into_attempt(),
                successor,
                message,
            },
        }
    }
}

/// Failure-atomic batch bind result retaining every prepared submission for rejection outcomes.
#[derive(Debug)]
pub struct UiBackendSubmissionBatchBindError {
    admission_id: UiPresentationAttemptId,
    required: UiPresentationBackendGeneration,
    actual: UiPresentationBackendGeneration,
    submissions: Vec<UiBackendSubmission>,
}

impl UiBackendSubmissionBatchBindError {
    #[must_use]
    pub const fn admission_id(&self) -> UiPresentationAttemptId {
        self.admission_id
    }

    #[must_use]
    pub const fn required(&self) -> UiPresentationBackendGeneration {
        self.required
    }

    #[must_use]
    pub const fn actual(&self) -> UiPresentationBackendGeneration {
        self.actual
    }

    #[must_use]
    pub fn into_submissions(self) -> Vec<UiBackendSubmission> {
        self.submissions
    }
}

/// Binds original attempts to the trusted backend's actual generation.
///
/// All retry-generation constraints are checked before binding any attempt. A refusal returns
/// the complete original batch through [`UiBackendSubmissionBatchBindError::into_submissions`].
/// Binding does not assert drawing or presentation; the backend retains that responsibility.
pub fn bind_backend_submission_batch(
    submissions: Vec<UiBackendSubmission>,
    backend_generation: UiPresentationBackendGeneration,
) -> Result<Vec<UiSubmittedBackendSubmission>, UiBackendSubmissionBatchBindError> {
    if let Some((admission_id, required)) = submissions.iter().find_map(|submission| {
        submission
            .submission()
            .required_backend_generation()
            .filter(|required| *required != backend_generation)
            .map(|required| (submission.submission().identity().admission_id(), required))
    }) {
        return Err(UiBackendSubmissionBatchBindError {
            admission_id,
            required,
            actual: backend_generation,
            submissions,
        });
    }
    Ok(submissions
        .into_iter()
        .map(|submission| {
            let UiBackendSubmission { submission, permit } = submission;
            UiSubmittedBackendSubmission {
                submission: submission.bind_backend_unchecked(backend_generation),
                permit,
            }
        })
        .collect())
}

impl UiBackendSubmission {
    pub fn pair(
        submission: UiPreparedBackendSubmission,
        permit: UiBackendOutcomePermit,
    ) -> Result<Self, UiPresentationIntegrationError> {
        let actual = submission.identity();
        let expected = permit.identity;
        if actual != expected {
            return Err(
                UiPresentationIntegrationError::BackendPermitIdentityMismatch {
                    expected: Box::new(expected),
                    actual: Box::new(actual),
                },
            );
        }
        Ok(Self { submission, permit })
    }

    #[cfg(feature = "runtime")]
    pub(super) fn pair_exact(
        submission: UiPreparedBackendSubmission,
        permit: UiBackendOutcomePermit,
    ) -> Self {
        debug_assert_eq!(submission.identity(), permit.identity);
        Self { submission, permit }
    }

    #[must_use]
    pub fn submission(&self) -> &UiPreparedBackendSubmission {
        &self.submission
    }

    /// Cancels an admitted attempt before it is bound to a concrete backend generation.
    #[must_use]
    pub fn aborted(self, message: String) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::Aborted {
                attempt: submission.into_attempt(),
                message,
            },
        }
    }

    #[must_use]
    pub fn content_rejected(self, message: String) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::ContentRejected {
                attempt: submission.into_attempt(),
                message,
            },
        }
    }

    #[must_use]
    pub fn backend_fault(
        self,
        backend_generation: Option<UiPresentationBackendGeneration>,
        domain: crate::UiPresentationBackendFaultDomain,
        message: String,
    ) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::BackendFault {
                attempt: Some(submission.into_attempt()),
                backend_generation,
                domain,
                message,
            },
        }
    }

    #[must_use]
    pub fn superseded(
        self,
        successor: UiPresentationAttemptIdentity,
        message: String,
    ) -> UiCompletedBackendOutcome {
        let Self { submission, permit } = self;
        UiCompletedBackendOutcome {
            permit,
            outcome: UiPresentationOutcome::Superseded {
                attempt: submission.into_attempt(),
                successor,
                message,
            },
        }
    }
}

/// Exact draw packets and admission attempts for one concrete backend frame.
#[derive(Debug, Default)]
pub struct UiBackendPresentationFrame {
    packets: Vec<Arc<UiPaintPacket>>,
    submissions: Vec<UiBackendSubmission>,
}

impl UiBackendPresentationFrame {
    pub(crate) fn from_parts(
        packets: Vec<Arc<UiPaintPacket>>,
        submissions: Vec<UiBackendSubmission>,
    ) -> Self {
        Self {
            packets,
            submissions,
        }
    }

    /// Returns whether this backend frame contains at least one concrete UI paint packet.
    #[must_use]
    pub fn has_packets(&self) -> bool {
        !self.packets.is_empty()
    }

    /// Returns whether any exact UI frame in this backend batch owns a modal input barrier.
    #[must_use]
    pub fn blocks_unhandled_input(&self) -> bool {
        self.packets
            .iter()
            .any(|packet| packet.frame().blocks_unhandled_input())
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<Arc<UiPaintPacket>>, Vec<UiBackendSubmission>) {
        (self.packets, self.submissions)
    }
}

impl UiPresentationOutcomes {
    pub(crate) fn begin_close(&self) {
        self.receiver.begin_close();
    }

    pub(crate) fn has_unsettled_backend_results(&self) -> bool {
        !self.outcomes.is_empty() || self.receiver.has_unsettled_backend_results()
    }

    pub fn commit_backend(&mut self, completed: UiCompletedBackendOutcome) {
        self.try_commit_backend(completed)
            .expect("a backend outcome must return to the runtime that reserved its permit");
    }

    pub(crate) fn try_commit_backend(
        &mut self,
        completed: UiCompletedBackendOutcome,
    ) -> Result<(), UiCompletedBackendOutcome> {
        let Some(owner) = &self.backend else {
            return Err(completed);
        };
        if !Arc::ptr_eq(owner, &completed.permit.owner) {
            return Err(completed);
        }
        let mut batch = owner.lock().unwrap_or_else(|error| error.into_inner());
        if !batch.accepts(&completed.permit) {
            return Err(completed);
        }
        batch.commit(completed);
        Ok(())
    }

    #[cfg(feature = "runtime")]
    pub(super) fn try_commit_backend_batch(
        &mut self,
        completed: Vec<UiCompletedBackendOutcome>,
    ) -> Result<(), Vec<UiCompletedBackendOutcome>> {
        let Some(owner) = &self.backend else {
            return if completed.is_empty() {
                Ok(())
            } else {
                Err(completed)
            };
        };
        if completed
            .iter()
            .any(|outcome| !Arc::ptr_eq(owner, &outcome.permit.owner))
        {
            return Err(completed);
        }
        let mut batch = owner.lock().unwrap_or_else(|error| error.into_inner());
        if completed
            .iter()
            .any(|outcome| !batch.accepts(&outcome.permit))
        {
            return Err(completed);
        }
        // Each private slot issues exactly one move-only permit. A caller cannot duplicate it;
        // complete owner/slot preflight therefore admits the whole batch without new scratch.
        for completed in completed {
            batch.commit(completed);
        }
        Ok(())
    }

    #[cfg(feature = "runtime")]
    pub(super) fn replace_content(
        &mut self,
        outcomes: Vec<UiPresentationOutcome>,
    ) -> Result<(), UiPresentationIntegrationError> {
        let pending = self.pending_backend_count();
        if pending != 0 {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: pending,
                },
            );
        }
        self.collect_backend();
        if !self.outcomes.is_empty() {
            return Err(
                UiPresentationIntegrationError::PendingPresentationOutcomes {
                    count: self.outcomes.len(),
                },
            );
        }
        self.outcomes = outcomes;
        Ok(())
    }

    pub fn reserve_backend(
        &mut self,
        submissions: &[UiPreparedBackendSubmission],
    ) -> Result<Vec<UiBackendOutcomePermit>, UiPresentationIntegrationError> {
        if self
            .receiver
            .tracking
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closing
        {
            return Err(UiPresentationIntegrationError::BackendOutcomeAdmissionClosed);
        }
        let pending = self.pending_backend_count();
        if pending != 0 {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: pending,
                },
            );
        }
        if let Some(owner) = &self.backend {
            let count = owner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .outcomes
                .len();
            if count != 0 {
                return Err(UiPresentationIntegrationError::PendingPresentationOutcomes { count });
            }
        }
        self.collect_backend();
        if submissions.is_empty() {
            return Ok(Vec::new());
        }
        self.outcomes.try_reserve(submissions.len()).map_err(|_| {
            UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                additional: submissions.len(),
            }
        })?;
        let mut identities = HashSet::new();
        identities.try_reserve(submissions.len()).map_err(|_| {
            UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                additional: submissions.len(),
            }
        })?;
        for submission in submissions {
            let identity = submission.identity();
            if !identities.insert(identity) {
                return Err(UiPresentationIntegrationError::DuplicateBackendAttempt {
                    admission_id: identity.admission_id().get(),
                });
            }
        }
        let mut slots = Vec::new();
        let mut abandoned = Vec::new();
        let mut permits = Vec::new();
        let mut completed = Vec::new();
        abandoned
            .try_reserve_exact(submissions.len())
            .map_err(
                |_| UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                    additional: submissions.len(),
                },
            )?;
        permits.try_reserve_exact(submissions.len()).map_err(|_| {
            UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                additional: submissions.len(),
            }
        })?;
        completed
            .try_reserve_exact(submissions.len())
            .map_err(
                |_| UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                    additional: submissions.len(),
                },
            )?;
        slots.try_reserve_exact(submissions.len()).map_err(|_| {
            UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                additional: submissions.len(),
            }
        })?;
        for submission in submissions {
            let mut message = String::new();
            message
                .try_reserve_exact(BACKEND_ABANDONED_MESSAGE.len())
                .map_err(
                    |_| UiPresentationIntegrationError::OutcomeCapacityUnavailable {
                        additional: submissions.len(),
                    },
                )?;
            message.push_str(BACKEND_ABANDONED_MESSAGE);
            abandoned.push(UiPresentationOutcome::Aborted {
                attempt: UiPresentationAttemptIdentity::backend(submission.identity()),
                message,
            });
            slots.push(BackendOutcomeSlot {
                identity: submission.identity(),
                returned: false,
            });
        }
        let owner = Arc::new(Mutex::new(BackendOutcomeBatch {
            slots,
            outcomes: completed,
            pending: submissions.len(),
        }));
        for (slot, (submission, abandoned)) in submissions.iter().zip(abandoned).enumerate() {
            permits.push(UiBackendOutcomePermit {
                identity: submission.identity(),
                owner: Arc::clone(&owner),
                slot,
                abandoned: Some(abandoned),
            });
        }
        let mut tracking = self
            .receiver
            .tracking
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if tracking.closing {
            return Err(UiPresentationIntegrationError::BackendOutcomeAdmissionClosed);
        }
        tracking.current = Some(Arc::clone(&owner));
        self.backend = Some(owner);
        Ok(permits)
    }

    pub(crate) fn drain(
        &mut self,
    ) -> Result<Vec<UiPresentationOutcome>, UiPresentationIntegrationError> {
        let pending = self.pending_backend_count();
        if pending != 0 {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: pending,
                },
            );
        }
        self.collect_backend();
        Ok(std::mem::take(&mut self.outcomes))
    }

    fn pending_backend_count(&self) -> usize {
        self.backend.as_ref().map_or(0, |owner| {
            owner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pending
        })
    }

    #[cfg(feature = "runtime")]
    pub(super) fn admission_state(&self) -> (usize, usize) {
        let Some(owner) = &self.backend else {
            return (self.outcomes.len(), 0);
        };
        // Observe both axes together: a last permit can return from another thread between
        // independent completed/pending reads, before the planner is allowed to mutate.
        let batch = owner.lock().unwrap_or_else(|error| error.into_inner());
        (self.outcomes.len() + batch.outcomes.len(), batch.pending)
    }

    fn collect_backend(&mut self) {
        let Some(owner) = self.backend.take() else {
            return;
        };
        let completed = {
            let mut batch = owner.lock().unwrap_or_else(|error| error.into_inner());
            debug_assert_eq!(batch.pending, 0);
            std::mem::take(&mut batch.outcomes)
        };
        self.outcomes.extend(completed);
    }
}

const BACKEND_ABANDONED_MESSAGE: &str = "the owning backend attempt was abandoned";

impl BackendOutcomeBatch {
    fn accepts(&self, permit: &UiBackendOutcomePermit) -> bool {
        self.slots
            .get(permit.slot)
            .is_some_and(|slot| slot.identity == permit.identity && !slot.returned)
    }

    fn commit(&mut self, completed: UiCompletedBackendOutcome) {
        let UiCompletedBackendOutcome {
            mut permit,
            outcome,
        } = completed;
        permit.abandoned = None;
        self.slots[permit.slot].returned = true;
        self.outcomes.push(outcome);
        self.pending -= 1;
    }
}

/// Consumes a stale backend acceptance without advancing the planner's presented packet.
#[doc(hidden)]
#[must_use]
pub fn discard_ui_backend_acceptance(
    commit: OneShotPresentationCommit,
) -> UiAcceptedBackendPresentation {
    commit.accept()
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationIntegrationError {
    #[error("UI presentation integration is unavailable because UiRenderPlugin is not installed")]
    Unavailable,
    #[error("the original UI backend outcome owner has closed admission")]
    BackendOutcomeAdmissionClosed,
    #[error("UI backend attempt {admission_id} appears more than once in one submitted frame")]
    DuplicateBackendAttempt { admission_id: u64 },
    #[error(
        "UI presentation target {target:?} is owned by backend attempts {first_admission_id} and {second_admission_id} in one submitted frame"
    )]
    DuplicateBackendTarget {
        target: RenderTarget,
        first_admission_id: u64,
        second_admission_id: u64,
    },
    #[error(
        "UI backend attempt {admission_id} does not own an exact packet in its submitted frame"
    )]
    SubmissionPacketMissing { admission_id: u64 },
    #[error(
        "UI backend attempt {admission_id} no longer matches the planner's exact active packet"
    )]
    SubmissionPlannerMismatch { admission_id: u64 },
    #[error(
        "UI backend outcome permit identity mismatch: expected {expected:?}, received {actual:?}"
    )]
    BackendPermitIdentityMismatch {
        expected: Box<UiBackendSubmissionIdentity>,
        actual: Box<UiBackendSubmissionIdentity>,
    },
    #[error("{count} UI presentation outcomes remain undrained from the previous frame")]
    PendingPresentationOutcomes { count: usize },
    #[error("{count} UI backend outcome reservations remain incomplete from the submitted frame")]
    OutstandingBackendOutcomeReservations { count: usize },
    #[error(
        "UI backend outcome capacity for {additional} submissions could not be reserved before submit"
    )]
    OutcomeCapacityUnavailable { additional: usize },
    #[error("the UI backend queue no longer matches its original outcome receiver")]
    BackendOutcomeOwnerChanged,
}

#[cfg(all(test, feature = "runtime"))]
mod tests {
    use super::*;
    use crate::integration::tests::{frame, prepared_submission};
    use crate::{UiPreparedImages, UiPresentationPlanner};
    use nara_ecs::World;
    use nara_ui_kernel::{UiColor, UiVisual};
    use nara_window::WindowId;

    #[test]
    fn outcome_storage_survives_its_world_resource_until_the_last_abandoned_permission() {
        let mut world = World::new();
        world.init_resource::<UiPresentationOutcomes>();
        let packet = UiPresentationPlanner::new(Default::default())
            .unwrap()
            .plan(
                frame(1, 1, UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::new(),
            )
            .unwrap();
        let submissions = [
            prepared_submission(Arc::clone(&packet), 93, 9),
            prepared_submission(packet, 94, 10),
        ];
        let permits = world
            .resource_mut::<UiPresentationOutcomes>()
            .reserve_backend(&submissions)
            .unwrap();
        let mut external = submissions
            .into_iter()
            .zip(permits)
            .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
            .collect::<Vec<_>>();
        let retained = Arc::downgrade(&external[0].permit.owner);
        drop(world.remove_resource::<UiPresentationOutcomes>().unwrap());
        drop(world);
        drop(external.pop());
        assert!(retained.upgrade().is_some());
        drop(external);
        assert!(retained.upgrade().is_none());
    }
}
