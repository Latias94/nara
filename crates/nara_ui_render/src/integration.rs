use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use nara_app::{App, Plugin, PluginDefinition, PluginDefinitionId, PluginError};
use nara_asset::AssetServer;
use nara_ecs::{Mut, ResMut, Resource, World, system::SystemParam};
use nara_image::{IMAGE_PREPARE_PLUGIN_ID, PreparedImageResource};
use nara_render::{PreparedRenderResources, RenderTarget};
use nara_text::FontFace;
use nara_ui_kernel::{UiFrame, UiSurfaceId};
use nara_window::WindowId;
use thiserror::Error;

use crate::packet::{
    UiAcceptedBackendPresentation, UiPreparedBackendSubmission, UiSubmittedBackendPresentation,
};
use crate::planner::UiPlannedPacketAbortOutcome;
use crate::planner::collect_referenced_image_ids;
use crate::{
    OneShotPresentationCommit, UiBackendSubmissionIdentity, UiPaintPacket, UiPaintPacketIdentity,
    UiPreparedImages, UiPresentationAttemptId, UiPresentationAttemptIdentity,
    UiPresentationBackendGeneration, UiPresentationContentError, UiPresentationOutcome,
    UiPresentationPlanner,
};

/// Host-owned explicit mapping from one stable UI surface to one render window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPresentationRoute {
    surface_id: UiSurfaceId,
    window_id: WindowId,
}

