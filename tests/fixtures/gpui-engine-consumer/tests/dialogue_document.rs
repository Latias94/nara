use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, bail, ensure};
use nara_engine::{
    ProductRecipe,
    app::RuntimeCandidateRetirementState,
    core::DocumentRevision,
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorProjectCloseProgress, EditorProjectSession, TrustedCompiledEditorProjectIntent,
        advanced::built_in_schema_providers,
    },
    reflect::AuthoringSchemaBundle,
    scene::SceneDocument,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
        EditorDocumentId, EditorDocumentMutation, EditorDocumentTabModel, EditorExtensionCommand,
        EditorExtensionDocument, EditorPersistenceAction, EditorPersistenceAttemptId,
        EditorPersistenceFailureStage, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPersistenceView, EditorReopenTarget,
    },
};
use nara_gpui_engine_consumer::dialogue::{
    Choice, ChoiceDraft, ChoiceId, DialogueCommand, DialogueGraph, DialogueView, NodeBody, NodeId,
    NodePosition, NodeRecipe, OutputPort,
};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(10);
const SOURCE: &str = "dialogue/conversation.json";

struct Session(EditorProjectSession);

impl Session {
    fn close(&mut self) -> Result<()> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let EditorProjectCloseProgress::Complete(receipt) = self.0.advance_close() {
                ensure!(
                    !receipt.has_cleanup_failure(),
                    "dialogue Host cleanup failed"
                );
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "dialogue Host did not retire");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            if std::thread::panicking() {
                eprintln!("dialogue Host cleanup during unwinding: {error}");
            } else {
                panic!("{error}");
            }
        }
    }
}

fn create_project() -> Result<TempDir> {
    let project = tempfile::Builder::new()
        .prefix("nara-dialogue-host-")
        .tempdir()?;
    for directory in ["assets", "scenes", "prefabs", "schema", "dialogue"] {
        fs::create_dir(project.path().join(directory))?;
    }
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1
[project]
name = "Independent dialogue Host"
source_universe = "b30857c5-c840-539c-af6f-18dab1142e06"
native_document_identity = "project_location"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
"#,
    )?;
    let bundle =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())?;
    fs::write(
        project.path().join("schema/authoring.bundle.json"),
        bundle.to_json_string()?,
    )?;
    fs::write(
        project.path().join("scenes/startup.scene.json"),
        SceneDocument::default().to_json_string()?,
    )?;
    fs::write(project.path().join(SOURCE), DialogueGraph::new().encode()?)?;
    Ok(project)
}

fn open(project: &TempDir) -> Result<(Session, EditorDocumentId)> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    let root = DirectoryCapability::from_host_handle(
        options.open(project.path())?,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )?;
    match EditorProjectSession::open_trusted_compiled(
        root,
        TrustedCompiledEditorProjectIntent::new().with_recipe(ProductRecipe::new()),
    ) {
        Ok(session) => {
            let mut owner = Session(session);
            owner.0.register_extension_document::<DialogueGraph>()?;
            let document = owner.0.open_extension_document::<DialogueGraph>(SOURCE)?;
            Ok((owner, document))
        }
        Err(mut error) => {
            let deadline = Instant::now() + TIMEOUT;
            while error.advance_retirement() != RuntimeCandidateRetirementState::Retired {
                ensure!(
                    Instant::now() < deadline,
                    "failed dialogue Host Open did not retire"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            bail!("dialogue Host Open failed: {:?}", error.diagnostics());
        }
    }
}

fn facts(editor: &EditorProjectSession, document: EditorDocumentId) -> EditorDocumentTabModel {
    editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone()
}

fn view(editor: &EditorProjectSession, document: EditorDocumentId) -> Arc<DialogueView> {
    let frame = editor.current_frame();
    let projection = frame.workspace().extension_document(document).unwrap();
    assert_eq!(projection.type_id(), DialogueGraph::TYPE_ID);
    assert_eq!(projection.revision(), facts(editor, document).revision);
    projection.get::<DialogueGraph>().unwrap().clone()
}

fn encoded_content(view: &DialogueView) -> Vec<u8> {
    serde_json::to_vec(&(
        view.entry(),
        view.nodes().collect::<Vec<_>>(),
        view.links().collect::<Vec<_>>(),
    ))
    .unwrap()
}

fn choices(view: &DialogueView, node: NodeId) -> &[Choice] {
    let NodeBody::Choice { choices, .. } = view.node(node).unwrap().body.as_ref() else {
        panic!("expected a choice node");
    };
    choices
}

fn action(
    document: EditorDocumentId,
    revision: DocumentRevision,
    command: DialogueCommand,
) -> EditorAction {
    EditorAction::Extension(EditorExtensionCommand::new::<DialogueGraph>(
        document, revision, command,
    ))
}

fn dispatch(
    editor: &mut EditorProjectSession,
    actions: impl IntoIterator<Item = EditorAction>,
) -> Vec<EditorActionOutcome> {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, actions))
        .unwrap();
    assert_eq!(report.frame_revision(), revision);
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes()
}

