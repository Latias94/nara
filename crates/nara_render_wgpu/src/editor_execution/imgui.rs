use dear_imgui_rs::FrameToken;
use dear_imgui_wgpu::multi_viewport::{
    WgpuPreparedViewportFrame, WgpuViewportFrameReport, WinitViewportRoute,
};
use nara_render::RenderWindowPacket;
use nara_window::WindowId;
use winit::event_loop::ActiveEventLoop;

use super::{
    WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorFrameOutcome,
    WgpuEditorViewportSample, WgpuEditorWindowFrame, join_composite_receipts,
};
use crate::backend::WgpuEditorDeviceNamespace;
use crate::presented::{WgpuEditorWindowPresentedReceipt, WgpuSubmittedImageReceipt};

fn delegated_window_id(primary: WindowId, surface_id: u64) -> Option<WindowId> {
    let raw = if surface_id >= primary.get() {
        surface_id.checked_add(1)?
    } else {
        surface_id
    };
    Some(WindowId::new(raw))
}

impl WgpuEditorExecution {
    /// Prepares one fresh toolkit frame and immediately joins its actual secondary presents.
    ///
    /// This synchronous borrow prevents Nara image targets from being submitted or replaced
    /// between secondary rendering and the join. The route retains secondary acquire/submit/
    /// present ownership; the returned main frame still needs the ordinary primary paint.
    pub fn prepare_imgui_viewports<'context>(
        &mut self,
        window: RenderWindowPacket,
        route: &WinitViewportRoute,
        event_loop: &ActiveEventLoop,
        frame: FrameToken<'context>,
        viewport_submissions: &[WgpuSubmittedImageReceipt],
    ) -> Result<
        (
            WgpuPreparedViewportFrame<'context>,
            Vec<WgpuEditorFrameOutcome>,
        ),
        WgpuEditorExecutionError,
    > {
        let host_frame_index = self
            .host_frame_index
            .ok_or(WgpuEditorExecutionError::FrameTransactionUnavailable)?;
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        for &image in viewport_submissions {
            self.validate_viewport_submission(&namespace, image)?;
        }
        let prepared = route
            .prepare(event_loop, frame)
            .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?;
        let outcomes = self.join_imgui_secondary_presentations(
            &namespace,
            host_frame_index,
            prepared.secondary_report(),
            viewport_submissions,
        )?;
        Ok((prepared, outcomes))
    }

    fn join_imgui_secondary_presentations(
        &mut self,
        namespace: &WgpuEditorDeviceNamespace,
        host_frame_index: u64,
        report: &WgpuViewportFrameReport,
        viewport_submissions: &[WgpuSubmittedImageReceipt],
    ) -> Result<Vec<WgpuEditorFrameOutcome>, WgpuEditorExecutionError> {
        let mut outcomes = Vec::new();
        for presented in report.presented_frames() {
            if presented.device() != &namespace.device || presented.queue() != &namespace.queue {
                return Err(WgpuEditorExecutionError::toolkit_paint(
                    "a secondary presentation used another GPU device or queue",
                ));
            }
            if presented.presentation_id() <= self.last_imgui_presentation_id {
                return Err(WgpuEditorExecutionError::toolkit_paint(
                    "a secondary presentation record was already consumed",
                ));
            }
            // Surface identities are process-global and never reused. Skip the primary identity.
            let window_id = delegated_window_id(self.primary_window_id, presented.surface_id())
                .ok_or_else(|| {
                    WgpuEditorExecutionError::toolkit_paint(
                        "the secondary window identity space was exhausted",
                    )
                })?;
            let samples = viewport_submissions
                .iter()
                .copied()
                .filter(|image| {
                    self.views
                        .get(&image.view_id())
                        .and_then(|view| view.target.as_ref())
                        .is_some_and(|target| {
                            presented.sampled_texture_views().contains(&target.view)
                        })
                })
                .map(|image| WgpuEditorViewportSample { image })
                .collect::<Vec<_>>();
            let window = WgpuEditorWindowPresentedReceipt::new(
                window_id,
                presented.surface_generation(),
                namespace.backend_instance_id,
                namespace.device_epoch,
                host_frame_index,
            );
            let composites = join_composite_receipts(
                &mut self.last_presentation_generation,
                viewport_submissions,
                &samples,
                window,
            )?;
            self.last_imgui_presentation_id = presented.presentation_id();
            self.backend.record_editor_window_presentation();
            outcomes.push(WgpuEditorFrameOutcome {
                window: Some(window),
                composites,
            });
        }
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests;

impl WgpuEditorWindowFrame<'_> {
    /// Records the prepared primary viewport and its actual sampled Nara images together.
    pub fn paint_imgui_main(
        &mut self,
        route: &WinitViewportRoute,
        prepared: WgpuPreparedViewportFrame<'_>,
        clear: wgpu::Color,
    ) -> Result<(), WgpuEditorExecutionError> {
        self.paint_imgui(clear, |pass, extent| {
            route
                .render_main_with_samples(prepared, pass, extent)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))
        })
    }

    /// Records a single-window reconciled viewport using the same exact sampling contract.
    pub fn paint_imgui_docked(
        &mut self,
        renderer: &mut dear_imgui_wgpu::WgpuRenderer,
        prepared: dear_imgui_rs::render::ReconciledFrame<'_>,
        clear: wgpu::Color,
    ) -> Result<(), WgpuEditorExecutionError> {
        self.paint_imgui(clear, |pass, extent| {
            renderer
                .render_reconciled_with_samples(prepared, pass, extent)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))
        })
    }

    fn paint_imgui(
        &mut self,
        clear: wgpu::Color,
        paint: impl FnOnce(
            &mut wgpu::RenderPass<'_>,
            dear_imgui_wgpu::FramebufferExtent,
        )
            -> Result<dear_imgui_wgpu::WgpuRenderReport, WgpuEditorExecutionError>,
    ) -> Result<(), WgpuEditorExecutionError> {
        let extent = self.surface_extent();
        let report = self.encode(|_, _, encoder, target| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_editor_imgui_primary"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            paint(
                &mut pass,
                dear_imgui_wgpu::FramebufferExtent::new(extent.width, extent.height),
            )
        })?;
        for (image, texture) in self.viewport_textures {
            if report.sampled_texture_views().contains(texture) {
                self.sampled_viewports
                    .push(WgpuEditorViewportSample { image: *image });
            }
        }
        Ok(())
    }
}
