use dear_imgui_rs::Id;

#[cfg(feature = "wgpu-30")]
use super::runtime::WgpuViewportError;
#[cfg(feature = "wgpu-30")]
use crate::{FramebufferExtent, WgpuRenderReport};
#[cfg(feature = "wgpu-30")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "wgpu-30")]
static NEXT_SURFACE_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "wgpu-30")]
static NEXT_PRESENTATION_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "wgpu-30")]
fn next_identity(counter: &AtomicU64, operation: &'static str) -> Result<u64, WgpuViewportError> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| WgpuViewportError::SurfaceOperationFailed { operation })
}

#[cfg(feature = "wgpu-30")]
pub(super) fn next_surface_id() -> Result<u64, WgpuViewportError> {
    next_identity(&NEXT_SURFACE_ID, "allocate a viewport surface identity")
}

/// Backend-owned evidence for one secondary surface frame passed to WGPU's presentation API.
///
/// The record certifies API submission, not GPU completion or monitor scanout. Its sampled views
/// cover emitted standard element draws only; resources used by raw callbacks are not observed.
#[cfg(feature = "wgpu-30")]
#[derive(Debug, Eq, PartialEq)]
pub struct WgpuViewportPresentedFrame {
    viewport_id: Id,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[cfg(feature = "multi-viewport-winit")]
    window_id: winit::window::WindowId,
    surface_id: u64,
    surface_generation: u64,
    presentation_id: u64,
    extent: FramebufferExtent,
    sampled_texture_views: Vec<wgpu::TextureView>,
}

#[cfg(feature = "wgpu-30")]
impl WgpuViewportPresentedFrame {
    /// Returns the ImGui viewport ID used for routing this presentation.
    pub fn viewport_id(&self) -> Id {
        self.viewport_id
    }

    /// Returns the exact device used to record this frame.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns the exact queue used to submit and present this frame.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Returns the native Winit window receiving this frame.
    #[cfg(feature = "multi-viewport-winit")]
    pub fn window_id(&self) -> winit::window::WindowId {
        self.window_id
    }

    /// Returns a process-unique identity for this actual surface lifetime.
    ///
    /// Reconfiguration preserves this value; surface recreation allocates a new one.
    pub fn surface_id(&self) -> u64 {
        self.surface_id
    }

    /// Returns the configuration epoch within this surface lifetime.
    pub fn surface_generation(&self) -> u64 {
        self.surface_generation
    }

    /// Returns a process-unique, increasing identity allocated after the presentation call.
    ///
    /// Consumers can retain this value to reject replay of an already accepted presentation.
    pub fn presentation_id(&self) -> u64 {
        self.presentation_id
    }

    /// Returns the actual acquired texture's physical dimensions.
    pub fn extent(&self) -> FramebufferExtent {
        self.extent
    }

    /// Returns the unique texture views used by successfully emitted standard element draws.
    pub fn sampled_texture_views(&self) -> &[wgpu::TextureView] {
        &self.sampled_texture_views
    }
}

/// Kept beside the exact acquired surface texture until it is presented or discarded.
#[cfg(feature = "wgpu-30")]
pub(super) struct PendingViewportPresentation {
    pub(super) viewport_id: Id,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    #[cfg(feature = "multi-viewport-winit")]
    pub(super) window_id: winit::window::WindowId,
    pub(super) surface_id: u64,
    pub(super) surface_generation: u64,
    pub(super) extent: FramebufferExtent,
    pub(super) samples: WgpuRenderReport,
}

#[cfg(feature = "wgpu-30")]
impl PendingViewportPresentation {
    /// Called only after the associated acquired frame has been presented.
    pub(super) fn presented(self) -> Result<WgpuViewportPresentedFrame, WgpuViewportError> {
        Ok(WgpuViewportPresentedFrame {
            viewport_id: self.viewport_id,
            device: self.device,
            queue: self.queue,
            #[cfg(feature = "multi-viewport-winit")]
            window_id: self.window_id,
            surface_id: self.surface_id,
            surface_generation: self.surface_generation,
            presentation_id: next_identity(
                &NEXT_PRESENTATION_ID,
                "allocate a viewport presentation identity",
            )?,
            extent: self.extent,
            sampled_texture_views: self.samples.into_sampled_texture_views(),
        })
    }
}

/// Secondary-viewport GPU submissions collected while preparing one frame.
///
/// A viewport appears in [`Self::render_submitted_viewport_ids`] only after its command buffer
/// was submitted and its acquired surface frame was retained for presentation. It appears in
/// [`Self::present_submitted_viewport_ids`] only after the backend called the WGPU presentation
/// API for that retained frame. Both ID sets are sorted and contain no duplicates.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct WgpuViewportFrameReport {
    render_submitted_viewport_ids: Vec<Id>,
    present_submitted_viewport_ids: Vec<Id>,
    #[cfg(feature = "wgpu-30")]
    presented_frames: Vec<WgpuViewportPresentedFrame>,
}

