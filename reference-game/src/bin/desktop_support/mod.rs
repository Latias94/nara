use nara::{
    render::{RenderFrame, RenderFrameState},
    render_wgpu::WgpuRenderBackend,
};

pub(crate) fn submitted_product_frame(frame: &RenderFrame, backend: &WgpuRenderBackend) -> bool {
    let transaction = backend.frame_transaction_stats();
    frame.state == RenderFrameState::Submitted
        && transaction.frame_index() == Some(frame.index)
        && transaction.packet_admissions() == 1
        && transaction.packet_rejections() == 0
        && transaction.surface_acquire_attempts() == 1
        && transaction.surface_acquires() == 1
        && transaction.queue_submissions() == 1
        && transaction.presents() == 1
}