fn assert_applied(
    outcome: &EditorActionOutcome,
    document: EditorDocumentId,
    revision: DocumentRevision,
) {
    let EditorActionOutcome::Workspace(report) = outcome else {
        panic!("expected an exact workspace receipt: {outcome:?}");
    };
    assert!(report.applied, "{report:?}");
    assert_eq!(report.document, Some(document));
    assert_eq!(report.active_document, Some(document));
    assert_eq!(report.revision, Some(revision));
    assert!(report.diagnostics.is_empty());
}

fn edit(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    command: DialogueCommand,
) -> EditorDocumentTabModel {
    let before = facts(editor, document);
    let outcomes = dispatch(editor, [action(document, before.revision, command)]);
    assert_eq!(outcomes.len(), 1);
    assert_applied(&outcomes[0], document, before.revision.next());
    let after = facts(editor, document);
    assert_eq!(after.revision, before.revision.next());
    assert_eq!(after.saved_revision, before.saved_revision);
    after
}

fn add(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    recipe: NodeRecipe,
    position: [f32; 2],
) -> NodeId {
    let previous = view(editor, document);
    let before = facts(editor, document);
    let after = edit(editor, document, DialogueCommand::Add { recipe, position });
    assert_eq!(after.history.undo_depth, before.history.undo_depth + 1);
    let current = view(editor, document);
    let added: Vec<_> = current
        .nodes()
        .filter(|node| previous.node(node.id).is_none())
        .map(|node| node.id)
        .collect();
    assert_eq!(added.len(), 1);
    added[0]
}

fn seed_conversation(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> (NodeId, NodeId, NodeId) {
    let line = add(editor, document, NodeRecipe::Line, [24.0, 24.0]);
    let choice = add(editor, document, NodeRecipe::Choice, [240.0, 24.0]);
    let ending = add(editor, document, NodeRecipe::End, [480.0, 24.0]);
    edit(
        editor,
        document,
        DialogueCommand::SetLine {
            node: line,
            speaker: "Guide".to_owned(),
            text: "Welcome, traveler.\nWhere will you go?".to_owned(),
        },
    );
    edit(
        editor,
        document,
        DialogueCommand::SetEnd {
            node: ending,
            text: "You reached the forest.".to_owned(),
        },
    );
    edit(
        editor,
        document,
        DialogueCommand::SetTarget {
            port: OutputPort::Next { node: line },
            target: Some(choice),
        },
    );
    let choice_id = choices(&view(editor, document), choice)[0].id;
    edit(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice,
            prompt: "Choose a destination".to_owned(),
            choices: vec![ChoiceDraft {
                id: Some(choice_id),
                label: "Enter the forest".to_owned(),
                target: Some(ending),
            }],
        },
    );
    (line, choice, ending)
}