impl UiPresentationRoute {
    pub fn new(
        surface_id: UiSurfaceId,
        window_id: WindowId,
    ) -> Result<Self, UiPresentationRouteError> {
        if window_id.get() == 0 {
            return Err(UiPresentationRouteError::ZeroWindowId);
        }
        Ok(Self {
            surface_id,
            window_id,
        })
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    const fn target(self) -> RenderTarget {
        RenderTarget::Window(self.window_id)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationRouteError {
    #[error("UI presentation routes require a non-zero window identifier")]
    ZeroWindowId,
}

#[derive(Debug, Default, Resource)]
struct UiPresentationRoutes {
    routes: BTreeMap<UiSurfaceId, UiPresentationRoute>,
}

#[derive(Resource)]
struct UiPresentationState(UiPresentationPlanner);

#[doc(hidden)]
#[derive(Debug, Default, Resource)]
pub struct UiPresentationOutcomes {
    outcomes: Vec<UiPresentationOutcome>,
    reserved_backend: HashSet<UiBackendSubmissionIdentity>,
    backend_commit_scratch: HashSet<UiBackendSubmissionIdentity>,
}

/// Move-only permission to publish exactly one backend outcome for one admitted submission.
#[doc(hidden)]
#[derive(Debug)]
pub struct UiBackendOutcomePermit {
    identity: UiBackendSubmissionIdentity,
}

impl UiBackendOutcomePermit {
    #[must_use]
    pub const fn identity(&self) -> UiBackendSubmissionIdentity {
        self.identity
    }
}

/// One exact backend outcome whose storage was reserved before submission.
#[doc(hidden)]
#[derive(Debug)]
pub struct UiCompletedBackendOutcome {
    permit: UiBackendOutcomePermit,
    outcome: UiPresentationOutcome,
}

/// One prepared backend submission paired with its pre-submit outcome permission.
#[doc(hidden)]
#[derive(Debug)]
pub struct UiBackendSubmission {
    submission: UiPreparedBackendSubmission,
    permit: UiBackendOutcomePermit,
}

/// One backend-generation-bound submission retaining its exact outcome permission.
#[doc(hidden)]
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
#[doc(hidden)]
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

#[doc(hidden)]
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

    fn pair_exact(submission: UiPreparedBackendSubmission, permit: UiBackendOutcomePermit) -> Self {
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

/// Move-only frame exchange populated by the Host and drained once by the concrete backend.
#[doc(hidden)]
#[derive(Debug, Default, Resource)]
pub(crate) struct UiBackendSubmissionQueue {
    packets: Vec<Arc<UiPaintPacket>>,
    submissions: Vec<UiPreparedBackendSubmission>,
}

impl UiBackendSubmissionQueue {
    /// Replaces the next backend frame and returns prepared attempts that never reached submit.
    fn replace(
        &mut self,
        packets: Vec<Arc<UiPaintPacket>>,
        submissions: Vec<UiPreparedBackendSubmission>,
    ) -> Vec<UiPreparedBackendSubmission> {
        self.packets = packets;
        std::mem::replace(&mut self.submissions, submissions)
    }

    fn take(&mut self) -> (Vec<Arc<UiPaintPacket>>, Vec<UiPreparedBackendSubmission>) {
        (
            std::mem::take(&mut self.packets),
            std::mem::take(&mut self.submissions),
        )
    }
}

/// Exact draw packets and admission attempts for one concrete backend frame.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct UiBackendPresentationFrame {
    packets: Vec<Arc<UiPaintPacket>>,
    submissions: Vec<UiBackendSubmission>,
}

impl UiBackendPresentationFrame {
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
    pub fn commit_backend(&mut self, completed: UiCompletedBackendOutcome) {
        self.try_commit_backend(completed)
            .expect("a backend outcome must return to the runtime that reserved its permit");
    }

    fn try_commit_backend(
        &mut self,
        completed: UiCompletedBackendOutcome,
    ) -> Result<(), UiCompletedBackendOutcome> {
        if !self.reserved_backend.remove(&completed.permit.identity) {
            return Err(completed);
        }
        let UiCompletedBackendOutcome { permit, outcome } = completed;
        let _ = permit;
        self.outcomes.push(outcome);
        Ok(())
    }

    fn try_commit_backend_batch(
        &mut self,
        completed: Vec<UiCompletedBackendOutcome>,
    ) -> Result<(), Vec<UiCompletedBackendOutcome>> {
        self.backend_commit_scratch.clear();
        if self
            .backend_commit_scratch
            .try_reserve(completed.len())
            .is_err()
            || completed.iter().any(|outcome| {
                !self.reserved_backend.contains(&outcome.permit.identity)
                    || !self.backend_commit_scratch.insert(outcome.permit.identity)
            })
        {
            self.backend_commit_scratch.clear();
            return Err(completed);
        }
        self.backend_commit_scratch.clear();
        for completed in completed {
            self.reserved_backend.remove(&completed.permit.identity);
            let UiCompletedBackendOutcome { permit, outcome } = completed;
            let _ = permit;
            self.outcomes.push(outcome);
        }
        Ok(())
    }

    fn replace_content(
        &mut self,
        outcomes: Vec<UiPresentationOutcome>,
    ) -> Result<(), UiPresentationIntegrationError> {
        if !self.reserved_backend.is_empty() {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: self.reserved_backend.len(),
                },
            );
        }
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
        if !self.reserved_backend.is_empty() {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: self.reserved_backend.len(),
                },
            );
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
        self.reserved_backend = identities;
        Ok(submissions
            .iter()
            .map(|submission| UiBackendOutcomePermit {
                identity: submission.identity(),
            })
            .collect())
    }

    fn drain(&mut self) -> Result<Vec<UiPresentationOutcome>, UiPresentationIntegrationError> {
        if !self.reserved_backend.is_empty() {
            return Err(
                UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                    count: self.reserved_backend.len(),
                },
            );
        }
        Ok(std::mem::take(&mut self.outcomes))
    }
}

/// Summary of one runtime-frame to paint-packet compilation pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiPaintCompileStats {
    pub admitted_packets: u32,
    pub rejected_packets: u32,
}

