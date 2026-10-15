use nara::{
    prelude::World,
    render_wgpu::WgpuRenderBackend,
    ui_render::advanced::{PresentedUiObservation, PresentedUiWgpuFacts},
};
use nara_reference_game::ReferenceUiObservation;

use super::measurement::{
    CandidateCompletionSample, CandidateMeasurementIdentity, MEASUREMENT_SAMPLE_COUNT,
    WARMUP_SAMPLE_COUNT,
};

#[derive(Clone, Debug)]
pub(crate) struct CandidateSmokeEvidence {
    ui: ReferenceUiObservation,
    backend: CandidateBackendObservation,
}

impl CandidateSmokeEvidence {
    pub(crate) const fn ui(&self) -> &ReferenceUiObservation {
        &self.ui
    }

    pub(crate) fn backend_json_line(&self) -> String {
        self.backend.to_json_line(&self.ui)
    }

    pub(super) fn capture(
        backend: &WgpuRenderBackend,
        armed: ArmedCandidate,
        presented_completion: CandidateCompletionSample,
        completion_samples: &[CandidateCompletionSample],
    ) -> Result<Self, &'static str> {
        Ok(Self {
            backend: CandidateBackendObservation::capture(
                backend,
                &armed,
                presented_completion,
                completion_samples,
            )?,
            ui: armed.ui,
        })
    }
}

#[derive(Clone, Debug)]
struct CandidateBackendObservation {
    presented_completion: CandidateCompletionSample,
    completion_samples: Vec<CandidateCompletionSample>,
    wgpu: PresentedUiWgpuFacts,
    measurement_end_current_texture_bytes: u64,
    measurement_end_current_instance_buffer_bytes: u64,
    measurement_end_current_total_bytes: u64,
    measurement_end_pending_retirements: u32,
    measurement_end_lost_retirements: u64,
    candidate_run_peak_texture_bytes: u64,
    candidate_run_peak_instance_buffer_bytes: u64,
    candidate_run_peak_total_bytes: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ArmedCandidate {
    pub(super) identity: CandidateMeasurementIdentity,
    ui: ReferenceUiObservation,
    wgpu: PresentedUiWgpuFacts,
}

impl ArmedCandidate {
    pub(super) fn capture(
        world: &World,
        presented: PresentedUiObservation,
    ) -> Result<Option<Self>, &'static str> {
        let Some(wgpu) = presented.wgpu() else {
            return Ok(None);
        };
        if wgpu.primitive_count() == 0
            || wgpu.glyph_primitive_count() == 0
            || wgpu.glyph_page_count() == 0
            || wgpu.submitted_quad_count() == 0
            || wgpu.submitted_glyph_quad_count() == 0
            || wgpu.referenced_texture_binding_count() == 0
            || wgpu.frame_submitted_sprite_count() == 0
            || wgpu.frame_submitted_image_sprite_count() == 0
            || wgpu.submitted_texture_bytes() == 0
            || wgpu.submitted_instance_buffer_bytes() == 0
            || wgpu.submitted_total_bytes() == 0
        {
            return Ok(None);
        }

        let backend = world.resource::<WgpuRenderBackend>();
        if backend.frame_completion_stats().lost_samples() != 0 {
            return Err("desktop candidate lost a GPU completion before measurement armed");
        }
        let backend_generation = presented.backend_generation();
        if backend.instance_id() != backend_generation.backend_instance_id()
            || backend.device_epoch() != backend_generation.device_epoch()
        {
            return Err(
                "desktop candidate presented observation belongs to a retired WGPU generation",
            );
        }
        let identity = CandidateMeasurementIdentity {
            backend_generation,
            app_frame_index: wgpu.app_frame_index(),
            render_transaction_frame_index: wgpu.render_transaction_frame_index(),
        };
        let Some(ui) = ReferenceUiObservation::from_presented_observation(world, &presented) else {
            return Ok(None);
        };

        Ok(Some(Self { identity, ui, wgpu }))
    }
}

impl CandidateBackendObservation {
    fn capture(
        backend: &WgpuRenderBackend,
        armed: &ArmedCandidate,
        presented_completion: CandidateCompletionSample,
        completion_samples: &[CandidateCompletionSample],
    ) -> Result<Self, &'static str> {
        let completion = backend.frame_completion_stats();
        let resources = backend.gpu_resource_stats();
        if backend.instance_id() != armed.identity.backend_instance_id()
            || backend.device_epoch() != armed.identity.device_epoch()
            || completion.device_epoch() != armed.identity.device_epoch()
            || resources.device_epoch() != armed.identity.device_epoch()
        {
            return Err("desktop candidate WGPU generation changed during measurement");
        }
        if completion.lost_samples() != 0
            || completion.pending_samples() != 0
            || completion.buffered_samples() != 0
        {
            return Err("desktop candidate GPU completion window did not close cleanly");
        }
        if !resources.tracking_enabled()
            || resources.pending_retirements() != 0
            || resources.lost_retirements() != 0
        {
            return Err("desktop candidate GPU resource retirement was incomplete");
        }
        if completion_samples.len() != MEASUREMENT_SAMPLE_COUNT {
            return Err("desktop candidate measurement window was incomplete");
        }