impl WgpuViewportFrameReport {
    /// Returns sorted, unique viewport IDs whose render commands were submitted.
    pub fn render_submitted_viewport_ids(&self) -> &[Id] {
        &self.render_submitted_viewport_ids
    }

    /// Returns sorted, unique viewport IDs whose surface frames were submitted for presentation.
    pub fn present_submitted_viewport_ids(&self) -> &[Id] {
        &self.present_submitted_viewport_ids
    }

    /// Returns immutable records for actual secondary presentations in this preparation scope.
    ///
    /// A cleared or fully clipped viewport can present with no sampled views. Dropped acquired
    /// frames never produce a record, and a skipped main surface does not undo these records.
    #[cfg(feature = "wgpu-30")]
    pub fn presented_frames(&self) -> &[WgpuViewportPresentedFrame] {
        &self.presented_frames
    }

    fn finish(mut self) -> Self {
        normalize_ids(&mut self.render_submitted_viewport_ids);
        normalize_ids(&mut self.present_submitted_viewport_ids);
        self
    }
}

fn normalize_ids(ids: &mut Vec<Id>) {
    ids.sort_unstable_by_key(|id| id.raw());
    ids.dedup();
}

#[derive(Debug, Default)]
pub(super) struct FrameTraceState {
    active: Option<WgpuViewportFrameReport>,
}

impl FrameTraceState {
    pub(super) fn begin(&mut self) -> bool {
        if self.active.is_some() {
            return false;
        }
        self.active = Some(WgpuViewportFrameReport::default());
        true
    }

    pub(super) fn record_render_submitted(&mut self, viewport_id: Id) {
        if let Some(trace) = self.active.as_mut() {
            trace.render_submitted_viewport_ids.push(viewport_id);
        }
    }

    pub(super) fn record_present_submitted(&mut self, viewport_id: Id) {
        if let Some(trace) = self.active.as_mut() {
            trace.present_submitted_viewport_ids.push(viewport_id);
        }
    }

    #[cfg(feature = "wgpu-30")]
    pub(super) fn record_presented_frame(&mut self, frame: WgpuViewportPresentedFrame) {
        if let Some(trace) = self.active.as_mut() {
            trace.presented_frames.push(frame);
        }
    }

    pub(super) fn finish(&mut self) -> WgpuViewportFrameReport {
        self.active
            .take()
            .expect("a live WGPU frame-trace guard owns the active trace")
            .finish()
    }

    pub(super) fn abort(&mut self) {
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "wgpu-30")]
    static_assertions::assert_not_impl_any!(WgpuViewportPresentedFrame: Default);

    #[test]
    fn trace_is_non_nested_and_normalizes_id_sets() {
        let mut state = FrameTraceState::default();
        let low = Id::from(3_u32);
        let high = Id::from(7_u32);

        assert!(state.begin());
        assert!(!state.begin());
        state.record_render_submitted(high);
        state.record_render_submitted(low);
        state.record_render_submitted(high);
        state.record_present_submitted(high);
        state.record_present_submitted(low);
        state.record_present_submitted(low);

        let report = state.finish();
        assert_eq!(report.render_submitted_viewport_ids(), &[low, high]);
        assert_eq!(report.present_submitted_viewport_ids(), &[low, high]);
    }

    #[test]
    fn abort_discards_observations_and_inactive_recording_is_ignored() {
        let mut state = FrameTraceState::default();
        let viewport_id = Id::from(9_u32);

        state.record_render_submitted(viewport_id);
        assert!(state.begin());
        state.record_render_submitted(viewport_id);
        state.abort();

        assert!(state.begin());
        let report = state.finish();
        assert!(report.render_submitted_viewport_ids().is_empty());
        assert!(report.present_submitted_viewport_ids().is_empty());
    }

    #[cfg(feature = "wgpu-30")]
    #[test]
    fn presentation_and_surface_identity_counters_never_reuse_or_wrap() {
        let counter = AtomicU64::new(1);
        assert_eq!(next_identity(&counter, "test identity").unwrap(), 1);
        assert_eq!(next_identity(&counter, "test identity").unwrap(), 2);

        let exhausted = AtomicU64::new(u64::MAX - 1);
        assert_eq!(
            next_identity(&exhausted, "test identity").unwrap(),
            u64::MAX - 1
        );
        assert!(next_identity(&exhausted, "test identity").is_err());
        assert!(next_identity(&exhausted, "test identity").is_err());
        assert_eq!(exhausted.load(Ordering::Relaxed), u64::MAX);
    }
}
