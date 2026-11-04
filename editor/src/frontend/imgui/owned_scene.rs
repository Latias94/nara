use std::sync::Arc;

use dear_imgui_rs::Ui;
use nara::render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};
use nara_ui_kernel::{
    UiEnvironment, UiFrame, UiPoint, UiRootKey, UiSize, UiSurfaceContentTransform,
    UiSurfaceGenerations, UiSurfaceId,
};

use crate::workspace::EditorPanelInstanceId;

use super::view::{OwnedSceneView, SceneViewImage, SceneViewInput, SceneViewPlacement};
use super::{ImguiEditorFrontend, ImguiEditorPanelContext};

impl ImguiEditorFrontend {
    pub(crate) fn owned_scene_placement(
        &self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
    ) -> Option<SceneViewPlacement> {
        self.presentation
            .widgets
            .get(&panel)?
            .scene_view
            .as_ref()?
            .placement(frame)
            .ok()
    }

    pub(crate) fn set_owned_scene_pointer(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        point: Option<UiPoint>,
    ) {
        if let Some(scene) = self
            .presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.scene_view.as_mut())
        {
            match point {
                Some(point) => {
                    let _ = scene.set_pointer(frame, point);
                }
                None => {
                    let _ = scene.clear_pointer(frame);
                }
            }
        }
    }

    pub(crate) fn pan_owned_scene(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        delta: [f32; 2],
    ) -> Result<bool, String> {
        self.presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.scene_view.as_mut())
            .ok_or("Scene panel is retired")?
            .pan(frame, delta)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn zoom_owned_scene(
        &mut self,
        panel: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
        point: UiPoint,
        wheel: f32,
    ) -> Result<bool, String> {
        self.presentation
            .widgets
            .get_mut(&panel)
            .and_then(|widgets| widgets.scene_view.as_mut())
            .ok_or("Scene panel is retired")?
            .zoom(frame, point, wheel)
            .map_err(|error| error.to_string())
    }
}

pub(super) fn paint(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    if let Err(error) = paint_panel(ui, context) {
        ui.text_wrapped(error);
    }
}