fn history(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    redo: bool,
) -> EditorDocumentTabModel {
    let before = facts(editor, document);
    let mutation = if redo {
        EditorDocumentMutation::Redo {
            document,
            base_revision: before.revision,
        }
    } else {
        EditorDocumentMutation::Undo {
            document,
            base_revision: before.revision,
        }
    };
    let outcomes = dispatch(editor, [EditorAction::Mutate(mutation)]);
    assert_eq!(outcomes.len(), 1);
    assert_applied(&outcomes[0], document, before.revision.next());
    let after = facts(editor, document);
    assert_eq!(after.revision, before.revision.next());
    assert_eq!(after.saved_revision, before.saved_revision);
    after
}

fn accepted_attempt(outcome: &EditorActionOutcome) -> EditorPersistenceAttemptId {
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        outcome
    else {
        panic!("persistence was not accepted: {outcome:?}");
    };
    *attempt
}

fn settle(
    editor: &mut EditorProjectSession,
    attempt: EditorPersistenceAttemptId,
) -> EditorPersistenceResult {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let observation = editor.current_frame().persistence();
        assert_eq!(observation.attempt(), Some(attempt));
        if let Some(result) = observation.result() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "dialogue persistence did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn persist(
    editor: &mut EditorProjectSession,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    let outcomes = dispatch(editor, [EditorAction::Persistence(action)]);
    assert_eq!(outcomes.len(), 1);
    settle(editor, accepted_attempt(&outcomes[0]))
}

fn acknowledge(editor: &mut EditorProjectSession) {
    let persistence = editor.current_frame().persistence();
    assert!(persistence.result().is_some());
    let attempt = persistence.attempt().unwrap();
    assert_eq!(
        dispatch(
            editor,
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )]
        ),
        [EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt }
        )]
    );
    assert_eq!(
        editor.current_frame().persistence(),
        EditorPersistenceView::Idle
    );
}

