use std::{num::NonZeroU64, sync::Arc};

use nara_render::RenderTarget;

use super::{UiPaintPacket, UiPaintPacketIdentity};

/// Exact packet and target identity for one presentation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiPresentationAttemptIdentity {
    packet: UiPaintPacketIdentity,
    target: Option<RenderTarget>,
}

impl UiPresentationAttemptIdentity {
    pub(crate) const fn routed(packet: UiPaintPacketIdentity, target: RenderTarget) -> Self {
        Self {
            packet,
            target: Some(target),
        }
    }

    pub(crate) const fn unrouted(packet: UiPaintPacketIdentity) -> Self {
        Self {
            packet,
            target: None,
        }
    }

    #[must_use]
    pub const fn packet(self) -> UiPaintPacketIdentity {
        self.packet
    }

    #[must_use]
    pub const fn target(self) -> Option<RenderTarget> {
        self.target
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

/// Exact retry authority for one backend generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiPresentationRetryIdentity {
    attempt: UiPresentationAttemptIdentity,
    backend_generation: UiPresentationBackendGeneration,
}

impl UiPresentationRetryIdentity {
    pub(crate) const fn new(
        attempt: UiPresentationAttemptIdentity,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Self {
        Self {
            attempt,
            backend_generation,
        }
    }

    #[must_use]
    pub const fn attempt(self) -> UiPresentationAttemptIdentity {
        self.attempt
    }

    #[must_use]
    pub const fn backend_generation(self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }
}

/// Backend-issued, non-cloneable authority to commit one exact accepted packet.
#[derive(Debug)]
pub struct OneShotPresentationCommit {
    packet: Arc<UiPaintPacket>,
    presentation_epoch: u64,
    backend_generation: UiPresentationBackendGeneration,
}

impl OneShotPresentationCommit {
    pub(crate) fn issue(
        packet: Arc<UiPaintPacket>,
        presentation_epoch: u64,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Self {
        Self {
            packet,
            presentation_epoch,
            backend_generation,
        }
    }

    #[must_use]
    pub fn attempt(&self) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::routed(self.packet.identity(), self.packet.target())
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }

    #[must_use]
    pub fn into_parts(self) -> (Arc<UiPaintPacket>, u64, UiPresentationBackendGeneration) {
        (
            self.packet,
            self.presentation_epoch,
            self.backend_generation,
        )
    }
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
        retry: UiPresentationRetryIdentity,
        message: String,
    },
    BackendFault {
        attempt: Option<UiPresentationAttemptIdentity>,
        backend_generation: Option<UiPresentationBackendGeneration>,
        message: String,
    },
}

impl UiPresentationOutcome {
    #[must_use]
    pub fn attempt(&self) -> Option<UiPresentationAttemptIdentity> {
        match self {
            Self::Presented(commit) => Some(commit.attempt()),
            Self::ContentRejected { attempt, .. } => Some(*attempt),
            Self::Deferred { retry, .. } => Some(retry.attempt()),
            Self::BackendFault { attempt, .. } => *attempt,
        }
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Presented(_) => None,
            Self::ContentRejected { message, .. }
            | Self::Deferred { message, .. }
            | Self::BackendFault { message, .. } => Some(message),
        }
    }
}
