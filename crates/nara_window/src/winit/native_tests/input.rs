use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use nara_app::CoreStage;
use nara_ecs::{Res, ResMut, Resource};
use nara_input::{
    ActionBinding, ActionId, ActionMap, ActionMaps, ActionState, ButtonTransitionPhase,
    InputEdgeKind, InputFrame, InputPlugin, InputWindowId, KeyCode,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{VK_OEM_1, VK_OEM_102, VK_RETURN, VK_TAB},
    WindowsAndMessaging::{
        PostMessageW, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_NCACTIVATE, WM_SETFOCUS,
    },
};

use super::*;

#[derive(Default, Resource)]
struct KeyEdges(Vec<(KeyCode, ButtonTransitionPhase)>);

#[test]
#[ignore = "requires a Windows message pump with a hidden HWND; run in its own test process"]
fn native_posted_keys_and_focus_loss_publish_balanced_edges() {
    use ButtonTransitionPhase::{Pressed, Released};
    let keys = [
        (VK_TAB, 0x0f, KeyCode::Tab, "tab"),
        (VK_RETURN, 0x1c, KeyCode::Enter, "enter"),
        (VK_OEM_1, 0x27, KeyCode::Semicolon, "semicolon"),
        (VK_OEM_102, 0x56, KeyCode::IntlBackslash, "iso"),
    ];
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(ActionMaps::new(
        ActionMap::from_bindings(
            keys.map(|(_, _, key, name)| ActionBinding::key(ActionId::new(name).unwrap(), key)),
        )
        .unwrap(),
    ))
    .unwrap();
    app.init_resource::<KeyEdges>().unwrap();
    app.add_systems(
        CoreStage::Last,
        |input: Res<InputFrame>, mut keys: ResMut<KeyEdges>| {
            for edge in input.edges() {
                if let InputEdgeKind::Key { key, phase } = edge.kind {
                    assert_eq!(edge.window, InputWindowId::PRIMARY);
                    keys.0.push((key, phase));
                }
            }
        },
    )
    .unwrap();

    let mut session = WinitRunner::new(WinitControlFlow::Wait)
        .with_initial_visibility(false)
        .start_with_presentation_on(
            RuntimeInstance::start(app).unwrap(),
            Box::new(NoPresentationDriver),
            Duration::from_secs(1),
            Some(event_loop),
        );
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        poll_until(&mut session, |session| {
            session
                .app
                .as_ref()
                .unwrap()
                .platform_windows
                .contains_key(&WindowId::PRIMARY)
        });
        let window = &session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY];
        assert_eq!(window.is_visible(), Some(false));
        let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
            panic!("expected a Windows window handle");
        };
        let hwnd = handle.hwnd.get() as _;
        let post = |message, wparam, lparam| {
            assert_ne!(
                unsafe { PostMessageW(hwnd, message, wparam, lparam) },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
        };
        post(WM_NCACTIVATE, 1, -1);
        post(WM_SETFOCUS, 0, 0);
        poll_until(&mut session, |session| {
            session.app.as_ref().unwrap().input_focused
        });
        let mut expected = Vec::new();
        for (virtual_key, scan, key, action) in keys {
            post(
                WM_KEYDOWN,
                usize::from(virtual_key),
                ((scan << 16) | 1) as isize,
            );
            expected.push((key, Pressed));
            poll_until(&mut session, |session| actions(session).button(action));
            if key == KeyCode::IntlBackslash {
                post(WM_KILLFOCUS, 0, 0);
            } else {
                post(
                    WM_KEYUP,
                    usize::from(virtual_key),
                    ((scan << 16) | 0xc000_0001_u32) as i32 as isize,
                );
            }
            expected.push((key, Released));
            poll_until(&mut session, |session| !actions(session).button(action));
        }
        assert_eq!(session.runtime().world().resource::<KeyEdges>().0, expected);
    }));
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    outcome.unwrap_or_else(|payload| resume_unwind(payload));
}

fn actions(session: &WinitSession) -> &ActionState {
    session.runtime().world().resource::<ActionState>()
}

fn poll_until(session: &mut WinitSession, ready: impl Fn(&WinitSession) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready(session) {
        assert!(Instant::now() < deadline, "native input deadline elapsed");
        assert_eq!(
            session.poll(Duration::from_millis(5)),
            WinitRunProgress::Running
        );
    }
}
