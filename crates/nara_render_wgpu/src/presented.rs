use nara_app::RuntimeGeneration;
use nara_render::{RenderFrameSourceGeneration, RenderImagePacket};
use nara_window::WindowId;

use crate::editor_execution::WgpuEditorViewId;

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

/// Exact process-local facts for one submitted Editor image target.
///
/// This receipt proves queue submission for the image target. It deliberately carries no surface
/// or present fact.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuSubmittedImageReceipt {
    view_id: WgpuEditorViewId,
    target: RenderImagePacket,
    target_generation: u64,
    source_generation: RenderFrameSourceGeneration,
    source_handoff_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    render_transaction_frame_index: u64,
}

impl WgpuSubmittedImageReceipt {
    pub(crate) const fn new(
        view_id: WgpuEditorViewId,
        target: RenderImagePacket,
        target_generation: u64,
        source_generation: RenderFrameSourceGeneration,
        source_handoff_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        render_transaction_frame_index: u64,
    ) -> Self {
        Self {
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

    #[must_use]
    pub const fn view_id(self) -> WgpuEditorViewId {
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
enum WgpuEditorWindowPresentationTarget {
    HostSurface {
        generation: u64,
    },
    /// Process-unique Dear ImGui native registration, not a surface configuration generation.
    #[cfg(feature = "editor-imgui")]
    DelegatedNativeWindow {
        instance: u64,
    },
}

/// Exact Editor-window submission and presentation-request facts.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuEditorWindowPresentedReceipt {
    window_id: WindowId,
    target: WgpuEditorWindowPresentationTarget,
    backend_instance_id: u64,
    device_epoch: u64,
    editor_frame_index: u64,
}

impl WgpuEditorWindowPresentedReceipt {
    pub(crate) const fn new(
        window_id: WindowId,
        surface_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        editor_frame_index: u64,
    ) -> Self {
        Self {
            window_id,
            target: WgpuEditorWindowPresentationTarget::HostSurface {
                generation: surface_generation,
            },
            backend_instance_id,
            device_epoch,
            editor_frame_index,
        }
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn surface_generation(self) -> Option<u64> {
        match self.target {
            WgpuEditorWindowPresentationTarget::HostSurface { generation } => Some(generation),
            #[cfg(feature = "editor-imgui")]
            WgpuEditorWindowPresentationTarget::DelegatedNativeWindow { .. } => None,
        }
    }

    #[must_use]
    pub const fn delegated_native_instance(self) -> Option<u64> {
        match self.target {
            #[cfg(feature = "editor-imgui")]
            WgpuEditorWindowPresentationTarget::DelegatedNativeWindow { instance } => {
                Some(instance)
            }
            WgpuEditorWindowPresentationTarget::HostSurface { .. } => None,
        }
    }

    #[cfg(feature = "editor-imgui")]
    pub(crate) const fn delegated(
        window_id: WindowId,
        instance: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        editor_frame_index: u64,
    ) -> Self {
        Self {
            window_id,
            target: WgpuEditorWindowPresentationTarget::DelegatedNativeWindow { instance },
            backend_instance_id,
            device_epoch,
            editor_frame_index,
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
    pub const fn editor_frame_index(self) -> u64 {
        self.editor_frame_index
    }
}

/// Unforgeable join proving the exact viewport image was sampled in a presented Editor window.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuEditorCompositeReceipt {
    image: WgpuSubmittedImageReceipt,
    window: WgpuEditorWindowPresentedReceipt,
    presentation_generation: u64,
}

impl WgpuEditorCompositeReceipt {
    pub(crate) const fn new(
        image: WgpuSubmittedImageReceipt,
        window: WgpuEditorWindowPresentedReceipt,
        presentation_generation: u64,
    ) -> Self {
        Self {
            image,
            window,
            presentation_generation,
        }
    }

    #[must_use]
    pub const fn image(self) -> WgpuSubmittedImageReceipt {
        self.image
    }

    #[must_use]
    pub const fn window(self) -> WgpuEditorWindowPresentedReceipt {
        self.window
    }

    #[must_use]
    pub const fn presentation_generation(self) -> u64 {
        self.presentation_generation
    }
}
