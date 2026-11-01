use dear_imgui_rs::{Context, ContextId, FrameToken};
use dear_imgui_wgpu::{
    ExternalTextureBinding, ExternalTextureId, WgpuDrawDependencies, WgpuFrameStage, WgpuInitInfo,
    WgpuRenderer,
    multi_viewport::{WgpuPreparedMainFrame, WgpuViewportFrameReport, WinitViewportRoute},
};
use dear_imgui_winit::WinitPlatform;
use nara_render::RenderWindowPacket;
use nara_window::WindowId;
use winit::event_loop::ActiveEventLoop;

use super::{ImageSample, WgpuRenderHost, WgpuRenderHostError, join_composite_receipts};
use crate::{
    WgpuImageDisplay, WgpuWindowDeviceScope, WgpuWindowFrame, WgpuWindowFrameOutcome,
    WgpuWindowPresentedReceipt,
};

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
/// revisions are joined with the exact retained output and display occurrence registered here.
#[doc(hidden)]
pub struct WgpuEditorImguiRenderer {
    owner: RendererOwner,
    context_id: ContextId,
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend_instance_id: u64,
    device_epoch: u64,
    bindings: Vec<(ExternalTextureBinding, WgpuImageDisplay)>,
    last_secondary_scope: Option<dear_imgui_wgpu::WgpuFrameScope>,
    closed: bool,
}

/// Renderer construction failure retaining any renderer returned by native route attachment.
///
/// A failure from the SDK renderer constructor has no renderer to return. The caller must retain
/// a separate GPU retirement token even for that case, because initialization may queue uploads.
#[doc(hidden)]
pub struct WgpuEditorImguiRendererFailure {
    error: WgpuRenderHostError,
    renderer: Option<Box<WgpuEditorImguiRenderer>>,
}

impl WgpuEditorImguiRendererFailure {
    #[must_use]
    pub fn error(&self) -> &WgpuRenderHostError {
        &self.error
    }

    #[must_use]
    pub fn into_parts(self) -> (WgpuRenderHostError, Option<WgpuEditorImguiRenderer>) {
        (self.error, self.renderer.map(|renderer| *renderer))
    }
}

impl std::fmt::Debug for WgpuEditorImguiRendererFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorImguiRendererFailure")
            .field("error", &self.error)
            .field("retains_renderer", &self.renderer.is_some())
            .finish()
    }
}

impl std::fmt::Display for WgpuEditorImguiRendererFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for WgpuEditorImguiRendererFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Move-only prepared UI work, retaining the toolkit's frame ownership.
#[doc(hidden)]
pub struct WgpuEditorImguiFrame<'frame>(PreparedFrame<'frame>);

enum PreparedFrame<'frame> {
    Docked(dear_imgui_rs::render::ReconciledFrame<'frame>),
    Native(WgpuPreparedMainFrame<'frame>),
}

impl WgpuWindowDeviceScope<'_> {
    pub fn create_imgui_renderer(
        &self,
        context: &mut Context,
        native_platform: Option<&WinitPlatform>,
    ) -> Result<WgpuEditorImguiRenderer, WgpuEditorImguiRendererFailure> {
        let mut renderer = WgpuRenderer::new(
            WgpuInitInfo::new(self.device.clone(), self.queue.clone(), self.surface_format)
                .with_instance(self.instance.clone())
                .with_adapter(self.adapter.clone()),
            context,
        )
        .map_err(|error| WgpuEditorImguiRendererFailure {
            error: WgpuRenderHostError::window_paint(error.to_string()),
            renderer: None,
        })?;
        renderer.set_gamma_mode(dear_imgui_wgpu::GammaMode::Auto);
        let owner = if let Some(platform) = native_platform {
            match WinitViewportRoute::attach(context, platform, renderer) {
                Ok(route) => RendererOwner::Native(route),
                Err(failure) => {
                    let (error, renderer) = failure.into_parts();
                    return Err(WgpuEditorImguiRendererFailure {
                        error: WgpuRenderHostError::window_paint(error.to_string()),
                        renderer: Some(Box::new(WgpuEditorImguiRenderer::from_owner(
                            RendererOwner::Docked(renderer),
                            self,
                            context.id(),
                        ))),
                    });
                }
            }
        } else {
            RendererOwner::Docked(renderer)
        };
        Ok(WgpuEditorImguiRenderer::from_owner(
            owner,
            self,
            context.id(),
        ))
    }
}

impl WgpuEditorImguiRenderer {
    fn from_owner(
        owner: RendererOwner,
        scope: &WgpuWindowDeviceScope<'_>,
        context_id: ContextId,
    ) -> Self {
        Self {
            owner,
            context_id,
            device: scope.device.clone(),
            queue: scope.queue.clone(),
            backend_instance_id: scope.backend_instance_id,
            device_epoch: scope.device_epoch,
            bindings: Vec::new(),
            last_secondary_scope: None,
            closed: false,
        }
    }

