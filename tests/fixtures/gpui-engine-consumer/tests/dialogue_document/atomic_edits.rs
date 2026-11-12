use nara_engine::{fs::ContentDigest, tooling::EditorDocumentDigest};
use nara_gpui_engine_consumer::dialogue::{Link, WireEndpoint};

use super::*;

fn exact_result(
    outcome: &EditorActionOutcome,
    command: &EditorExtensionCommand,
) -> EditorExtensionCommandOutcome {
    let EditorActionOutcome::Workspace(report) = outcome else {
        panic!("expected the original workspace result: {outcome:?}");
    };
    assert!(report.applied);
    assert_eq!(report.document, Some(command.document()));
    assert_eq!(report.revision, Some(command.base_revision().next()));
    let result = report.extension_outcome.as_ref().unwrap();
    assert_eq!(result.command(), command);
    assert_eq!(result.revision(), command.base_revision().next());
    assert!(result.get::<DialogueGraph>().is_some());
    result.clone()
}

fn created(result: &EditorExtensionCommandOutcome) -> (NodeId, Vec<OutputPort>, Option<Link>) {
    match result.get::<DialogueGraph>().unwrap().outcome() {
        DialogueCommandOutcome::Created {
            node,
            outputs,
            link,
        } => (*node, outputs.clone(), *link),
        other => panic!("expected allocator output, received {other:?}"),
    }
}

#[test]
fn ordered_results_retain_original_allocations_across_stale_and_noop_actions() -> Result<()> {
    let project = create_project()?;
    let (mut session, document) = open(&project)?;
    let editor = &mut session.0;
    let initial = facts(editor, document);
    let first = EditorExtensionCommand::new::<DialogueGraph>(
        document,
        initial.revision,
        DialogueCommand::Add {
            recipe: NodeRecipe::Line,
            position: [20.0, 40.0],
        },
    );
    let stale = EditorExtensionCommand::new::<DialogueGraph>(
        document,
        initial.revision,
        DialogueCommand::Add {
            recipe: NodeRecipe::End,
            position: [400.0, 40.0],
        },
    );
    let outcomes = dispatch(
        editor,
        [
            EditorAction::Extension(first.clone()),
            EditorAction::Extension(stale.clone()),
        ],
    );
    assert_eq!(outcomes.len(), 2);
    let first_result = exact_result(&outcomes[0], &first);
    let (line, line_outputs, link) = created(&first_result);
    assert_eq!(line_outputs, [OutputPort::Next { node: line }]);
    assert_eq!(link, None);
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
            document,
            expected: initial.revision,
            actual: Some(initial.revision.next()),
        })
    );
    assert_ne!(first_result.command(), &stale);
    assert_eq!(view(editor, document).nodes().count(), 1);
    let after_first = facts(editor, document);
    assert_eq!(after_first.history.undo_depth, 1);
    let (after_second, second_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::Add {
            recipe: NodeRecipe::Choice,
            position: [240.0, 40.0],
        },
    );
    let (choice, choice_outputs, link) = created(&second_result);
    assert_eq!(choice.0, line.0 + 1);
    assert_eq!(link, None);
    assert_eq!(
        choice_outputs,
        view(editor, document)
            .node(choice)
            .unwrap()
            .output_ports()
            .collect::<Vec<_>>()
    );
    assert_eq!(after_second.history.undo_depth, 2);
    let before_noop = view(editor, document);
    let (after_noop, noop_result) =
        edit_with_outcome(editor, document, DialogueCommand::SetEntry(Some(line)));
    assert_eq!(
        noop_result
            .get::<DialogueGraph>()
            .map(|receipt| receipt.outcome()),
        Some(&DialogueCommandOutcome::Noop)
    );
    assert!(Arc::ptr_eq(&before_noop, &view(editor, document)));
    assert_eq!(after_noop.history, after_second.history);
    assert_eq!(after_noop.dirty, after_second.dirty);
    assert_eq!(first_result.revision(), initial.revision.next());
    assert_eq!(created(&first_result), (line, line_outputs.clone(), None));
    assert_eq!(first_result.command(), &first);
    assert_ne!(first_result, second_result);
    assert_ne!(first_result, noop_result);
    assert_eq!(facts(editor, document).revision, after_noop.revision);
    session.close()?;
    assert_eq!(created(&first_result), (line, line_outputs, None));
    assert_eq!(first_result.command(), &first);
    assert_eq!(first_result.revision(), initial.revision.next());
    assert_eq!(created(&second_result).0, choice);
    Ok(())
}

