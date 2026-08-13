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
        let Some(ui) = ReferenceUiObservation::from_presented_observation(&presented) else {
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
                "{{\"schema\":\"nara-reference-game.ui-performance-v2\",",
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
                "\"presented_texture_bytes\":{},",
                "\"presented_instance_buffer_bytes\":{},",
                "\"presented_total_bytes\":{},",
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
            self.candidate_run_peak_texture_bytes,
            self.candidate_run_peak_instance_buffer_bytes,
            self.candidate_run_peak_total_bytes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_performance_output_names_the_full_presented_identity() {
        let samples = (0..MEASUREMENT_SAMPLE_COUNT)
            .map(|offset| {
                CandidateCompletionSample::new(
                    3,
                    12 + offset as u64,
                    11 + offset as u64,
                    110 + offset as u64,
                )
            })
            .collect::<Vec<_>>();
        let json = performance_json_fixture(&samples);

        assert!(json.contains("\"schema\":\"nara-reference-game.ui-performance-v2\""));
        assert!(json.contains("\"app_frame_index\":11"));
        assert!(json.contains("\"render_transaction_frame_index\":10"));
        assert!(json.contains("\"ui_admission_id\":5"));
        assert!(json.contains("\"surface_id\":1"));
        assert!(json.contains("\"measurement_samples\":16"));
        assert!(!json.contains("\"frame_index\":"));
    }

    fn performance_json_fixture(samples: &[CandidateCompletionSample]) -> String {
        let samples = samples
            .iter()
            .copied()
            .map(CandidateCompletionSample::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{\"schema\":\"nara-reference-game.ui-performance-v2\",",
                "\"backend_instance_id\":7,\"device_epoch\":3,",
                "\"app_frame_index\":11,\"render_transaction_frame_index\":10,",
                "\"ui_admission_id\":5,\"presentation_epoch\":6,",
                "\"surface_id\":1,\"ui_frame_revision\":4,",
                "\"source_revision\":4,\"slot_generation\":1,",
                "\"route_revision\":1,\"geometry_generation\":1,",
                "\"presentation_revision\":1,\"theme_revision\":1,",
                "\"font_revision\":1,\"warmup_samples\":4,",
                "\"measurement_samples\":16,\"completion_sample_loss\":0,",
                "\"presented_completion\":{{\"device_epoch\":3,",
                "\"app_frame_index\":11,\"render_transaction_frame_index\":10,",
                "\"duration_ns\":100}},\"completion_samples\":[{}],",
                "\"ui_primitives\":32,\"ui_images\":0,",
                "\"ui_glyph_primitives\":20,\"ui_glyph_pages\":1,",
                "\"submitted_ui_quads\":32,",
                "\"submitted_glyph_quads\":20,",
                "\"referenced_texture_bindings\":1,",
                "\"frame_submitted_sprites\":4,",
                "\"frame_submitted_image_sprites\":3,",
                "\"presented_texture_bytes\":4096,",
                "\"presented_instance_buffer_bytes\":2048,",
                "\"presented_total_bytes\":6144,",
                "\"candidate_run_peak_texture_bytes\":4096,",
                "\"candidate_run_peak_instance_buffer_bytes\":2048,",
                "\"candidate_run_peak_total_bytes\":6144}}\n"
            ),
            samples
        )
    }
}