#[doc(hidden)]
pub fn replace_ui_presentation_routes(
    world: &mut World,
    routes: impl IntoIterator<Item = UiPresentationRoute>,
) -> Result<(), UiPresentationIntegrationError> {
    let replacement = routes
        .into_iter()
        .map(|route| (route.surface_id(), route))
        .collect::<BTreeMap<_, _>>();
    let previous = {
        let Some(mut installed) = world.get_resource_mut::<UiPresentationRoutes>() else {
            return Err(UiPresentationIntegrationError::Unavailable);
        };
        std::mem::replace(&mut installed.routes, replacement.clone())
    };
    let retired_targets = previous
        .iter()
        .filter_map(|(surface, route)| {
            (!replacement
                .get(surface)
                .is_some_and(|replacement| replacement.target() == route.target()))
            .then_some((*surface, route.target()))
        })
        .collect::<HashSet<_>>();
    if let Some(mut planner) = world.get_resource_mut::<UiPresentationState>() {
        for (surface_id, target) in retired_targets {
            planner.0.retire_surface_target(surface_id, target);
        }
    }
    Ok(())
}

/// Compiles exact immutable runtime frames without querying ECS UI authoring components.
pub fn compile_ui_paint_packets(
    world: &mut World,
    frames: &[Arc<UiFrame>],
) -> Result<UiPaintCompileStats, UiPresentationIntegrationError> {
    if !world.contains_resource::<UiPresentationState>()
        || !world.contains_resource::<UiPresentationOutcomes>()
        || !world.contains_resource::<UiPresentationRoutes>()
    {
        return Err(UiPresentationIntegrationError::Unavailable);
    }
    let (pending_outcomes, outstanding_backend) = {
        let outcome_state = world.resource::<UiPresentationOutcomes>();
        (
            outcome_state.outcomes.len(),
            outcome_state.reserved_backend.len(),
        )
    };
    if pending_outcomes != 0 {
        return Err(
            UiPresentationIntegrationError::PendingPresentationOutcomes {
                count: pending_outcomes,
            },
        );
    }
    if outstanding_backend != 0 {
        return Err(
            UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                count: outstanding_backend,
            },
        );
    }
    let routes = world.resource::<UiPresentationRoutes>().routes.clone();
    let target_conflicts = target_conflicts(&routes);
    let duplicate_surfaces = duplicate_frame_surfaces(frames);
    let referenced_images = collect_referenced_image_ids(frames);
    let image_index = UiPreparedImages::capture(
        &referenced_images,
        world.get_resource::<AssetServer>(),
        world.get_resource::<PreparedRenderResources<PreparedImageResource>>(),
    );
    let mut outcomes = Vec::new();
    let mut stats = UiPaintCompileStats::default();
    world.resource_scope(|_world, mut planner: nara_ecs::Mut<UiPresentationState>| {
        for frame in frames {
            let identity = UiPaintPacketIdentity::from_frame(frame);
            if duplicate_surfaces.contains(&identity.surface_id()) {
                let error = UiPresentationContentError::DuplicateSurfaceFrame {
                    surface_id: identity.surface_id(),
                };
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(match routes.get(&identity.surface_id()).copied() {
                    Some(route) => {
                        build_rejection(identity, route.target(), error)
                    }
                    None => UiPresentationOutcome::ContentRejected {
                        attempt: UiPresentationAttemptIdentity::unrouted(identity),
                        message: error.to_string(),
                    },
                });
                continue;
            }
            let Some(route) = routes.get(&identity.surface_id()).copied() else {
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(UiPresentationOutcome::ContentRejected {
                    attempt: UiPresentationAttemptIdentity::unrouted(identity),
                    message: "the Host has not installed an explicit presentation route for this UI surface"
                        .to_owned(),
                });
                continue;
            };
            let target = route.target();
            if let Some((first_surface, second_surface)) = target_conflicts.get(&target).copied() {
                let error = UiPresentationContentError::TargetRouteConflict {
                    target,
                    first_surface,
                    second_surface,
                };
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(build_rejection(identity, target, error));
                continue;
            }
            match planner.0.plan(Arc::clone(frame), target, &image_index) {
                Ok(_) => {
                    stats.admitted_packets = stats.admitted_packets.saturating_add(1);
                }
                Err(error) => {
                    stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                    outcomes.push(build_rejection(identity, target, error));
                    let _ = planner
                        .0
                        .abort_surface_target(identity.surface_id(), target);
                }
            }
        }
    });
    world
        .resource_mut::<UiPresentationOutcomes>()
        .replace_content(outcomes)?;
    Ok(stats)
}

