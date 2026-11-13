use super::*;
use nara_gpui_engine_consumer::dialogue::{
    preview::ConversationStep,
    workflow::{
        ConflictResolution, DialogueDraft, DialoguePreview, DialogueSnapshot, DraftField,
        DraftState,
    },
};

fn snapshot(editor: &EditorProjectSession, document: EditorDocumentId) -> DialogueSnapshot {
    DialogueSnapshot::capture(editor, document).unwrap()
}

fn submit_draft(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    draft: &mut DialogueDraft,
    position_only: bool,
) -> Result<()> {
    let command = draft
        .prepare(&snapshot(editor, document), position_only)
        .map_err(anyhow::Error::msg)?;
    let outcomes = dispatch(editor, [EditorAction::Extension(command.clone())]);
    draft
        .settle(&command, &outcomes[0], &snapshot(editor, document))
        .map_err(anyhow::Error::msg)?;
    Ok(())
}

#[test]
fn review_preserves_peer_fields_and_explicit_resolution_uses_original_session_history() -> Result<()>
{
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, _) = seed_conversation(editor, document);
    let initial = snapshot(editor, document);
    let mut first = DialogueDraft::new(initial.clone(), line).unwrap();
    let mut second = DialogueDraft::new(initial.clone(), line).unwrap();
    second.edit(|value| value.text = "Second View text\n草稿".into());
    first.edit(|value| value.speaker = "First View speaker".into());
    submit_draft(editor, document, &mut first, false)?;
    let current = snapshot(editor, document);
    second.observe(&current);
    assert!(second.review(&current));
    assert!(second.conflicts().is_empty());
    assert_eq!(second.value().speaker, "First View speaker");
    submit_draft(editor, document, &mut second, false)?;
    let merged = snapshot(editor, document);
    assert!(matches!(merged.view.node(line).unwrap().body.as_ref(),
        NodeBody::Line { speaker, text } if speaker == "First View speaker" && text == "Second View text\n草稿"));
    assert_eq!(merged.history.undo_depth, initial.history.undo_depth + 2);
    first.observe(&merged);
    first.edit(|value| value.text = "Local competing text".into());
    second.edit(|value| value.text = "Peer competing text".into());
    submit_draft(editor, document, &mut second, false)?;
    let competing = snapshot(editor, document);
    assert!(first.review(&competing));
    assert_eq!(first.conflicts().len(), 1);
    assert_eq!(first.conflicts()[0].current, "Peer competing text");
    assert_eq!(first.conflicts()[0].draft, "Local competing text");
    assert!(first.prepare(&competing, false).is_err());
    assert_eq!(snapshot(editor, document), competing);
    assert!(first.resolve(DraftField::Text, ConflictResolution::Draft, &competing));
    submit_draft(editor, document, &mut first, false)?;
    let accepted = snapshot(editor, document);
    assert_eq!(accepted.history.undo_depth, initial.history.undo_depth + 4);
    save(editor, &project, document)?;
    let saved = snapshot(editor, document);
    assert!(!saved.dirty);
    history(editor, document, false);
    assert_eq!(view(editor, document), competing.view);
    history(editor, document, true);
    assert_eq!(view(editor, document), accepted.view);
    assert!(!snapshot(editor, document).dirty);

    let basis = snapshot(editor, document);
    let mut local_choice = DialogueDraft::new(basis.clone(), choice).unwrap();
    let mut peer_choice = DialogueDraft::new(basis, choice).unwrap();
    let id = local_choice.value().choices[0].id.unwrap();
    local_choice.edit(|value| {
        value.text = "Keep local prompt".into();
        value.choices[0].label = "Local choice label".into();
    });
    peer_choice.edit(|value| {
        value.choices[0].label = "Peer choice label".into();
        value.choices[0].target = Some(line);
    });
    submit_draft(editor, document, &mut peer_choice, false)?;
    let competing_choice = snapshot(editor, document);
    assert!(local_choice.review(&competing_choice));
    assert_eq!(
        local_choice.conflicts()[0].field,
        DraftField::ChoiceLabel(id)
    );
    assert!(local_choice.edit(|value| value.choices.clear()));
    assert_eq!(local_choice.conflicts()[0].draft, "Removed");
    assert!(local_choice.prepare(&competing_choice, false).is_err());
    assert_eq!(snapshot(editor, document), competing_choice);
    assert!(local_choice.resolve(
        DraftField::ChoiceLabel(id),
        ConflictResolution::Current,
        &competing_choice,
    ));
    submit_draft(editor, document, &mut local_choice, false)?;
    let accepted_choice = snapshot(editor, document);
    assert_eq!(local_choice.state(), DraftState::Accepted);
    assert_eq!(
        choices(&accepted_choice.view, choice),
        choices(&competing_choice.view, choice)
    );
    assert_eq!(
        accepted_choice.view.target(OutputPort::Choice {
            node: choice,
            choice: id
        }),
        Some(line)
    );
    assert!(
        matches!(accepted_choice.view.node(choice).unwrap().body.as_ref(),
        NodeBody::Choice { prompt, .. } if prompt == "Keep local prompt")
    );
    assert_eq!(
        accepted_choice.history.undo_depth,
        competing_choice.history.undo_depth + 1
    );
    history(editor, document, false);
    assert_eq!(view(editor, document), competing_choice.view);
    history(editor, document, true);
    assert_eq!(view(editor, document), accepted_choice.view);
    Ok(())
}

