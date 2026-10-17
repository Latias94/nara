use dear_imgui_rs::{Context, FrameToken};
use dear_imgui_wgpu::{
    ExternalTextureBinding, ExternalTextureId, WgpuDrawDependencies, WgpuFrameStage, WgpuInitInfo,
    WgpuRenderer,
    multi_viewport::{WgpuPreparedMainFrame, WgpuViewportFrameReport, WinitViewportRoute},
};
use dear_imgui_winit::WinitPlatform;
use nara_render::RenderWindowPacket;
use nara_window::WindowId;
use winit::event_loop::ActiveEventLoop;

use super::{
    WgpuEditorDeviceScope, WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorFrameOutcome,
    WgpuEditorViewportSample, WgpuEditorWindowFrame, join_composite_receipts,
};
use crate::presented::{WgpuEditorWindowPresentedReceipt, WgpuSubmittedImageReceipt};

fn delegated_window_id(primary: WindowId, instance: u64) -> Option<WindowId> {
    let raw = if instance >= primary.get() {
        instance.checked_add(1)?
    } else {
        instance
    };
    Some(WindowId::new(raw))
}

enum RendererOwner {
    Docked(WgpuRenderer),
    Native(WinitViewportRoute),
}

/// First-party renderer owned by the exact Nara GPU scope that initialized it.
///
/// No entry point accepts caller-constructed observations or arbitrary renderer routes. Binding
/// revisions are joined with the actual registered view and Nara's current content receipt.
#[doc(hidden)]
pub struct WgpuEditorImguiRenderer {
    owner: RendererOwner,
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend_instance_id: u64,
    device_epoch: u64,
    bindings: Vec<(ExternalTextureBinding, wgpu::TextureView)>,
    last_secondary_scope: Option<dear_imgui_wgpu::WgpuFrameScope>,
}

/// Move-only prepared UI work, retaining the toolkit's frame ownership.
#[doc(hidden)]
pub struct WgpuEditorImguiFrame<'frame>(PreparedFrame<'frame>);

enum PreparedFrame<'frame> {
    Docked(dear_imgui_rs::render::ReconciledFrame<'frame>),
    Native(WgpuPreparedMainFrame<'frame>),
}

impl WgpuEditorDeviceScope<'_> {
    pub fn create_imgui_renderer(
        &self,
        context: &mut Context,
        native_platform: Option<&WinitPlatform>,
    ) -> Result<WgpuEditorImguiRenderer, WgpuEditorExecutionError> {
        let mut renderer = WgpuRenderer::new(
            WgpuInitInfo::new(self.device.clone(), self.queue.clone(), self.surface_format)
                .with_instance(self.instance.clone())
                .with_adapter(self.adapter.clone()),
            context,
        )
        .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?;
        renderer.set_gamma_mode(dear_imgui_wgpu::GammaMode::Auto);
        let owner = if let Some(platform) = native_platform {
            match WinitViewportRoute::attach(context, platform, renderer) {
                Ok(route) => RendererOwner::Native(route),
                Err(failure) => {
                    let (error, mut renderer) = failure.into_parts();
                    self.queue.submit([]);
                    let drain = self.device.poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(std::time::Duration::from_secs(5)),
                    });
                    let cleanup = renderer.shutdown(context);
                    return Err(WgpuEditorExecutionError::toolkit_paint(
                        match (drain, cleanup) {
                            (Ok(_), Ok(())) => error.to_string(),
                            (drain, cleanup) => format!(
                                "{error}; initialization cleanup: drain={drain:?}, renderer={cleanup:?}"
                            ),
                        },
                    ));
                }
            }
        } else {
            RendererOwner::Docked(renderer)
        };
        Ok(WgpuEditorImguiRenderer {
            owner,
            device: self.device.clone(),
            queue: self.queue.clone(),
            backend_instance_id: self.backend_instance_id,
            device_epoch: self.device_epoch,
            bindings: Vec::new(),
            last_secondary_scope: None,
        })
    }
}