fn paint_panel(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) -> Result<(), String> {
    let panel = context.model.instance().id();
    let serial = context.widgets.incarnation.serial;
    if context.widgets.scene_view.is_none() {
        let fonts = if context.input.owned_fonts.is_empty() {
            vec![nara_text::bundled_font_face().map_err(|error| error.to_string())?]
        } else {
            context.input.owned_fonts.to_vec()
        };
        let root = UiRootKey::runtime("nara.editor.scene", serial.to_string())
            .map_err(|error| error.to_string())?;
        context.widgets.scene_view =
            Some(OwnedSceneView::new(root, fonts).map_err(|error| error.to_string())?);
    }
    let available = ui.content_region_avail();
    let size = [available[0].max(1.0), available[1].max(1.0)];
    let scale = super::framebuffer_scale(ui)[0];
    let scene = context
        .widgets
        .scene_view
        .as_mut()
        .ok_or("Scene controls are absent")?;
    let changed = scene.compiled_frame().is_none_or(|frame| {
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
            .ok_or("Scene geometry identity exhausted")?;
    }
    let slot = serial.checked_add(1).ok_or("Scene identity exhausted")?;
    let surface = UiSurfaceId::new(slot).ok_or("Scene identity is invalid")?;
    let logical = UiSize::new(size[0], size[1]).ok_or("Scene logical size is invalid")?;
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
    let model = context.input.frame;
    let input = SceneViewInput {
        image: context.input.scene.and_then(|image| {
            Some(SceneViewImage {
                extent: Extent2d::new(image.extent[0], image.extent[1])?,
                occurrence: image.occurrence?.get(),
            })
        }),
        presented: context.input.canvas_presented,
        editing: !context.input.read_only
            && model
                .scene_preview_document()
                .is_some_and(|document| model.workspace().active_document == Some(document)),
        unsaved_dependencies: model.scene_preview_unsaved_dependencies(),
    };
    let candidate = scene
        .compile(environment, input)
        .map_err(|error| error.to_string())?;
    let next_placement = scene
        .placement(&candidate)
        .map_err(|error| error.to_string())?;
    context.output.scene_extent = Some([
        (next_placement.clip.max[0] - next_placement.clip.min[0]).max(1.0) * scale,
        (next_placement.clip.max[1] - next_placement.clip.min[1]).max(1.0) * scale,
    ]);
    context.output.owned_candidates.push((panel, candidate));
    let min = ui.cursor_screen_pos();
    ui.dummy(size);
    let Some(display) = context.input.owned_images.get(&panel).filter(|display| {
        display.frame.environment().surface_id() == surface
            && display.frame.environment().slot_generation().get() == slot
            && display.frame.environment().logical_size() == logical
            && display.frame.environment().scale_factor() == scale
    }) else {
        ui.get_window_draw_list()
            .add_rect(
                min,
                [min[0] + size[0], min[1] + size[1]],
                [0.075, 0.085, 0.11, 1.0],
            )
            .filled(true)
            .build();
        context.widgets.scene_pick = None;
        return Ok(());
    };
    let placement = scene
        .placement(&display.frame)
        .map_err(|error| error.to_string())?;
    let interaction = super::image_interaction(ui, display.image.occurrence);
    let draw = ui.get_window_draw_list();
    draw.add_image(
        display.image.texture,
        min,
        [min[0] + size[0], min[1] + size[1]],
        [0.0, 0.0],
        [1.0, 1.0],
        [1.0; 4],
    );
    context.output.owned_displays.push((panel, interaction));
    context.output.scene_camera_changed = placement.mapping.map(DisplayMapping2d::uv_rect)
        != next_placement.mapping.map(DisplayMapping2d::uv_rect);
    if let Some((image, local)) = display.scene.zip(placement.mapping) {
        let shift = |point: [f32; 2]| [point[0] + min[0], point[1] + min[1]];
        let panel_rect = local.panel_rect();
        let mapping = DisplayMapping2d::fit(
            local.target(),
            DisplayRect {
                min: shift(panel_rect.min),
                max: shift(panel_rect.max),
            },
            scale,
            DisplayScale::Fit,
        )
        .and_then(|mapping| mapping.with_uv(local.uv_rect()))
        .ok_or("Scene display mapping is invalid")?;
        let clip_min = shift(placement.clip.min);
        let clip_max = shift(placement.clip.max);
        let parent = DisplayRect {
            min: draw.clip_rect_min(),
            max: draw.clip_rect_max(),
        };
        let content = mapping.content_rect();
        let clip = DisplayRect {
            min: [
                clip_min[0].max(parent.min[0]).max(content.min[0]),
                clip_min[1].max(parent.min[1]).max(content.min[1]),
            ],
            max: [
                clip_max[0].min(parent.max[0]).min(content.max[0]),
                clip_max[1].min(parent.max[1]).min(content.max[1]),
            ],
        };
        if clip.min[0] < clip.max[0] && clip.min[1] < clip.max[1] {
            let _clip = draw.push_clip_rect(clip.min, clip.max, true);
            draw.add_image(
                image.texture,
                content.min,
                content.max,
                local.uv_rect().min,
                local.uv_rect().max,
                [1.0; 4],
            );
            context.output.scene_mapping = Some(mapping);
            context.output.scene_clip = Some(clip);
            context.output.authoring_occurrence = image.occurrence;
            if let Some((frame, pick)) = context.widgets.scene_pick.take()
                && Arc::ptr_eq(&frame, &display.frame)
                && Some(pick.image) == placement.image
                && pick.mapping == local
                && clip.contains(shift(pick.point))
                && input.editing
            {
                context.output.scene_pick_point = Some(shift(pick.point));
            }
        }
    }
    context.widgets.scene_pick = None;
    Ok(())
}