#[test]
fn position_apply_keeps_other_drafts_and_choice_apply_adopts_original_allocations() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, ending) = seed_conversation(editor, document);
    let initial = snapshot(editor, document);
    let original_choices = choices(&initial.view, choice).to_vec();
    let mut position = DialogueDraft::new(initial.clone(), line).unwrap();
    position.edit(|draft| {
        draft.position = ["56".into(), "72".into()];
        draft.text = "Keep this uncommitted\n保留正文".into();
    });
    submit_draft(editor, document, &mut position, true)?;
    let moved = snapshot(editor, document);
    assert_eq!(moved.view.node(line).unwrap().position, [56., 72.]);
    assert_eq!(
        moved.view.node(line).unwrap().body,
        initial.view.node(line).unwrap().body
    );
    assert_eq!(position.value().text, "Keep this uncommitted\n保留正文");
    assert_eq!(position.state(), DraftState::Editing);
    assert_eq!(moved.history.undo_depth, initial.history.undo_depth + 1);
    submit_draft(editor, document, &mut position, false)?;
    assert_eq!(position.state(), DraftState::Accepted);
    assert_eq!(
        snapshot(editor, document).view.node(line).unwrap().position,
        [56., 72.]
    );

    let basis = snapshot(editor, document);
    let mut draft = DialogueDraft::new(basis.clone(), choice).unwrap();
    draft.edit(|draft| {
        draft.choices.push(ChoiceDraft {
            id: None,
            label: "New branch\n新选项".into(),
            target: Some(ending),
        });
        draft.choices.reverse();
    });
    submit_draft(editor, document, &mut draft, false)?;
    let accepted = snapshot(editor, document);
    let canonical = choices(&accepted.view, choice);
    assert_eq!(draft.state(), DraftState::Accepted);
    assert_eq!(draft.value().choices[0].id, Some(canonical[0].id));
    assert!(
        !original_choices
            .iter()
            .any(|choice| choice.id == canonical[0].id)
    );
    assert_eq!(draft.value().choices[1].id, Some(original_choices[0].id));
    assert_eq!(
        accepted.view.target(OutputPort::Choice {
            node: choice,
            choice: canonical[0].id
        }),
        Some(ending)
    );
    assert_eq!(accepted.history.undo_depth, basis.history.undo_depth + 1);
    history(editor, document, false);
    assert_eq!(choices(&view(editor, document), choice), original_choices);
    history(editor, document, true);
    assert_eq!(
        encoded_content(&snapshot(editor, document).view),
        encoded_content(&accepted.view)
    );
    Ok(())
}