fn save(
    editor: &mut EditorProjectSession,
    project: &TempDir,
    document: EditorDocumentId,
) -> Result<Vec<u8>> {
    let before = facts(editor, document);
    let content = encoded_content(&view(editor, document));
    let result = persist(
        editor,
        EditorPersistenceAction::Save {
            document,
            base_revision: before.revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = result
    else {
        panic!("dialogue Save did not publish: {result:?}");
    };
    assert_eq!((saved_document, revision), (document, before.revision));
    let bytes = fs::read(project.path().join(SOURCE))?;
    let decoded = DialogueGraph::decode(&bytes)?;
    assert_eq!(decoded.encode()?, bytes);
    assert_eq!(encoded_content(&decoded.view()), content);
    assert_eq!(digest.encoded_bytes(), bytes.len() as u64);
    let after = facts(editor, document);
    assert!(!after.dirty);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.saved_revision, before.revision);
    assert_eq!(after.history, before.history);
    acknowledge(editor);
    Ok(bytes)
}

#[test]
fn node_and_property_edits_share_atomic_history_and_saved_content() -> Result<()> {
    let project = create_project()?;
    let initial_bytes = fs::read(project.path().join(SOURCE))?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let initial = facts(editor, document);
    assert!(!initial.dirty);
    assert_eq!(initial.revision, initial.saved_revision);
    assert_eq!(initial.history, Default::default());
    assert_eq!(view(editor, document).entry(), None);
    let (line, choice, ending) = seed_conversation(editor, document);
    let alternate = add(editor, document, NodeRecipe::End, [480.0, 192.0]);
    let first = choices(&view(editor, document), choice)[0].id;
    let before_choices = facts(editor, document);
    let changed = edit(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice,
            prompt: "Choose a destination".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(first),
                    label: "Enter the forest".to_owned(),
                    target: Some(ending),
                },
                ChoiceDraft {
                    id: None,
                    label: "Go home".to_owned(),
                    target: Some(alternate),
                },
            ],
        },
    );
    assert_eq!(
        changed.history.undo_depth,
        before_choices.history.undo_depth + 1
    );
    let second = choices(&view(editor, document), choice)[1].id;
    let first_port = OutputPort::Choice {
        node: choice,
        choice: first,
    };
    let second_port = OutputPort::Choice {
        node: choice,
        choice: second,
    };
    let before_reorder = facts(editor, document);
    let reordered = edit(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice,
            prompt: "Choose a destination".to_owned(),
            choices: vec![
                ChoiceDraft {
                    id: Some(second),
                    label: "Return home".to_owned(),
                    target: Some(alternate),
                },
                ChoiceDraft {
                    id: Some(first),
                    label: "Explore the forest".to_owned(),
                    target: Some(ending),
                },
            ],
        },
    );
    assert_eq!(
        reordered.history.undo_depth,
        before_reorder.history.undo_depth + 1
    );
    let reordered_view = view(editor, document);
    assert_eq!(
        choices(&reordered_view, choice)
            .iter()
            .map(|choice| choice.id)
            .collect::<Vec<_>>(),
        [second, first]
    );
    assert_eq!(reordered_view.target(first_port), Some(ending));
    assert_eq!(reordered_view.target(second_port), Some(alternate));
    let property_text = "台詞 from the property editor.\nKeep \"quotes\" and \\ escapes.";
    let before_property = facts(editor, document);
    let property = edit(
        editor,
        document,
        DialogueCommand::SetLine {
            node: line,
            speaker: "Narrator".to_owned(),
            text: property_text.to_owned(),
        },
    );
    assert_eq!(
        property.history.undo_depth,
        before_property.history.undo_depth + 1
    );
    assert_eq!(
        view(editor, document).node(line).unwrap().body.as_ref(),
        &NodeBody::Line {
            speaker: "Narrator".to_owned(),
            text: property_text.to_owned(),
        }
    );
    let before_move = facts(editor, document);
    let moved = edit(
        editor,
        document,
        DialogueCommand::Move(vec![
            NodePosition {
                node: line,
                position: [48.0, 72.0],
            },
            NodePosition {
                node: choice,
                position: [264.0, 72.0],
            },
        ]),
    );
    assert_eq!(moved.history.undo_depth, before_move.history.undo_depth + 1);
    assert_eq!(
        view(editor, document).node(line).unwrap().position,
        [48.0, 72.0]
    );
    assert_eq!(
        view(editor, document).node(choice).unwrap().position,
        [264.0, 72.0]
    );
    edit(editor, document, DialogueCommand::SetEntry(None));
    assert_eq!(view(editor, document).entry(), None);
    edit(editor, document, DialogueCommand::SetEntry(Some(line)));
    assert_eq!(fs::read(project.path().join(SOURCE))?, initial_bytes);
    let saved_content = encoded_content(&view(editor, document));
    let saved_bytes = save(editor, &project, document)?;
    let saved = facts(editor, document);
    let changed = edit(
        editor,
        document,
        DialogueCommand::SetChoices {
            node: choice,
            prompt: "Return immediately?".to_owned(),
            choices: vec![ChoiceDraft {
                id: Some(first),
                label: "Return home".to_owned(),
                target: Some(alternate),
            }],
        },
    );
    assert!(changed.dirty);
    assert_eq!(changed.history.undo_depth, saved.history.undo_depth + 1);
    assert!(!view(editor, document).contains_output(second_port));
    assert_eq!(view(editor, document).target(second_port), None);
    assert_eq!(view(editor, document).target(first_port), Some(alternate));
    let changed_content = encoded_content(&view(editor, document));
    let undone = history(editor, document, false);
    assert!(!undone.dirty);
    assert_eq!(undone.history.undo_depth, saved.history.undo_depth);
    assert_eq!(undone.history.redo_depth, 1);
    assert_eq!(encoded_content(&view(editor, document)), saved_content);
    let redone = history(editor, document, true);
    assert!(redone.dirty);
    assert_eq!(redone.history, changed.history);
    assert_eq!(encoded_content(&view(editor, document)), changed_content);
    assert!(!history(editor, document, false).dirty);
    let observed = view(editor, document);
    let original_node_lifetime = observed.node_incarnation(line).unwrap();
    let next_port = OutputPort::Next { node: line };
    let original_link_lifetime = observed.link_incarnation(next_port).unwrap();
    let before_remove = facts(editor, document);
    let outcomes = dispatch(
        editor,
        [
            action(
                document,
                before_remove.revision,
                DialogueCommand::Remove(vec![line, ending]),
            ),
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: before_remove.revision.next(),
            }),
        ],
    );
    assert_eq!(outcomes.len(), 2);
    assert_applied(&outcomes[0], document, before_remove.revision.next());
    assert_applied(&outcomes[1], document, before_remove.revision.next().next());
    let restored = facts(editor, document);
    assert!(!restored.dirty);
    assert_eq!(restored.history.undo_depth, saved.history.undo_depth);
    assert_eq!(restored.history.redo_depth, 1);
    let restored_view = view(editor, document);
    assert_eq!(encoded_content(&restored_view), saved_content);
    assert_ne!(
        restored_view.node_incarnation(line).unwrap(),
        original_node_lifetime
    );
    assert_ne!(
        restored_view.link_incarnation(next_port).unwrap(),
        original_link_lifetime
    );
    assert_eq!(restored_view.target(next_port), Some(choice));
    assert_eq!(restored_view.target(first_port), Some(ending));
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved_bytes);
    Ok(())
}