pub fn drain_ui_presentation_outcomes(
    world: &mut World,
) -> Result<Vec<UiPresentationOutcome>, UiPresentationIntegrationError> {
    let Some(mut outcomes) = world.get_resource_mut::<UiPresentationOutcomes>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    outcomes.drain()
}

/// Moves one exact prepared backend frame out of a runtime-owned world.
///
/// Outcome storage is reserved before any packet or attempt leaves the runtime. A second take is
/// rejected until every returned permit reaches a terminal outcome.
#[doc(hidden)]
pub fn take_ui_backend_presentation_frame(
    world: &mut World,
) -> Result<UiBackendPresentationFrame, UiPresentationIntegrationError> {
    if !world.contains_resource::<UiBackendSubmissionQueue>()
        || !world.contains_resource::<UiPresentationOutcomes>()
    {
        return Err(UiPresentationIntegrationError::Unavailable);
    }
    world.resource_scope(|world, mut queue: Mut<UiBackendSubmissionQueue>| {
        let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
        take_backend_presentation_frame(&mut queue, &mut outcomes)
    })
}

/// Returns one move-only backend outcome to the runtime that reserved its permit.
///
/// A foreign or already-completed runtime returns the outcome unchanged so its owning Host can
/// still terminalize it correctly.
#[doc(hidden)]
pub fn commit_ui_backend_outcome(
    world: &mut World,
    completed: UiCompletedBackendOutcome,
) -> Result<(), UiCompletedBackendOutcome> {
    let Some(mut outcomes) = world.get_resource_mut::<UiPresentationOutcomes>() else {
        return Err(completed);
    };
    outcomes.try_commit_backend(completed)
}

/// Atomically returns one candidate's terminal backend outcomes to its owning Runtime.
///
/// A foreign identity or duplicate keeps the entire move-only batch with the caller.
#[doc(hidden)]
pub fn commit_ui_backend_outcomes(
    world: &mut World,
    completed: Vec<UiCompletedBackendOutcome>,
) -> Result<(), Vec<UiCompletedBackendOutcome>> {
    let Some(mut outcomes) = world.get_resource_mut::<UiPresentationOutcomes>() else {
        return Err(completed);
    };
    outcomes.try_commit_backend_batch(completed)
}

/// Publishes the exact replay packets and move-only attempts for the next backend frame.
#[doc(hidden)]
pub fn replace_ui_backend_frame(
    world: &mut World,
    packets: Vec<Arc<UiPaintPacket>>,
    submissions: Vec<UiPreparedBackendSubmission>,
) -> Result<Vec<UiPreparedBackendSubmission>, UiPresentationIntegrationError> {
    if !world.contains_resource::<UiBackendSubmissionQueue>()
        || !world.contains_resource::<UiPresentationState>()
    {
        return Err(UiPresentationIntegrationError::Unavailable);
    }
    let mut admission_ids = HashSet::with_capacity(submissions.len());
    let mut target_admissions = HashMap::with_capacity(submissions.len());
    for submission in &submissions {
        let identity = submission.identity();
        if !admission_ids.insert(identity.admission_id()) {
            return Err(UiPresentationIntegrationError::DuplicateBackendAttempt {
                admission_id: identity.admission_id().get(),
            });
        }
        if let Some(first) = target_admissions.insert(identity.target(), identity.admission_id()) {
            return Err(UiPresentationIntegrationError::DuplicateBackendTarget {
                target: identity.target(),
                first_admission_id: first.get(),
                second_admission_id: identity.admission_id().get(),
            });
        }
        if !packets.iter().any(|packet| {
            Arc::ptr_eq(packet, submission.packet())
                && packet.identity() == identity.packet()
                && packet.target() == identity.target()
        }) {
            return Err(UiPresentationIntegrationError::SubmissionPacketMissing {
                admission_id: identity.admission_id().get(),
            });
        }
    }
    world.resource_scope(|world, mut queue: Mut<UiBackendSubmissionQueue>| {
        world
            .resource_mut::<UiPresentationState>()
            .0
            .replace_backend_submissions(&submissions, &queue.submissions)
            .map_err(
                |admission_id| UiPresentationIntegrationError::SubmissionPlannerMismatch {
                    admission_id: admission_id.get(),
                },
            )?;
        Ok(queue.replace(packets, submissions))
    })
}