#[test]
fn view_drafts_cancel_apply_and_history_share_the_original_file_owner() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    save(editor, &project, document)?;
    let initial = snapshot(editor, document);
    let mut first = DialogueDraft::new(initial.clone(), line).unwrap();
    let mut second = DialogueDraft::new(initial.clone(), line).unwrap();
    let mut property = DialogueDraft::new(initial.clone(), ending).unwrap();
    let text = "First line\n第二行 🦀\n\nTrailing newline\n";
    first.edit(|draft| draft.text = "Cancelled\n未提交".into());
    assert!(first.cancel(&initial));
    assert_eq!(snapshot(editor, document), initial);
    assert_eq!(first.state(), DraftState::Clean);

    second.edit(|draft| draft.text = "Independent unsaved draft".into());
    first.edit(|draft| draft.text = text.into());
    submit_draft(editor, document, &mut first, false)?;
    assert_eq!(first.state(), DraftState::Accepted);
    let current = snapshot(editor, document);
    assert_eq!(current.history.undo_depth, initial.history.undo_depth + 1);
    assert!(current.dirty);
    second.observe(&current);
    property.observe(&current);
    assert_eq!(second.state(), DraftState::Conflict);
    assert_eq!(second.value().text, "Independent unsaved draft");
    assert_eq!(second.basis().revision, initial.revision);
    assert!(second.prepare(&current, false).is_err());
    assert!(second.cancel(&current));
    assert_eq!(second.value().text, text);
    assert_eq!(property.state(), DraftState::Clean);

    history(editor, document, false);
    let undone = snapshot(editor, document);
    assert!(!undone.dirty);
    assert_eq!(undone.view, initial.view);
    history(editor, document, true);
    let redone = snapshot(editor, document);
    assert_eq!(redone.view, current.view);
    save(editor, &project, document)?;
    let saved = snapshot(editor, document);
    assert!(!saved.dirty);
    assert!(
        matches!(saved.view.node(line).unwrap().body.as_ref(), NodeBody::Line { text: value, .. } if value == text)
    );
    let old_command = saved.command(DialogueCommand::SetEnd {
        node: ending,
        text: "Old incarnation".into(),
    });
    let mut old_preview = DialoguePreview::new(saved.clone());
    let mut old_draft = DialogueDraft::new(saved.clone(), line).unwrap();
    old_draft.edit(|draft| draft.text = "Retire me".into());
    let result = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: saved.revision,
            },
        },
    );
    assert!(matches!(result, EditorPersistenceResult::Opened { .. }));
    acknowledge(editor);
    let reopened = snapshot(editor, document);
    assert!(!saved.same_source(&reopened));
    assert_eq!(
        encoded_content(&saved.view),
        encoded_content(&reopened.view)
    );
    old_draft.observe(&reopened);
    assert_eq!(old_draft.state(), DraftState::Retired);
    assert!(old_draft.prepare(&reopened, false).is_err());
    assert!(old_preview.advance(&reopened).is_err());
    let before = reopened.clone();
    let refused = dispatch(editor, [EditorAction::Extension(old_command)]);
    assert!(matches!(&refused[0], EditorActionOutcome::Rejected(_)));
    assert_eq!(snapshot(editor, document), before);
    drop(first);
    drop(second);
    drop(property);
    assert!(DialogueSnapshot::capture(editor, document).is_some());
    assert_eq!(snapshot(editor, document).history, before.history);
    Ok(())
}

