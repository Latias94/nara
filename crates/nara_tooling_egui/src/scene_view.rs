use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, RichText, Sense, TextureId, Ui, Vec2,
};
use nara_render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};

const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 8.0;
const WHEEL_ZOOM_RATE: f32 = 0.0025;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiSceneViewContent {
    Presented {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Previewing {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Resizing,
    Unavailable,
}

/// Toolkit-local texture facts for one Scene View redraw.
///
/// Exact document, target, backend, and presentation authority remains in the private Editor Host.
#[derive(Debug, Clone, Copy)]
pub struct EguiSceneViewInput {
    content: EguiSceneViewContent,
    notice: Option<&'static str>,
}

impl EguiSceneViewInput {
    #[must_use]
    pub const fn presented(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiSceneViewContent::Presented { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn previewing(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiSceneViewContent::Previewing { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn resizing() -> Self {
        Self {
            content: EguiSceneViewContent::Resizing,
            notice: None,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            content: EguiSceneViewContent::Unavailable,
            notice: None,
        }
    }

    #[must_use]
    pub const fn with_notice(mut self, notice: &'static str) -> Self {
        self.notice = Some(notice);
        self
    }
}

impl Default for EguiSceneViewInput {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// Transient interaction emitted by one Scene View redraw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EguiSceneViewOutput {
    desired_size_points: Vec2,
    pick_uv: Option<[f32; 2]>,
    pick_point: Option<[f32; 2]>,
    mapping: Option<DisplayMapping2d>,
    camera_changed: bool,
}

impl EguiSceneViewOutput {
    /// No visible view produced a mapping or an input claim in this redraw.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            desired_size_points: Vec2::ZERO,
            pick_uv: None,
            pick_point: None,
            mapping: None,
            camera_changed: false,
        }
    }

    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }

    #[must_use]
    pub const fn pick_uv(self) -> Option<[f32; 2]> {
        self.pick_uv
    }

    #[must_use]
    pub const fn pick_point(self) -> Option<[f32; 2]> {
        self.pick_point
    }

    #[must_use]
    pub const fn display_mapping(self) -> Option<DisplayMapping2d> {
        self.mapping
    }

    #[must_use]
    pub const fn camera_changed(self) -> bool {
        self.camera_changed
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EguiSceneViewPanel {
    view_uv: Rect,
}

impl Default for EguiSceneViewPanel {
    fn default() -> Self {
        Self {
            view_uv: Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        }
    }
}

impl EguiSceneViewPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        input: EguiSceneViewInput,
        panel_size: Vec2,
    ) -> EguiSceneViewOutput {
        let initial_view_uv = self.view_uv;
        let panel_top = ui.cursor().top();
        ui.horizontal(|ui| {
            let _heading = ui.heading("Scene View");
            #[cfg(test)]
            crate::editor::record_test_control("Scene View heading", _heading.rect);

            ui.label(RichText::new(state_label(input.content)).weak());
            if ui
                .add_enabled(!self.is_reset(), egui::Button::new("Reset View"))
                .on_hover_text("Reset Scene View camera")
                .clicked()
            {
                self.reset();
            }
        });

        let header_height = (ui.cursor().top() - panel_top).max(0.0);
        let desired_size =
            Vec2::new(panel_size.x, panel_size.y - header_height).max(Vec2::splat(1.0));
        let (outer_rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        ui.painter()
            .rect_filled(outer_rect, 2.0, Color32::from_rgb(18, 20, 25));

        let input_view_uv = self.view_uv;
        let mapping = texture_facts(input.content).and_then(|(_, extent)| {
            display_mapping(
                outer_rect,
                extent,
                ui.ctx().pixels_per_point(),
                DisplayScale::Fit,
            )?
            .with_uv(display_rect(input_view_uv))
        });
        let image_rect = mapping.map_or(outer_rect, |mapping| egui_rect(mapping.content_rect()));
        if let Some((texture_id, _)) = texture_facts(input.content) {
            ui.painter()
                .image(texture_id, image_rect, self.view_uv, Color32::WHITE);
        }

        let pointer_in_content = response
            .interact_pointer_pos()
            .filter(|pointer| image_rect.contains(*pointer));
        if pointer_in_content.is_some() && response.dragged_by(PointerButton::Middle) {
            let delta = response.drag_delta();
            if delta != Vec2::ZERO {
                self.pan(delta, image_rect.size());
            }
        }
        if let Some(pointer) = pointer_in_content {
            let wheel = ui.input(|state| state.smooth_scroll_delta().y);
            if wheel != 0.0 {
                self.zoom_around(pointer, image_rect, wheel);
            }
        }

        let pick_point = if matches!(input.content, EguiSceneViewContent::Presented { .. })
            && response.clicked_by(PointerButton::Primary)
        {
            pointer_in_content.map(|pointer| [pointer.x, pointer.y])
        } else {
            None
        };
        let pick_uv = pick_point.and_then(|point| mapping?.panel_to_uv(point));

        if !matches!(input.content, EguiSceneViewContent::Presented { .. })
            || input.notice.is_some()
        {
            let label = input.notice.unwrap_or_else(|| state_label(input.content));
            ui.painter().text(
                outer_rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.0),
                Color32::from_gray(210),
            );
        }

        EguiSceneViewOutput {
            desired_size_points: outer_rect.size(),
            pick_uv,
            pick_point,
            mapping,
            camera_changed: self.view_uv != initial_view_uv,
        }
    }

    fn is_reset(self) -> bool {
        self.view_uv == Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
    }

    fn reset(&mut self) {
        self.view_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }

    fn pan(&mut self, delta: Vec2, display_size: Vec2) {
        if display_size.x <= 0.0 || display_size.y <= 0.0 {
            return;
        }
        let shift = Vec2::new(
            -delta.x / display_size.x * self.view_uv.width(),
            -delta.y / display_size.y * self.view_uv.height(),
        );
        self.view_uv = clamp_view_uv(self.view_uv.translate(shift));
    }

    fn zoom_around(&mut self, pointer: Pos2, display: Rect, wheel: f32) {
        let anchor = normalized_pointer(pointer, display);
        let old_size = self.view_uv.size();
        let old_zoom = 1.0 / old_size.x.max(old_size.y);
        let next_zoom = (old_zoom * (wheel * WHEEL_ZOOM_RATE).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
        let next_size = Vec2::splat(1.0 / next_zoom);
        let texture_anchor = self.view_uv.min + anchor * old_size;
        let next_min = texture_anchor - anchor * next_size;
        self.view_uv = clamp_view_uv(Rect::from_min_size(next_min, next_size));
    }
}

fn state_label(content: EguiSceneViewContent) -> &'static str {
    match content {
        EguiSceneViewContent::Presented { .. } => "Ready",
        EguiSceneViewContent::Previewing { .. } => "Updating",
        EguiSceneViewContent::Resizing => "Resizing",
        EguiSceneViewContent::Unavailable => "Unavailable",
    }
}

fn texture_facts(content: EguiSceneViewContent) -> Option<(TextureId, [u32; 2])> {
    match content {
        EguiSceneViewContent::Presented { texture_id, extent }
        | EguiSceneViewContent::Previewing { texture_id, extent } => Some((texture_id, extent)),
        EguiSceneViewContent::Resizing | EguiSceneViewContent::Unavailable => None,
    }
}

fn normalized_pointer(pointer: Pos2, rect: Rect) -> Vec2 {
    Vec2::new(
        ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    )
}

fn clamp_view_uv(mut view: Rect) -> Rect {
    if view.left() < 0.0 {
        view = view.translate(Vec2::new(-view.left(), 0.0));
    }
    if view.right() > 1.0 {
        view = view.translate(Vec2::new(1.0 - view.right(), 0.0));
    }
    if view.top() < 0.0 {
        view = view.translate(Vec2::new(0.0, -view.top()));
    }
    if view.bottom() > 1.0 {
        view = view.translate(Vec2::new(0.0, 1.0 - view.bottom()));
    }
    view
}

pub(crate) fn display_mapping(
    outer: Rect,
    extent: [u32; 2],
    pixels_per_point: f32,
    scale: DisplayScale,
) -> Option<DisplayMapping2d> {
    DisplayMapping2d::fit(
        Extent2d::new(extent[0], extent[1])?,
        display_rect(outer),
        pixels_per_point,
        scale,
    )
}

fn display_rect(rect: Rect) -> DisplayRect {
    DisplayRect {
        min: [rect.min.x, rect.min.y],
        max: [rect.max.x, rect.max.y],
    }
}

pub(crate) fn egui_rect(rect: DisplayRect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min[0], rect.min[1]),
        Pos2::new(rect.max[0], rect.max[1]),
    )
}
