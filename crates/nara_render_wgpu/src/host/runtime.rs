//! Managed Runtime integration for the same program, native submission and surface owner.

use nara_app::{RuntimeFault, RuntimeFaultKind};
use nara_render::{FrameStats, RenderBackendStatus, RenderFrameSkipReason, RenderFrameTarget};
use nara_window::{WindowId, backend::BackendWindowHandles};

use super::{ProgramSurfaceTarget, WgpuRenderHost, WgpuRenderHostError};
use crate::{
    WgpuAdapterSummary, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
    WgpuFrameCompletionStats, WgpuGpuResourceStats, WgpuRenderError,
    backend::render_backend_state,
    runtime::{WgpuRuntimeCapture, WgpuRuntimeResult},
};

/// Owned CPU facts for native-session observation, independent of GPU handle authority.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct WgpuRuntimeHostFacts {
    pub backend_instance_id: u64,
    pub device_epoch: u64,
    pub adapter: Option<WgpuAdapterSummary>,
    pub completion: WgpuFrameCompletionStats,
    pub resources: WgpuGpuResourceStats,
}

/// Polling can retain exact completion samples even when another maintenance responsibility fails.
#[doc(hidden)]
pub struct WgpuRuntimeMaintenance {
    pub samples: Vec<WgpuFrameCompletionSample>,
    pub error: Option<WgpuRenderHostError>,
}

pub(super) struct RuntimeWorldPresentation {
    _guard: nara_render::__private::WorldPresentationGuard,
    window: nara_render::RenderWindowPacket,
    canvas: Option<nara_render::WorldCanvasFrame>,
}

impl WgpuRenderHost {
    #[doc(hidden)]
    pub fn configure_telemetry(&mut self, config: WgpuBackendTelemetryConfig) {
        self.backend.configure_telemetry(config);
    }

    #[doc(hidden)]
    pub fn runtime_facts(&self) -> WgpuRuntimeHostFacts {
        WgpuRuntimeHostFacts {
            backend_instance_id: self.backend.instance_id(),
            device_epoch: self.backend.device_epoch(),
            adapter: self.backend.adapter_summary(),
            completion: self.backend.frame_completion_stats(),
            resources: self.backend.gpu_resource_stats(),
        }
    }

    #[doc(hidden)]
    pub fn poll_runtime_maintenance(&mut self) -> WgpuRuntimeMaintenance {
        let owner_error = self.maintain_producers().err();
        let telemetry_error = self.backend.poll_telemetry_completions().err();
        let samples = self.backend.drain_frame_completion_samples();
        let error = match (owner_error, telemetry_error) {
            (Some(owner), Some(telemetry)) => Some(WgpuRenderHostError::NativeExecution {
                message: format!("{owner}; telemetry maintenance failed: {telemetry}"),
            }),
            (Some(error), None) => Some(error),
            (None, Some(error)) => Some(error.into()),
            (None, None) => None,
        };
        WgpuRuntimeMaintenance { samples, error }
    }

    #[doc(hidden)]
    pub fn poll_runtime_retirement(
        &mut self,
        window_ids: &[WindowId],
    ) -> Result<bool, WgpuRenderHostError> {
        self.poll_retirement_for_targets(window_ids)
    }

