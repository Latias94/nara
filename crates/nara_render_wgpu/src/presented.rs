use nara_app::RuntimeGeneration;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_IMAGE_OUTPUT: AtomicU64 = AtomicU64::new(1);
use nara_render::{RenderFrameSourceGeneration, RenderImagePacket};
use nara_window::WindowId;

use crate::host::WgpuRenderProducerId;

/// Exact process-local facts for one WGPU surface submission and present call.
///
/// The receipt proves that the backend submitted commands and invoked present for the surface
/// lease identified here. It does not claim GPU completion, display scan-out, or user visibility.
/// Construction is crate-private so product verification cannot forge backend acceptance.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuPresentedFrameReceipt {
    runtime_generation: RuntimeGeneration,
    window_id: WindowId,
    surface_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    app_frame_index: u64,
    render_transaction_frame_index: u64,
}

impl WgpuPresentedFrameReceipt {
    pub(crate) const fn new(
        runtime_generation: RuntimeGeneration,
        window_id: WindowId,
        surface_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        app_frame_index: u64,
        render_transaction_frame_index: u64,
    ) -> Self {
        Self {
            runtime_generation,
            window_id,
            surface_generation,
            backend_instance_id,
            device_epoch,
            app_frame_index,
            render_transaction_frame_index,
        }
    }

    #[must_use]
    pub const fn runtime_generation(self) -> RuntimeGeneration {
        self.runtime_generation
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn surface_generation(self) -> u64 {
        self.surface_generation
    }

    #[must_use]
    pub const fn backend_instance_id(self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn app_frame_index(self) -> u64 {
        self.app_frame_index
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> u64 {
        self.render_transaction_frame_index
    }
}

/// Exact process-local facts for one successfully published immutable image output.
///
/// This receipt proves queue submission for the image target. It deliberately carries no surface
/// or present fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuSubmittedImageReceipt {
    output_version: u64,
    view_id: WgpuRenderProducerId,
    target: RenderImagePacket,
    target_generation: u64,
    source_generation: RenderFrameSourceGeneration,
    source_handoff_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    render_transaction_frame_index: u64,
}

impl WgpuSubmittedImageReceipt {
    pub(crate) fn new(
        view_id: WgpuRenderProducerId,
        target: RenderImagePacket,
        target_generation: u64,
        source_generation: RenderFrameSourceGeneration,
        source_handoff_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        render_transaction_frame_index: u64,
    ) -> Self {
        let output_version = NEXT_IMAGE_OUTPUT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .unwrap_or_else(|_| std::process::abort());
        Self {
            output_version,
            view_id,
            target,
            target_generation,
            source_generation,
            source_handoff_generation,
            backend_instance_id,
            device_epoch,
            render_transaction_frame_index,
        }
    }

    /// Identifies exact pixels, independently of the target's configuration generation.
    #[must_use]
    pub const fn output_version(self) -> u64 {
        self.output_version
    }

    #[must_use]
    pub const fn producer_id(self) -> WgpuRenderProducerId {
        self.view_id
    }

    #[must_use]
    pub const fn target(self) -> RenderImagePacket {
        self.target
    }

    #[must_use]
    pub const fn target_generation(self) -> u64 {
        self.target_generation
    }

    #[must_use]
    pub const fn source_generation(self) -> RenderFrameSourceGeneration {
        self.source_generation
    }

    #[must_use]
    pub const fn source_handoff_generation(self) -> u64 {
        self.source_handoff_generation
    }

    #[must_use]
    pub const fn backend_instance_id(self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> u64 {
        self.render_transaction_frame_index
    }
}

/// Target identity whose presentation API was called by its owning backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WgpuWindowPresentationTarget {
    HostSurface {
        generation: u64,
    },
    /// Process-unique Dear ImGui native registration, not a surface configuration generation.
    #[cfg(feature = "editor-imgui")]
    DelegatedNativeWindow {
        instance: u64,
    },
}

/// Exact native-window submission and presentation-request facts from its original Host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuWindowPresentedReceipt {
    window_id: WindowId,
    target: WgpuWindowPresentationTarget,
    backend_instance_id: u64,
    device_epoch: u64,
    display_frame_index: u64,
}

impl WgpuWindowPresentedReceipt {
    pub(crate) const fn new(
        window_id: WindowId,
        surface_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        display_frame_index: u64,
    ) -> Self {
        Self {
            window_id,
            target: WgpuWindowPresentationTarget::HostSurface {
                generation: surface_generation,
            },
            backend_instance_id,
            device_epoch,
            display_frame_index,
        }
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn surface_generation(self) -> Option<u64> {
        match self.target {
            WgpuWindowPresentationTarget::HostSurface { generation } => Some(generation),
            #[cfg(feature = "editor-imgui")]
            WgpuWindowPresentationTarget::DelegatedNativeWindow { .. } => None,
        }
    }

    #[must_use]
    pub const fn delegated_native_instance(self) -> Option<u64> {
        match self.target {
            #[cfg(feature = "editor-imgui")]
            WgpuWindowPresentationTarget::DelegatedNativeWindow { instance } => Some(instance),
            WgpuWindowPresentationTarget::HostSurface { .. } => None,
        }
    }

    #[cfg(feature = "editor-imgui")]
    pub(crate) const fn delegated(
        window_id: WindowId,
        instance: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        display_frame_index: u64,
    ) -> Self {
        Self {
            window_id,
            target: WgpuWindowPresentationTarget::DelegatedNativeWindow { instance },
            backend_instance_id,
            device_epoch,
            display_frame_index,
        }
    }

    #[must_use]
    pub const fn backend_instance_id(self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn display_frame_index(self) -> u64 {
        self.display_frame_index
    }
}

/// Exact image occurrence sampled in one successfully presented native-window attempt.
///
/// This does not establish widget mapping, interaction authority or monitor scanout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuImageCompositeReceipt {
    image: WgpuSubmittedImageReceipt,
    occurrence: crate::output::WgpuDisplayOccurrence,
    window: WgpuWindowPresentedReceipt,
    presentation_generation: u64,
}

impl WgpuImageCompositeReceipt {
    pub(crate) const fn new(
        image: WgpuSubmittedImageReceipt,
        occurrence: crate::output::WgpuDisplayOccurrence,
        window: WgpuWindowPresentedReceipt,
        presentation_generation: u64,
    ) -> Self {
        Self {
            image,
            occurrence,
            window,
            presentation_generation,
        }
    }

    #[must_use]
    pub const fn occurrence(self) -> crate::output::WgpuDisplayOccurrence {
        self.occurrence
    }

    #[must_use]
    pub const fn image(self) -> WgpuSubmittedImageReceipt {
        self.image
    }

    #[must_use]
    pub const fn window(self) -> WgpuWindowPresentedReceipt {
        self.window
    }

    #[must_use]
    pub const fn presentation_generation(self) -> u64 {
        self.presentation_generation
    }
}
