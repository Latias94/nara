use std::{num::NonZeroU64, sync::Arc};

use nara_render::RenderTarget;
use nara_ui_kernel::UiPresentationReceipt;

use super::{UiPaintPacket, UiPaintPacketIdentity};

macro_rules! non_zero_identity {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Option<Self> {
                match NonZeroU64::new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

non_zero_identity!(
    UiPresentationAttemptId,
    "Host-assigned admission identity for one exact presentation attempt."
);
non_zero_identity!(
    UiPresentationEpoch,
    "Monotonic presentation epoch reserved before backend submission."
);
non_zero_identity!(
    UiPresentationRuntimeGeneration,
    "Runtime generation captured by one presentation attempt."
);
non_zero_identity!(
    UiPresentationCapabilityGeneration,
    "Backend-capability generation captured by one presentation attempt."
);

/// Exact packet and target identity for a planner attempt or an admitted backend submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiPresentationAttemptIdentity(UiPresentationAttemptKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum UiPresentationAttemptKind {
    Planned {
        packet: UiPaintPacketIdentity,
        target: Option<RenderTarget>,
    },
    Backend(UiBackendSubmissionIdentity),
}

impl UiPresentationAttemptIdentity {
    pub(crate) const fn routed(packet: UiPaintPacketIdentity, target: RenderTarget) -> Self {
        Self(UiPresentationAttemptKind::Planned {
            packet,
            target: Some(target),
        })
    }

    pub(crate) const fn unrouted(packet: UiPaintPacketIdentity) -> Self {
        Self(UiPresentationAttemptKind::Planned {
            packet,
            target: None,
        })
    }

    pub(crate) const fn backend(identity: UiBackendSubmissionIdentity) -> Self {
        Self(UiPresentationAttemptKind::Backend(identity))
    }

    #[must_use]
    pub const fn packet(self) -> UiPaintPacketIdentity {
        match self.0 {
            UiPresentationAttemptKind::Planned { packet, .. } => packet,
            UiPresentationAttemptKind::Backend(identity) => identity.packet(),
        }
    }

    #[must_use]
    pub const fn target(self) -> Option<RenderTarget> {
        match self.0 {
            UiPresentationAttemptKind::Planned { target, .. } => target,
            UiPresentationAttemptKind::Backend(identity) => Some(identity.target()),
        }
    }

    #[must_use]
    pub const fn admission_id(self) -> Option<UiPresentationAttemptId> {
        match self.0 {
            UiPresentationAttemptKind::Planned { .. } => None,
            UiPresentationAttemptKind::Backend(identity) => Some(identity.admission_id()),
        }
    }

    #[must_use]
    pub const fn backend_submission(self) -> Option<UiBackendSubmissionIdentity> {
        match self.0 {
            UiPresentationAttemptKind::Planned { .. } => None,
            UiPresentationAttemptKind::Backend(identity) => Some(identity),
        }
    }
}

/// Immutable identity reserved by the Host before a packet reaches a concrete backend.
///
/// `packet` includes the exact surface, frame, source, slot, route, geometry, presentation,
/// theme, and font generations. The remaining fields close the identity over the Host/runtime
/// authority that admitted the attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiBackendSubmissionIdentity {
    admission_id: UiPresentationAttemptId,
    packet: UiPaintPacketIdentity,
    target: RenderTarget,
    presentation_epoch: UiPresentationEpoch,
    runtime_generation: UiPresentationRuntimeGeneration,
    capability_generation: UiPresentationCapabilityGeneration,
}

impl UiBackendSubmissionIdentity {
    pub(crate) const fn new(
        admission_id: UiPresentationAttemptId,
        packet: UiPaintPacketIdentity,
        target: RenderTarget,
        presentation_epoch: UiPresentationEpoch,
        runtime_generation: UiPresentationRuntimeGeneration,
        capability_generation: UiPresentationCapabilityGeneration,
    ) -> Self {
        Self {
            admission_id,
            packet,
            target,
            presentation_epoch,
            runtime_generation,
            capability_generation,
        }
    }

    #[must_use]
    pub const fn admission_id(self) -> UiPresentationAttemptId {
        self.admission_id
    }

    #[must_use]
    pub const fn packet(self) -> UiPaintPacketIdentity {
        self.packet
    }