    /// Executes the original move-only capture; even rejection returns its original result custody.
    #[doc(hidden)]
    pub fn execute_runtime_frame(
        &mut self,
        producer: &super::WgpuRenderProducer,
        handles: &BackendWindowHandles,
        mut capture: WgpuRuntimeCapture,
    ) -> WgpuRuntimeResult {
        if !capture.allows_execution() || self.backend.runtime_close_started() {
            return crate::runtime::cancel_runtime_frame(capture);
        }
        let mut frame = capture.pending.frame;
        let mut stats = FrameStats::default();
        let mut status = RenderBackendStatus::default();
        status.mark_state(
            crate::WGPU_RENDER_BACKEND,
            render_backend_state(self.backend.state()),
        );
        let original_runtime = capture.identity().runtime_generation();
        self.validate_runtime_presentation(
            producer,
            original_runtime,
            capture.pending.window,
            capture.pending.canvas,
        );
        self.backend.begin_frame_transaction(frame.index, None);
        #[cfg(feature = "stock-ui")]
        let mut ui = capture.pending.ui.take();
        let mut fault = None;
        let mut presented_world = None;
        let result = match std::mem::replace(&mut capture.pending.captured, Ok(None)) {
            Ok(Some(captured))
                if captured
                    .input
                    .packet()
                    .source_generation()
                    .runtime_generation()
                    != Some(original_runtime) =>
            {
                self.backend.reject_captured_packet();
                fault = Some(RuntimeFault::engine(
                    RuntimeFaultKind::System,
                    "nara.render.frame-packet",
                ));
                Err(WgpuRenderHostError::Backend(
                    WgpuRenderError::StaleFrameGeneration {
                        expected: original_runtime.into(),
                        actual: captured.input.packet().source_generation(),
                    },
                ))
            }
            Ok(Some(captured)) => self.render_runtime_packet(
                producer,
                handles,
                captured,
                #[cfg(feature = "stock-ui")]
                ui.as_mut(),
                &mut frame,
                &mut stats,
                &mut status,
                &mut presented_world,
            ),
            Ok(None) => {
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::NoViews,
                    "no admitted render frame packet",
                );
                frame.mark_skipped();
                Ok(())
            }
            Err(error) => {
                self.backend.reject_captured_packet();
                frame.mark_skipped();
                match error {
                    crate::WgpuFrameCaptureError::Policy(error) => {
                        // Domain capture rejection is recoverable on the next frame, just
                        // as rejection by the same policy during program construction is.
                        Err(error.into())
                    }
                    error => {
                        status.mark_skipped_with_message(
                            frame.index,
                            RenderFrameSkipReason::InvalidTopology,
                            error.to_string(),
                        );
                        fault = Some(RuntimeFault::engine(
                            RuntimeFaultKind::System,
                            "nara.render.frame-packet",
                        ));
                        Err(WgpuRenderHostError::Backend(
                            WgpuRenderError::ContentRejected {
                                message: error.to_string(),
                            },
                        ))
                    }
                }
            }
        };
        #[cfg(feature = "stock-ui")]
        if let Some(ui) = ui {
            let backend_result = result
                .as_ref()
                .map(|_| ())
                .map_err(|error| runtime_error(error));
            let device_generation = backend_result.as_ref().err().and_then(|error| {
                (crate::frame::classify_ui_backend_fault(error)
                    == nara_ui_render::UiPresentationBackendFaultDomain::Device)
                    .then(|| {
                        nara_ui_render::UiPresentationBackendGeneration::new(
                            self.backend.instance_id(),
                            self.backend.device_epoch(),
                        )
                    })
                    .flatten()
            });
            let completion = crate::frame::ui_completion(
                &ui,
                &backend_result,
                frame.state,
                device_generation,
                status.last_skip().and_then(|skip| skip.message()),
            );
            crate::frame::complete_ui_submissions_into(ui, completion, &mut capture.completed);
        }
        if let Err(error) = result {
            let backend_error = runtime_error(&error);
            if backend_error.target_window().is_some()
                && let Ok(id) = self.producer_id(producer)
            {
                self.invalidate_runtime_presentation(id);
            }
            let render_fault = crate::handle_wgpu_render_error(
                &mut self.backend,
                backend_error,
                frame.index,
                &mut status,
            );
            fault = fault.or(render_fault);
            frame.mark_skipped();
        }
        let observation = self.backend.take_observation();
        capture.into_result(frame, stats, status, observation, presented_world, fault)
    }

    fn render_runtime_packet(
        &mut self,
        producer: &super::WgpuRenderProducer,
        handles: &BackendWindowHandles,
        captured: crate::frame::WgpuCapturedFrame,
        #[cfg(feature = "stock-ui")] mut ui: Option<&mut crate::frame::WgpuUiSubmissionState>,
        frame: &mut nara_render::RenderFrame,
        stats: &mut FrameStats,
        status: &mut RenderBackendStatus,
        presented_world: &mut Option<nara_render::__private::PresentedWorldFrame>,
    ) -> Result<(), WgpuRenderHostError> {
        let crate::frame::WgpuCapturedFrame {
            input,
            app_frame_index,
            work_started_at,
        } = captured;
        let packet = input.packet();
        let id = self.producer_id(producer)?;
        self.backend
            .begin_frame_transaction(packet.frame_index(), Some(app_frame_index));
        if let Err(error) = self.views[&id]
            .admission
            .admit(packet.source_generation(), packet.frame_index())
        {
            self.backend.reject_editor_packet();
            return Err(error.into());
        }
        self.backend.record_runtime_packet_admission();
        let Some(runtime) = packet.source_generation().runtime_generation() else {
            return Err(WgpuRenderProgramError::feature(
                "the Runtime window requires its original managed source",
            )
            .into());
        };
        let RenderFrameTarget::Window(window) = packet.target() else {
            return Err(WgpuRenderProgramError::feature(
                "the Runtime window requires an admitted window target",
            )
            .into());
        };
        if window.resolution.physical_width == 0 || window.resolution.physical_height == 0 {
            self.invalidate_runtime_presentation(id);
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::NoRenderableTarget,
                "the admitted render target is zero sized",
            );
            return Ok(());
        }
        self.admit_policy_requirements(input.requirements())?;
        match &self.handles {
            Some(original) if !original.shares_authority_with(handles) => {
                return Err(WgpuRenderProgramError::feature(
                    "the native window authority changed within its Host",
                )
                .into());
            }
            None => {}
            Some(_) => {}
        }
        let namespace = if !handles.is_surface_target_active(window.id) {
            Err(WgpuRenderHostError::Backend(
                WgpuRenderError::SurfaceMissing {
                    window_id: window.id,
                },
            ))
        } else {
            self.prepare_namespace_with_authority(Some(window), Some(handles))
        };
        if namespace.is_ok() || self.backend.surface_count() != 0 {
            self.handles.get_or_insert_with(|| handles.clone());
        }
        let namespace = match namespace {
            Err(WgpuRenderHostError::Backend(WgpuRenderError::SurfaceMissing { .. })) => {
                self.invalidate_runtime_presentation(id);
                frame.mark_skipped();
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::SurfaceUnavailable,
                    "the admitted native window target is unavailable",
                );
                return Ok(());
            }
            result => result?,
        };
        self.host_frame_index = Some(
            self.host_frame_index
                .map_or(Some(1), |frame| frame.checked_add(1))
                .ok_or(WgpuRenderHostError::TargetGenerationExhausted)?,
        );
        #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
        self.backend
            .begin_editor_submitter_frame(self.host_frame_index.expect("admitted Host frame"));
        status.mark_ready(crate::WGPU_RENDER_BACKEND);
        status.clear_skip();
        #[cfg(feature = "stock-ui")]
        if let Some(ui) = ui.as_deref_mut() {
            let generation = nara_ui_render::UiPresentationBackendGeneration::new(
                namespace.backend_instance_id,
                namespace.device_epoch,
            )
            .ok_or(WgpuRenderHostError::StaleImageSubmission)?;
            ui.bind_backend(generation)?;
        }
        let Some(acquired) = self.backend.acquire_editor_surface(window.id)? else {
            if self.backend.configured_surface_extent(window.id).is_none() {
                self.invalidate_runtime_presentation(id);
            }
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::SurfaceUnavailable,
                "the admitted surface did not produce a presentable frame",
            );
            return Ok(());
        };
        let mut candidate =
            self.prepare_candidate_in_namespace(id, &namespace, &input, acquired.format, false)?;
        let policy = &candidate.built.output;
        Self::validate_policy_target(policy, packet, acquired.format, false)?;
        #[cfg(feature = "stock-ui")]
        let ui_facts = policy
            .ui
            .as_ref()
            .map_or(&[][..], |binding| binding.facts());
        #[cfg(feature = "stock-ui")]
        if let Some(binding) = &policy.ui {
            self.backend
                .record_runtime_ui_payload(binding.payload_facts());
        }
        #[cfg(feature = "stock-ui")]
        let mut submitted_ui = ui
            .as_deref()
            .map(|ui| {
                crate::backend::WgpuRenderBackend::prepare_submitted_ui_facts(
                    ui.bound_submissions(),
                    ui_facts,
                    crate::backend::WgpuSubmittedFrameFacts::new(
                        app_frame_index,
                        packet.frame_index(),
                        policy.facts.frame.sprites,
                        policy.facts.image_sprites,
                        policy.program.modeled_resource_bytes(),
                        policy.facts.instance_buffer_bytes,
                    ),
                )
            })
            .transpose()?;
        let mut submitted_texture_bytes = 0;
        let program = policy.program.clone();
        let final_version = policy.final_version.clone();
        let policy_facts = policy.facts;
        let mut published = self.execute_program_in_namespace(
            id,
            &namespace,
            &program,
            None,
            Some(ProgramSurfaceTarget {
                acquired: &acquired,
                final_version: &final_version,
                instance_buffer_bytes: policy_facts.instance_buffer_bytes,
                app_frame_index,
                work_started_at,
                submitted_texture_bytes: &mut submitted_texture_bytes,
            }),
            Some(&mut candidate.built),
        )?;
        let presentation = (|| {
            #[cfg(feature = "stock-ui")]
            if let Some(facts) = &mut submitted_ui {
                for facts in facts {
                    facts.set_resource_bytes(
                        submitted_texture_bytes,
                        policy_facts.instance_buffer_bytes,
                    );
                }
            }
            let scope = namespace
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            namespace.queue.present(acquired.texture);
            let error = pollster::block_on(scope.pop());
            self.backend.check_device_health()?;
            if let Some(error) = error {
                return Err(Self::native_error(error));
            }
            let receipt = crate::presented::WgpuPresentedFrameReceipt::new(
                runtime,
                window.id,
                acquired.surface_generation,
                namespace.backend_instance_id,
                namespace.device_epoch,
                app_frame_index,
                packet.frame_index(),
            );
            *presented_world = self.commit_runtime_world_presentation(id, &packet)?;
            self.backend.commit_runtime_presentation(receipt);
            #[cfg(feature = "stock-ui")]
            if let Some(facts) = submitted_ui {
                self.backend.publish_submitted_ui_facts(facts);
            }
            *stats = policy_facts.frame;
            stats.draw_calls = stats.draw_calls.saturating_add(1);
            frame.mark_submitted();
            Ok(())
        })();
        self.release_published_candidate(candidate, &mut published);
        self.release_publication_value((program, input, published));
        presentation
    }
    fn invalidate_runtime_presentation(&mut self, id: super::WgpuRenderProducerId) {
        if let Some(view) = self.views.get_mut(&id) {
            view.world_presentation.invalidate();
            view.runtime_presentation = None;
        }
        self.backend.retire_editor_world_presentation(id);
    }

    fn validate_runtime_presentation(
        &mut self,
        producer: &super::WgpuRenderProducer,
        runtime: nara_app::RuntimeGeneration,
        window: Option<nara_render::RenderWindowPacket>,
        canvas: Option<nara_render::WorldCanvas>,
    ) {
        let Ok(id) = self.producer_id(producer) else {
            return;
        };
        if self.views[&id]
            .admission
            .source_generation
            .is_some_and(|source| source.runtime_generation() != Some(runtime))
        {
            return;
        }
        let incompatible = self.views[&id]
            .runtime_presentation
            .as_ref()
            .is_some_and(|presented| {
                window.is_none_or(|window| {
                    window.id != presented.window.id
                        || window.resolution != presented.window.resolution
                }) || presented
                    .canvas
                    .map(|canvas| (canvas.extent(), canvas.bar_color()))
                    != canvas.map(|canvas| (canvas.extent(), canvas.bar_color()))
                    || self.handles.as_ref().is_some_and(|handles| {
                        !handles.is_surface_target_active(presented.window.id)
                    })
            });
        if incompatible {
            self.invalidate_runtime_presentation(id);
        }
    }

    fn commit_runtime_world_presentation(
        &mut self,
        id: super::WgpuRenderProducerId,
        packet: &nara_render::RenderFramePacket,
    ) -> Result<Option<nara_render::__private::PresentedWorldFrame>, WgpuRenderHostError> {
        let RenderFrameTarget::Window(window) = packet.target() else {
            return Ok(None);
        };
        // A successful surface present replaces pixels in this physical target, including
        // a different Runtime's last frame, even if its successor has no picking mapping.
        // Presentations in other windows remain current.
        for (other_id, view) in &mut self.views {
            if *other_id == id
                || view
                    .runtime_presentation
                    .as_ref()
                    .is_some_and(|presented| presented.window.id == window.id)
            {
                view.world_presentation.invalidate();
                view.runtime_presentation = None;
                self.backend.retire_editor_world_presentation(*other_id);
            }
        }
        let Some((snapshot, guard)) = nara_render::__private::prepare_world_presentation(packet)
        else {
            return Ok(None);
        };
        self.backend
            .retain_editor_world_presentation(id, guard.revoker())?;
        guard.mark_presented();
        let view = self
            .views
            .get_mut(&id)
            .ok_or(WgpuRenderHostError::ProducerClosed)?;
        view.world_presentation
            .replace(&view.closed, guard.revoker())?;
        view.runtime_presentation = Some(RuntimeWorldPresentation {
            _guard: guard,
            window,
            canvas: packet.world_canvas(),
        });
        Ok(Some(snapshot))
    }
}

use crate::WgpuRenderProgramError;

fn runtime_error(error: &WgpuRenderHostError) -> WgpuRenderError {
    if let Some(error) = error.backend_error() {
        return error.clone();
    }
    if error.is_deferred() {
        return WgpuRenderError::UploadDeferred {
            message: error.to_string(),
        };
    }
    if error.is_content_rejected() {
        return WgpuRenderError::ContentRejected {
            message: error.to_string(),
        };
    }
    if error.requires_owner_replacement() {
        WgpuRenderError::DeviceUnavailable {
            message: error.to_string(),
        }
    } else {
        WgpuRenderError::ContentRejected {
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests;