        Ok(Self {
            presented_completion,
            completion_samples: completion_samples.to_vec(),
            wgpu: armed.wgpu,
            measurement_end_current_texture_bytes: resources.current_texture_bytes(),
            measurement_end_current_instance_buffer_bytes: resources
                .current_instance_buffer_bytes(),
            measurement_end_current_total_bytes: resources.current_total_bytes(),
            measurement_end_pending_retirements: resources.pending_retirements(),
            measurement_end_lost_retirements: resources.lost_retirements(),
            candidate_run_peak_texture_bytes: resources.peak_texture_bytes(),
            candidate_run_peak_instance_buffer_bytes: resources.peak_instance_buffer_bytes(),
            candidate_run_peak_total_bytes: resources.peak_total_bytes(),
        })
    }

    fn to_json_line(&self, ui: &ReferenceUiObservation) -> String {
        let presentation = ui
            .presentation()
            .expect("desktop candidate evidence must retain presented UI facts");
        let samples = self
            .completion_samples
            .iter()
            .copied()
            .map(CandidateCompletionSample::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{\"schema\":\"nara-reference-game.ui-performance-v3\",",
                "\"backend_instance_id\":{},\"device_epoch\":{},",
                "\"app_frame_index\":{},\"render_transaction_frame_index\":{},",
                "\"ui_admission_id\":{},\"presentation_epoch\":{},",
                "\"surface_id\":{},\"ui_frame_revision\":{},",
                "\"source_revision\":{},\"slot_generation\":{},",
                "\"route_revision\":{},\"geometry_generation\":{},",
                "\"presentation_revision\":{},\"theme_revision\":{},",
                "\"font_revision\":{},\"warmup_samples\":{},",
                "\"measurement_samples\":{},\"completion_sample_loss\":0,",
                "\"presented_completion\":{},\"completion_samples\":[{}],",
                "\"ui_primitives\":{},\"ui_images\":{},",
                "\"ui_glyph_primitives\":{},\"ui_glyph_pages\":{},",
                "\"submitted_ui_quads\":{},",
                "\"submitted_glyph_quads\":{},",
                "\"referenced_texture_bindings\":{},",
                "\"frame_submitted_sprites\":{},",
                "\"frame_submitted_image_sprites\":{},",
                "\"submitted_frame_logical_texture_bytes\":{},",
                "\"submitted_frame_logical_instance_buffer_bytes\":{},",
                "\"submitted_frame_logical_total_bytes\":{},",
                "\"measurement_end_current_texture_bytes\":{},",
                "\"measurement_end_current_instance_buffer_bytes\":{},",
                "\"measurement_end_current_total_bytes\":{},",
                "\"measurement_end_pending_retirements\":{},",
                "\"measurement_end_lost_retirements\":{},",
                "\"candidate_run_peak_texture_bytes\":{},",
                "\"candidate_run_peak_instance_buffer_bytes\":{},",
                "\"candidate_run_peak_total_bytes\":{}}}\n"
            ),
            presentation.backend_instance_id,
            presentation.device_epoch,
            presentation.app_frame_index,
            presentation.render_transaction_frame_index,
            presentation.ui_admission_id,
            presentation.presentation_epoch,
            ui.surface_id,
            presentation.frame_revision,
            presentation.source_revision,
            presentation.slot_generation,
            presentation.route_revision,
            presentation.geometry_generation,
            presentation.presentation_revision,
            presentation.theme_revision,
            presentation.font_revision,
            WARMUP_SAMPLE_COUNT,
            MEASUREMENT_SAMPLE_COUNT,
            self.presented_completion.to_json(),
            samples,
            self.wgpu.primitive_count(),
            self.wgpu.image_count(),
            self.wgpu.glyph_primitive_count(),
            self.wgpu.glyph_page_count(),
            self.wgpu.submitted_quad_count(),
            self.wgpu.submitted_glyph_quad_count(),
            self.wgpu.referenced_texture_binding_count(),
            self.wgpu.frame_submitted_sprite_count(),
            self.wgpu.frame_submitted_image_sprite_count(),
            self.wgpu.submitted_texture_bytes(),
            self.wgpu.submitted_instance_buffer_bytes(),
            self.wgpu.submitted_total_bytes(),
            self.measurement_end_current_texture_bytes,
            self.measurement_end_current_instance_buffer_bytes,
            self.measurement_end_current_total_bytes,
            self.measurement_end_pending_retirements,
            self.measurement_end_lost_retirements,
            self.candidate_run_peak_texture_bytes,
            self.candidate_run_peak_instance_buffer_bytes,
            self.candidate_run_peak_total_bytes,
        )
    }
}
