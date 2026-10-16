use egui::{Align2, Color32, FontId, PointerButton, Rect, RichText, Sense, TextureId, Ui, Vec2};
use nara_render::DisplayScale;

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
    capture_active: bool,
    modal_active: bool,
    restore_focus: bool,
    scale: DisplayScale,
}

impl EguiGameViewInput {
    #[must_use]
    pub const fn presented(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiGameViewContent::Presented { texture_id, extent },
            notice: None,
            capture_active: false,
            modal_active: false,
            restore_focus: false,
            scale: DisplayScale::Fit,
        }
    }

    #[must_use]
    pub const fn updating(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiGameViewContent::Updating { texture_id, extent },
            notice: None,
            capture_active: false,
            modal_active: false,
            restore_focus: false,
            scale: DisplayScale::Fit,
        }
    }

    #[must_use]
    pub const fn waiting() -> Self {
        Self {
            content: EguiGameViewContent::Waiting,
            notice: None,
            capture_active: false,
            modal_active: false,
            restore_focus: false,
            scale: DisplayScale::Fit,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            content: EguiGameViewContent::Unavailable,
            notice: None,
            capture_active: false,
            modal_active: false,
            restore_focus: false,
            scale: DisplayScale::Fit,
        }
    }

    #[must_use]
    pub const fn with_notice(mut self, notice: &'static str) -> Self {
        self.notice = Some(notice);
        self
    }

    #[must_use]
    pub const fn with_integer_scaling(mut self, enabled: bool) -> Self {
        self.scale = if enabled {
            DisplayScale::IntegerFit
        } else {
            DisplayScale::Fit
        };
        self
    }

    #[must_use]
    pub const fn with_capture_active(mut self, active: bool) -> Self {
        self.capture_active = active;
        self
    }

    #[must_use]
    pub const fn with_modal_active(mut self, active: bool) -> Self {
        self.modal_active = active;
        self
    }

    #[must_use]
    pub const fn with_restore_focus(mut self, restore: bool) -> Self {
        self.restore_focus = restore;
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
    content_rect_points: Option<Rect>,
    image_layer: egui::LayerId,
    capture_requested: bool,
    image_has_focus: bool,
    image_hovered: bool,
    primary_pointer_pressed_outside_image: bool,
    requested_scale: DisplayScale,
}