#[test]
fn atomic_canvas_edits_share_original_history_file_and_reopen_admission() -> Result<()> {
    let project = create_project()?;
    let (mut session, document) = open(&project)?;
    let editor = &mut session.0;
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let ending = add(editor, document, NodeRecipe::End, [600.0, 24.0]);
    let saved = save(editor, &project, document)?;
    let baseline = view(editor, document);
    let baseline_content = encoded_content(&baseline);
    let before_creation = facts(editor, document);
    let line_output = OutputPort::Next { node: line };
    let (after_creation, choice_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::CreateFromWire {
            recipe: NodeRecipe::Choice,
            position: [240.0, 24.0],
            endpoint: WireEndpoint::Output(line_output),
            reconnect: None,
        },
    );
    let (choice_node, choice_outputs, link) = created(&choice_result);
    assert_eq!(
        link,
        Some(Link {
            from: line_output,
            to: choice_node,
        })
    );
    assert_eq!(choice_outputs.len(), 1);
    let first_choice = match choice_outputs[0] {
        OutputPort::Choice { node, choice } if node == choice_node => choice,
        other => panic!("expected the allocated primary choice output: {other:?}"),
    };
    assert_eq!(
        after_creation.history.undo_depth,
        before_creation.history.undo_depth + 1
    );
    let created_source = view(editor, document);
    let created_content = encoded_content(&created_source);
    history(editor, document, false);
    assert_eq!(encoded_content(&view(editor, document)), baseline_content);
    assert!(!facts(editor, document).dirty);
    history(editor, document, true);
    let redone = view(editor, document);
    assert_eq!(encoded_content(&redone), created_content);
    assert_ne!(
        redone.node_incarnation(choice_node),
        created_source.node_incarnation(choice_node)
    );
    let before_choices = facts(editor, document);
    let (after_choices, choices_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice_node,
            prompt: "Choose the next scene".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(first_choice),
                    label: "Keep the original".to_owned(),
                    target: Some(ending),
                },
                ChoiceDraft {
                    id: None,
                    label: "Return to the line".to_owned(),
                    target: None,
                },
            ],
        },
    );
    let DialogueCommandOutcome::ChoicesChanged {
        node,
        choices: allocated_choices,
    } = choices_result.get::<DialogueGraph>().unwrap().outcome()
    else {
        panic!("expected the domain's complete choice identity result");
    };
    assert_eq!(*node, choice_node);
    assert_eq!(allocated_choices.len(), 2);
    assert_eq!(allocated_choices[0], first_choice);
    assert_eq!(
        choices(&view(editor, document), choice_node)
            .iter()
            .map(|choice| choice.id)
            .collect::<Vec<_>>(),
        *allocated_choices
    );
    assert_eq!(
        after_choices.history.undo_depth,
        before_choices.history.undo_depth + 1
    );
    let from = OutputPort::Choice {
        node: choice_node,
        choice: first_choice,
    };
    let to = OutputPort::Choice {
        node: choice_node,
        choice: allocated_choices[1],
    };
    let before_reconnect = view(editor, document);
    let before_reconnect_content = encoded_content(&before_reconnect);
    let before_reconnect_facts = facts(editor, document);
    let (after_reconnect, reconnect_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::Reconnect {
            from,
            to,
            target: line,
        },
    );
    assert_eq!(
        reconnect_result
            .get::<DialogueGraph>()
            .map(|receipt| receipt.outcome()),
        Some(&DialogueCommandOutcome::Changed)
    );
    assert_eq!(
        after_reconnect.history.undo_depth,
        before_reconnect_facts.history.undo_depth + 1
    );
    let reconnected = view(editor, document);
    assert_eq!(reconnected.target(from), None);
    assert_eq!(reconnected.target(to), Some(line));
    assert_eq!(
        reconnected.output_incarnation(from),
        before_reconnect.output_incarnation(from)
    );
    assert_eq!(reconnected.link_incarnation(from), None);
    assert!(reconnected.link_incarnation(to).is_some());
    let reconnected_content = encoded_content(&reconnected);
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        before_reconnect_content
    );
    history(editor, document, true);
    assert_eq!(
        encoded_content(&view(editor, document)),
        reconnected_content
    );
    let before_input_creation = facts(editor, document);
    let (after_input_creation, input_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::CreateFromWire {
            recipe: NodeRecipe::Line,
            position: [440.0, 180.0],
            endpoint: WireEndpoint::Input(ending),
            reconnect: None,
        },
    );
    let (new_line, new_outputs, new_link) = created(&input_result);
    let new_output = OutputPort::Next { node: new_line };
    assert_eq!(new_outputs, [new_output]);
    assert_eq!(
        new_link,
        Some(Link {
            from: new_output,
            to: ending,
        })
    );
    assert_eq!(
        after_input_creation.history.undo_depth,
        before_input_creation.history.undo_depth + 1
    );
    let before_remove = view(editor, document);
    let before_remove_content = encoded_content(&before_remove);
    let before_remove_facts = facts(editor, document);
    let (after_remove, remove_result) = edit_with_outcome(
        editor,
        document,
        DialogueCommand::Remove {
            nodes: vec![choice_node, new_line],
            links: vec![line_output, to, new_output],
        },
    );
    assert_eq!(
        remove_result
            .get::<DialogueGraph>()
            .map(|receipt| receipt.outcome()),
        Some(&DialogueCommandOutcome::Changed)
    );
    assert_eq!(
        after_remove.history.undo_depth,
        before_remove_facts.history.undo_depth + 1
    );
    assert_eq!(encoded_content(&view(editor, document)), baseline_content);
    assert!(after_remove.dirty);
    history(editor, document, false);
    assert_eq!(
        encoded_content(&view(editor, document)),
        before_remove_content
    );
    history(editor, document, true);
    assert_eq!(encoded_content(&view(editor, document)), baseline_content);
    assert_eq!(save(editor, &project, document)?, saved);
    let before_reopen = facts(editor, document);
    let stale = EditorExtensionCommand::new::<DialogueGraph>(
        document,
        before_reopen.revision,
        DialogueCommand::SetEntry(Some(line)),
    );
    let reopened = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: before_reopen.revision,
            },
        },
    );
    let current = facts(editor, document);
    let expected_digest = ContentDigest::of_bytes(&saved);
    assert_eq!(
        reopened,
        EditorPersistenceResult::Opened {
            document,
            revision: current.revision,
            digest: EditorDocumentDigest::new(
                expected_digest.length(),
                *expected_digest.as_bytes()
            ),
        }
    );
    assert_ne!(
        current.revision.source_id(),
        before_reopen.revision.source_id()
    );
    assert_eq!(current.history, Default::default());
    assert!(!current.dirty);
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved);
    acknowledge(editor);
    assert_eq!(
        dispatch(editor, [EditorAction::Extension(stale)]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: before_reopen.revision,
                actual: Some(current.revision),
            }
        )]
    );
    assert_eq!(created(&choice_result), (choice_node, choice_outputs, link));
    assert_eq!(choice_result.revision(), after_creation.revision);
    assert_eq!(facts(editor, document), current);
    Ok(())
}
