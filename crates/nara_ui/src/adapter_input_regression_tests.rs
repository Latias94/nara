use super::*;
use nara_input::SemanticActions;
use nara_ui_render::{UiPacketPrimitive, UiPreparedImages, UiPresentationPlanner};

fn test_root(world: &mut World, layer: UiLayer, key: &str) -> Entity {
    world
        .spawn((
            UiRoot::new(layer),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, key).unwrap(),
        ))
        .id()
}

fn test_button(world: &mut World, name: &str, style: UiStyle) -> Entity {
    let (button, _, visual, key) = button_bundle(name, name, UiColor::rgba(0.3, 0.5, 0.8, 1.0));
    world.spawn((button, UiNode::new(style), visual, key)).id()
}

fn actions<'a>(app: &'a UiTestApp, cursor: &UiActionCursor) -> &'a [crate::UiActionEvent] {
    app.world()
        .resource::<UiActionJournal>()
        .pending(cursor)
        .unwrap()
}

fn primary_click(app: &mut UiTestApp, route: UiRouteStamp, point: Option<UiPoint>) {
    for phase in [UiButtonPhase::Pressed, UiButtonPhase::Released] {
        app.dispatch_input(
            route,
            UiInputEvent::Pointer(UiPointerInput::button(
                UiSeatId::PRIMARY,
                UiPointerId::PRIMARY,
                UiPointerButton::Primary,
                phase,
                point,
            )),
        );
    }
}

#[test]
fn modal_underlay_motion_clears_the_hover_before_position_free_clicks() {
    let mut app = ui_app();
    app.install_surface(test_slot(surface_id(), 11, 320.0, 180.0))
        .unwrap();
    let world = app.configuration_world();
    let base = test_root(world, UiLayer::Base, "base");
    let underlay = test_button(world, "underlay", UiStyle::absolute(10.0, 10.0, 80.0, 40.0));
    place_ui_tree(world, base, &[underlay]);
    let modal = test_root(world, UiLayer::Modal, "modal");
    let retry = test_button(world, "retry", UiStyle::absolute(210.0, 120.0, 80.0, 40.0));
    place_ui_tree(world, modal, &[retry]);
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let route = UiRouteStamp::from_receipt(app.present_frame(frame, 1).unwrap());
    let cursor = app
        .register_consumer(UiActionConsumerId::new("modal-underlay-motion").unwrap())
        .unwrap();
    let retry_position = UiPoint::new(220.0, 130.0);
    let underlay_position = UiPoint::new(20.0, 20.0);
    let move_pointer = |point| {
        UiInputEvent::Pointer(UiPointerInput::moved(
            UiSeatId::PRIMARY,
            UiPointerId::PRIMARY,
            point,
        ))
    };

    for click_position in [None, Some(underlay_position)] {
        assert!(matches!(
            app.dispatch_input(route, move_pointer(retry_position)),
            UiRouteDisposition::Consumed(_)
        ));
        let moved = app.dispatch_input(route, move_pointer(underlay_position));
        primary_click(&mut app, route, click_position);
        assert!(actions(&app, &cursor).is_empty(), "{click_position:?}");
        let UiRouteDisposition::Consumed(batch) = moved else {
            panic!("observed underlay motion must commit hover cleanup: {moved:?}");
        };
        let exited = expected_runtime_node("modal", "retry");
        assert!(batch.events().iter().any(|event| event.target() == &exited
            && event.kind()
                == &nara_ui_kernel::UiRoutedEventKind::HoverChanged { hovered: false }));
    }
    app.dispatch_input(route, move_pointer(retry_position));
    primary_click(&mut app, route, None);
    let [activation] = actions(&app, &cursor) else {
        panic!("Retry must remain clickable")
    };
    assert_eq!(activation.action().as_str(), "retry");
}

