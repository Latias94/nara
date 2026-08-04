use nara::{
    app::RuntimeGeneration,
    render::{ExtractedViews, build_render_frame_packet},
    window::Window,
};

fn capture_with_phases(generation: RuntimeGeneration, window: &Window, views: &ExtractedViews) {
    let _ = build_render_frame_packet(generation, 0, None, [window], views, []);
}

fn main() {}
