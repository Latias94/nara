use nara::{
    render::{RenderFrame, RenderFrameState},
    render_wgpu::WgpuRenderBackend,
};

pub(crate) fn submitted_product_frame(frame: &RenderFrame, backend: &WgpuRenderBackend) -> bool {
    let transaction = backend.frame_transaction_stats();
    frame.state == RenderFrameState::Submitted
        && transaction.render_transaction_frame_index() == Some(frame.index)
        && transaction.packet_admissions() == 1
        && transaction.packet_rejections() == 0
        && transaction.surface_acquire_attempts() == 1
        && transaction.surface_acquires() == 1
        && transaction.queue_submissions() == 1
        && transaction.presents() == 1
}

pub(crate) fn resident_product_glyphs(backend: &WgpuRenderBackend) -> bool {
    let transaction = backend.frame_transaction_stats();
    let textures = backend.texture_stats();
    transaction.ui_packet_count() == 1
        && transaction.ui_packet_identity().is_some()
        && transaction.ui_glyph_primitives() > 0
        && transaction.ui_glyph_pages() > 0
        && textures
            .uploaded_glyph_pages
            .saturating_add(textures.reused_glyph_pages)
            > 0
}