impl EguiGameViewOutput {
    /// Tests the actual topmost toolkit layer as well as the image's displayed rectangle.
    #[must_use]
    pub fn owns_pointer(self, context: &egui::Context, point: egui::Pos2) -> bool {
        self.content_rect_points
            .is_some_and(|rect| rect.contains(point))
            && context.layer_id_at(point) == Some(self.image_layer)
    }

    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }

    #[must_use]
    pub const fn content_rect_points(self) -> Option<Rect> {
        self.content_rect_points
    }

    #[must_use]
    pub const fn capture_requested(self) -> bool {
        self.capture_requested
    }

    #[must_use]
    pub const fn image_has_focus(self) -> bool {
        self.image_has_focus
    }

    #[must_use]
    pub const fn image_hovered(self) -> bool {
        self.image_hovered
    }

    #[must_use]
    pub const fn primary_pointer_pressed_outside_image(self) -> bool {
        self.primary_pointer_pressed_outside_image
    }

    #[must_use]
    pub const fn requested_scale(self) -> DisplayScale {
        self.requested_scale
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
        let mut integer_scaling = input.scale == DisplayScale::IntegerFit;
        ui.horizontal(|ui| {
            let _heading = ui.heading("Game View");
            #[cfg(test)]
            crate::editor::record_test_control("Game View heading", _heading.rect);
            ui.label(RichText::new(state_label(input.content)).weak());
            let _scaling = ui
                .checkbox(&mut integer_scaling, "Integer scaling")
                .on_hover_text(
                    "Keep the game canvas size and use nearest sampling with whole-pixel scaling",
                );
            #[cfg(test)]
            crate::editor::record_test_control("Game View integer scaling", _scaling.rect);
        });

        let header_height = (ui.cursor().top() - panel_top).max(0.0);
        let desired_size =
            Vec2::new(panel_size.x, panel_size.y - header_height).max(Vec2::splat(1.0));
        let (outer_rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        if input.restore_focus {
            response.request_focus();
        }
        ui.painter()
            .rect_filled(outer_rect, 2.0, Color32::from_rgb(18, 20, 25));

        let content_rect_points = if let Some((texture_id, extent)) = texture_facts(input.content) {
            let image_rect = crate::scene_view::display_mapping(
                outer_rect,
                extent,
                ui.ctx().pixels_per_point(),
                input.scale,
            )
            .map_or(outer_rect, |mapping| {
                crate::scene_view::egui_rect(mapping.content_rect())
            });
            ui.painter().image(
                texture_id,
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            Some(image_rect)
        } else {
            None
        };

        let capture_requested = !input.capture_active
            && !input.modal_active
            && matches!(input.content, EguiGameViewContent::Presented { .. })
            && response.clicked_by(PointerButton::Primary)
            && response
                .interact_pointer_pos()
                .zip(content_rect_points)
                .is_some_and(|(position, content)| content.contains(position));
        if capture_requested {
            response.request_focus();
        }
        let primary_pointer_pressed_outside_image = ui.input(|state| {
            state.pointer.button_pressed(PointerButton::Primary)
                && !state
                    .pointer
                    .interact_pos()
                    .zip(content_rect_points)
                    .is_some_and(|(position, content)| content.contains(position))
        });
        if primary_pointer_pressed_outside_image {
            response.surrender_focus();
        }
        let image_has_focus =
            content_rect_points.is_some() && !input.modal_active && response.has_focus();
        let image_hovered = response.hovered()
            && ui.input(|state| {
                state
                    .pointer
                    .hover_pos()
                    .zip(content_rect_points)
                    .is_some_and(|(position, content)| content.contains(position))
            });

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
        if input.capture_active {
            ui.painter().text(
                outer_rect.left_top() + Vec2::new(8.0, 8.0),
                Align2::LEFT_TOP,
                "Game input captured — Esc to release",
                FontId::proportional(12.0),
                Color32::from_gray(220),
            );
        }
        EguiGameViewOutput {
            desired_size_points: outer_rect.size(),
            content_rect_points,
            image_layer: ui.layer_id(),
            capture_requested,
            image_has_focus,
            image_hovered,
            primary_pointer_pressed_outside_image,
            requested_scale: if integer_scaling {
                DisplayScale::IntegerFit
            } else {
                DisplayScale::Fit
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_floating_inspector_owns_hits_inside_the_game_image_rectangle() {
        let context = egui::Context::default();
        let mut panel = EguiGameViewPanel;
        let mut text = String::new();
        let mut facts = None;
        for _ in 0..3 {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let game_layer = ui.layer_id();
                    let game = panel.show(
                        ui,
                        EguiGameViewInput::presented(TextureId::User(1), [320, 240]),
                        Vec2::new(600.0, 400.0),
                    );
                    egui::Window::new("Floating Inspector")
                        .fixed_pos(egui::pos2(250.0, 90.0))
                        .fixed_size(Vec2::new(200.0, 150.0))
                        .show(ui.ctx(), |ui| {
                            let field = ui.text_edit_singleline(&mut text);
                            facts = Some((game, game_layer, field.rect.center()));
                        });
                },
            );
            output.drop_without_applying_deltas();
        }
        let (game, layer, point) = facts.unwrap();
        assert!(game.content_rect_points().unwrap().contains(point));
        assert_ne!(context.layer_id_at(point), Some(layer));
        assert!(!game.owns_pointer(&context, point));
        assert!(game.owns_pointer(
            &context,
            game.content_rect_points().unwrap().min + Vec2::splat(10.0)
        ));
    }

    fn paint(
        context: &egui::Context,
        panel: &mut EguiGameViewPanel,
        input: EguiGameViewInput,
        events: Vec<egui::Event>,
    ) -> EguiGameViewOutput {
        let mut result = None;
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(900.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                result = Some(panel.show(ui, input, Vec2::new(600.0, 400.0)));
            },
        );
        output.drop_without_applying_deltas();
        result.unwrap()
    }

    #[test]
    fn integer_canvas_is_an_ordinary_panel_choice_and_uses_physical_pixel_fit() {
        let context = egui::Context::default();
        context.set_pixels_per_point(2.0);
        let mut panel = EguiGameViewPanel;
        let input = EguiGameViewInput::presented(TextureId::User(1), [160, 90]);
        let before = paint(&context, &mut panel, input, vec![]);
        assert_eq!(before.requested_scale(), DisplayScale::Fit);
        let position = crate::editor::test_control_rect("Game View integer scaling")
            .unwrap()
            .center();
        let _ = paint(
            &context,
            &mut panel,
            input,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        let requested = paint(
            &context,
            &mut panel,
            input,
            vec![egui::Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(requested.requested_scale(), DisplayScale::IntegerFit);
        assert_eq!(
            requested.content_rect_points(),
            before.content_rect_points(),
            "the policy request applies after the current native paint transaction"
        );
        let applied = paint(
            &context,
            &mut panel,
            input.with_integer_scaling(true),
            vec![],
        );
        let content = applied.content_rect_points().unwrap();
        let scale = content.width() * context.pixels_per_point() / 160.0;
        assert_eq!(scale.fract(), 0.0);
        assert_eq!(scale, content.height() * context.pixels_per_point() / 90.0);
        assert_eq!((content.min.x * context.pixels_per_point()).fract(), 0.0);
    }

    #[test]
    fn game_image_focus_releases_to_an_inspector_text_field() {
        fn draw(
            context: &egui::Context,
            panel: &mut EguiGameViewPanel,
            text: &mut String,
            events: Vec<egui::Event>,
        ) -> (EguiGameViewOutput, Rect, bool) {
            let mut result = None;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(900.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let mut game_ui =
                        ui.new_child(egui::UiBuilder::new().id_salt("game-panel").max_rect(
                            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(600.0, 400.0)),
                        ));
                    let game = panel.show(
                        &mut game_ui,
                        EguiGameViewInput::presented(TextureId::User(1), [320, 240]),
                        Vec2::new(600.0, 400.0),
                    );
                    let mut inspector =
                        ui.new_child(egui::UiBuilder::new().id_salt("inspector-panel").max_rect(
                            Rect::from_min_size(egui::pos2(650.0, 40.0), Vec2::new(200.0, 200.0)),
                        ));
                    let field = inspector.text_edit_singleline(text);
                    result = Some((game, field.rect, field.has_focus()));
                },
            );
            output.drop_without_applying_deltas();
            result.unwrap()
        }
        fn pointer(position: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        }
        let context = egui::Context::default();
        let mut panel = EguiGameViewPanel;
        let mut text = String::new();
        let (initial, inspector, _) = draw(&context, &mut panel, &mut text, vec![]);
        let game_position = initial.content_rect_points().unwrap().center();
        let _ = draw(
            &context,
            &mut panel,
            &mut text,
            pointer(game_position, true),
        );
        let (captured, _, _) = draw(
            &context,
            &mut panel,
            &mut text,
            pointer(game_position, false),
        );
        assert!(captured.capture_requested());
        assert!(captured.image_has_focus() && captured.image_hovered());

        let (released, _, field_has_focus) = draw(
            &context,
            &mut panel,
            &mut text,
            pointer(inspector.center(), true),
        );
        assert!(released.primary_pointer_pressed_outside_image());
        assert!(!released.image_has_focus() && !released.image_hovered());
        assert!(field_has_focus);
        let mut events = pointer(inspector.center(), false);
        events.push(egui::Event::Text("Inspector".to_owned()));
        let (game, _, field_has_focus) = draw(&context, &mut panel, &mut text, events);
        assert_eq!(text, "Inspector");
        assert!(field_has_focus);
        assert!(!game.capture_requested() && !game.image_has_focus());
    }
}