    #[must_use]
    pub const fn target(self) -> RenderTarget {
        self.target
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> UiPresentationEpoch {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn runtime_generation(self) -> UiPresentationRuntimeGeneration {
        self.runtime_generation
    }

    #[must_use]
    pub const fn capability_generation(self) -> UiPresentationCapabilityGeneration {
        self.capability_generation
    }

    #[must_use]
    pub fn matches_receipt(self, receipt: UiPresentationReceipt) -> bool {
        let packet = self.packet;
        receipt.surface_id() == packet.surface_id()
            && receipt.frame_revision() == packet.frame_revision()
            && receipt.slot_generation() == packet.slot_generation()
            && receipt.route_revision() == packet.route_revision()
            && receipt.geometry_generation() == packet.geometry_generation()
            && receipt.presentation_revision() == packet.presentation_revision()
            && receipt.presentation_epoch() == self.presentation_epoch.get()
    }
}

/// Names one concrete backend instance and one device epoch within that instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiPresentationBackendGeneration {
    backend_instance_id: NonZeroU64,
    device_epoch: NonZeroU64,
}

impl UiPresentationBackendGeneration {
    #[must_use]
    pub const fn new(backend_instance_id: u64, device_epoch: u64) -> Option<Self> {
        let Some(backend_instance_id) = NonZeroU64::new(backend_instance_id) else {
            return None;
        };
        let Some(device_epoch) = NonZeroU64::new(device_epoch) else {
            return None;
        };
        Some(Self {
            backend_instance_id,
            device_epoch,
        })
    }

    #[must_use]
    pub const fn backend_instance_id(self) -> u64 {
        self.backend_instance_id.get()
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch.get()
    }
}

/// A Host-admitted packet that has not yet been bound to a concrete backend generation.
///
/// This type is intentionally move-only. A retry may require the same backend generation that
/// produced its deferred result.
#[derive(Debug)]
pub struct UiPreparedBackendSubmission {
    identity: UiBackendSubmissionIdentity,
    packet: Arc<UiPaintPacket>,
    required_backend_generation: Option<UiPresentationBackendGeneration>,
}

impl UiPreparedBackendSubmission {
    pub(crate) fn new(
        admission_id: UiPresentationAttemptId,
        packet: Arc<UiPaintPacket>,
        presentation_epoch: UiPresentationEpoch,
        runtime_generation: UiPresentationRuntimeGeneration,
        capability_generation: UiPresentationCapabilityGeneration,
    ) -> Self {
        let identity = UiBackendSubmissionIdentity::new(
            admission_id,
            packet.identity(),
            packet.target(),
            presentation_epoch,
            runtime_generation,
            capability_generation,
        );
        Self {
            identity,
            packet,
            required_backend_generation: None,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> UiBackendSubmissionIdentity {
        self.identity
    }

    #[must_use]
    pub fn packet(&self) -> &Arc<UiPaintPacket> {
        &self.packet
    }

    #[must_use]
    pub const fn required_backend_generation(&self) -> Option<UiPresentationBackendGeneration> {
        self.required_backend_generation
    }

    pub(crate) fn bind_backend_unchecked(
        self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> UiSubmittedBackendPresentation {
        UiSubmittedBackendPresentation {
            prepared: self,
            backend_generation,
        }
    }

    pub(crate) fn into_attempt(self) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::backend(self.identity)
    }
}

/// Exact submitted state after WGPU binds the prepared attempt to its live device generation.
#[derive(Debug)]
pub struct UiSubmittedBackendPresentation {
    prepared: UiPreparedBackendSubmission,
    backend_generation: UiPresentationBackendGeneration,
}

impl UiSubmittedBackendPresentation {
    #[must_use]
    pub const fn identity(&self) -> UiBackendSubmissionIdentity {
        self.prepared.identity()
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }

    #[must_use]
    pub fn packet(&self) -> &Arc<UiPaintPacket> {
        self.prepared.packet()
    }

    pub(crate) fn into_attempt(self) -> UiPresentationAttemptIdentity {
        self.prepared.into_attempt()
    }

    pub(crate) fn defer(self) -> UiDeferredBackendPresentation {
        UiDeferredBackendPresentation { submitted: self }
    }
}

/// Copyable diagnostic identity for one exact deferred backend submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiPresentationRetryIdentity {
    submission: UiBackendSubmissionIdentity,
    backend_generation: UiPresentationBackendGeneration,
}

impl UiPresentationRetryIdentity {
    #[must_use]
    pub const fn attempt(self) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::backend(self.submission)
    }

    #[must_use]
    pub const fn submission(self) -> UiBackendSubmissionIdentity {
        self.submission
    }

    #[must_use]
    pub const fn backend_generation(self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }
}

/// Move-only retry authority retained after a retryable backend deferral.
#[derive(Debug)]
pub struct UiDeferredBackendPresentation {
    submitted: UiSubmittedBackendPresentation,
}

impl UiDeferredBackendPresentation {
    #[must_use]
    pub const fn identity(&self) -> UiPresentationRetryIdentity {
        UiPresentationRetryIdentity {
            submission: self.submitted.identity(),
            backend_generation: self.submitted.backend_generation(),
        }
    }

