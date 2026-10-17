use dear_imgui_rs::Ui;
use nara::{
    render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d},
    tooling::{EditorPlayState, EditorPlayView},
};

use super::{ImguiImageInteraction, ImguiViewImage};

#[derive(Default)]
pub(super) struct ImguiGameView {
    integer_scaling: bool,
}

impl ImguiGameView {
    pub(super) fn show(
        &mut self,
        ui: &Ui,
        image: Option<ImguiViewImage>,
        play: &EditorPlayView,
        captured: bool,
    ) -> (Option<[f32; 2]>, Option<ImguiImageInteraction>) {
        ui.checkbox("Integer scaling", &mut self.integer_scaling);

        let min = ui.cursor_screen_pos();
        let available = ui.content_region_avail();
        let size = [available[0].max(1.0), available[1].max(1.0)];
        let pixels_per_point = super::framebuffer_scale(ui);
        let requested_extent = (!self.integer_scaling)
            .then_some([size[0] * pixels_per_point[0], size[1] * pixels_per_point[1]]);
        let panel = DisplayRect {
            min,
            max: [min[0] + size[0], min[1] + size[1]],
        };
        let display_scale = if self.integer_scaling {
            DisplayScale::IntegerFit
        } else {
            DisplayScale::Fit
        };
        let mapping = image.and_then(|image| {
            DisplayMapping2d::fit(
                Extent2d::new(image.extent[0], image.extent[1])?,
                panel,
                pixels_per_point[0],
                display_scale,
            )
        });
        let interaction = if let Some((image, mapping)) = image.zip(mapping) {
            let content = mapping.content_rect();
            ui.set_cursor_screen_pos(content.min);
            let draw = ui.get_window_draw_list();
            if self.integer_scaling {
                draw.set_sampler_nearest();
            }
            ui.image(
                image.texture,
                [
                    content.max[0] - content.min[0],
                    content.max[1] - content.min[1],
                ],
            );
            let interaction = super::image_interaction(ui);
            if self.integer_scaling {
                draw.set_sampler_linear();
            }
            if captured {
                draw.with_clip_rect(panel.min, panel.max, || {
                    draw.add_text(
                        [content.min[0] + 8.0, content.min[1] + 8.0],
                        [0.9, 0.9, 0.9, 1.0],
                        "Game input captured — Esc to release",
                    );
                });
            }
            Some(interaction)
        } else {
            ui.text("Start an admitted embedded Game View");
            None
        };
        if play.is_out_of_date() || play.state() == EditorPlayState::Paused {
            // Feedback overlays the established image rectangle. Transient Play state must not
            // resize the Runtime target or revoke the input route while a fixed Step presents.
            ui.set_cursor_screen_pos(panel.min);
            ui.child_window("game_notice_overlay")
                .size(size)
                .flags(
                    dear_imgui_rs::WindowFlags::NO_INPUTS
                        | dear_imgui_rs::WindowFlags::NO_NAV_FOCUS
                        | dear_imgui_rs::WindowFlags::NO_FOCUS_ON_APPEARING
                        | dear_imgui_rs::WindowFlags::NO_BACKGROUND
                        | dear_imgui_rs::WindowFlags::NO_SCROLLBAR
                        | dear_imgui_rs::WindowFlags::NO_SCROLL_WITH_MOUSE,
                )
                .build(ui, || {
                    ui.set_cursor_screen_pos([
                        panel.min[0] + 8.0,
                        panel.min[1]
                            + if captured {
                                ui.text_line_height_with_spacing() + 8.0
                            } else {
                                8.0
                            },
                    ]);
                    if play.is_out_of_date() {
                        ui.text_wrapped(
                            "Game View is out of date; save and refresh content before Restart",
                        );
                    } else {
                        ui.text_wrapped(
                            "Play is paused; Scene authoring remains isolated from this Runtime",
                        );
                    }
                });
        }
        ui.set_cursor_screen_pos([panel.min[0], panel.max[1]]);
        ui.dummy([0.0, 0.0]);
        (requested_extent, interaction)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use dear_imgui_rs::{
        ClipboardBackend, Condition, Context, MouseButton, TextureId, WindowFlags, render::DrawCmd,
        sys,
    };
    use nara::{render::DisplayRect, tooling::EditorPlayState};

    use super::*;

    const GAME_TEXTURE: TextureId = TextureId::new(71);
    const FOLLOWING_TEXTURE: TextureId = TextureId::new(72);

    struct RenderedText(Rc<RefCell<String>>);

    impl ClipboardBackend for RenderedText {
        fn get(&mut self) -> Option<String> {
            Some(self.0.borrow().clone())
        }

        fn set(&mut self, value: &str) {
            value.clone_into(&mut self.0.borrow_mut());
        }
    }

    unsafe extern "C" fn nearest(
        _draw_list: *const sys::ImDrawList,
        _command: *const sys::ImDrawCmd,
    ) {
    }

    unsafe extern "C" fn linear(
        _draw_list: *const sys::ImDrawList,
        _command: *const sys::ImDrawCmd,
    ) {
    }

    fn context() -> Context {
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_display_framebuffer_scale([1.5, 1.5]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        // These markers prove command generation only; this Context has no GPU renderer.
        unsafe {
            context
                .platform_io_mut()
                .set_draw_callback_set_sampler_nearest_raw(Some(nearest));
            context
                .platform_io_mut()
                .set_draw_callback_set_sampler_linear_raw(Some(linear));
        }
        context
    }

    struct RenderedGame {
        requested_extent: Option<[f32; 2]>,
        image: ImguiImageInteraction,
        checkbox_point: [f32; 2],
        available_panel: DisplayRect,
        commands: Vec<&'static str>,
        vertices: usize,
    }

    fn draw(
        context: &mut Context,
        game: &mut ImguiGameView,
        play: &EditorPlayView,
        captured: bool,
        log_text: bool,
    ) -> RenderedGame {
        context.main_viewport().set_framebuffer_scale([1.5, 1.5]);
        let binding = context.binding();
        let ui = context.frame();
        let mut output = None;
        ui.window("Embedded Game")
            .position([20.0, 20.0], Condition::Always)
            .size([347.0, 263.0], Condition::Always)
            .flags(
                WindowFlags::NO_DECORATION
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::NO_SCROLL_WITH_MOUSE,
            )
            .build(|| {
                let origin = ui.cursor_screen_pos();
                let available = ui.content_region_avail();
                let header = ui.frame_height_with_spacing();
                if log_text {
                    ui.log_to_clipboard(0_u32);
                }
                let (requested, image) = game.show(
                    ui,
                    Some(ImguiViewImage {
                        texture: GAME_TEXTURE,
                        extent: [128, 64],
                    }),
                    play,
                    captured,
                );
                if log_text {
                    ui.log_finish();
                }
                // A subsequent image must return to linear sampling after the Game image.
                ui.get_window_draw_list().add_image(
                    FOLLOWING_TEXTURE,
                    [30.0, 30.0],
                    [32.0, 32.0],
                    [0.0, 0.0],
                    [1.0, 1.0],
                    dear_imgui_rs::Color::WHITE,
                );
                output = Some((
                    requested,
                    image.expect("the supplied Game image must create an interaction"),
                    [origin[0] + 8.0, origin[1] + 8.0],
                    DisplayRect {
                        min: [origin[0], origin[1] + header],
                        max: [origin[0] + available[0], origin[1] + available[1]],
                    },
                ));
            });
        let frame = context.render_legacy();
        let commands = binding.with_bound_context(|| {
            frame
                .draw_data()
                .draw_lists()
                .flat_map(|list| list.commands())
                .filter_map(|command| match command {
                    DrawCmd::SetSamplerNearest => Some("nearest"),
                    DrawCmd::SetSamplerLinear => Some("linear"),
                    DrawCmd::Elements { count, cmd_params } if count > 0 => {
                        if cmd_params.texture_id == GAME_TEXTURE {
                            Some("game-image")
                        } else if cmd_params.texture_id == FOLLOWING_TEXTURE {
                            Some("following-image")
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect()
        });
        let vertices = frame.total_vtx_count();
        let (requested_extent, image, checkbox_point, available_panel) = output.unwrap();
        RenderedGame {
            requested_extent,
            image,
            checkbox_point,
            available_panel,
            commands,
            vertices,
        }
    }

    fn play(state: EditorPlayState, stale: bool) -> EditorPlayView {
        EditorPlayView::new(state, Some(1), None, None, stale, None)
    }

    fn click_scaling(
        context: &mut Context,
        game: &mut ImguiGameView,
        play: &EditorPlayView,
        point: [f32; 2],
    ) -> RenderedGame {
        context.io_mut().add_mouse_pos_event(point);
        draw(context, game, play, false, false);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        draw(context, game, play, false, false);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        draw(context, game, play, false, false)
    }

    #[test]
    fn integer_scaling_checkbox_keeps_canvas_resolution_and_scopes_nearest_sampling() {
        let mut context = context();
        let play = play(EditorPlayState::Running, false);
        let mut game = ImguiGameView::default();
        for _ in 0..4 {
            draw(&mut context, &mut game, &play, false, false);
        }
        let fit = draw(&mut context, &mut game, &play, false, false);
        assert!(fit.requested_extent.is_some());
        let integer = click_scaling(&mut context, &mut game, &play, fit.checkbox_point);
        assert_eq!(
            integer.requested_extent, None,
            "integer scaling must retain the game canvas"
        );
        let rect = integer.image.rect;
        let physical = [
            (rect.max[0] - rect.min[0]) * integer.image.pixels_per_point,
            (rect.max[1] - rect.min[1]) * integer.image.pixels_per_point,
        ];
        // The nonintegral 496.5-pixel panel fits three complete 128-pixel game canvases.
        assert!((physical[0] - 384.0).abs() < 0.001);
        assert!((physical[1] - 192.0).abs() < 0.001);
        for axis in 0..2 {
            let physical_origin = rect.min[axis] * integer.image.pixels_per_point;
            assert!((physical_origin - physical_origin.round()).abs() < 0.001);
            let content_center = (rect.min[axis] + rect.max[axis]) * 0.5;
            let panel_center =
                (integer.available_panel.min[axis] + integer.available_panel.max[axis]) * 0.5;
            assert!(
                (content_center - panel_center).abs()
                    <= 0.5 / integer.image.pixels_per_point + 0.001
            );
        }
        assert_eq!(
            integer.commands,
            ["nearest", "game-image", "linear", "following-image"]
        );
        let unchanged = draw(&mut context, &mut game, &play, false, false);
        assert_eq!(unchanged.requested_extent, None);
        let fit_again = click_scaling(&mut context, &mut game, &play, unchanged.checkbox_point);
        let requested = fit_again
            .requested_extent
            .expect("Fit resumes resizing the game canvas");
        assert_eq!(
            requested,
            [
                496.5,
                (fit_again.available_panel.max[1] - fit_again.available_panel.min[1]) * 1.5
            ]
        );
        let mut another_panel = ImguiGameView::default();
        assert!(
            draw(&mut context, &mut another_panel, &play, false, false)
                .requested_extent
                .is_some()
        );
    }

    #[test]
    fn stale_and_paused_game_notices_describe_the_embedded_runtime_boundary() {
        let log = Rc::new(RefCell::new(String::new()));
        let mut context = context();
        context.set_clipboard_backend(RenderedText(Rc::clone(&log)));
        let mut game = ImguiGameView::default();
        for _ in 0..3 {
            draw(
                &mut context,
                &mut game,
                &play(EditorPlayState::Paused, true),
                false,
                true,
            );
        }
        assert!(
            log.borrow()
                .contains("Game View is out of date; save and refresh content before Restart")
        );
        log.borrow_mut().clear();
        draw(
            &mut context,
            &mut game,
            &play(EditorPlayState::Paused, false),
            false,
            true,
        );
        assert!(
            log.borrow()
                .contains("Play is paused; Scene authoring remains isolated from this Runtime")
        );
        assert!(!log.borrow().contains("out of date"));
        let paused = draw(
            &mut context,
            &mut game,
            &play(EditorPlayState::Paused, false),
            false,
            false,
        );
        for state in [
            EditorPlayState::Stepping,
            EditorPlayState::Running,
            EditorPlayState::Paused,
        ] {
            let rendered = draw(&mut context, &mut game, &play(state, false), false, false);
            assert_eq!(
                rendered.requested_extent, paused.requested_extent,
                "transient Play feedback must not resize the Runtime canvas"
            );
            assert_eq!(
                rendered.image.rect, paused.image.rect,
                "transient Play feedback must keep the same presented image mapping"
            );
        }
        let stale = draw(
            &mut context,
            &mut game,
            &play(EditorPlayState::Paused, true),
            false,
            false,
        );
        assert_eq!(stale.requested_extent, paused.requested_extent);
        assert_eq!(stale.image.rect, paused.image.rect);
    }

    #[test]
    fn capture_hint_is_an_overlay_that_preserves_the_game_image_and_resize_request() {
        let mut context = context();
        let mut game = ImguiGameView::default();
        let play = play(EditorPlayState::Running, false);
        for _ in 0..4 {
            draw(&mut context, &mut game, &play, false, false);
        }
        let before = draw(&mut context, &mut game, &play, false, false);
        let captured = draw(&mut context, &mut game, &play, true, false);
        let released = draw(&mut context, &mut game, &play, false, false);
        assert_eq!(captured.image.rect, before.image.rect);
        assert_eq!(captured.requested_extent, before.requested_extent);
        assert_eq!(released.image.rect, before.image.rect);
        assert_eq!(released.requested_extent, before.requested_extent);
        assert!(
            captured.vertices > before.vertices,
            "capture must paint its release hint"
        );
        assert_eq!(released.vertices, before.vertices);
    }
}