#[test]
fn invalid_read_only_pending_and_foreign_receipts_do_not_settle_a_draft() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let initial = snapshot(editor, document);
    let mut draft = DialogueDraft::new(initial.clone(), line).unwrap();
    draft.set_read_only(true);
    assert!(!draft.edit(|value| value.text = "Must not change".into()));
    assert!(draft.prepare(&initial, false).is_err());
    draft.set_read_only(false);
    for invalid in ["", "NaN", "inf", "1000001", "-inf", "1e99"] {
        draft.edit(|value| value.position[0] = invalid.into());
        assert!(draft.prepare(&initial, true).is_err());
        assert_eq!(snapshot(editor, document), initial);
    }
    draft.cancel(&initial);
    draft.edit(|value| value.text = "🦀".repeat(1025));
    assert!(draft.prepare(&initial, false).is_err());
    assert!(draft.error().is_some());
    draft.cancel(&initial);
    draft.edit(|value| value.text = "Pending\n正文".into());
    let command = draft.prepare(&initial, false).unwrap();
    assert_eq!(draft.state(), DraftState::Pending);
    assert!(!draft.edit(|value| value.text.clear()));
    assert!(!draft.cancel(&initial));
    assert!(draft.prepare(&initial, false).is_err());
    let accepted = dispatch(editor, [EditorAction::Extension(command.clone())]);
    let other = snapshot(editor, document).command(DialogueCommand::SetEnd {
        node: ending,
        text: "Other slot".into(),
    });
    let foreign = dispatch(editor, [EditorAction::Extension(other.clone())]);
    let current = snapshot(editor, document);
    assert!(draft.settle(&other, &foreign[0], &current).is_err());
    assert_eq!(draft.state(), DraftState::Pending);
    assert!(draft.settle(&command, &foreign[0], &current).is_err());
    draft.settle(&command, &accepted[0], &current).unwrap();
    assert_eq!(draft.state(), DraftState::Clean);
    assert!(draft.settle(&command, &accepted[0], &current).is_err());
    assert_eq!(current.history.undo_depth, initial.history.undo_depth + 2);
    Ok(())
}

#[test]
fn review_is_explicit_and_old_node_incarnations_retire_after_coalesced_undo() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let initial = snapshot(editor, document);
    let mut draft = DialogueDraft::new(initial.clone(), line).unwrap();
    draft.edit(|value| value.text = "Retained proposal".into());
    edit(
        editor,
        document,
        DialogueCommand::SetEnd {
            node: ending,
            text: "Other field".into(),
        },
    );
    let current = snapshot(editor, document);
    draft.observe(&current);
    assert_eq!(draft.state(), DraftState::Conflict);
    assert!(draft.review(&current));
    submit_draft(editor, document, &mut draft, false)?;
    edit(
        editor,
        document,
        DialogueCommand::Remove {
            nodes: vec![line],
            links: Vec::new(),
        },
    );
    history(editor, document, false);
    let restored = snapshot(editor, document);
    draft.observe(&restored);
    assert_eq!(draft.state(), DraftState::Retired);
    assert!(!draft.review(&restored));
    assert!(draft.prepare(&restored, false).is_err());
    Ok(())
}

#[test]
fn preview_runs_real_choices_loops_and_incomplete_links_without_history() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, ending) = seed_conversation(editor, document);
    let choice_id = choices(&view(editor, document), choice)[0].id;
    edit(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice,
            prompt: "One explicit transition".into(),
            choices: vec![
                ChoiceDraft {
                    id: Some(choice_id),
                    label: "Loop".into(),
                    target: Some(line),
                },
                ChoiceDraft {
                    id: None,
                    label: "End".into(),
                    target: Some(ending),
                },
                ChoiceDraft {
                    id: None,
                    label: "Incomplete".into(),
                    target: None,
                },
            ],
        },
    );
    let basis = snapshot(editor, document);
    let mut preview = DialoguePreview::new(basis.clone());
    assert!(matches!(
        preview.step(&basis).unwrap(),
        ConversationStep::Line { .. }
    ));
    assert_eq!(preview.advance(&basis).unwrap(), choice);
    assert_eq!(preview.choose(&basis, choice_id).unwrap(), line);
    assert_eq!(preview.advance(&basis).unwrap(), choice);
    let choices = choices(&basis.view, choice);
    assert!(preview.choose(&basis, choices[2].id).is_err());
    assert!(
        matches!(preview.step(&basis).unwrap(), ConversationStep::Choice { node, .. } if node == choice)
    );
    assert_eq!(preview.choose(&basis, choices[1].id).unwrap(), ending);
    assert!(matches!(
        preview.step(&basis).unwrap(),
        ConversationStep::End { .. }
    ));
    assert_eq!(snapshot(editor, document), basis);
    edit(editor, document, DialogueCommand::SetEntry(None));
    let incomplete = snapshot(editor, document);
    assert!(preview.step(&incomplete).is_err());
    assert!(
        DialoguePreview::new(incomplete.clone())
            .step(&incomplete)
            .is_err()
    );
    assert_eq!(snapshot(editor, document), incomplete);
    Ok(())
}