/// Consumes backend acceptance and advances only the planner's committed packet snapshot.
///
/// Plugin installation and exact-token staging are pre-submit invariants. Therefore this function
/// deliberately has no fallible arm after the backend has accepted pixels.
#[doc(hidden)]
#[must_use]
pub fn commit_ui_backend_presentation(
    world: &mut World,
    commit: OneShotPresentationCommit,
) -> UiAcceptedBackendPresentation {
    let accepted = commit.accept();
    world
        .resource_mut::<UiPresentationState>()
        .0
        .commit_presented(accepted.identity(), Arc::clone(accepted.packet()));
    accepted
}

/// Consumes a stale backend acceptance without advancing the planner's presented packet.
#[doc(hidden)]
#[must_use]
pub fn discard_ui_backend_acceptance(
    commit: OneShotPresentationCommit,
) -> UiAcceptedBackendPresentation {
    commit.accept()
}

/// Terminates the exact active plan after rejection, abort, or supersession.
///
/// The previous presented packet becomes the replay plan. If this target has never presented, the
/// target-local planner state is removed instead of leaving rejected content active.
#[doc(hidden)]
#[must_use]
pub fn abort_ui_planned_packet(
    world: &mut World,
    attempt: UiPresentationAttemptIdentity,
) -> UiPlannedPacketAbortOutcome {
    world
        .resource_mut::<UiPresentationState>()
        .0
        .abort_planned(attempt)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationIntegrationError {
    #[error("UI presentation integration is unavailable because UiRenderPlugin is not installed")]
    Unavailable,
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
}

fn target_conflicts(
    routes: &BTreeMap<UiSurfaceId, UiPresentationRoute>,
) -> HashMap<RenderTarget, (UiSurfaceId, UiSurfaceId)> {
    let mut first_owner = HashMap::<RenderTarget, UiSurfaceId>::new();
    let mut conflicts = HashMap::new();
    for route in routes.values().copied() {
        match first_owner.get(&route.target()).copied() {
            Some(first_surface) if first_surface != route.surface_id() => {
                conflicts
                    .entry(route.target())
                    .or_insert((first_surface, route.surface_id()));
            }
            Some(_) => {}
            None => {
                first_owner.insert(route.target(), route.surface_id());
            }
        }
    }
    conflicts
}

fn duplicate_frame_surfaces(frames: &[Arc<UiFrame>]) -> HashSet<UiSurfaceId> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for frame in frames {
        let surface_id = frame.environment().surface_id();
        if !seen.insert(surface_id) {
            duplicates.insert(surface_id);
        }
    }
    duplicates
}

fn build_rejection(
    identity: UiPaintPacketIdentity,
    target: RenderTarget,
    error: UiPresentationContentError,
) -> UiPresentationOutcome {
    UiPresentationOutcome::ContentRejected {
        attempt: UiPresentationAttemptIdentity::routed(identity, target),
        message: error.to_string(),
    }
}

