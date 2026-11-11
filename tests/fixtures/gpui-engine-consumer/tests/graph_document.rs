use std::{
    fs,
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
        EditorExtensionDocument, EditorPersistenceAction, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPersistenceView,
    },
};
use nara_gpui_engine_consumer::graph::{
    GraphCommand, GraphView, Link, NodeId, NodeKind, ValueGraph, evaluate,
};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(10);
const SOURCE: &str = "graphs/value.json";
const INITIAL: &[u8] = br#"{"nodes":[],"links":[]}"#;
const SAVED: &[u8] = concat!(
    r#"{"nodes":[{"id":1,"kind":{"Scalar":7.0},"position":[0.0,0.0]},"#,
    r#"{"id":2,"kind":{"Scalar":3.0},"position":[0.0,96.0]},"#,
    r#"{"id":3,"kind":"Add","position":[192.0,48.0]},"#,
    r#"{"id":4,"kind":"Output","position":[384.0,48.0]}],"#,
    r#""links":[{"from":1,"to":3,"input":0},{"from":2,"to":3,"input":1},"#,
    r#"{"from":3,"to":4,"input":0}]}"#,
)
.as_bytes();

struct Session(EditorProjectSession);

impl Session {
    fn close(&mut self) -> Result<()> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let EditorProjectCloseProgress::Complete(receipt) = self.0.advance_close() {
                ensure!(!receipt.has_cleanup_failure(), "graph Host cleanup failed");
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "graph Host did not retire");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            if std::thread::panicking() {
                eprintln!("graph Host cleanup during unwinding: {error}");
            } else {
                panic!("{error}");
            }
        }
    }
}

fn create_project() -> Result<TempDir> {
    let project = tempfile::Builder::new()
        .prefix("nara-graph-host-")
        .tempdir()?;
    for directory in ["assets", "scenes", "prefabs", "schema", "graphs"] {
        fs::create_dir(project.path().join(directory))?;
    }
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1
[project]
name = "Independent graph Host"
source_universe = "9c1c168c-a0f5-56b0-bd78-a38a7f3dc0e6"
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
    fs::write(project.path().join(SOURCE), INITIAL)?;
    Ok(project)
}

fn open(project: &TempDir) -> Result<Session> {
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
        Ok(session) => Ok(Session(session)),
        Err(mut error) => {
            let deadline = Instant::now() + TIMEOUT;
            while error.advance_retirement() != RuntimeCandidateRetirementState::Retired {
                ensure!(
                    Instant::now() < deadline,
                    "failed graph Host Open did not retire"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            bail!("graph Host Open failed: {:?}", error.diagnostics());
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

fn graph(editor: &EditorProjectSession, document: EditorDocumentId) -> GraphView {
    let frame = editor.current_frame();
    let projection = frame.workspace().extension_document(document).unwrap();
    assert_eq!(projection.type_id(), "consumer.value-graph");
    assert_eq!(projection.revision(), facts(editor, document).revision);
    projection.get::<ValueGraph>().unwrap().clone()
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
    assert_eq!(report.dirty, Some(true));
    assert!(report.diagnostics.is_empty());
}

fn edit(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    commands: impl IntoIterator<Item = GraphCommand>,
) {
    let before = facts(editor, document);
    let mut revision = before.revision;
    let actions: Vec<_> = commands
        .into_iter()
        .map(|command| {
            let action = EditorAction::Extension(EditorExtensionCommand::new::<ValueGraph>(
                document, revision, command,
            ));
            revision = revision.next();
            action
        })
        .collect();
    let count = actions.len();
    let outcomes = dispatch(editor, actions);
    assert_eq!(outcomes.len(), count);
    let mut expected = before.revision;
    for outcome in &outcomes {
        expected = expected.next();
        assert_applied(outcome, document, expected);
    }
    let after = facts(editor, document);
    assert_eq!(after.revision, revision);
    assert_eq!(after.history.undo_depth, before.history.undo_depth + count);
    assert_eq!(after.history.redo_depth, 0);
    assert_eq!(after.saved_revision, before.saved_revision);
}

fn persist(
    editor: &mut EditorProjectSession,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    let outcomes = dispatch(editor, [EditorAction::Persistence(action)]);
    let [EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt })] =
        outcomes.as_slice()
    else {
        panic!("persistence was not accepted: {outcomes:?}");
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let observation = editor.current_frame().persistence();
        assert_eq!(observation.attempt(), Some(*attempt));
        if let Some(result) = observation.result() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "graph persistence did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
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
        )],
    );
    assert_eq!(
        editor.current_frame().persistence(),
        EditorPersistenceView::Idle
    );
}

