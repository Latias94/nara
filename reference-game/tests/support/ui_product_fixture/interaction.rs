use std::time::Duration;

use accesskit::{Action, ActionRequest, TreeId};
use nara::{
    app::RuntimeInstance,
    input::{
        ButtonDriverInput, ButtonTransitionPhase, InputPointerId, InputSeatId, InputWindowEvent,
        InputWindowId, KeyCode, MouseButton, PointerDriverInput, apply_gamepad_driver_input_at,
        apply_keyboard_driver_input_at, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
        apply_window_input_driver_event,
    },
    prelude::{FixedTime, Vec2},
    ui::{
        UiAccessibilityInput, UiSeatId,
        advanced::{active_ui_surface_slots, ui_semantic_snapshot},
    },
};
use nara_reference_game::WaveSnapshot;
use nara_ui_kernel::{UiPoint, UiRouteStamp, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform};
use nara_winit::WinitAccessibilityPublisher;

use super::{
    REFERENCE_RETRY_ACTION,
    presenter::{
        ComponentPresentationState, queue_component_accessibility_input,
        queue_component_surface_update,
    },
    reference_surface_id,
};

pub(super) struct WinitAccessibilityMapping {
    pub(super) input: UiAccessibilityInput,
    pub(super) source_route: UiRouteStamp,
    pub(super) published_route: UiRouteStamp,
    pub(super) mapped_route: UiRouteStamp,
    pub(super) retry_was_published_focused: bool,
}

pub(super) fn drive_to_terminal(runtime: &mut RuntimeInstance) {
    for _ in 0..96 {
        drive_fixed(runtime);
        if runtime.world().resource::<WaveSnapshot>().is_terminal() {
            return;
        }
    }
    panic!("the reference UI product runtime did not reach a terminal state");
}

pub(super) fn drive_fixed(runtime: &mut RuntimeInstance) {
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();
}

pub(super) fn focus_retry(runtime: &mut RuntimeInstance) {
    inject_keyboard(runtime, KeyCode::Character('\t'));
    runtime.drive(Duration::ZERO).unwrap();
    let receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the component presenter should retain one exact submission");
    let semantics = ui_semantic_snapshot(runtime.world(), receipt, UiSeatId::PRIMARY).unwrap();
    assert!(semantics.nodes().iter().any(|node| {
        node.actions()
            .iter()
            .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
            && node.state().focused()
    }));
}

pub(super) fn clear_focus(runtime: &mut RuntimeInstance) {
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
}