#[test]
fn mouse_hit_and_paint_agree_for_zero_and_one_pixel_foreground_buttons() {
    for (width, expected, rectangle_widths) in [
        (0.2, "background", &[100][..]),
        (1.2, "foreground", &[100, 1][..]),
    ] {
        let mut app = ui_app();
        install_test_surface(app.configuration_world());
        let world = app.configuration_world();
        let root = test_root(world, UiLayer::Base, "root");
        let background = test_button(
            world,
            "background",
            UiStyle::absolute(0.0, 0.0, 100.0, 50.0),
        );
        let foreground = test_button(
            world,
            "foreground",
            UiStyle::absolute(19.9, 0.0, width, 50.0).with_z_index(1),
        );
        place_ui_tree(world, root, &[background, foreground]);
        app.update().unwrap();
        let frame = ui_candidate_frame(app.world(), surface_id())
            .unwrap()
            .unwrap();
        let packet = UiPresentationPlanner::default()
            .plan(
                Arc::clone(&frame),
                nara_render::RenderTarget::Window(nara_window::WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .unwrap();
        let widths: Vec<_> = packet
            .primitives()
            .iter()
            .filter_map(|primitive| match primitive {
                UiPacketPrimitive::Rect { rect, .. } => Some(rect.width()),
                _ => None,
            })
            .collect();
        assert_eq!(widths, rectangle_widths);

        let receipt = app.present_frame(Arc::clone(&frame), 1).unwrap();
        let route = UiRouteStamp::from_receipt(receipt);
        let cursor = app
            .register_consumer(UiActionConsumerId::new("pixel-coverage-input").unwrap())
            .unwrap();
        let point = UiPoint::new(20.0, 20.0);
        assert_eq!(
            frame.hit_test(point).unwrap().key(),
            &expected_runtime_node("root", expected)
        );
        primary_click(&mut app, route, Some(point));
        let [activation] = actions(&app, &cursor) else {
            panic!("one button must activate at width {width}");
        };
        assert_eq!(activation.action().as_str(), expected);
        let one_pixel = width > 1.0;
        let foreground_key = expected_runtime_node("root", "foreground");
        assert_eq!(
            frame
                .focus_order()
                .iter()
                .any(|fact| fact.key() == &foreground_key),
            one_pixel
        );
        assert_eq!(
            frame
                .semantics()
                .iter()
                .any(|node| node.key() == &foreground_key),
            one_pixel
        );
    }
}

#[test]
fn tab_advances_once_and_wraps_with_keyboard_activation() {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let world = app.configuration_world();
    let root = test_root(world, UiLayer::Base, "menu");
    let resume = test_button(world, "Resume", UiStyle::absolute(10.0, 10.0, 80.0, 40.0));
    let settings = test_button(
        world,
        "Settings",
        UiStyle::absolute(100.0, 10.0, 80.0, 40.0),
    );
    place_ui_tree(world, root, &[resume, settings]);
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("tab-menu").unwrap())
        .unwrap();
    let window = InputWindowId::new(11);
    let seat = InputSeatId::PRIMARY;
    let tab = KeyCode::Character('\t');
    for (key, expected) in [
        (tab, "Resume"),
        (tab, "Settings"),
        (KeyCode::Enter, "Settings"),
        (tab, "Resume"),
    ] {
        for edge in [
            ButtonDriverInput::Press(key),
            ButtonDriverInput::Release(key),
        ] {
            app.timeline().admit_keyboard(window, seat, edge).unwrap();
            app.update().unwrap();
            let semantic = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
            let focused = semantic
                .nodes()
                .iter()
                .filter(|node| node.state().focused())
                .map(|node| node.name())
                .collect::<Vec<_>>();
            assert_eq!(focused, [expected]);
        }
    }
    let [activation] = actions(&app, &cursor) else {
        panic!("only Enter must activate the focused button");
    };
    assert_eq!(activation.action().as_str(), "Settings");
    assert_eq!(activation.source(), UiActivationSource::Keyboard);
}

#[test]
fn stock_dpad_navigates_each_direction_and_consumes_the_gameplay_edge() {
    const SOUTH: u16 = 0;
    const RIGHT_THUMB: u16 = 12;
    const UP: u16 = 13;
    const DOWN: u16 = 14;
    const LEFT: u16 = 15;
    const RIGHT: u16 = 16;
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let movement = ActionId::new("move-right").unwrap();
    let inspect = ActionId::new("inspect").unwrap();
    install_gameplay_capture(
        &mut app,
        &[
            ActionBinding::gamepad_button(movement.clone(), RIGHT),
            ActionBinding::gamepad_button(inspect.clone(), RIGHT_THUMB),
        ],
    );
    let world = app.configuration_world();
    let root = test_root(world, UiLayer::Base, "cross");
    let buttons = [
        ("center", 80.0, 40.0),
        ("up", 80.0, 10.0),
        ("down", 80.0, 70.0),
        ("left", 30.0, 40.0),
        ("right", 130.0, 40.0),
    ]
    .map(|(name, x, y)| test_button(world, name, UiStyle::absolute(x, y, 40.0, 24.0)));
    place_ui_tree(world, root, &buttons);
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("gamepad-cross-menu").unwrap())
        .unwrap();
    let seat = InputSeatId::PRIMARY;
    let window = InputWindowId::new(11);
    let gamepad = app.timeline().connect_gamepad(window, seat, 0).unwrap();
    let gamepad_edge = |app: &mut UiTestApp, button, phase| {
        app.timeline()
            .admit_gamepad_button(gamepad.clone(), button, phase)
            .unwrap();
        app.update().unwrap();
    };
    let press_key = |app: &mut UiTestApp, key| {
        for edge in [
            ButtonDriverInput::Press(key),
            ButtonDriverInput::Release(key),
        ] {
            app.timeline().admit_keyboard(window, seat, edge).unwrap();
            app.update().unwrap();
        }
    };
    let assert_focus = |app: &UiTestApp, expected| {
        let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
        let focused: Vec<_> = semantics
            .nodes()
            .iter()
            .filter(|node| node.state().focused())
            .map(|node| node.name())
            .collect();
        assert_eq!(focused, [expected]);
    };
    press_key(&mut app, KeyCode::Character('\t'));
    assert_focus(&app, "center");
    for (index, (button, expected, return_key)) in [
        (UP, "up", KeyCode::ArrowDown),
        (DOWN, "down", KeyCode::ArrowUp),
        (LEFT, "left", KeyCode::ArrowRight),
        (RIGHT, "right", KeyCode::ArrowLeft),
    ]
    .into_iter()
    .enumerate()
    {
        gamepad_edge(&mut app, button, ButtonTransitionPhase::Pressed);
        assert_focus(&app, expected);
        let gameplay = app.world().resource::<SemanticActions>();
        assert!(!gameplay.pressed(seat, &movement, &ActionContext::gameplay()));
        gamepad_edge(&mut app, button, ButtonTransitionPhase::Released);
        gamepad_edge(&mut app, SOUTH, ButtonTransitionPhase::Pressed);
        gamepad_edge(&mut app, SOUTH, ButtonTransitionPhase::Released);
        let journal = actions(&app, &cursor);
        assert_eq!(journal.len(), index + 1);
        assert_eq!(journal[index].action().as_str(), expected);
        assert_eq!(journal[index].source(), UiActivationSource::Gamepad);
        press_key(&mut app, return_key);
        assert_focus(&app, "center");
    }
    assert!(
        app.world()
            .resource::<CapturedGameplayPhases>()
            .0
            .is_empty()
    );
    gamepad_edge(&mut app, RIGHT_THUMB, ButtonTransitionPhase::Pressed);
    assert!(app.world().resource::<SemanticActions>().pressed(
        seat,
        &inspect,
        &ActionContext::gameplay()
    ));
    assert_focus(&app, "center");
    gamepad_edge(&mut app, RIGHT_THUMB, ButtonTransitionPhase::Released);
    assert_eq!(actions(&app, &cursor).len(), 4);
    assert_eq!(
        app.world().resource::<CapturedGameplayPhases>().0,
        [ActionPhase::Started, ActionPhase::Released]
    );
}
