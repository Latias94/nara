use nara::{
    render_wgpu::{__private::presented_frame_this_transaction, WgpuRenderObservation},
    ui_render::advanced::PresentedUiObservation,
};

pub(crate) fn submitted_product_frame(observation: &WgpuRenderObservation) -> bool {
    let Some(frame) = presented_frame_this_transaction(observation) else {
        return false;
    };
    let transaction = observation.frame_transaction_stats();
    observation.instance_id() == frame.backend_instance_id()
        && observation.device_epoch() == frame.device_epoch()
        && transaction.app_frame_index() == Some(frame.app_frame_index())
        && transaction.render_transaction_frame_index()
            == Some(frame.render_transaction_frame_index())
        && transaction.packet_admissions() == 1
        && transaction.packet_rejections() == 0
        && transaction.surface_acquire_attempts() == 1
        && transaction.surface_acquires() == 1
        && transaction.queue_submissions() == 1
        && transaction.presents() == 1
}

/// Requires actual glyph draws and bound textures in the same accepted UI presentation. Cache
/// activity or a packet's declared glyph pages alone cannot establish that product evidence.
pub(crate) fn resident_product_glyphs(
    observation: &WgpuRenderObservation,
    presented: &PresentedUiObservation,
) -> bool {
    let Some(facts) = presented.wgpu() else {
        return false;
    };
    let transaction = observation.frame_transaction_stats();
    submitted_product_frame(observation)
        && observation.instance_id() == presented.backend_generation().backend_instance_id()
        && observation.device_epoch() == presented.backend_generation().device_epoch()
        && transaction.app_frame_index() == Some(facts.app_frame_index())
        && transaction.render_transaction_frame_index()
            == Some(facts.render_transaction_frame_index())
        && transaction.ui_packet_count() == 1
        && transaction.ui_packet_identity() == Some(presented.packet().identity())
        && transaction.ui_glyph_primitives() > 0
        && transaction.ui_glyph_pages() > 0
        && facts.glyph_primitive_count() > 0
        && facts.glyph_page_count() > 0
        && facts.submitted_glyph_quad_count() > 0
        && facts.referenced_texture_binding_count() > 0
        && facts.submitted_texture_bytes() > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_cpu_observation_cannot_prove_product_submission() {
        assert!(!submitted_product_frame(&WgpuRenderObservation::default()));
    }
}
