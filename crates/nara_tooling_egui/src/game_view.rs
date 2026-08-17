use egui::{Align2, Color32, FontId, RichText, Sense, TextureId, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiGameViewContent {
    Presented {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Updating {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Waiting,
    Unavailable,
}

/// Toolkit-local texture facts for one Game View redraw.
///
/// Runtime, target, backend, and presentation authority remain in the private Editor Host.
#[derive(Debug, Clone, Copy)]
pub struct EguiGameViewInput {
    content: EguiGameViewContent,
    notice: Option<&'static str>,
}

impl EguiGameViewInput {
    #[must_use]
    pub const fn presented(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiGameViewContent::Presented { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn updating(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiGameViewContent::Updating { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn waiting() -> Self {
        Self {
            content: EguiGameViewContent::Waiting,
            notice: None,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            content: EguiGameViewContent::Unavailable,
            notice: None,
        }
    }

    #[must_use]
    pub const fn with_notice(mut self, notice: &'static str) -> Self {
        self.notice = Some(notice);
        self
    }
}

impl Default for EguiGameViewInput {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// Transient layout facts emitted by one Game View redraw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EguiGameViewOutput {
    desired_size_points: Vec2,
}

impl EguiGameViewOutput {
    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }
}

#[derive(Debug, Default)]
pub(crate) struct EguiGameViewPanel;

impl EguiGameViewPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        input: EguiGameViewInput,
        panel_size: Vec2,
    ) -> EguiGameViewOutput {
        let panel_top = ui.cursor().top();
        ui.horizontal(|ui| {
            let _heading = ui.heading("Game View");
            #[cfg(test)]
            crate::editor::record_test_control("Game View heading", _heading.rect);
            ui.label(RichText::new(state_label(input.content)).weak());
        });

        let header_height = (ui.cursor().top() - panel_top).max(0.0);
        let desired_size =
            Vec2::new(panel_size.x, panel_size.y - header_height).max(Vec2::splat(1.0));
        let (outer_rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
        ui.painter()
            .rect_filled(outer_rect, 2.0, Color32::from_rgb(18, 20, 25));

        if let Some((texture_id, extent)) = texture_facts(input.content) {
            let image_rect = crate::scene_view::fitted_image_rect(outer_rect, extent);
            ui.painter().image(
                texture_id,
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        if !matches!(input.content, EguiGameViewContent::Presented { .. }) || input.notice.is_some()
        {
            ui.painter().text(
                outer_rect.center(),
                Align2::CENTER_CENTER,
                input.notice.unwrap_or_else(|| state_label(input.content)),
                FontId::proportional(13.0),
                Color32::from_gray(210),
            );
        }
        EguiGameViewOutput {
            desired_size_points: outer_rect.size(),
        }
    }
}

fn state_label(content: EguiGameViewContent) -> &'static str {
    match content {
        EguiGameViewContent::Presented { .. } => "Ready",
        EguiGameViewContent::Updating { .. } => "Updating",
        EguiGameViewContent::Waiting => "Waiting for Runtime",
        EguiGameViewContent::Unavailable => "Unavailable",
    }
}

fn texture_facts(content: EguiGameViewContent) -> Option<(TextureId, [u32; 2])> {
    match content {
        EguiGameViewContent::Presented { texture_id, extent }
        | EguiGameViewContent::Updating { texture_id, extent } => Some((texture_id, extent)),
        EguiGameViewContent::Waiting | EguiGameViewContent::Unavailable => None,
    }
}