#[test]
fn invalid_typed_edits_preserve_source_history_and_both_allocators() -> Result<()> {
    let project = create_project()?;
    let initial_bytes = fs::read(project.path().join(SOURCE))?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, choice, ending) = seed_conversation(editor, document);
    let foreign = add(editor, document, NodeRecipe::Choice, [240.0, 192.0]);
    let own_id = choices(&view(editor, document), choice)[0].id;
    let foreign_id = choices(&view(editor, document), foreign)[0].id;
    let missing = NodeId(10_000);
    let valid_port = OutputPort::Choice {
        node: choice,
        choice: own_id,
    };
    for (command, code) in [
        (
            DialogueCommand::Add {
                recipe: NodeRecipe::Choice,
                position: [f32::NAN, 0.0],
            },
            "dialogue.position",
        ),
        (
            DialogueCommand::SetChoices {
                node: choice,
                prompt: "Must not partly replace choices".to_owned(),
                choices: vec![
                    ChoiceDraft {
                        id: None,
                        label: "Allocated only on success".to_owned(),
                        target: Some(ending),
                    },
                    ChoiceDraft {
                        id: Some(foreign_id),
                        label: "Foreign choice".to_owned(),
                        target: Some(ending),
                    },
                ],
            },
            "dialogue.choice-owner",
        ),
        (
            DialogueCommand::SetChoices {
                node: choice,
                prompt: "Must not duplicate a port".to_owned(),
                choices: vec![
                    ChoiceDraft {
                        id: Some(own_id),
                        label: "First".to_owned(),
                        target: Some(ending),
                    },
                    ChoiceDraft {
                        id: Some(own_id),
                        label: "Second".to_owned(),
                        target: Some(line),
                    },
                ],
            },
            "dialogue.choice-duplicate",
        ),
        (
            DialogueCommand::SetTarget {
                port: OutputPort::Choice {
                    node: choice,
                    choice: foreign_id,
                },
                target: Some(ending),
            },
            "dialogue.port-missing",
        ),
        (
            DialogueCommand::SetTarget {
                port: valid_port,
                target: Some(missing),
            },
            "dialogue.node-missing",
        ),
        (
            DialogueCommand::SetLine {
                node: choice,
                speaker: "Invalid".to_owned(),
                text: "Wrong node kind".to_owned(),
            },
            "dialogue.node-kind",
        ),
        (
            DialogueCommand::Move(vec![
                NodePosition {
                    node: line,
                    position: [120.0, 120.0],
                },
                NodePosition {
                    node: missing,
                    position: [240.0, 120.0],
                },
            ]),
            "dialogue.node-missing",
        ),
        (
            DialogueCommand::Remove(vec![line, missing]),
            "dialogue.node-missing",
        ),
    ] {
        let before = facts(editor, document);
        let content = encoded_content(&view(editor, document));
        let outcomes = dispatch(editor, [action(document, before.revision, command)]);
        let [EditorActionOutcome::Workspace(report)] = outcomes.as_slice() else {
            panic!("invalid dialogue command did not reach its owner: {outcomes:?}");
        };
        assert!(!report.applied, "{report:?}");
        assert_eq!(report.document, Some(document));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == code),
            "missing {code}: {report:?}"
        );
        assert_eq!(facts(editor, document), before);
        assert_eq!(encoded_content(&view(editor, document)), content);
        assert_eq!(fs::read(project.path().join(SOURCE))?, initial_bytes);
    }
    let added = add(editor, document, NodeRecipe::Choice, [480.0, 192.0]);
    assert_eq!(added, NodeId(foreign.0 + 1));
    assert_eq!(
        choices(&view(editor, document), added)[0].id,
        ChoiceId(foreign_id.0 + 1)
    );
    assert_eq!(view(editor, document).target(valid_port), Some(ending));
    Ok(())
}