impl WgpuEditorImguiRenderer {
    pub fn register_external_texture(
        &mut self,
        view: &wgpu::TextureView,
    ) -> Result<ExternalTextureId, WgpuEditorExecutionError> {
        let texture = match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .register_external_texture(view)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .register_external_texture(view)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
        };
        let binding = self.external_binding(texture)?;
        self.bindings.push((binding, view.clone()));
        Ok(texture)
    }

    fn external_binding(
        &self,
        texture: ExternalTextureId,
    ) -> Result<ExternalTextureBinding, WgpuEditorExecutionError> {
        match &self.owner {
            RendererOwner::Docked(renderer) => renderer
                .external_texture_binding(texture)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
            RendererOwner::Native(route) => route
                .external_texture_binding(texture)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
        }
    }

    #[cfg(test)]
    fn update_external_texture(
        &mut self,
        texture: ExternalTextureId,
        view: &wgpu::TextureView,
    ) -> Result<(), WgpuEditorExecutionError> {
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .update_external_texture(texture, view)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .update_external_texture(texture, view)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
        }
        let updated = self.external_binding(texture)?;
        if let Some((binding, registered)) = self
            .bindings
            .iter_mut()
            .find(|(binding, _)| binding.texture == texture)
        {
            *binding = updated;
            *registered = view.clone();
        }
        Ok(())
    }

    pub fn unregister_external_texture(
        &mut self,
        texture: ExternalTextureId,
    ) -> Result<(), WgpuEditorExecutionError> {
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .unregister_external_texture(texture)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .unregister_external_texture(texture)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?,
        }
        self.bindings
            .retain(|(binding, _)| binding.texture != texture);
        Ok(())
    }

    fn samples(&self, dependencies: &WgpuDrawDependencies, view: &wgpu::TextureView) -> bool {
        self.bindings.iter().any(|(binding, registered_view)| {
            registered_view == view && dependencies.external_bindings.contains(binding)
        })
    }

    pub fn shutdown(&mut self, context: &mut Context) -> Result<(), WgpuEditorExecutionError> {
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .shutdown(context)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
            RendererOwner::Native(route) => route
                .shutdown(context)
                .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
        }
    }

    pub fn paint(
        &mut self,
        prepared: WgpuEditorImguiFrame<'_>,
        frame: &mut WgpuEditorWindowFrame<'_>,
        clear: wgpu::Color,
    ) -> Result<(), WgpuEditorExecutionError> {
        if frame.device != &self.device || frame.queue != &self.queue {
            return Err(WgpuEditorExecutionError::toolkit_paint(
                "the renderer belongs to another GPU device or queue",
            ));
        }
        let extent = frame.surface_extent();
        let dependencies = frame.encode(|_, _, encoder, target| {
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
            let extent = dear_imgui_wgpu::FramebufferExtent::new(extent.width, extent.height);
            match (&mut self.owner, prepared.0) {
                (RendererOwner::Docked(renderer), PreparedFrame::Docked(prepared)) => renderer
                    .render_reconciled(prepared, &mut pass, extent)
                    .map(|recorded| recorded.dependencies)
                    .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
                (RendererOwner::Native(route), PreparedFrame::Native(prepared)) => route
                    .record_main(prepared, &mut pass, extent)
                    .map(|observed| observed.dependencies)
                    .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string())),
                _ => Err(WgpuEditorExecutionError::toolkit_paint(
                    "prepared frame belongs to another renderer route",
                )),
            }
        })?;
        for (image, texture) in frame.viewport_textures {
            if self.samples(&dependencies, texture) {
                frame
                    .sampled_viewports
                    .push(WgpuEditorViewportSample { image: *image });
            }
        }
        Ok(())
    }
}

