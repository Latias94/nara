use std::path::PathBuf;

use dear_imgui_rs::{ConfigFlags, Context};
use nara_editor::{EditorApplication, ImguiEditorFrontend, ImguiEditorSubmission};

pub fn context() -> Context {
    let mut context = Context::create();
    context.set_ini_filename::<PathBuf>(None).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::DOCKING_ENABLE | ConfigFlags::NAV_ENABLE_KEYBOARD);
    context.io_mut().set_display_size([1280.0, 800.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context
}

pub fn draw(
    context: &mut Context,
    frontend: &mut ImguiEditorFrontend,
    application: &mut EditorApplication,
) -> Option<ImguiEditorSubmission> {
    let batch = frontend
        .show(context.frame(), application)
        .expect("the ordinary Dear ImGui frontend must render");
    let _ = context.render_legacy();
    batch
}