#[test]
fn no_op_preserves_saved_content_and_redo_before_a_fresh_identity_branch() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let saved_bytes = save(editor, &project, document)?;
    let saved_content = encoded_content(&view(editor, document));
    let saved = facts(editor, document);
    let discarded = add(editor, document, NodeRecipe::Choice, [240.0, 240.0]);
    let discarded_choice = choices(&view(editor, document), discarded)[0].id;
    let discarded_content = encoded_content(&view(editor, document));
    assert!(facts(editor, document).dirty);
    let undone = history(editor, document, false);
    assert!(!undone.dirty);
    assert_eq!(undone.history.undo_depth, saved.history.undo_depth);
    assert_eq!(undone.history.redo_depth, 1);
    assert_eq!(encoded_content(&view(editor, document)), saved_content);
    let stale = action(
        document,
        undone.revision,
        DialogueCommand::SetEnd {
            node: ending,
            text: "Must not use the pre-no-op revision".to_owned(),
        },
    );
    let current = view(editor, document);
    let NodeBody::Line { speaker, text } = current.node(line).unwrap().body.as_ref() else {
        panic!("expected the opening line");
    };
    let no_op = edit(
        editor,
        document,
        DialogueCommand::SetLine {
            node: line,
            speaker: speaker.clone(),
            text: text.clone(),
        },
    );
    assert_eq!(no_op.revision, undone.revision.next());
    assert!(!no_op.dirty);
    assert_eq!(no_op.saved_revision, saved.saved_revision);
    assert_eq!(no_op.history, undone.history);
    assert_eq!(encoded_content(&view(editor, document)), saved_content);
    assert_eq!(
        dispatch(editor, [stale]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: undone.revision,
                actual: Some(no_op.revision),
            }
        )]
    );
    assert_eq!(facts(editor, document), no_op);
    assert_eq!(fs::read(project.path().join(SOURCE))?, saved_bytes);
    let redone = history(editor, document, true);
    assert!(redone.dirty);
    assert_eq!(encoded_content(&view(editor, document)), discarded_content);
    assert_eq!(
        choices(&view(editor, document), discarded)[0].id,
        discarded_choice
    );
    let discarded_bytes = save(editor, &project, document)?;
    assert!(history(editor, document, false).dirty);
    assert_eq!(encoded_content(&view(editor, document)), saved_content);
    let branch = add(editor, document, NodeRecipe::Choice, [264.0, 264.0]);
    let branch_choice = choices(&view(editor, document), branch)[0].id;
    assert!(branch > discarded);
    assert!(branch_choice > discarded_choice);
    assert!(view(editor, document).node(discarded).is_none());
    let branched = facts(editor, document);
    assert!(branched.dirty);
    assert_eq!(branched.history.undo_depth, saved.history.undo_depth + 1);
    assert_eq!(branched.history.redo_depth, 0);
    let branch_content = encoded_content(&view(editor, document));
    assert_ne!(branch_content, discarded_content);
    assert!(history(editor, document, false).dirty);
    assert_eq!(encoded_content(&view(editor, document)), saved_content);
    assert!(history(editor, document, true).dirty);
    assert_eq!(encoded_content(&view(editor, document)), branch_content);
    assert_eq!(fs::read(project.path().join(SOURCE))?, discarded_bytes);
    Ok(())
}