/// Stable planner snapshots used by the Host to choose new submissions versus replay packets.
#[doc(hidden)]
#[derive(Debug)]
pub struct UiPaintPacketSnapshots {
    planned: Vec<Arc<UiPaintPacket>>,
    presented: Vec<Arc<UiPaintPacket>>,
}

impl UiPaintPacketSnapshots {
    #[must_use]
    pub fn into_parts(self) -> (Vec<Arc<UiPaintPacket>>, Vec<Arc<UiPaintPacket>>) {
        (self.planned, self.presented)
    }
}

#[doc(hidden)]
pub fn snapshot_ui_paint_packets(
    world: &World,
) -> Result<UiPaintPacketSnapshots, UiPresentationIntegrationError> {
    let Some(state) = world.get_resource::<UiPresentationState>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    Ok(UiPaintPacketSnapshots {
        planned: state.0.snapshot_planned_packets(),
        presented: state.0.snapshot_presented_packets(),
    })
}

#[doc(hidden)]
#[derive(SystemParam)]
pub struct UiBackendSubmissionReader<'w> {
    queue: ResMut<'w, UiBackendSubmissionQueue>,
    outcomes: ResMut<'w, UiPresentationOutcomes>,
}

impl UiBackendSubmissionReader<'_> {
    pub fn take(&mut self) -> Result<UiBackendPresentationFrame, UiPresentationIntegrationError> {
        take_backend_presentation_frame(&mut self.queue, &mut self.outcomes)
    }
}

fn take_backend_presentation_frame(
    queue: &mut UiBackendSubmissionQueue,
    outcomes: &mut UiPresentationOutcomes,
) -> Result<UiBackendPresentationFrame, UiPresentationIntegrationError> {
    let permits = outcomes.reserve_backend(&queue.submissions)?;
    let (packets, submissions) = queue.take();
    let submissions = submissions
        .into_iter()
        .zip(permits)
        .map(|(submission, permit)| UiBackendSubmission::pair_exact(submission, permit))
        .collect::<Vec<_>>();
    Ok(UiBackendPresentationFrame {
        packets,
        submissions,
    })
}

#[derive(Debug, Default, Clone)]
pub struct UiRenderPlugin {
    font: Option<FontFace>,
}

impl UiRenderPlugin {
    /// Selects the admitted font used to rasterize frames compiled with the same face.
    #[must_use]
    pub fn with_font(font: FontFace) -> Self {
        Self { font: Some(font) }
    }
}

const UI_RENDER_PLUGIN_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.ui-render.configured", 1);

/// Creates a repeatable presentation plugin definition bound to one font content identity.
#[must_use]
pub fn plugin(font: FontFace) -> PluginDefinition {
    let mut configuration = b"nara.ui-render.font.v1\0".to_vec();
    configuration.extend_from_slice(font.id().as_bytes());
    PluginDefinition::infallible::<UiRenderPlugin, _>(
        UI_RENDER_PLUGIN_DEFINITION_ID,
        configuration,
        move || UiRenderPlugin::with_font(font.clone()),
    )
}

pub const UI_RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui-render");
const UI_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[nara_render::RENDER_PLUGIN_ID, IMAGE_PREPARE_PLUGIN_ID];
const UI_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("presentation-ui")];
pub const UI_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(UI_RENDER_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_RENDER_PRODUCT_REQUIREMENTS);

impl Plugin for UiRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let planner = match self.font.clone() {
            Some(font) => UiPresentationPlanner::with_font(font, Default::default()),
            None => UiPresentationPlanner::new(Default::default()),
        }
        .map_err(|error| PluginError::SetupFailed {
            plugin: UI_RENDER_PLUGIN_ID,
            message: error.to_string(),
        })?;
        app.insert_resource(UiPresentationState(planner))?;
        app.init_resource::<UiPresentationRoutes>()?;
        app.init_resource::<UiPresentationOutcomes>()?;
        app.init_resource::<UiBackendSubmissionQueue>()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;
