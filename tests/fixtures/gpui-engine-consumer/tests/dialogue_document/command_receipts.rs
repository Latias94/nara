use nara_engine::{fs::ContentDigest, tooling::EditorDocumentDigest};

use super::*;

#[test]
fn exact_command_views_survive_later_operations_reopen_and_session_retirement() -> Result<()> {
    let project = create_project()?;
    let (mut session, document) = open(&project)?;
    let initial = facts(&session.0, document);
    let initial_view = view(&session.0, document);
    let (after_first, first_result) = edit_with_outcome(
        &mut session.0,
        document,
        DialogueCommand::Add {
            recipe: NodeRecipe::Line,
            position: [24.0, 48.0],
        },
    );
    let first_receipt = first_result.get::<DialogueGraph>().unwrap();
    let DialogueCommandOutcome::Created {
        node,
        outputs,
        link,
    } = first_receipt.outcome()
    else {
        panic!("expected the original allocation effect");
    };
    let node = *node;
    assert_eq!(*outputs, [OutputPort::Next { node }]);
    assert!(link.is_none());
    let observed_first = view(&session.0, document);
    assert!(Arc::ptr_eq(first_receipt.view(), &observed_first));
    assert!(!Arc::ptr_eq(first_receipt.view(), &initial_view));
    assert!(initial_view.node(node).is_none());
    assert_eq!(first_receipt.view().entry(), Some(node));
    assert_eq!(first_receipt.view().nodes().len(), 1);
    assert_eq!(
        first_receipt.view().node(node).unwrap().body.as_ref(),
        &NodeBody::Line {
            speaker: "Guide".to_owned(),
            text: "New line".to_owned(),
        }
    );
    let first_content = encoded_content(first_receipt.view());
    let (after_second, second_result) = edit_with_outcome(
        &mut session.0,
        document,
        DialogueCommand::SetLine {
            node,
            speaker: "Narrator".to_owned(),
            text: "The second operation has its own immutable source.".to_owned(),
        },
    );
    let second_receipt = second_result.get::<DialogueGraph>().unwrap();
    assert_eq!(second_receipt.outcome(), &DialogueCommandOutcome::Changed);
    let observed_second = view(&session.0, document);
    assert!(Arc::ptr_eq(second_receipt.view(), &observed_second));
    assert!(!Arc::ptr_eq(first_receipt.view(), second_receipt.view()));
    assert_eq!(
        second_receipt.view().node(node).unwrap().body.as_ref(),
        &NodeBody::Line {
            speaker: "Narrator".to_owned(),
            text: "The second operation has its own immutable source.".to_owned(),
        }
    );
    assert_eq!(encoded_content(first_receipt.view()), first_content);
    let second_content = encoded_content(second_receipt.view());
    assert_ne!(first_content, second_content);
    assert_eq!(after_first.revision, initial.revision.next());
    assert_eq!(after_second.revision, after_first.revision.next());
    assert_eq!(after_first.history.undo_depth, 1);
    assert_eq!(after_second.history.undo_depth, 2);
    let (after_noop, noop_result) = edit_with_outcome(
        &mut session.0,
        document,
        DialogueCommand::SetLine {
            node,
            speaker: "Narrator".to_owned(),
            text: "The second operation has its own immutable source.".to_owned(),
        },
    );
    let noop_receipt = noop_result.get::<DialogueGraph>().unwrap();
    assert_eq!(noop_receipt.outcome(), &DialogueCommandOutcome::Noop);
    assert!(Arc::ptr_eq(noop_receipt.view(), second_receipt.view()));
    assert!(Arc::ptr_eq(
        noop_receipt.view(),
        &view(&session.0, document)
    ));
    assert_eq!(after_noop.revision, after_second.revision.next());
    assert_eq!(after_noop.history, after_second.history);
    assert_eq!(after_noop.dirty, after_second.dirty);
    let saved = save(&mut session.0, &project, document)?;
    let before_reopen = facts(&session.0, document);
    let reopened = persist(
        &mut session.0,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: before_reopen.revision,
            },
        },
    );
    let current = facts(&session.0, document);
    let digest = ContentDigest::of_bytes(&saved);
    assert_eq!(
        reopened,
        EditorPersistenceResult::Opened {
            document,
            revision: current.revision,
            digest: EditorDocumentDigest::new(digest.length(), *digest.as_bytes()),
        }
    );
    assert_ne!(
        current.revision.source_id(),
        before_reopen.revision.source_id()
    );
    assert_eq!(current.history, Default::default());
    assert!(!current.dirty);
    let reopened_view = view(&session.0, document);
    assert_eq!(encoded_content(&reopened_view), second_content);
    assert!(!Arc::ptr_eq(second_receipt.view(), &reopened_view));
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved);
    acknowledge(&mut session.0);
    for result in [&first_result, &second_result, &noop_result] {
        assert_eq!(
            dispatch(
                &mut session.0,
                [EditorAction::Extension(result.command().clone())]
            ),
            [EditorActionOutcome::Rejected(
                EditorActionRejection::StaleDocumentFact {
                    document,
                    expected: result.command().base_revision(),
                    actual: Some(current.revision),
                }
            )]
        );
        assert_eq!(facts(&session.0, document), current);
        assert!(Arc::ptr_eq(&view(&session.0, document), &reopened_view));
    }
    assert_eq!(encoded_content(first_receipt.view()), first_content);
    assert!(Arc::ptr_eq(first_receipt.view(), &observed_first));
    assert!(Arc::ptr_eq(second_receipt.view(), &observed_second));
    session.close()?;
    drop(session);
    assert_eq!(first_result.revision(), after_first.revision);
    assert_eq!(second_result.revision(), after_second.revision);
    assert_eq!(noop_result.revision(), after_noop.revision);
    assert_eq!(
        first_receipt.outcome(),
        &DialogueCommandOutcome::Created {
            node,
            outputs: vec![OutputPort::Next { node }],
            link: None,
        }
    );
    assert_eq!(second_receipt.outcome(), &DialogueCommandOutcome::Changed);
    assert_eq!(noop_receipt.outcome(), &DialogueCommandOutcome::Noop);
    assert_eq!(encoded_content(first_receipt.view()), first_content);
    assert_eq!(encoded_content(second_receipt.view()), second_content);
    assert_eq!(encoded_content(noop_receipt.view()), second_content);
    assert!(Arc::ptr_eq(first_receipt.view(), &observed_first));
    assert!(Arc::ptr_eq(second_receipt.view(), &observed_second));
    assert!(Arc::ptr_eq(noop_receipt.view(), second_receipt.view()));
    Ok(())
}