    fn ensure_open(&self) -> Result<(), WgpuRenderHostError> {
        if self.closed {
            return Err(WgpuRenderHostError::window_paint("the renderer is closed"));
        }
        Ok(())
    }

    /// Registers a distinct SDK binding retaining this exact output and display occurrence.
    pub fn register_image(
        &mut self,
        display: &WgpuImageDisplay,
    ) -> Result<ExternalTextureId, WgpuRenderHostError> {
        self.ensure_open()?;
        if !display.output().is_current()
            || display.receipt().backend_instance_id() != self.backend_instance_id
            || display.receipt().device_epoch() != self.device_epoch
        {
            return Err(WgpuRenderHostError::StaleImageSubmission);
        }
        let view = display.output().0.view();
        let texture = match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .register_external_texture(view)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .register_external_texture(view)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
        };
        let binding = self.external_binding(texture)?;
        self.bindings.push((binding, display.clone()));
        Ok(texture)
    }

    fn external_binding(
        &self,
        texture: ExternalTextureId,
    ) -> Result<ExternalTextureBinding, WgpuRenderHostError> {
        self.ensure_open()?;
        match &self.owner {
            RendererOwner::Docked(renderer) => renderer
                .external_texture_binding(texture)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
            RendererOwner::Native(route) => route
                .external_texture_binding(texture)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
        }
    }

    #[cfg(test)]
    fn update_image(
        &mut self,
        texture: ExternalTextureId,
        display: &WgpuImageDisplay,
    ) -> Result<(), WgpuRenderHostError> {
        self.ensure_open()?;
        if display.receipt().backend_instance_id() != self.backend_instance_id
            || display.receipt().device_epoch() != self.device_epoch
        {
            return Err(WgpuRenderHostError::StaleImageSubmission);
        }
        let view = display.output().0.view();
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .update_external_texture(texture, view)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .update_external_texture(texture, view)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
        }
        let updated = self.external_binding(texture)?;
        if let Some((binding, registered)) = self
            .bindings
            .iter_mut()
            .find(|(binding, _)| binding.texture == texture)
        {
            *binding = updated;
            *registered = display.clone();
        }
        Ok(())
    }

    pub fn unregister_external_texture(
        &mut self,
        texture: ExternalTextureId,
    ) -> Result<(), WgpuRenderHostError> {
        self.ensure_open()?;
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .unregister_external_texture(texture)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
            RendererOwner::Native(route) => route
                .unregister_external_texture(texture)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?,
        }
        self.bindings
            .retain(|(binding, _)| binding.texture != texture);
        Ok(())
    }

    fn samples(&self, dependencies: &WgpuDrawDependencies, display: &WgpuImageDisplay) -> bool {
        display.output().is_current()
            && self.bindings.iter().any(|(binding, registered_display)| {
                registered_display == display && dependencies.external_bindings.contains(binding)
            })
    }

    /// Detaches the SDK owner, then releases this adapter's texture references and observations.
    ///
    /// Errors preserve adapter custody for retry, including native errors reported after physical
    /// teardown. Only an actual successful SDK result closes this adapter. Success is idempotent.
    pub fn shutdown(&mut self, context: &mut Context) -> Result<(), WgpuRenderHostError> {
        if context.id() != self.context_id {
            return Err(WgpuRenderHostError::window_paint(
                "the renderer belongs to another Context",
            ));
        }
        if self.closed {
            return Ok(());
        }
        match &mut self.owner {
            RendererOwner::Docked(renderer) => renderer
                .shutdown(context)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
            RendererOwner::Native(route) => route
                .shutdown(context)
                .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
        }?;
        self.closed = true;
        self.bindings.clear();
        self.last_secondary_scope = None;
        Ok(())
    }

    pub fn paint(
        &mut self,
        prepared: WgpuEditorImguiFrame<'_>,
        frame: &mut WgpuWindowFrame<'_>,
        clear: wgpu::Color,
    ) -> Result<(), WgpuRenderHostError> {
        self.ensure_open()?;
        if frame.device != &self.device || frame.queue != &self.queue {
            return Err(WgpuRenderHostError::window_paint(
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
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
                (RendererOwner::Native(route), PreparedFrame::Native(prepared)) => route
                    .record_main(prepared, &mut pass, extent)
                    .map(|observed| observed.dependencies)
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string())),
                _ => Err(WgpuRenderHostError::window_paint(
                    "prepared frame belongs to another renderer route",
                )),
            }
        })?;
        let viewports = frame.viewports;
        for display in viewports {
            if self.samples(&dependencies, display) {
                frame.record_image_sample(display)?;
            }
        }
        Ok(())
    }
}

