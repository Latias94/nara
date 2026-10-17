use dear_imgui_rs::Context;

pub(super) fn needs_followup(context: &Context) -> bool {
    context.binding().with_bound_context(|| {
        // SAFETY: the live Context is bound on its UI thread after EndFrame. Its window and
        // viewport vectors and pointees remain owned by Dear ImGui; only scalars leave here.
        unsafe {
            let context = &*dear_imgui_rs::sys::igGetCurrentContext();
            context.InputEventsQueue.Size > 0
                // Navigation scores during this frame and applies its result at NewFrame.
                || context.NavMoveSubmitted
                || context.NavMoveForwardToNextFrame
                || context.NavInitResult.ID != 0
                || (1..context.Viewports.Size).any(|index| {
                    let viewport = &**context.Viewports.Data.add(index as usize);
                    viewport.LastFrameActive < context.FrameCount
                })
                || (0..context.Windows.Size).any(|index| {
                    let window = &**context.Windows.Data.add(index as usize);
                    // Begin consumes these sizing counters on subsequent frames. Hidden alone
                    // also describes inactive dock tabs and clipped children that stay hidden.
                    window.Active
                        && (window.AutoFitFramesX > 0
                            || window.AutoFitFramesY > 0
                            || window.HiddenFramesCannotSkipItems > 0)
                })
        }
    })
}

#[cfg(test)]
mod tests {
    use dear_imgui_rs::{Condition, ConfigFlags, Key, MouseButton, sys};

    use super::*;

    fn render_combo(context: &mut Context) -> ([f32; 2], bool) {
        let ui = context.frame();
        let mut center = [0.0; 2];
        let mut open = false;
        ui.window("Inspector")
            .position([20.0, 20.0], Condition::Always)
            .size([360.0, 300.0], Condition::Always)
            .build(|| {
                if let Some(_combo) = ui.begin_combo("Add Component", "Choose a component") {
                    open = true;
                    ui.button("nara.scene.Name");
                    ui.button("nara.scene.Visibility");
                } else {
                    let min = ui.item_rect_min();
                    let max = ui.item_rect_max();
                    center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
                }
            });
        let _ = context.render_legacy();
        (center, open)
    }

    fn combo_is_drawn(context: &Context) -> bool {
        context.binding().with_bound_context(|| {
            // SAFETY: the rendered Context owns these windows and draw lists until NewFrame.
            unsafe {
                let window = sys::igFindWindowByName(c"##Combo_00".as_ptr());
                if window.is_null() || !(*window).Active || (*window).Hidden {
                    return false;
                }
                let draw_data = &*sys::igGetDrawData();
                (0..draw_data.CmdLists.Size)
                    .any(|index| *draw_data.CmdLists.Data.add(index as usize) == (*window).DrawList)
            }
        })
    }

    fn render_buttons(context: &mut Context) -> ([u32; 2], [f32; 2]) {
        let ui = context.frame();
        let mut ids = [0; 2];
        let mut second_center = [0.0; 2];
        ui.window("Navigation")
            .position([20.0, 20.0], Condition::Always)
            .size([360.0, 300.0], Condition::Always)
            .build(|| {
                ui.button("First");
                ids[0] = ui.item_id().raw();
                ui.button("Second");
                ids[1] = ui.item_id().raw();
                let min = ui.item_rect_min();
                let max = ui.item_rect_max();
                second_center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
            });
        let _ = context.render_legacy();
        (ids, second_center)
    }

    fn navigation_id(context: &Context) -> u32 {
        context.binding().with_bound_context(|| {
            // SAFETY: only the scalar navigation identity leaves the bound live Context.
            unsafe { (*sys::igGetCurrentContext()).NavId }
        })
    }

    #[test]
    fn newly_opened_combo_repaints_until_visible_then_becomes_idle() {
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();

        let mut center = [0.0; 2];
        for _ in 0..4 {
            center = render_combo(&mut context).0;
        }
        assert!(!needs_followup(&context));
        context.io_mut().add_mouse_pos_event(center);
        render_combo(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        render_combo(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        assert!(
            render_combo(&mut context).1,
            "the click must open the combo"
        );
        assert!(context.binding().with_bound_context(|| {
            // SAFETY: only a scalar is read from the bound live Context after rendering.
            unsafe { (*sys::igGetCurrentContext()).InputEventsQueue.Size == 0 }
        }));
        assert!(
            !combo_is_drawn(&context),
            "the first popup frame measures content without drawing it"
        );

        let mut followup_frames = 0;
        while needs_followup(&context) && followup_frames < 4 {
            assert!(render_combo(&mut context).1);
            followup_frames += 1;
        }
        assert!(
            combo_is_drawn(&context),
            "on-demand repaint must finish the popup's hidden sizing frame"
        );
        assert!(followup_frames > 0 && followup_frames < 4);
        assert!(
            !needs_followup(&context),
            "a visible idle popup must not cause continuous redraw"
        );
        assert!(render_combo(&mut context).1);
        assert!(combo_is_drawn(&context));
        assert!(!needs_followup(&context));
    }

    #[test]
    fn tab_navigation_repaints_until_focus_moves_then_becomes_idle() {
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .io_mut()
            .set_config_flags(ConfigFlags::NAV_ENABLE_KEYBOARD);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut second_center = [0.0; 2];
        for _ in 0..4 {
            second_center = render_buttons(&mut context).1;
        }
        context.io_mut().add_mouse_pos_event(second_center);
        render_buttons(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        render_buttons(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let (ids, _) = render_buttons(&mut context);
        assert_eq!(navigation_id(&context), ids[1]);
        assert!(!needs_followup(&context));
        assert!(!context.io().want_text_input());

        // The first Tab after mouse focus restores the keyboard navigation cursor on the
        // current button. Complete that transition before testing a subsequent focus move.
        context.io_mut().add_key_event(Key::Tab, true);
        render_buttons(&mut context);
        render_buttons(&mut context);
        assert_eq!(navigation_id(&context), ids[1]);
        assert!(context.binding().with_bound_context(|| {
            // SAFETY: only the scalar navigation-cursor state leaves the bound live Context.
            unsafe { (*sys::igGetCurrentContext()).NavCursorVisible }
        }));
        context.io_mut().add_key_event(Key::Tab, false);
        render_buttons(&mut context);
        assert!(!needs_followup(&context));

        context.io_mut().add_key_event(Key::Tab, true);
        render_buttons(&mut context);
        assert!(context.binding().with_bound_context(|| {
            // SAFETY: the live Context owns both navigation state and its input queue.
            unsafe {
                let native = &*sys::igGetCurrentContext();
                native.NavMoveSubmitted && native.InputEventsQueue.Size == 0
            }
        }));
        assert_eq!(
            navigation_id(&context),
            ids[1],
            "Tab scores its destination before the next frame applies focus"
        );

        let mut followup_frames = 0;
        while needs_followup(&context) && followup_frames < 4 {
            render_buttons(&mut context);
            followup_frames += 1;
        }
        assert_eq!(
            navigation_id(&context),
            ids[0],
            "on-demand repaint must apply the queued Tab focus without a key release"
        );
        assert!(followup_frames > 0 && followup_frames < 4);
        assert!(!needs_followup(&context));
        context.io_mut().add_key_event(Key::Tab, false);
        render_buttons(&mut context);
        assert_eq!(navigation_id(&context), ids[0]);
        assert!(!needs_followup(&context));
    }
}
