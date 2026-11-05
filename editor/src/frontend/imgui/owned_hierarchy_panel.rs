use dear_imgui_rs::Ui;
use nara_ui_kernel::{
    UiEnvironment, UiRootKey, UiSize, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
    UiTheme,
};

use super::ImguiEditorPanelContext;
use crate::frontend::controls::HierarchyControls;

pub(super) fn paint(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    let specialized = context.input.ui_authoring.is_some();
    if context
        .widgets
        .owned_hierarchy
        .as_ref()
        .is_some_and(|owner| owner.is_ui() != specialized)
    {
        if let Some(mut owner) = context.widgets.owned_hierarchy.take() {
            let _ = owner.cancel_input();
        }
        context.widgets.hierarchy_pending.clear();
    }
    for submission in std::mem::take(&mut context.widgets.hierarchy_pending) {
        match submission.into_action(
            context.input.frame,
            context.input.ui_authoring,
            context.input.read_only,
        ) {
            Ok(action) => {
                let result = context.submit_hierarchy(action);
                if let Some(hierarchy) = context.widgets.owned_hierarchy.as_mut() {
                    hierarchy.admitted(result);
                }
            }
            Err(error) => {
                if let Some(hierarchy) = context.widgets.owned_hierarchy.as_mut() {
                    hierarchy.submission_failed(format!(
                        "The source changed before this action was queued: {error}. Review it and retry."
                    ));
                }
            }
        }
    }
    if let Err(error) = paint_panel(ui, context) {
        ui.text_wrapped(error);
    }
}

fn paint_panel(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) -> Result<(), String> {
    let panel = context.model.instance().id();
    let serial = context.widgets.incarnation.serial;
    if context.widgets.owned_hierarchy.is_none() {
        let fonts = if context.input.owned_fonts.is_empty() {
            vec![nara_text::bundled_font_face().map_err(|error| error.to_string())?]
        } else {
            context.input.owned_fonts.to_vec()
        };
        let root = UiRootKey::runtime("nara.editor.hierarchy", serial.to_string())
            .map_err(|error| error.to_string())?;
        context.widgets.owned_hierarchy = Some(HierarchyControls::new(
            root,
            fonts,
            context.input.ui_authoring.is_some(),
        )?);
    }
    context.widgets.hierarchy_read_only = context.input.read_only;
    let available = ui.content_region_avail();
    let size = [available[0].max(60.0), available[1].max(60.0)];
    let scale = super::framebuffer_scale(ui)[0];
    let hierarchy = context
        .widgets
        .owned_hierarchy
        .as_mut()
        .ok_or("Hierarchy controls are absent")?;
    let changed = hierarchy.compiled_frame().is_none_or(|frame| {
        let environment = frame.environment();
        environment.logical_size().width() != size[0]
            || environment.logical_size().height() != size[1]
            || environment.scale_factor() != scale
    });
    if changed {
        context.widgets.owned_revision = context
            .widgets
            .owned_revision
            .checked_add(1)
            .ok_or("Hierarchy geometry identity exhausted")?;
    }
    let slot = serial
        .checked_add(1)
        .ok_or("Hierarchy identity exhausted")?;
    let surface = UiSurfaceId::new(slot).ok_or("Hierarchy surface is invalid")?;
    let logical = UiSize::new(size[0], size[1]).ok_or("Hierarchy logical size is invalid")?;
    let transform =
        UiSurfaceContentTransform::direct(logical, scale).map_err(|error| error.to_string())?;
    let revision = context.widgets.owned_revision;
    let environment = UiEnvironment::for_surface(
        surface,
        transform,
        UiSurfaceGenerations::new(slot, 1, revision, revision)
            .map_err(|error| error.to_string())?,
        1,
        1,
    )
    .map_err(|error| error.to_string())?;
    let candidate = hierarchy
        .compile(
            context.input.frame,
            context.input.ui_authoring,
            environment,
            UiTheme::default(),
            context.input.read_only,
        )
        .map_err(|error| error.to_string())?;
    context.output.owned_candidates.push((panel, candidate));
    if let Some(display) = context.input.owned_images.get(&panel).filter(|display| {
        hierarchy.retains_frame(&display.frame)
            && display.frame.environment().surface_id() == surface
            && display.frame.environment().slot_generation().get() == slot
            && display.frame.environment().logical_size() == logical
            && display.frame.environment().scale_factor() == scale
    }) {
        ui.image_config(display.image.texture, size).build();
        context.output.owned_displays.push((
            panel,
            super::image_interaction(ui, display.image.occurrence),
        ));
    } else {
        ui.text_disabled("Preparing Nara Hierarchy");
    }
    Ok(())
}