pub(super) fn activate_gamepad(runtime: &mut RuntimeInstance) {
    runtime
        .with_driver_scope(|scope| {
            apply_gamepad_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                0,
                0,
                ButtonTransitionPhase::Pressed,
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn activate_component_accessibility(runtime: &mut RuntimeInstance) {
    let receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the component presenter should retain one exact submission");
    let semantics = ui_semantic_snapshot(runtime.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| {
            node.actions()
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        })
        .expect("terminal UI should expose Retry semantics");
    let action = retry
        .actions()
        .iter()
        .find(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        .cloned()
        .expect("terminal UI should expose the exact Retry action");
    let input = UiAccessibilityInput::semantic_action(
        receipt,
        UiSeatId::PRIMARY,
        retry.key().clone(),
        action,
    );
    runtime
        .with_driver_scope(|scope| queue_component_accessibility_input(scope, input))
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn map_winit_retry_accessibility_action(
    runtime: &RuntimeInstance,
) -> WinitAccessibilityMapping {
    let receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the component presenter should retain one exact submission");
    let semantics = ui_semantic_snapshot(runtime.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| {
            node.actions()
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        })
        .expect("terminal UI should expose Retry semantics");
    let retry_action = retry
        .actions()
        .iter()
        .find(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        .cloned()
        .expect("terminal UI should expose the exact Retry action");
    let expected = UiAccessibilityInput::semantic_action(
        receipt,
        UiSeatId::PRIMARY,
        retry.key().clone(),
        retry_action,
    );

    let mut publisher = WinitAccessibilityPublisher::new(reference_surface_id()).unwrap();
    let accessibility_receipt = publisher.publish_update(&semantics).unwrap();
    let update = publisher
        .committed_update()
        .expect("the first-party publisher should retain the exact committed tree");
    let retry_node = publisher
        .node_id(retry.key())
        .expect("the platform publisher should retain the exact Retry key mapping");
    let platform_retry = update
        .accesskit_update()
        .nodes
        .iter()
        .find(|(node, _)| *node == retry_node)
        .map(|(_, node)| node)
        .expect("the platform tree should retain the Retry semantic node");
    assert_eq!(platform_retry.label(), Some(retry.name()));
    assert!(platform_retry.supports_action(Action::Click));
    let retry_was_published_focused = update.accesskit_update().focus == retry_node;
    let input = publisher
        .map_action(&accessibility_receipt.bind_action(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: retry_node,
            data: None,
        }))
        .expect("the first-party publisher should map the exact AccessKit Retry action");
    assert_eq!(input, expected);

    WinitAccessibilityMapping {
        source_route: UiRouteStamp::from_receipt(receipt),
        published_route: accessibility_receipt.route(),
        mapped_route: input.route(),
        retry_was_published_focused,
        input,
    }
}

pub(super) fn inject_keyboard(runtime: &mut RuntimeInstance, key: KeyCode) {
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(key),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn release_keyboard(runtime: &mut RuntimeInstance, key: KeyCode) {
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                ButtonDriverInput::Release(key),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn inject_pointer_click(runtime: &mut RuntimeInstance) {
    let point = retry_platform_center(runtime);
    pointer_move(runtime, point);
    mouse(runtime, ButtonDriverInput::Press(MouseButton::Left));
    mouse(runtime, ButtonDriverInput::Release(MouseButton::Left));
}

pub(super) fn retry_platform_center(runtime: &RuntimeInstance) -> Vec2 {
    let receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the component presenter should retain one exact submission");
    let semantics = ui_semantic_snapshot(runtime.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| {
            node.actions()
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        })
        .expect("terminal UI should expose Retry semantics");
    let slot = active_ui_surface_slots(runtime.world())
        .unwrap()
        .into_iter()
        .find(|slot| slot.surface_id() == reference_surface_id())
        .unwrap();
    let rect = retry.rect();
    let authored = UiPoint::new(
        rect.origin().x() + rect.size().width() * 0.5,
        rect.origin().y() + rect.size().height() * 0.5,
    );
    let physical = slot
        .transform()
        .authored_to_physical(authored)
        .expect("the Retry center should remain inside the authored content extent");
    let platform_scale = slot.transform().platform_scale_factor();
    Vec2::new(physical.x() / platform_scale, physical.y() / platform_scale)
}

pub(super) fn pointer_move(runtime: &mut RuntimeInstance, point: Vec2) {
    runtime
        .with_driver_scope(|scope| {
            apply_pointer_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(point),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn pointer_left(runtime: &mut RuntimeInstance) {
    runtime
        .with_driver_scope(|scope| {
            apply_pointer_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Left,
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn resize(runtime: &mut RuntimeInstance, width: u32, height: u32) {
    let logical_size = Vec2::new(width as f32, height as f32);
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::Resized { logical_size },
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(960.0, 540.0).expect("the reference authored extent is valid"),
        UiSize::new(width as f32, height as f32).expect("the resized target is valid"),
        UiSafeAreaInsets::default(),
        1.0,
    )
    .expect("the resized target should produce one exact content transform");
    runtime
        .with_driver_scope(|scope| queue_component_surface_update(scope, transform))
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

pub(super) fn mouse(runtime: &mut RuntimeInstance, input: ButtonDriverInput<MouseButton>) {
    runtime
        .with_driver_scope(|scope| {
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                input,
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}
