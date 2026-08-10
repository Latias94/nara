#![cfg(feature = "accessibility")]

use std::sync::Arc;

use accesskit::{Action, ActionRequest, NodeId, Role, TreeId};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAccessibilityInput, UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment,
    UiFrameJournal, UiInputEvent, UiInteraction, UiInteractionSession, UiKey, UiLayer, UiLocalKey,
    UiNode, UiRevision, UiRoot, UiRootKey, UiRouteStamp, UiSeatId, UiSemanticRole, UiSemantics,
    UiSize, UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};
use nara_winit::{
    WinitAccessibilityActionRequest, WinitAccessibilityEndpointRetirement, WinitAccessibilityError,
    WinitAccessibilityPublisher, WinitAccessibilityReceipt, WinitAccessibilitySuspension,
};

fn exact_key(root: &UiRootKey, local: &UiLocalKey) -> UiKey {
    UiKey::scoped(root.clone(), local.clone()).unwrap()
}

fn platform_action(
    receipt: WinitAccessibilityReceipt,
    action: Action,
    target_node: NodeId,
) -> WinitAccessibilityActionRequest {
    receipt.bind_action(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node,
        data: None,
    })
}

#[test]
fn recording_publisher_commits_exact_tree_mapping_and_retirement() {
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
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(surface, frame.environment().slot_generation().get())
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 7)
        .unwrap();
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

    let mut publisher = WinitAccessibilityPublisher::new(surface).unwrap();
    let publisher_id = publisher.publisher_id();
    let accessibility_receipt = publisher.publish_update(&semantics).unwrap();
    let update = publisher
        .committed_update()
        .expect("a receipt requires an exact committed tree record");
    assert_eq!(update.surface_id(), surface);
    assert_eq!(update.presentation_epoch(), 7);
    assert_eq!(update.publication_generation(), 1);
    let tree = update.accesskit_update();
    let retry_id = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Retry"))
        .map(|(node_id, _)| *node_id)
        .expect("Retry should have an AccessKit identity");
    let base_id = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Underlay"))
        .map(|(node_id, _)| *node_id)
        .expect("Underlay should have an AccessKit identity");
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
    assert_eq!(accessibility_receipt.route(), route);
    assert_eq!(accessibility_receipt.publication_generation(), 1);
    assert_eq!(accessibility_receipt.publisher_id(), publisher_id);

    let click = publisher
        .map_action(&platform_action(
            accessibility_receipt,
            Action::Click,
            retry_id,
        ))
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
        publisher
            .map_action(&platform_action(
                accessibility_receipt,
                Action::Focus,
                retry_id,
            ))
            .unwrap(),
        UiAccessibilityInput::focus_target(receipt, UiSeatId::PRIMARY, retry_key.clone())
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                accessibility_receipt,
                Action::Increment,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::UnsupportedAction {
            action: Action::Increment,
        }
    );

    let repeated = publisher.publish_update(&semantics).unwrap();
    assert_eq!(repeated.publication_generation(), 2);
    assert_eq!(
        publisher
            .committed_update()
            .expect("the repeated receipt must have a committed record")
            .publication_generation(),
        repeated.publication_generation()
    );
    assert_eq!(publisher.node_id(&retry_key), Some(retry_id));
    assert_eq!(
        publisher
            .map_action(&platform_action(
                accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::ActionPublicationMismatch {
            current_generation: repeated.publication_generation(),
            candidate_generation: accessibility_receipt.publication_generation(),
        }
    );

    let mut foreign_publisher = WinitAccessibilityPublisher::new(surface).unwrap();
    let foreign_publisher_id = foreign_publisher.publisher_id();
    assert_ne!(foreign_publisher_id, publisher_id);
    let foreign_publication = foreign_publisher.publish_update(&semantics).unwrap();
    assert_eq!(
        publisher
            .map_action(&platform_action(
                foreign_publication,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::ForeignPublisher {
            expected: publisher_id,
            actual: foreign_publisher_id,
        }
    );

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
    let successor_receipt = authority.submit(&mut journal, successor_frame, 8).unwrap();
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
        publisher
            .map_action(&platform_action(repeated, Action::Click, retry_id))
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            retry_action.clone(),
        )
    );
    let successor_accessibility_receipt = publisher.publish_update(&successor_semantics).unwrap();
    assert_eq!(successor_accessibility_receipt.route(), successor_route);
    assert_eq!(
        publisher
            .committed_update()
            .expect("the successor receipt must have a committed record")
            .publication_generation(),
        3
    );
    assert_eq!(publisher.node_id(&retry_key), Some(retry_id));
    assert_eq!(
        publisher
            .map_action(&platform_action(
                successor_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap(),
        UiAccessibilityInput::semantic_action(
            successor_receipt,
            UiSeatId::PRIMARY,
            retry_key.clone(),
            continue_action.clone(),
        )
    );
    assert!(matches!(
        publisher.publish_update(&semantics),
        Err(WinitAccessibilityError::StaleSnapshot { .. })
    ));
    assert_eq!(
        publisher
            .committed_update()
            .expect("a rejected stale snapshot must preserve the prior record")
            .publication_generation(),
        3
    );

    let foreign_surface = UiSurfaceId::new(42).unwrap();
    let foreign_frame = compiler
        .compile(
            &successor_snapshot,
            UiEnvironment::new(foreign_surface, 320.0, 180.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let (mut foreign_journal, foreign_authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    foreign_journal.install_surface(foreign_surface, 1).unwrap();
    let foreign_receipt = foreign_authority
        .submit(&mut foreign_journal, foreign_frame, 8)
        .unwrap();
    let foreign_semantics = UiInteractionSession::default()
        .semantic_snapshot_with_action_authority(
            &foreign_journal,
            UiRouteStamp::from_receipt(foreign_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &continue_action,
        )
        .unwrap();
    assert_eq!(
        publisher.publish_update(&foreign_semantics).unwrap_err(),
        WinitAccessibilityError::SurfaceMismatch {
            expected: surface,
            actual: foreign_surface,
        }
    );
    assert_eq!(
        publisher
            .committed_update()
            .expect("a foreign snapshot must preserve the prior record")
            .publication_generation(),
        3
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                successor_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
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
    let (mut conflicting_journal, conflicting_authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    conflicting_journal.install_surface(surface, 1).unwrap();
    let conflicting_receipt = conflicting_authority
        .submit(&mut conflicting_journal, conflicting_frame, 8)
        .unwrap();
    let conflicting_semantics = session
        .semantic_snapshot_with_action_authority(
            &conflicting_journal,
            UiRouteStamp::from_receipt(conflicting_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &continue_action,
        )
        .unwrap();
    assert!(matches!(
        publisher.publish_update(&conflicting_semantics),
        Err(WinitAccessibilityError::ConflictingSnapshot { .. })
    ));
    assert_eq!(
        publisher
            .committed_update()
            .expect("a conflicting snapshot must preserve the prior record")
            .publication_generation(),
        3
    );

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
    let disabled_receipt = authority.submit(&mut journal, disabled_frame, 9).unwrap();
    let disabled_route = UiRouteStamp::from_receipt(disabled_receipt);
    let disabled_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            disabled_route,
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    let disabled_accessibility_receipt = publisher.publish_update(&disabled_semantics).unwrap();
    assert_eq!(disabled_accessibility_receipt.publication_generation(), 4);
    let disabled_tree = publisher
        .committed_update()
        .expect("the disabled receipt must have a committed record")
        .accesskit_update();
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
    assert_eq!(
        publisher
            .map_action(&platform_action(
                disabled_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::ActionUnavailable { node: retry_id }
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                disabled_accessibility_receipt,
                Action::Focus,
                retry_id,
            ))
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
    let slot_two_receipt = authority.submit(&mut journal, slot_two_frame, 10).unwrap();
    let slot_two_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            UiRouteStamp::from_receipt(slot_two_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    let slot_two_accessibility_receipt = publisher.publish_update(&slot_two_semantics).unwrap();
    assert_eq!(
        slot_two_accessibility_receipt.route(),
        UiRouteStamp::from_receipt(slot_two_receipt)
    );
    let slot_two_update = publisher
        .committed_update()
        .expect("the new slot receipt must have a committed record");
    assert_eq!(
        slot_two_update.route(),
        UiRouteStamp::from_receipt(slot_two_receipt)
    );
    assert_eq!(slot_two_update.publication_generation(), 5);
    assert_eq!(
        publisher
            .retire(
                old_retirement,
                WinitAccessibilityEndpointRetirement::PlatformTreeRemoved,
            )
            .unwrap_err(),
        WinitAccessibilityError::RetirementGenerationMismatch {
            expected: slot_two_receipt.slot_generation(),
            actual: old_retirement.slot_generation(),
        }
    );
    assert_eq!(
        publisher
            .suspend(WinitAccessibilitySuspension::new(
                surface,
                UiRevision::new(3).unwrap(),
            ))
            .unwrap()
            .publication_generation(),
        6
    );
    let suspension_receipt = publisher.suspension().unwrap();
    assert_eq!(suspension_receipt.publisher_id(), publisher_id);
    assert_eq!(suspension_receipt.surface_id(), surface);
    assert_eq!(suspension_receipt.slot_generation().get(), 3);
    assert!(publisher.committed_update().is_none());
    assert_eq!(
        publisher
            .suspend(WinitAccessibilitySuspension::new(
                surface,
                UiRevision::new(3).unwrap(),
            ))
            .unwrap(),
        suspension_receipt
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                slot_two_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::SurfaceSuspended {
            surface,
            slot_generation: UiRevision::new(3).unwrap(),
        }
    );
    assert_eq!(
        publisher.publish_update(&slot_two_semantics).unwrap_err(),
        WinitAccessibilityError::SnapshotDoesNotSucceedSuspension {
            suspended: UiRevision::new(3).unwrap(),
            candidate: UiRouteStamp::from_receipt(slot_two_receipt),
        }
    );

    journal.advance_surface_slot(surface, 2, 3).unwrap();
    let suspended_generation_frame = compiler
        .compile(
            &disabled_snapshot,
            UiEnvironment::for_surface(
                surface,
                UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 2.0).unwrap(),
                UiSurfaceGenerations::new(3, 3, 1, 3).unwrap(),
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let suspended_generation_receipt = authority
        .submit(&mut journal, suspended_generation_frame, 11)
        .unwrap();
    let suspended_generation_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            UiRouteStamp::from_receipt(suspended_generation_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    assert_eq!(
        publisher
            .publish_update(&suspended_generation_semantics)
            .unwrap_err(),
        WinitAccessibilityError::SnapshotDoesNotSucceedSuspension {
            suspended: UiRevision::new(3).unwrap(),
            candidate: UiRouteStamp::from_receipt(suspended_generation_receipt),
        }
    );

    journal.advance_surface_slot(surface, 3, 4).unwrap();
    let restored_frame = compiler
        .compile(
            &disabled_snapshot,
            UiEnvironment::for_surface(
                surface,
                UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 2.0).unwrap(),
                UiSurfaceGenerations::new(4, 4, 1, 4).unwrap(),
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let restored_receipt = authority.submit(&mut journal, restored_frame, 12).unwrap();
    let restored_semantics = session
        .semantic_snapshot_with_action_authority(
            &journal,
            UiRouteStamp::from_receipt(restored_receipt),
            UiSeatId::PRIMARY,
            |key, action| key == &retry_key && action == &retry_action,
        )
        .unwrap();
    let restored_accessibility_receipt = publisher.publish_update(&restored_semantics).unwrap();
    assert_eq!(restored_accessibility_receipt.publication_generation(), 7);
    assert!(publisher.suspension().is_none());
    assert_eq!(
        publisher
            .map_action(&platform_action(
                slot_two_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::ActionPublicationMismatch {
            current_generation: restored_accessibility_receipt.publication_generation(),
            candidate_generation: slot_two_accessibility_receipt.publication_generation(),
        }
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                restored_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::ActionUnavailable { node: retry_id }
    );

    let mut successor_suspension_publisher = WinitAccessibilityPublisher::new(surface).unwrap();
    successor_suspension_publisher
        .publish_update(&slot_two_semantics)
        .unwrap();
    successor_suspension_publisher
        .suspend(WinitAccessibilitySuspension::new(
            surface,
            UiRevision::new(3).unwrap(),
        ))
        .unwrap();
    let successor_suspension = successor_suspension_publisher
        .suspend(WinitAccessibilitySuspension::new(
            surface,
            UiRevision::new(5).unwrap(),
        ))
        .unwrap();
    assert_eq!(successor_suspension.slot_generation().get(), 5);
    assert_eq!(successor_suspension.publication_generation(), 3);
    assert_eq!(
        successor_suspension_publisher
            .suspend(WinitAccessibilitySuspension::new(
                surface,
                UiRevision::new(4).unwrap(),
            ))
            .unwrap_err(),
        WinitAccessibilityError::StaleSuspension {
            current: UiRevision::new(5).unwrap(),
            candidate: UiRevision::new(4).unwrap(),
        }
    );
    assert_eq!(
        successor_suspension_publisher.suspension(),
        Some(successor_suspension)
    );

    let mut unpresented_restore_publisher = WinitAccessibilityPublisher::new(surface).unwrap();
    unpresented_restore_publisher
        .publish_update(&slot_two_semantics)
        .unwrap();
    unpresented_restore_publisher
        .suspend(WinitAccessibilitySuspension::new(
            surface,
            UiRevision::new(3).unwrap(),
        ))
        .unwrap();

    let retirement = journal.retire_surface(surface, 5).unwrap().unwrap();
    assert_eq!(retirement.slot_generation().get(), 4);
    assert_eq!(
        unpresented_restore_publisher
            .retire(
                old_retirement,
                WinitAccessibilityEndpointRetirement::NativeWindowDestroyed,
            )
            .unwrap_err(),
        WinitAccessibilityError::RetirementPredatesSuspension {
            suspended: UiRevision::new(3).unwrap(),
            candidate: UiRevision::new(1).unwrap(),
        }
    );
    let unpresented_retirement = unpresented_restore_publisher
        .retire(
            retirement,
            WinitAccessibilityEndpointRetirement::NativeWindowDestroyed,
        )
        .unwrap();
    assert_eq!(unpresented_retirement.token(), retirement);
    let retirement_receipt = publisher
        .retire(
            retirement,
            WinitAccessibilityEndpointRetirement::NativeWindowDestroyed,
        )
        .unwrap();
    assert_eq!(retirement_receipt.token(), retirement);
    assert_eq!(
        retirement_receipt.endpoint(),
        WinitAccessibilityEndpointRetirement::NativeWindowDestroyed
    );
    assert_eq!(retirement_receipt.publisher_id(), publisher_id);
    assert_eq!(retirement_receipt.publication_generation(), 8);
    assert!(publisher.committed_update().is_none());
    assert_eq!(publisher.retirement(), Some(retirement_receipt));
    assert_eq!(
        publisher
            .retire(
                retirement,
                WinitAccessibilityEndpointRetirement::NativeWindowDestroyed,
            )
            .unwrap(),
        retirement_receipt
    );
    assert_eq!(
        publisher
            .retire(
                retirement,
                WinitAccessibilityEndpointRetirement::PlatformTreeRemoved,
            )
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert_eq!(
        publisher.publish_update(&restored_semantics).unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert_eq!(
        publisher.publish_update(&disabled_semantics).unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
    assert_eq!(
        publisher
            .map_action(&platform_action(
                restored_accessibility_receipt,
                Action::Click,
                retry_id,
            ))
            .unwrap_err(),
        WinitAccessibilityError::SurfaceRetired { surface }
    );
}