impl WgpuRenderHost {
    /// Maps a process-unique native registration without depending on whether it rendered.
    pub fn imgui_window_id(
        &self,
        instance: dear_imgui_rs::NativeWindowInstanceId,
    ) -> Result<WindowId, WgpuRenderHostError> {
        delegated_window_id(self.primary_window_id, instance.get().get()).ok_or_else(|| {
            WgpuRenderHostError::window_paint("the secondary window identity space was exhausted")
        })
    }
    /// Completes secondary work and joins its observations before acquiring the primary surface.
    ///
    /// Partial successful presentations are appended before any toolkit error is returned. Each
    /// binding retains its immutable output independently of subsequent producer changes.
    pub fn prepare_imgui_frame<'context>(
        &mut self,
        window: RenderWindowPacket,
        renderer: &mut WgpuEditorImguiRenderer,
        event_loop: &ActiveEventLoop,
        frame: FrameToken<'context>,
        viewport_submissions: &[WgpuImageDisplay],
        outcomes: &mut Vec<WgpuWindowFrameOutcome>,
    ) -> Result<WgpuEditorImguiFrame<'context>, WgpuRenderHostError> {
        renderer.ensure_open()?;
        let host_frame_index = self
            .host_frame_index
            .ok_or(WgpuRenderHostError::FrameTransactionUnavailable)?;
        let namespace = self.prepare_namespace(Some(window))?;
        if renderer.backend_instance_id != namespace.backend_instance_id
            || renderer.device_epoch != namespace.device_epoch
            || renderer.device != namespace.device
            || renderer.queue != namespace.queue
        {
            return Err(WgpuRenderHostError::window_paint(
                "the renderer belongs to another GPU device or queue",
            ));
        }
        for display in viewport_submissions {
            self.validate_viewport_submission(&namespace, display.output())?;
        }
        let retention = crate::submission::WgpuSubmissionReservation::new(
            &namespace.device,
            &namespace.queue,
            viewport_submissions.to_vec(),
        );
        let validation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Internal);
        let allocation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let preparation = (|| match &mut renderer.owner {
            RendererOwner::Docked(inner) => {
                let consumer = inner
                    .renderer_consumer()
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?;
                let pending = frame
                    .try_render(consumer)
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?;
                let prepared = inner
                    .reconcile_frame(pending)
                    .map_err(|error| WgpuRenderHostError::window_paint(error.to_string()))?;
                return Ok((
                    Some(WgpuEditorImguiFrame(PreparedFrame::Docked(prepared))),
                    None,
                ));
            }
            RendererOwner::Native(route) => {
                let result = match route.prepare(event_loop, frame) {
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
                };
                Ok::<_, WgpuRenderHostError>((None, Some(result)))
            }
        })();
        // The SDK owns its native command submission. Bind its current original queue prefix
        // before another producer can enqueue work; retain exact output leases through sampling.
        let prefix = retention.bind_current_prefix();
        self.retiring_prefixes.push(prefix.clone());
        self.native_prefix = Some(prefix);
        let allocation_error = pollster::block_on(allocation.pop());
        let internal_error = pollster::block_on(internal.pop());
        let validation_error = pollster::block_on(validation.pop());
        self.backend.check_device_health()?;
        if let Some(error) = allocation_error.or(internal_error).or(validation_error) {
            return Err(Self::native_error(error));
        }
        let (docked, native) = preparation?;
        if let Some(prepared) = docked {
            return Ok(prepared);
        }
        let (prepared, report, fault) = native.expect("native route retains its report");
        self.join_imgui_secondary_presentations(
            renderer,
            host_frame_index,
            report,
            viewport_submissions,
            outcomes,
        )?;
        if let Some(message) = fault {
            return Err(WgpuRenderHostError::window_paint(message));
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
        viewport_submissions: &[WgpuImageDisplay],
        outcomes: &mut Vec<WgpuWindowFrameOutcome>,
    ) -> Result<(), WgpuRenderHostError> {
        if let Some(scope) = report.scope() {
            if renderer.last_secondary_scope == Some(scope) {
                return Err(WgpuRenderHostError::window_paint(
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
                    WgpuRenderHostError::window_paint(
                        "the secondary window identity space was exhausted",
                    )
                })?;
            let samples = viewport_submissions
                .iter()
                .filter(|display| renderer.samples(&observed.dependencies, display))
                .map(|display| ImageSample {
                    image: display.receipt(),
                    occurrence: display.occurrence(),
                })
                .collect::<Vec<_>>();
            let window = WgpuWindowPresentedReceipt::delegated(
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
            outcomes.push(WgpuWindowFrameOutcome {
                window: Some(window),
                composites,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
