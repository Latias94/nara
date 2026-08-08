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
    let prepared = bridge.prepare_update(&semantics).unwrap();
    let update = prepared.update();
    assert_eq!(update.surface_id(), surface);
    assert_eq!(update.presentation_epoch(), 7);
    let tree = update.accesskit_update();
    let retry_id = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Retry"))
        .map(|(node_id, _)| *node_id)
        .unwrap();
    let base_id = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Underlay"))
        .map(|(node_id, _)| *node_id)
        .unwrap();
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
    bridge.commit_update(prepared).unwrap();

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

    let repeated = bridge.prepare_update(&semantics).unwrap();
    bridge.commit_update(repeated).unwrap();
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
    let successor_prepared = bridge.prepare_update(&successor_semantics).unwrap();
    let obsolete_successor = bridge.prepare_update(&successor_semantics).unwrap();
    assert_eq!(successor_prepared.route(), successor_route);
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
    bridge.commit_update(successor_prepared).unwrap();
    assert_eq!(
        bridge.commit_update(obsolete_successor).unwrap_err(),
        WinitAccessibilityError::PreparedUpdateObsolete
    );
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
    assert!(matches!(
        bridge.prepare_update(&semantics),
        Err(WinitAccessibilityError::StaleSnapshot { .. })
    ));

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
    let foreign_prepared = WinitAccessibilityBridge::new(foreign_surface)
        .prepare_update(&foreign_semantics)
        .unwrap();
    assert_eq!(
        bridge.commit_update(foreign_prepared).unwrap_err(),
        WinitAccessibilityError::SurfaceMismatch {
            expected: surface,
            actual: foreign_surface,
        }
    );
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
    assert!(matches!(
        bridge.prepare_update(&conflicting_semantics),
        Err(WinitAccessibilityError::ConflictingSnapshot { .. })
    ));

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
    let disabled_prepared = bridge.prepare_update(&disabled_semantics).unwrap();
    let disabled_tree = disabled_prepared.accesskit_update();
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
    bridge.commit_update(disabled_prepared).unwrap();
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
    let slot_two_prepared = bridge.prepare_update(&slot_two_semantics).unwrap();
    bridge.commit_update(slot_two_prepared).unwrap();
    assert_eq!(
        bridge.retire(old_retirement).unwrap_err(),
        WinitAccessibilityError::RetirementGenerationMismatch {
            expected: slot_two_receipt.slot_generation(),
            actual: old_retirement.slot_generation(),
        }
    );
    assert_eq!(
        bridge.prepare_update(&slot_two_semantics).unwrap().route(),
        UiRouteStamp::from_receipt(slot_two_receipt)
    );

    let prepared_before_retirement = bridge.prepare_update(&slot_two_semantics).unwrap();
    let retirement = journal.retire_surface(surface, 3).unwrap().unwrap();
    assert_eq!(retirement.slot_generation().get(), 2);
    bridge.retire(retirement).unwrap();
    assert_eq!(
        bridge
            .commit_update(prepared_before_retirement)
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert_eq!(
        bridge.prepare_update(&disabled_semantics).unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
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