#[test]
fn public_host_owns_graph_commands_history_save_reopen_and_stale_refusal() -> Result<()> {
    let project = create_project()?;
    let mut owner = open(&project)?;
    let editor = &mut owner.0;
    editor.register_extension_document::<ValueGraph>()?;
    let document = editor.open_extension_document::<ValueGraph>(SOURCE)?;
    assert_eq!(ValueGraph::TYPE_ID, "consumer.value-graph");
    let initial = facts(editor, document);
    assert!(!initial.dirty);
    assert_eq!(initial.revision, initial.saved_revision);
    assert_eq!(initial.history, Default::default());
    assert!(graph(editor, document).nodes.is_empty());
    assert!(graph(editor, document).links.is_empty());
    assert_eq!(evaluate(&graph(editor, document))?, None);

    let add = |kind, position| GraphCommand::Add { kind, position };
    let connect = |from, to, input| {
        GraphCommand::Connect(Link {
            from: NodeId(from),
            to: NodeId(to),
            input,
        })
    };
    edit(
        editor,
        document,
        [
            add(NodeKind::Scalar(2.0), [0.0, 0.0]),
            add(NodeKind::Scalar(3.0), [0.0, 96.0]),
            add(NodeKind::Add, [192.0, 48.0]),
            add(NodeKind::Output, [384.0, 48.0]),
        ],
    );
    edit(
        editor,
        document,
        [connect(1, 3, 0), connect(2, 3, 1), connect(3, 4, 0)],
    );
    let saved_view: GraphView = serde_json::from_slice(SAVED)?;
    let mut connected = saved_view.clone();
    connected.nodes[0].kind = NodeKind::Scalar(2.0);
    assert_eq!(graph(editor, document), connected);
    assert_eq!(evaluate(&graph(editor, document))?, Some(5.0));
    assert_eq!(fs::read(project.path().join(SOURCE))?, INITIAL);

    edit(
        editor,
        document,
        [
            GraphCommand::Disconnect {
                to: NodeId(3),
                input: 0,
            },
            add(NodeKind::Vector([1.0, 2.0]), [0.0, 192.0]),
            add(NodeKind::Add, [192.0, 192.0]),
            connect(3, 6, 0),
        ],
    );
    for (from, to, code) in [
        (5, 3, "graph.type-mismatch"),
        (3, 3, "graph.self-link"),
        (6, 3, "graph.cycle"),
    ] {
        let before = facts(editor, document);
        let original = graph(editor, document);
        let command = EditorExtensionCommand::new::<ValueGraph>(
            document,
            before.revision,
            connect(from, to, 0),
        );
        let outcomes = dispatch(editor, [EditorAction::Extension(command)]);
        let [EditorActionOutcome::Workspace(report)] = outcomes.as_slice() else {
            panic!("invalid graph command did not reach its owner: {outcomes:?}");
        };
        assert!(!report.applied);
        assert_eq!(report.document, Some(document));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == code)
        );
        assert_eq!(facts(editor, document), before);
        assert_eq!(graph(editor, document), original);
        assert_eq!(fs::read(project.path().join(SOURCE))?, INITIAL);
    }
    edit(
        editor,
        document,
        [
            GraphCommand::Remove(NodeId(5)),
            GraphCommand::Remove(NodeId(6)),
            GraphCommand::Disconnect {
                to: NodeId(3),
                input: 1,
            },
            GraphCommand::Disconnect {
                to: NodeId(4),
                input: 0,
            },
            connect(1, 3, 0),
            connect(2, 3, 1),
            connect(3, 4, 0),
        ],
    );
    assert_eq!(graph(editor, document), connected);

    let history = facts(editor, document).history;
    edit(
        editor,
        document,
        [GraphCommand::SetScalar {
            node: NodeId(1),
            value: 7.0,
        }],
    );
    assert_eq!(graph(editor, document), saved_view);
    assert_eq!(evaluate(&graph(editor, document))?, Some(10.0));
    for (redo, expected, undo_depth, redo_depth) in [
        (false, &connected, history.undo_depth, 1),
        (true, &saved_view, history.undo_depth + 1, 0),
    ] {
        let before = facts(editor, document);
        let (document, base_revision) = (document, before.revision);
        let mutation = if redo {
            EditorDocumentMutation::Redo {
                document,
                base_revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document,
                base_revision,
            }
        };
        let outcomes = dispatch(editor, [EditorAction::Mutate(mutation)]);
        assert_eq!(outcomes.len(), 1);
        assert_applied(&outcomes[0], document, base_revision.next());
        let after = facts(editor, document);
        assert_eq!(after.history.undo_depth, undo_depth);
        assert_eq!(after.history.redo_depth, redo_depth);
        assert_eq!(&graph(editor, document), expected);
        assert_eq!(after.saved_revision, initial.saved_revision);
    }

    let captured = facts(editor, document);
    let result = persist(
        editor,
        EditorPersistenceAction::Save {
            document,
            base_revision: captured.revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = result
    else {
        panic!("graph Save did not publish: {result:?}");
    };
    assert_eq!((saved_document, revision), (document, captured.revision));
    assert_eq!(digest.encoded_bytes(), SAVED.len() as u64);
    assert_eq!(fs::read(project.path().join(SOURCE))?, SAVED);
    let saved = facts(editor, document);
    assert!(!saved.dirty);
    assert_eq!(saved.revision, captured.revision);
    assert_eq!(saved.saved_revision, captured.revision);
    assert_eq!(saved.history, captured.history);
    assert_eq!(graph(editor, document), saved_view);
    acknowledge(editor);

    edit(
        editor,
        document,
        [
            GraphCommand::SetScalar {
                node: NodeId(2),
                value: 99.0,
            },
            GraphCommand::Move {
                node: NodeId(3),
                position: [224.0, 64.0],
            },
        ],
    );
    assert!(facts(editor, document).dirty);
    assert_eq!(evaluate(&graph(editor, document))?, Some(106.0));
    assert_eq!(fs::read(project.path().join(SOURCE))?, SAVED);
    let old_revision = facts(editor, document).revision;
    let stale = EditorExtensionCommand::new::<ValueGraph>(
        document,
        old_revision,
        GraphCommand::SetScalar {
            node: NodeId(1),
            value: 999.0,
        },
    );
    let target = editor.current_frame().reopen_target();
    let result = persist(editor, EditorPersistenceAction::Reopen { target });
    let reopened = facts(editor, document);
    assert_eq!(
        result,
        EditorPersistenceResult::Opened {
            document,
            revision: reopened.revision,
            digest,
        }
    );
    assert_ne!(reopened.revision.source_id(), old_revision.source_id());
    assert_eq!(reopened.revision.generation(), 0);
    assert_eq!(reopened.saved_revision, reopened.revision);
    assert!(!reopened.dirty);
    assert_eq!(reopened.history, Default::default());
    assert_eq!(reopened.source_binding, saved.source_binding);
    assert_eq!(graph(editor, document), saved_view);
    assert_eq!(evaluate(&graph(editor, document))?, Some(10.0));
    assert_eq!(
        dispatch(editor, [EditorAction::Extension(stale)]),
        [EditorActionOutcome::Rejected(
            EditorActionRejection::StaleDocumentFact {
                document,
                expected: old_revision,
                actual: Some(reopened.revision),
            }
        ),]
    );
    assert_eq!(facts(editor, document), reopened);
    assert_eq!(graph(editor, document), saved_view);
    assert_eq!(fs::read(project.path().join(SOURCE))?, SAVED);
    acknowledge(editor);
    Ok(())
}