#[test]
fn captured_save_and_reopen_keep_exact_source_and_refuse_old_commands() -> Result<()> {
    let project = create_project()?;
    let (mut owner, document) = open(&project)?;
    let editor = &mut owner.0;
    let (line, _, ending) = seed_conversation(editor, document);
    let captured = facts(editor, document);
    let captured_content = encoded_content(&view(editor, document));
    let outcomes = dispatch(
        editor,
        [
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: captured.revision,
            }),
            action(
                document,
                captured.revision,
                DialogueCommand::SetLine {
                    node: line,
                    speaker: "Later author".to_owned(),
                    text: "This edit must remain dirty after the older Save.".to_owned(),
                },
            ),
        ],
    );
    assert_eq!(outcomes.len(), 2);
    let attempt = accepted_attempt(&outcomes[0]);
    assert_applied(&outcomes[1], document, captured.revision.next());
    let result = settle(editor, attempt);
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = result
    else {
        panic!("captured dialogue Save did not publish: {result:?}");
    };
    assert_eq!((saved_document, revision), (document, captured.revision));
    let saved_bytes = fs::read(project.path().join(SOURCE))?;
    let decoded = DialogueGraph::decode(&saved_bytes)?;
    assert_eq!(decoded.encode()?, saved_bytes);
    assert_eq!(encoded_content(&decoded.view()), captured_content);
    assert_eq!(digest.encoded_bytes(), saved_bytes.len() as u64);
    let later = facts(editor, document);
    assert!(later.dirty);
    assert_eq!(later.revision, captured.revision.next());
    assert_eq!(later.saved_revision, captured.revision);
    assert_eq!(later.history.undo_depth, captured.history.undo_depth + 1);
    assert_ne!(encoded_content(&view(editor, document)), captured_content);
    acknowledge(editor);
    let stale = action(
        document,
        later.revision,
        DialogueCommand::SetEnd {
            node: ending,
            text: "Must not cross source incarnations".to_owned(),
        },
    );
    let result = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: later.revision,
            },
        },
    );
    let reopened = facts(editor, document);
    assert_eq!(
        result,
        EditorPersistenceResult::Opened {
            document,
            revision: reopened.revision,
            digest,
        }
    );
    assert_eq!(reopened.document, document);
    assert_ne!(reopened.revision.source_id(), later.revision.source_id());
    assert_eq!(reopened.revision.generation(), 0);
    assert_eq!(reopened.saved_revision, reopened.revision);
    assert!(!reopened.dirty);
    assert_eq!(reopened.history, Default::default());
    assert_eq!(reopened.source_binding, captured.source_binding);
    assert_eq!(encoded_content(&view(editor, document)), captured_content);
    assert_eq!(
        dispatch(editor, [stale]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: later.revision,
                actual: Some(reopened.revision),
            }
        )]
    );
    assert_eq!(facts(editor, document), reopened);
    acknowledge(editor);
    edit(
        editor,
        document,
        DialogueCommand::SetEnd {
            node: ending,
            text: "Keep this unsaved ending when Reopen fails.".to_owned(),
        },
    );
    let before_failure = facts(editor, document);
    let content_before_failure = encoded_content(&view(editor, document));
    fs::write(
        project.path().join(SOURCE),
        br#"["This is a list Dialogue source, not a Dialogue Graph"]"#,
    )?;
    let result = persist(
        editor,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: before_failure.revision,
            },
        },
    );
    assert!(matches!(
        result,
        EditorPersistenceResult::Failed {
            document: Some(failed_document),
            stage: EditorPersistenceFailureStage::Decode,
            ..
        } if failed_document == document
    ));
    assert_eq!(facts(editor, document), before_failure);
    assert_eq!(
        encoded_content(&view(editor, document)),
        content_before_failure
    );
    assert!(facts(editor, document).dirty);
    assert_eq!(
        fs::read(project.path().join(SOURCE))?,
        br#"["This is a list Dialogue source, not a Dialogue Graph"]"#
    );
    acknowledge(editor);
    Ok(())
}
