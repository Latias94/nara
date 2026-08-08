#![cfg(feature = "accessibility")]

use std::sync::Arc;

use accesskit::{Action, ActionRequest, Role, TreeId};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrameJournal,
    UiInputEvent, UiInteraction, UiInteractionSession, UiKey, UiLayer, UiLocalKey, UiNode, UiRoot,
    UiRootKey, UiRouteStamp, UiSeatId, UiSemanticRole, UiSemantics, UiStyle, UiSurfaceId, UiTheme,
};
use nara_winit::{WinitAccessibilityBridge, WinitAccessibilityError};

fn exact_key(root: &UiRootKey, local: &UiLocalKey) -> UiKey {
    UiKey::scoped(root.clone(), local.clone()).unwrap()
}

#[test]
fn bounded_accesskit_trial_preserves_semantics_focus_geometry_and_actions() {
    let base_root = UiRootKey::runtime("accessibility-test", "base").unwrap();
    let base_local = UiLocalKey::runtime("accessibility-test", "base-action").unwrap();
    let modal_root = UiRootKey::runtime("accessibility-test", "modal").unwrap();
    let retry_local = UiLocalKey::runtime("accessibility-test", "retry").unwrap();
    let retry_action = UiActionId::new("retry").unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(base_root.clone(), UiLayer::Base, UiStyle::fill()).with_child(
                UiNode::new(UiStyle::absolute(10.0, 10.0, 80.0, 40.0))
                    .with_key(base_local.clone())
                    .with_interaction(UiInteraction::focus_target())
                    .with_semantics(
                        UiSemantics::new(UiSemanticRole::Button, "Underlay")
                            .unwrap()
                            .with_action(UiActionId::new("underlay").unwrap())
                            .unwrap(),
                    ),
            ),
        )
        .with_root(
            UiRoot::new(modal_root.clone(), UiLayer::Modal, UiStyle::fill()).with_child(
                UiNode::new(UiStyle::absolute(100.0, 60.0, 120.0, 44.0))
                    .with_key(retry_local.clone())
                    .with_interaction(UiInteraction::focus_target())
                    .with_semantics(
                        UiSemantics::new(UiSemanticRole::Button, "Retry")
                            .unwrap()
                            .with_selected(true)
                            .with_expanded(false)
                            .with_action(retry_action.clone())
                            .unwrap(),
                    ),
            ),
        )
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(41).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(surface, 320.0, 180.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let mut journal = UiFrameJournal::default();
    journal.submit(Arc::clone(&frame), 7).unwrap();
    let route = UiRouteStamp::new(surface, 7);
    let retry_key = exact_key(&modal_root, &retry_local);
    let base_key = exact_key(&base_root, &base_local);
    let mut session = UiInteractionSession::default();
    session
        .dispatch(
            &journal,
            route,
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: retry_key.clone(),
            },
        )
        .unwrap();
    let semantics = session
        .semantic_snapshot(&journal, route, UiSeatId::PRIMARY)
        .unwrap();
    let retry_semantics = semantics
        .nodes()
        .iter()
        .find(|fact| fact.key() == &retry_key)
        .unwrap();
    assert_eq!(retry_semantics.state().selected(), Some(true));
    assert_eq!(retry_semantics.state().expanded(), Some(false));

    let mut bridge = WinitAccessibilityBridge::new(surface);
    let update = bridge.update(&semantics, 2.0).unwrap();
    assert_eq!(update.surface_id(), surface);
    assert_eq!(update.presentation_epoch(), 7);
    let tree = update.accesskit_update();
    let retry_id = bridge.node_id(&retry_key).unwrap();
    let base_id = bridge.node_id(&base_key).unwrap();
    assert_eq!(tree.focus, retry_id);
    let retry_node = tree
        .nodes
        .iter()
        .find(|(node_id, _)| *node_id == retry_id)
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(retry_node.role(), Role::Button);
    assert_eq!(retry_node.label(), Some("Retry"));
    assert_eq!(retry_node.is_selected(), Some(true));
    assert_eq!(retry_node.is_expanded(), Some(false));
    assert!(retry_node.supports_action(Action::Click));
    assert!(retry_node.supports_action(Action::Focus));
    assert_eq!(
        retry_node.bounds().unwrap(),
        accesskit::Rect::new(200.0, 120.0, 440.0, 208.0)
    );
    let base_node = tree
        .nodes
        .iter()
        .find(|(node_id, _)| *node_id == base_id)
        .map(|(_, node)| node)
        .unwrap();
    assert!(base_node.is_disabled());
    assert!(!base_node.supports_action(Action::Click));

    let click = bridge
        .map_action(
            &ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            },
            UiSeatId::PRIMARY,
        )
        .unwrap();
    assert_eq!(
        click,
        UiInputEvent::SemanticAction {
            seat: UiSeatId::PRIMARY,
            target: retry_key.clone(),
            action: retry_action,
        }
    );
    assert_eq!(
        bridge
            .map_action(
                &ActionRequest {
                    action: Action::Focus,
                    target_tree: TreeId::ROOT,
                    target_node: retry_id,
                    data: None,
                },
                UiSeatId::PRIMARY,
            )
            .unwrap(),
        UiInputEvent::FocusTarget {
            seat: UiSeatId::PRIMARY,
            target: retry_key.clone(),
        }
    );
    assert_eq!(
        bridge
            .map_action(
                &ActionRequest {
                    action: Action::Increment,
                    target_tree: TreeId::ROOT,
                    target_node: retry_id,
                    data: None,
                },
                UiSeatId::PRIMARY,
            )
            .unwrap_err(),
        WinitAccessibilityError::UnsupportedAction {
            action: Action::Increment,
        }
    );

    bridge.update(&semantics, 2.0).unwrap();
    assert_eq!(bridge.node_id(&retry_key), Some(retry_id));

    let disabled_snapshot = UiAuthoringSnapshotBuilder::new(2)
        .with_root(
            UiRoot::new(modal_root, UiLayer::Modal, UiStyle::fill()).with_child(
                UiNode::new(UiStyle::absolute(100.0, 60.0, 120.0, 44.0))
                    .with_key(retry_local)
                    .with_interaction(UiInteraction::focus_target().with_enabled(false))
                    .with_semantics(
                        UiSemantics::new(UiSemanticRole::Button, "Retrying...")
                            .unwrap()
                            .with_action(UiActionId::new("retry").unwrap())
                            .unwrap(),
                    ),
            ),
        )
        .build()
        .unwrap();
    let disabled_frame = compiler
        .compile(
            &disabled_snapshot,
            UiEnvironment::new(surface, 320.0, 180.0, 2.0, 2, 2, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    journal.submit(disabled_frame, 8).unwrap();
    let disabled_route = UiRouteStamp::new(surface, 8);
    let disabled_semantics = session
        .semantic_snapshot(&journal, disabled_route, UiSeatId::PRIMARY)
        .unwrap();
    let disabled_update = bridge.update(&disabled_semantics, 2.0).unwrap();
    let disabled_tree = disabled_update.accesskit_update();
    assert_eq!(disabled_tree.focus, retry_id);
    let disabled_retry = disabled_tree
        .nodes
        .iter()
        .find(|(node_id, _)| *node_id == retry_id)
        .map(|(_, node)| node)
        .unwrap();
    assert!(disabled_retry.is_disabled());
    assert!(!disabled_retry.supports_action(Action::Click));
    assert!(!disabled_retry.supports_action(Action::Focus));
}
