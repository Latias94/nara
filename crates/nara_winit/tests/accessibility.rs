#![cfg(feature = "accessibility")]

use std::sync::Arc;

use accesskit::{Action, ActionRequest, Role, TreeId};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAccessibilityInput, UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment,
    UiFrameJournal, UiInputEvent, UiInteraction, UiInteractionSession, UiKey, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiRouteStamp, UiSeatId, UiSemanticRole, UiSemantics, UiSize,
    UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
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
    journal
        .install_surface(surface, frame.environment().slot_generation().get())
        .unwrap();
    let receipt = journal.submit(Arc::clone(&frame), 7).unwrap();
    let route = UiRouteStamp::from_receipt(receipt);
    let retry_key = exact_key(&modal_root, &retry_local);
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
        .semantic_snapshot_with_action_authority(
            &journal,
            route,
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    let retry_semantics = semantics
        .nodes()
        .iter()
        .find(|fact| fact.key() == &retry_key)
        .unwrap();
    assert_eq!(retry_semantics.state().selected(), Some(true));
    assert_eq!(retry_semantics.state().expanded(), Some(false));

    let mut bridge = WinitAccessibilityBridge::new(surface);
    let mut published_retry_id = None;
    let mut published_base_id = None;
    let accessibility_receipt = bridge
        .publish_update(&semantics, |update| {
            assert_eq!(update.surface_id(), surface);
            assert_eq!(update.presentation_epoch(), 7);
            assert_eq!(update.publication_generation(), 1);
            let tree = update.accesskit_update();
            published_retry_id = tree
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("Retry"))
                .map(|(node_id, _)| *node_id);
            published_base_id = tree
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("Underlay"))
                .map(|(node_id, _)| *node_id);
            let retry_id = published_retry_id.expect("Retry should have an AccessKit identity");
            let base_id = published_base_id.expect("Underlay should have an AccessKit identity");
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
            Ok(())
        })
        .unwrap();
    let retry_id = published_retry_id.expect("Retry should have an AccessKit identity");
    assert!(published_base_id.is_some());
    assert_eq!(accessibility_receipt.route(), route);
    assert_eq!(accessibility_receipt.publication_generation(), 1);

    let click = bridge
        .map_action(&ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: retry_id,
            data: None,
        })
        .unwrap();
    assert_eq!(
        click,
        UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            retry_action.clone(),
        )
    );
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap(),
        UiAccessibilityInput::focus_target(receipt, UiSeatId::PRIMARY, retry_key.clone())
    );
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Increment,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap_err(),
        WinitAccessibilityError::UnsupportedAction {
            action: Action::Increment,
        }
    );

    let repeated = bridge.publish_update(&semantics, |_| Ok(())).unwrap();
    assert_eq!(repeated.publication_generation(), 2);
    assert_eq!(bridge.node_id(&retry_key), Some(retry_id));

    let continue_action = UiActionId::new("continue").unwrap();
    let successor_snapshot = UiAuthoringSnapshotBuilder::new(2)
        .with_root(
            UiRoot::new(modal_root.clone(), UiLayer::Modal, UiStyle::fill()).with_child(
                UiNode::new(UiStyle::absolute(100.0, 60.0, 120.0, 44.0))
                    .with_key(retry_local.clone())
                    .with_interaction(UiInteraction::focus_target())
                    .with_semantics(
                        UiSemantics::new(UiSemanticRole::Button, "Continue")
                            .unwrap()
                            .with_action(continue_action.clone())
                            .unwrap(),
                    ),
            ),
        )
        .build()
        .unwrap();
    let successor_frame = compiler
        .compile(
            &successor_snapshot,
            UiEnvironment::new(surface, 320.0, 180.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let successor_receipt = journal.submit(successor_frame, 8).unwrap();
    let successor_route = UiRouteStamp::from_receipt(successor_receipt);
    let successor_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            successor_route,
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &continue_action,
        )
        .unwrap();
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            retry_action.clone(),
        )
    );
    let mut callback_called = false;
    assert_eq!(
        bridge
            .publish_update(&successor_semantics, |update| {
                callback_called = true;
                assert_eq!(update.publication_generation(), 3);
                Err(WinitAccessibilityError::PlatformPublish {
                    message: "synthetic platform failure".to_owned(),
                })
            })
            .unwrap_err(),
        WinitAccessibilityError::PlatformPublish {
            message: "synthetic platform failure".to_owned(),
        }
    );
    assert!(callback_called);
    assert_eq!(bridge.node_id(&retry_key), Some(retry_id));
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            retry_action.clone(),
        )
    );
    let successor_accessibility_receipt = bridge
        .publish_update(&successor_semantics, |update| {
            assert_eq!(update.route(), successor_route);
            assert_eq!(update.publication_generation(), 4);
            Ok(())
        })
        .unwrap();
    assert_eq!(successor_accessibility_receipt.route(), successor_route);
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            successor_receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            continue_action.clone(),
        )
    );
    let mut stale_callback_called = false;
    assert!(matches!(
        bridge.publish_update(&semantics, |_| {
            stale_callback_called = true;
            Ok(())
        }),
        Err(WinitAccessibilityError::StaleSnapshot { .. })
    ));
    assert!(!stale_callback_called);

    let foreign_surface = UiSurfaceId::new(42).unwrap();
    let foreign_frame = compiler
        .compile(
            &successor_snapshot,
            UiEnvironment::new(foreign_surface, 320.0, 180.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let mut foreign_journal = UiFrameJournal::default();
    foreign_journal.install_surface(foreign_surface, 1).unwrap();
    let foreign_receipt = foreign_journal.submit(foreign_frame, 8).unwrap();
    let foreign_semantics = UiInteractionSession::default()
        .semantic_snapshot_with_action_authority(
            &foreign_journal,
            UiRouteStamp::from_receipt(foreign_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &continue_action,
        )
        .unwrap();
    let mut foreign_callback_called = false;
    assert_eq!(
        bridge
            .publish_update(&foreign_semantics, |_| {
                foreign_callback_called = true;
                Ok(())
            })
            .unwrap_err(),
        WinitAccessibilityError::SurfaceMismatch {
            expected: surface,
            actual: foreign_surface,
        }
    );
    assert!(!foreign_callback_called);
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            successor_receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            continue_action.clone(),
        )
    );

    let conflicting_frame = compiler
        .compile(
            &successor_snapshot,
            UiEnvironment::new(surface, 320.0, 180.0, 2.0, 2, 2, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let mut conflicting_journal = UiFrameJournal::default();
    conflicting_journal.install_surface(surface, 1).unwrap();
    let conflicting_receipt = conflicting_journal.submit(conflicting_frame, 8).unwrap();
    let conflicting_semantics = session
        .semantic_snapshot_with_action_authority(
            &conflicting_journal,
            UiRouteStamp::from_receipt(conflicting_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &continue_action,
        )
        .unwrap();
    let mut conflicting_callback_called = false;
    assert!(matches!(
        bridge.publish_update(&conflicting_semantics, |_| {
            conflicting_callback_called = true;
            Ok(())
        }),
        Err(WinitAccessibilityError::ConflictingSnapshot { .. })
    ));
    assert!(!conflicting_callback_called);

    let disabled_snapshot = UiAuthoringSnapshotBuilder::new(3)
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
            UiEnvironment::new(surface, 320.0, 180.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let disabled_receipt = journal.submit(disabled_frame, 9).unwrap();
    let disabled_route = UiRouteStamp::from_receipt(disabled_receipt);
    let disabled_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            disabled_route,
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    bridge
        .publish_update(&disabled_semantics, |update| {
            assert_eq!(update.publication_generation(), 5);
            let disabled_tree = update.accesskit_update();
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
            Ok(())
        })
        .unwrap();
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap_err(),
        WinitAccessibilityError::ActionUnavailable { node: retry_id }
    );
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap_err(),
        WinitAccessibilityError::FocusUnavailable { node: retry_id }
    );

    let mut old_generation_journal = UiFrameJournal::default();
    old_generation_journal.install_surface(surface, 1).unwrap();
    let old_retirement = old_generation_journal
        .retire_surface(surface, 2)
        .unwrap()
        .unwrap();

    journal.advance_surface_slot(surface, 1, 2).unwrap();
    let slot_two_frame = compiler
        .compile(
            &disabled_snapshot,
            UiEnvironment::for_surface(
                surface,
                UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 2.0).unwrap(),
                UiSurfaceGenerations::new(2, 2, 1, 2).unwrap(),
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let slot_two_receipt = journal.submit(slot_two_frame, 10).unwrap();
    let slot_two_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            UiRouteStamp::from_receipt(slot_two_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    let slot_two_accessibility_receipt = bridge
        .publish_update(&slot_two_semantics, |update| {
            assert_eq!(update.route(), UiRouteStamp::from_receipt(slot_two_receipt));
            assert_eq!(update.publication_generation(), 6);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        slot_two_accessibility_receipt.route(),
        UiRouteStamp::from_receipt(slot_two_receipt)
    );
    assert_eq!(
        bridge.retire(old_retirement).unwrap_err(),
        WinitAccessibilityError::RetirementGenerationMismatch {
            expected: slot_two_receipt.slot_generation(),
            actual: old_retirement.slot_generation(),
        }
    );
    assert_eq!(
        bridge
            .publish_update(&slot_two_semantics, |_| Ok(()))
            .unwrap()
            .route(),
        UiRouteStamp::from_receipt(slot_two_receipt)
    );

    let retirement = journal.retire_surface(surface, 3).unwrap().unwrap();
    assert_eq!(retirement.slot_generation().get(), 2);
    bridge.retire(retirement).unwrap();
    let mut retired_callback_called = false;
    assert_eq!(
        bridge
            .publish_update(&slot_two_semantics, |_| {
                retired_callback_called = true;
                Ok(())
            })
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert!(!retired_callback_called);
    assert_eq!(
        bridge
            .publish_update(&disabled_semantics, |_| {
                retired_callback_called = true;
                Ok(())
            })
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert!(!retired_callback_called);
    assert_eq!(
        bridge
            .map_action(&ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: retry_id,
                data: None,
            })
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
}
