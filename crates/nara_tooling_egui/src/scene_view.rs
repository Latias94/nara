use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, RichText, Sense, TextureId, Ui, Vec2,
};

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
    camera_changed: bool,
}

impl EguiSceneViewOutput {
    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }

    #[must_use]
    pub const fn pick_uv(self) -> Option<[f32; 2]> {
        self.pick_uv
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

        let image_rect = texture_facts(input.content).map_or(outer_rect, |(_, extent)| {
            fitted_image_rect(outer_rect, extent)
        });
        let input_view_uv = self.view_uv;
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

        let pick_uv = if matches!(input.content, EguiSceneViewContent::Presented { .. })
            && response.clicked_by(PointerButton::Primary)
        {
            pointer_in_content.map(|pointer| texture_uv(input_view_uv, pointer, image_rect))
        } else {
            None
        };

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

fn texture_uv(view: Rect, pointer: Pos2, display: Rect) -> [f32; 2] {
    let normalized = normalized_pointer(pointer, display);
    let texture = view.min + normalized * view.size();
    [texture.x, texture.y]
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

fn fitted_image_rect(outer: Rect, extent: [u32; 2]) -> Rect {
    let source = Vec2::new(extent[0] as f32, extent[1] as f32);
    if source.x <= 0.0 || source.y <= 0.0 {
        return outer;
    }
    let scale = (outer.width() / source.x).min(outer.height() / source.y);
    Rect::from_center_size(outer.center(), source * scale)
}