    #[must_use]
    pub const fn attempt(&self) -> UiPresentationAttemptIdentity {
        self.identity().attempt()
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.submitted.backend_generation()
    }

    #[must_use]
    pub fn packet(&self) -> &Arc<UiPaintPacket> {
        self.submitted.packet()
    }

    pub(crate) fn into_retry(self) -> UiPreparedBackendSubmission {
        let UiSubmittedBackendPresentation {
            mut prepared,
            backend_generation,
        } = self.submitted;
        prepared.required_backend_generation = Some(backend_generation);
        prepared
    }
}

/// Backend-issued, non-cloneable authority to commit one exact accepted submission.
#[derive(Debug)]
pub struct OneShotPresentationCommit {
    submitted: UiSubmittedBackendPresentation,
}

impl OneShotPresentationCommit {
    pub(crate) const fn issue(submitted: UiSubmittedBackendPresentation) -> Self {
        Self { submitted }
    }

    #[must_use]
    pub const fn attempt(&self) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::backend(self.submitted.identity())
    }

    #[must_use]
    pub const fn submission(&self) -> UiBackendSubmissionIdentity {
        self.submitted.identity()
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> UiPresentationEpoch {
        self.submitted.identity().presentation_epoch()
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.submitted.backend_generation()
    }

    #[must_use]
    pub fn packet(&self) -> &Arc<UiPaintPacket> {
        self.submitted.packet()
    }

    pub(crate) fn accept(self) -> UiAcceptedBackendPresentation {
        UiAcceptedBackendPresentation {
            identity: self.submitted.identity(),
            packet: self.submitted.prepared.packet,
            backend_generation: self.submitted.backend_generation,
        }
    }
}

/// Exact backend acceptance after the one-shot commit authority has been consumed.
#[derive(Debug)]
pub struct UiAcceptedBackendPresentation {
    identity: UiBackendSubmissionIdentity,
    packet: Arc<UiPaintPacket>,
    backend_generation: UiPresentationBackendGeneration,
}

impl UiAcceptedBackendPresentation {
    #[must_use]
    pub const fn identity(&self) -> UiBackendSubmissionIdentity {
        self.identity
    }

    #[must_use]
    pub fn packet(&self) -> &Arc<UiPaintPacket> {
        &self.packet
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }
}

/// Fault ownership determines which lifecycle generation may be retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPresentationBackendFaultDomain {
    Attempt,
    Target,
    Device,
}

/// Closed presentation result classification shared by the planner and concrete backends.
#[derive(Debug)]
pub enum UiPresentationOutcome {
    Presented(OneShotPresentationCommit),
    ContentRejected {
        attempt: UiPresentationAttemptIdentity,
        message: String,
    },
    Deferred {
        retry: UiDeferredBackendPresentation,
        message: String,
    },
    BackendFault {
        attempt: Option<UiPresentationAttemptIdentity>,
        backend_generation: Option<UiPresentationBackendGeneration>,
        domain: UiPresentationBackendFaultDomain,
        message: String,
    },
    Aborted {
        attempt: UiPresentationAttemptIdentity,
        message: String,
    },
    Superseded {
        attempt: UiPresentationAttemptIdentity,
        successor: UiPresentationAttemptIdentity,
        message: String,
    },
}

impl UiPresentationOutcome {
    #[must_use]
    pub fn attempt(&self) -> Option<UiPresentationAttemptIdentity> {
        match self {
            Self::Presented(commit) => Some(commit.attempt()),
            Self::ContentRejected { attempt, .. }
            | Self::Aborted { attempt, .. }
            | Self::Superseded { attempt, .. } => Some(*attempt),
            Self::Deferred { retry, .. } => Some(retry.attempt()),
            Self::BackendFault { attempt, .. } => *attempt,
        }
    }

    #[must_use]
    pub const fn fault_domain(&self) -> Option<UiPresentationBackendFaultDomain> {
        match self {
            Self::BackendFault { domain, .. } => Some(*domain),
            _ => None,
        }
    }

    #[must_use]
    pub const fn successor(&self) -> Option<UiPresentationAttemptIdentity> {
        match self {
            Self::Superseded { successor, .. } => Some(*successor),
            _ => None,
        }
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Presented(_) => None,
            Self::ContentRejected { message, .. }
            | Self::Deferred { message, .. }
            | Self::BackendFault { message, .. }
            | Self::Aborted { message, .. }
            | Self::Superseded { message, .. } => Some(message),
        }
    }
}