impl WgpuEditorExecution {
    /// Maps a process-unique native registration without depending on whether it rendered.
    pub fn imgui_window_id(
        &self,
        instance: dear_imgui_rs::NativeWindowInstanceId,
    ) -> Result<WindowId, WgpuEditorExecutionError> {
        delegated_window_id(self.primary_window_id, instance.get().get()).ok_or_else(|| {
            WgpuEditorExecutionError::toolkit_paint(
                "the secondary window identity space was exhausted",
            )
        })
    }
    /// Completes secondary work and joins its observations before acquiring the primary surface.
    ///
    /// Partial successful presentations are appended before any toolkit error is returned. The
    /// exclusive execution borrow prevents content replacement between rendering and this join.
    pub fn prepare_imgui_frame<'context>(
        &mut self,
        window: RenderWindowPacket,
        renderer: &mut WgpuEditorImguiRenderer,
        event_loop: &ActiveEventLoop,
        frame: FrameToken<'context>,
        viewport_submissions: &[WgpuSubmittedImageReceipt],
        outcomes: &mut Vec<WgpuEditorFrameOutcome>,
    ) -> Result<WgpuEditorImguiFrame<'context>, WgpuEditorExecutionError> {
        let host_frame_index = self
            .host_frame_index
            .ok_or(WgpuEditorExecutionError::FrameTransactionUnavailable)?;
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        if renderer.backend_instance_id != namespace.backend_instance_id
            || renderer.device_epoch != namespace.device_epoch
            || renderer.device != namespace.device
            || renderer.queue != namespace.queue
        {
            return Err(WgpuEditorExecutionError::toolkit_paint(
                "the renderer belongs to another GPU device or queue",
            ));
        }
        for &image in viewport_submissions {
            self.validate_viewport_submission(&namespace, image)?;
        }
        let (prepared, report, fault) = match &mut renderer.owner {
            RendererOwner::Docked(inner) => {
                let consumer = inner
                    .renderer_consumer()
                    .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?;
                let pending = frame
                    .try_render(consumer)
                    .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?;
                let prepared = inner
                    .reconcile_frame(pending)
                    .map_err(|error| WgpuEditorExecutionError::toolkit_paint(error.to_string()))?;
                return Ok(WgpuEditorImguiFrame(PreparedFrame::Docked(prepared)));
            }
            RendererOwner::Native(route) => match route.prepare(event_loop, frame) {
                Ok(prepared) => match route.complete_secondary(prepared, []) {
                    Ok(prepared) => {
                        let report = prepared.secondary_report().clone();
                        (Some(prepared), report, None)
                    }
                    Err(error) => {
                        let message = error.to_string();
                        let (report, _) = error.into_parts();
                        (None, report, Some(message))
                    }
                },
                Err(error) => {
                    let message = error.to_string();
                    let (report, _) = error.into_parts();
                    (None, report, Some(message))
                }
            },
        };
        self.join_imgui_secondary_presentations(
            renderer,
            host_frame_index,
            report,
            viewport_submissions,
            outcomes,
        )?;
        if let Some(message) = fault {
            return Err(WgpuEditorExecutionError::toolkit_paint(message));
        }
        Ok(WgpuEditorImguiFrame(PreparedFrame::Native(
            prepared.expect("successful secondary work retains its main frame"),
        )))
    }

    // Only observations returned by this owned route's live work may establish presentation
    // authority. Observation structs alone are detached metadata, not a capability.
    fn join_imgui_secondary_presentations(
        &mut self,
        renderer: &mut WgpuEditorImguiRenderer,
        host_frame_index: u64,
        report: WgpuViewportFrameReport,
        viewport_submissions: &[WgpuSubmittedImageReceipt],
        outcomes: &mut Vec<WgpuEditorFrameOutcome>,
    ) -> Result<(), WgpuEditorExecutionError> {
        if let Some(scope) = report.scope() {
            if renderer.last_secondary_scope == Some(scope) {
                return Err(WgpuEditorExecutionError::toolkit_paint(
                    "the secondary frame report was already consumed",
                ));
            }
            renderer.last_secondary_scope = Some(scope);
        }
        for observed in report.into_observations() {
            if observed.stage != WgpuFrameStage::PresentationRequested || observed.target.is_main {
                continue;
            }
            let instance = observed.target.instance_id.get().get();
            let window_id =
                delegated_window_id(self.primary_window_id, instance).ok_or_else(|| {
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
                            renderer.samples(&observed.dependencies, &target.view)
                        })
                })
                .map(|image| WgpuEditorViewportSample { image })
                .collect::<Vec<_>>();
            let window = WgpuEditorWindowPresentedReceipt::delegated(
                window_id,
                instance,
                renderer.backend_instance_id,
                renderer.device_epoch,
                host_frame_index,
            );
            let composites = join_composite_receipts(
                &mut self.last_presentation_generation,
                viewport_submissions,
                &samples,
                window,
            )?;
            self.backend.record_editor_window_presentation();
            outcomes.push(WgpuEditorFrameOutcome {
                window: Some(window),
                composites,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
