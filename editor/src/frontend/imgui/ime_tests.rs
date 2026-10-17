use std::{path::PathBuf, sync::Arc};

use dear_imgui_rs::{Condition, ConfigFlags, Context};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use winit::{
    application::ApplicationHandler,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::ImguiProjectPanels;

fn draw_fields(
    context: &mut Context,
    platform: &mut WinitPlatform,
    window: &Window,
    panels: &mut ImguiProjectPanels,
    focus: Option<i32>,
) -> usize {
    platform.prepare_frame(context, window).unwrap();
    let ui = context.frame();
    let mut owner = None;
    ui.window("Nara project fields")
        .position([20.0, 20.0], Condition::Always)
        .size([600.0, 250.0], Condition::Always)
        .build(|| {
            if let Some(offset) = focus {
                ui.set_keyboard_focus_here_with_offset(offset);
            }
            panels.show_open_fields(ui, false, &mut owner);
        });
    platform.prepare_render(ui, window).unwrap();
    context.render_legacy().total_vtx_count()
}

fn prove_native_ime(event_loop: &ActiveEventLoop) {
    // A hidden, inactive owned window supplies the backend's actual native registration. Events
    // below enter that backend directly; this test does not inject input into the user's desktop.
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara IME integration proof")
                    .with_visible(false)
                    .with_active(false),
            )
            .unwrap(),
    );
    let mut context = Context::create();
    context.set_ini_filename::<PathBuf>(None).unwrap();
    context.io_mut().set_delta_time(1.0 / 60.0);
    context
        .io_mut()
        .set_config_flags(ConfigFlags::NO_MOUSE_CURSOR_CHANGE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    let mut platform = WinitPlatform::new(&mut context).unwrap();
    platform
        .attach_window(Arc::clone(&window), HiDpiMode::Default, &mut context)
        .unwrap();
    let mut panels = ImguiProjectPanels {
        project_path: "original".into(),
        ..Default::default()
    };
    draw_fields(&mut context, &mut platform, &window, &mut panels, Some(0));
    // Focus is applied on the next UI frame. Its text-input intent reaches IO at the following
    // NewFrame, where the platform's ordinary auto-management can enable the native IME.
    draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    let baseline = draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    assert!(context.io().want_text_input());
    assert!(
        platform.ime_enabled(),
        "the ordinary frame lifecycle must enable IME"
    );
    let owner = context
        .text_input_viewport()
        .expect("Nara's path field is active");

    for ime in [Ime::Enabled, Ime::Preedit("中文".into(), Some((0, 3)))] {
        let outcome = platform
            .handle_window_event(&mut context, window.id(), &WindowEvent::Ime(ime))
            .unwrap();
        assert_eq!(outcome.target.unwrap().native.viewport_id, owner);
    }
    assert_eq!(context.ime_preedit().unwrap().text(), "中文");
    let composed = draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    assert!(
        composed > baseline,
        "the core must draw composition for Nara's input"
    );
    assert_eq!(
        panels.project_path, "original",
        "preedit must not edit the path draft"
    );
    platform
        .handle_window_event(
            &mut context,
            window.id(),
            &WindowEvent::Ime(Ime::Commit("中文".into())),
        )
        .unwrap();
    for _ in 0..2 {
        draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    }
    assert_eq!(
        panels.project_path, "中文",
        "one native commit must edit the draft once"
    );

    platform
        .handle_window_event(
            &mut context,
            window.id(),
            &WindowEvent::Ime(Ime::Preedit("旧".into(), Some((3, 3)))),
        )
        .unwrap();
    assert!(context.has_ime_composition());
    draw_fields(&mut context, &mut platform, &window, &mut panels, Some(1));
    draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    assert!(!context.has_ime_composition());
    platform
        .handle_window_event(
            &mut context,
            window.id(),
            &WindowEvent::Ime(Ime::Commit("旧".into())),
        )
        .unwrap();
    draw_fields(&mut context, &mut platform, &window, &mut panels, None);
    assert_eq!(panels.project_path, "中文");
    assert!(
        panels.profile.is_empty(),
        "the old composition must not move to Profile"
    );
    platform.shutdown(&mut context).unwrap();
}

#[test]
#[ignore = "requires an owned Windows native window; no OS input is injected"]
fn native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit() {
    struct Host;
    impl ApplicationHandler for Host {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            prove_native_ime(event_loop);
            event_loop.exit();
        }

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(&mut Host).unwrap();
}
