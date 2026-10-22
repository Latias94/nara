use std::{
    fs,
    io::Write,
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

use nara::project_host::{
    EditorExtensionDocumentHostError, EditorProjectCloseProgress, EditorProjectOpenHost,
    EditorProjectOpenProgress, EditorProjectSession,
};
use nara_editor_panel_module_consumer::dialogue::{Dialogue, SetLine};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision, EditorDocumentId,
    EditorDocumentMutation, EditorExtensionCommand, EditorExtensionDocument,
    EditorExtensionDocumentError, EditorNavigationAction, EditorPersistenceAction,
    EditorPersistenceRejection, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult,
};

#[path = "support/dialogue_project.rs"]
mod dialogue_project;
use dialogue_project::DialogueProject;

const SOURCE: &str = "dialogue/intro.json";

// Shares both the public name and view type, but not the Rust owner identity.
struct OtherDialogue(Dialogue);
impl EditorExtensionDocument for OtherDialogue {
    type Command = SetLine;
    type View = <Dialogue as EditorExtensionDocument>::View;
    const TYPE_ID: &'static str = Dialogue::TYPE_ID;
    fn title(&self) -> &str {
        self.0.title()
    }
    fn content_state(&self) -> nara::core::DocumentContentState {
        self.0.content_state()
    }
    fn history_status(&self) -> nara::core::DocumentHistoryStatus {
        self.0.history_status()
    }
    fn view(&self) -> Self::View {
        self.0.view()
    }
    fn apply(&mut self, command: &SetLine) -> Result<(), EditorExtensionDocumentError> {
        self.0.apply(command)
    }
    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        self.0.undo()
    }
    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        self.0.redo()
    }
    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        Err(EditorExtensionDocumentError::new(
            nara::diagnostic::DiagnosticCode::new("dialogue.test-encode-failure").unwrap(),
            nara::diagnostic::SafeSummary::new("dialogue encoding was intentionally refused")
                .unwrap(),
        ))
    }
    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        Dialogue::decode(bytes).map(Self)
    }
}

struct Session(EditorProjectSession);
impl Deref for Session {
    type Target = EditorProjectSession;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        close_session(self);
    }
}

fn close_session(editor: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(
        editor.0.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline, "document Host did not retire");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open(project: &DialogueProject) -> Session {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.capability(), Some("desktop".to_owned()));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Opened(session) => return Session(session),
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Rejected(failure) => panic!("{:?}", failure.diagnostics()),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn dispatch(
    editor: &mut EditorProjectSession,
    actions: impl IntoIterator<Item = EditorAction>,
) -> Vec<EditorActionOutcome> {
    let frame = editor.current_frame();
    let outcomes = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), actions))
        .unwrap()
        .into_outcomes();
    editor.drive_editor_frame(Duration::ZERO);
    outcomes
}

fn edit_action(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    text: &str,
) -> EditorAction {
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    EditorAction::Extension(EditorExtensionCommand::new::<Dialogue>(
        document,
        revision,
        SetLine {
            index: 0,
            text: text.to_owned(),
        },
    ))
}

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, text: &str) {
    let action = edit_action(editor, document, text);
    assert!(
        matches!(dispatch(editor, [action]).as_slice(), [EditorActionOutcome::Workspace(report)] if report.applied)
    );
}

fn line(editor: &EditorProjectSession, document: EditorDocumentId) -> String {
    editor
        .current_frame()
        .workspace()
        .extension_document(document)
        .unwrap()
        .get::<Dialogue>()
        .unwrap()
        .lines[0]
        .clone()
}

fn settle(editor: &mut EditorProjectSession) -> EditorPersistenceResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = editor
            .drive_editor_frame(Duration::ZERO)
            .persistence()
            .result()
        {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "dialogue persistence did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn acknowledge(editor: &mut EditorProjectSession) {
    dispatch(
        editor,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
}

fn wait_for_browser_entry(editor: &mut EditorProjectSession, path: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while editor
        .current_frame()
        .project_browser()
        .entry(path)
        .is_none()
    {
        assert!(Instant::now() < deadline, "the source index did not settle");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_browser_document(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    wait_for_browser_entry(editor, path);
    let outcomes = dispatch(
        editor,
        [EditorAction::ProjectBrowser(
            nara_tooling::EditorProjectBrowserAction::Open {
                relative_path: path.into(),
            },
        )],
    );
    let [
        EditorActionOutcome::ProjectBrowser(
            nara_tooling::EditorProjectBrowserActionOutcome::Preparing { request },
        ),
    ] = outcomes.as_slice()
    else {
        panic!("ordinary Open was not admitted: {outcomes:?}");
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if let Some(result) = frame.document_open().result() {
            let nara_tooling::EditorDocumentOpenResult::Opened {
                request: completed,
                document,
            } = result
            else {
                panic!("ordinary Open did not publish: {result:?}");
            };
            assert_eq!(completed, *request);
            dispatch(
                editor,
                [EditorAction::ProjectBrowser(
                    nara_tooling::EditorProjectBrowserAction::AcknowledgeOpen { request: *request },
                )],
            );
            return document;
        }
        assert!(Instant::now() < deadline, "ordinary Open did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn extension_document_rejects_an_actual_file_alias() {
    let project = DialogueProject::new();
    let alias = "dialogue/alias.json";
    fs::hard_link(project.root.join(SOURCE), project.root.join(alias)).unwrap();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    let before = editor.current_frame();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    assert_eq!(editor.current_frame().workspace(), before.workspace());
    assert_eq!(line(&editor, document), "Welcome");
}

#[test]
fn scene_document_rejects_extension_open_through_an_actual_file_alias() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    let alias = "dialogue/scene-alias.json";
    fs::hard_link(
        project.root.join("worlds/entry.scene.json"),
        project.root.join(alias),
    )
    .unwrap();
    // Valid dialogue content makes duplicate ownership observable independently of codec refusal.
    fs::write(project.root.join(alias), br#"["Scene source"]"#).unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    let before = editor.current_frame();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    assert_eq!(editor.current_frame().workspace(), before.workspace());
}

#[test]
fn prefab_document_rejects_extension_open_through_an_actual_file_alias() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    let path = "blueprints/external.prefab.json";
    let document = open_browser_document(&mut editor, path);
    let alias = "dialogue/prefab-alias.json";
    fs::hard_link(project.root.join(path), project.root.join(alias)).unwrap();
    fs::write(project.root.join(alias), br#"["Prefab source"]"#).unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    let before = editor.current_frame();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    assert_eq!(editor.current_frame().workspace(), before.workspace());
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .is_some()
    );
}

#[test]
fn ordinary_document_open_rejects_an_actual_alias_owned_by_an_extension() {
    for path in [
        "worlds/external.scene.json",
        "blueprints/external.prefab.json",
    ] {
        let project = DialogueProject::new();
        let alias = "dialogue/document-alias.json";
        fs::hard_link(project.root.join(path), project.root.join(alias)).unwrap();
        let mut editor = open(&project);
        wait_for_browser_entry(&mut editor, path);
        fs::write(project.root.join(alias), br#"["Dialogue source"]"#).unwrap();
        editor.register_extension_document::<Dialogue>().unwrap();
        let document = editor.open_extension_document::<Dialogue>(alias).unwrap();
        let before = editor.current_frame();
        let outcomes = dispatch(
            &mut editor,
            [EditorAction::ProjectBrowser(
                nara_tooling::EditorProjectBrowserAction::Open {
                    relative_path: path.into(),
                },
            )],
        );
        assert!(matches!(
            outcomes.as_slice(),
            [EditorActionOutcome::ProjectBrowser(
                nara_tooling::EditorProjectBrowserActionOutcome::Rejected {
                    code: "project.editor.document-source-already-open"
                },
            )]
        ));
        assert_eq!(editor.current_frame().workspace(), before.workspace());
        assert_eq!(line(&editor, document), "Dialogue source");
    }
}

#[test]
fn extension_alias_checks_follow_retained_scene_and_prefab_parent_directories() {
    for (directory, file_name) in [
        ("worlds", "entry.scene.json"),
        ("blueprints", "external.prefab.json"),
    ] {
        let project = DialogueProject::new();
        let mut editor = open(&project);
        if directory == "blueprints" {
            open_browser_document(&mut editor, &format!("{directory}/{file_name}"));
        }
        let moved = format!("{directory}-moved");
        fs::rename(project.root.join(directory), project.root.join(&moved)).unwrap();
        fs::create_dir(project.root.join(directory)).unwrap();
        fs::write(
            project.root.join(directory).join(file_name),
            br#"["Different source at the old path"]"#,
        )
        .unwrap();
        let alias = format!("{moved}/{file_name}");
        fs::write(project.root.join(&alias), br#"["Retained source"]"#).unwrap();
        editor.register_extension_document::<Dialogue>().unwrap();
        assert_eq!(
            editor.open_extension_document::<Dialogue>(&alias),
            Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
        );
    }
}

#[cfg(windows)]
#[test]
fn scene_document_rejects_extension_open_through_a_case_alias() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    let alias = "WORLDS/ENTRY.SCENE.JSON";
    if !project.root.join(alias).exists() {
        return;
    }
    fs::write(project.root.join(alias), br#"["Scene source"]"#).unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
}

#[cfg(windows)]
#[test]
fn ordinary_document_open_rejects_a_case_alias_owned_by_an_extension() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    let path = "worlds/external.scene.json";
    let alias = "WORLDS/EXTERNAL.SCENE.JSON";
    if !project.root.join(alias).exists() {
        return;
    }
    wait_for_browser_entry(&mut editor, path);
    fs::write(project.root.join(alias), br#"["Dialogue source"]"#).unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(alias).unwrap();
    let outcomes = dispatch(
        &mut editor,
        [EditorAction::ProjectBrowser(
            nara_tooling::EditorProjectBrowserAction::Open {
                relative_path: path.into(),
            },
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::ProjectBrowser(
            nara_tooling::EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-source-already-open"
            },
        )]
    ));
    assert_eq!(line(&editor, document), "Dialogue source");
}

#[cfg(windows)]
#[test]
fn extension_document_rejects_case_aliases_before_and_after_save() {
    let project = DialogueProject::new();
    let root = project.capability();
    let source = root
        .open_file(&nara::fs::RelativePath::new(SOURCE).unwrap())
        .unwrap();
    let alias = "DIALOGUE/INTRO.JSON";
    let alias_file = match root.open_file(&nara::fs::RelativePath::new(alias).unwrap()) {
        Ok(file) => file,
        Err(nara::fs::FsError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            return;
        }
        Err(error) => panic!("case alias could not be inspected: {error:?}"),
    };
    assert_eq!(source.identity(), alias_file.identity());
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    edit(&mut editor, document, "Saved through a case alias");
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        })],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Saved { .. }
    ));
    acknowledge(&mut editor);
    assert_eq!(
        editor.open_extension_document::<Dialogue>(alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
}

#[test]
fn differently_cased_distinct_files_keep_independent_document_owners() {
    let project = DialogueProject::new();
    let other = "dialogue/INTRO.JSON";
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(project.root.join(other))
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return,
        Err(error) => panic!("distinct case-sensitive source could not be created: {error}"),
    };
    file.write_all(br#"["Independent dialogue"]"#).unwrap();
    drop(file);
    let root = project.capability();
    let source = root
        .open_file(&nara::fs::RelativePath::new(SOURCE).unwrap())
        .unwrap();
    let other_file = root
        .open_file(&nara::fs::RelativePath::new(other).unwrap())
        .unwrap();
    assert_ne!(source.identity(), other_file.identity());
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let first = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    let second = editor.open_extension_document::<Dialogue>(other).unwrap();
    assert_ne!(first, second);
    assert_eq!(line(&editor, first), "Welcome");
    assert_eq!(line(&editor, second), "Independent dialogue");
}

#[test]
fn extension_alias_checks_follow_atomic_save_replacement() {
    let project = DialogueProject::new();
    let previous_alias = "dialogue/before-save.json";
    let current_alias = "dialogue/after-save.json";
    fs::hard_link(project.root.join(SOURCE), project.root.join(previous_alias)).unwrap();
    let root = project.capability();
    let original = root
        .open_file(&nara::fs::RelativePath::new(SOURCE).unwrap())
        .unwrap();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    edit(&mut editor, document, "Saved replacement");
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        })],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Saved { .. }
    ));
    acknowledge(&mut editor);
    let replacement = root
        .open_file(&nara::fs::RelativePath::new(SOURCE).unwrap())
        .unwrap();
    assert_ne!(original.identity(), replacement.identity());
    fs::hard_link(project.root.join(SOURCE), project.root.join(current_alias)).unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(current_alias),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    let previous = editor
        .open_extension_document::<Dialogue>(previous_alias)
        .unwrap();
    assert_eq!(line(&editor, previous), "Welcome");
    assert_eq!(line(&editor, document), "Saved replacement");
}

#[test]
fn exact_extension_source_remains_reserved_after_external_replacement_or_deletion() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    let replacement = project.root.join("dialogue/replacement.json");
    fs::write(&replacement, br#"["External replacement"]"#).unwrap();
    fs::remove_file(project.root.join(SOURCE)).unwrap();
    fs::rename(&replacement, project.root.join(SOURCE)).unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(SOURCE),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    fs::remove_file(project.root.join(SOURCE)).unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>(SOURCE),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    assert_eq!(line(&editor, document), "Welcome");
}

#[test]
fn extension_source_names_do_not_alias_scene_relative_locators() {
    let project = DialogueProject::new();
    fs::write(
        project.root.join("entry.scene.json"),
        br#"["Independent dialogue"]"#,
    )
    .unwrap();
    let mut editor = open(&project);
    let scene = editor.current_frame().workspace().active_document.unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor
        .open_extension_document::<Dialogue>("entry.scene.json")
        .unwrap();
    assert_ne!(document, scene);
    assert_eq!(line(&editor, document), "Independent dialogue");
    assert!(editor.current_frame().workspace().document(scene).is_some());
    assert_eq!(
        editor.open_extension_document::<Dialogue>("entry.scene.json"),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    assert_eq!(
        editor.open_extension_document::<Dialogue>("worlds/entry.scene.json"),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
}

#[test]
fn ordinary_document_open_rejects_a_source_owned_by_an_extension() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    let deadline = Instant::now() + Duration::from_secs(10);
    while editor
        .current_frame()
        .project_browser()
        .entry("worlds/external.scene.json")
        .is_none()
    {
        assert!(Instant::now() < deadline, "the source index did not settle");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    // An explicitly chosen document owner can recognize a source independently of its suffix.
    fs::write(
        project.root.join("worlds/external.scene.json"),
        br#"["Dialogue source"]"#,
    )
    .unwrap();
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor
        .open_extension_document::<Dialogue>("worlds/external.scene.json")
        .unwrap();
    let before = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    fs::remove_file(project.root.join("worlds/external.scene.json")).unwrap();
    fs::write(
        project.root.join("worlds/external.scene.json"),
        br#"["External replacement"]"#,
    )
    .unwrap();
    let outcomes = dispatch(
        &mut editor,
        [EditorAction::ProjectBrowser(
            nara_tooling::EditorProjectBrowserAction::Open {
                relative_path: "worlds/external.scene.json".into(),
            },
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::ProjectBrowser(
            nara_tooling::EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-source-already-open"
            },
        )]
    ));
    assert_eq!(
        editor.current_frame().workspace().document(document),
        Some(&before)
    );
}

#[test]
fn dialogue_uses_host_history_save_reopen_and_rejects_stale_completion() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    assert_eq!(
        editor.open_extension_document::<Dialogue>(SOURCE),
        Err(EditorExtensionDocumentHostError::UnregisteredType)
    );
    editor.register_extension_document::<Dialogue>().unwrap();
    assert_eq!(
        editor.register_extension_document::<Dialogue>(),
        Err(EditorExtensionDocumentHostError::DuplicateTypeId)
    );
    fs::write(
        project.root.join("dialogue/invalid.json"),
        b"private invalid dialogue source",
    )
    .unwrap();
    let error = editor
        .open_extension_document::<Dialogue>("dialogue/invalid.json")
        .unwrap_err();
    assert!(
        matches!(error, EditorExtensionDocumentHostError::Decode(error)
        if error.code().as_str() == "dialogue.decode")
    );
    assert!(!format!("{error:?}").contains("private invalid dialogue source"));
    assert_eq!(
        editor.open_extension_document::<Dialogue>("../intro.json"),
        Err(EditorExtensionDocumentHostError::InvalidSource)
    );
    let scene = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor.open_extension_document::<Dialogue>("worlds/entry.scene.json"),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    assert_eq!(
        editor.register_extension_document::<OtherDialogue>(),
        Err(EditorExtensionDocumentHostError::DuplicateTypeId)
    );
    assert!(
        editor
            .current_frame()
            .workspace()
            .extension_document(document)
            .unwrap()
            .get::<OtherDialogue>()
            .is_none()
    );
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    let wrong_owner = EditorAction::Extension(EditorExtensionCommand::new::<OtherDialogue>(
        document,
        revision,
        SetLine {
            index: 0,
            text: "Wrong owner".to_owned(),
        },
    ));
    assert!(
        matches!(dispatch(&mut editor, [wrong_owner]).as_slice(), [EditorActionOutcome::Workspace(report)] if !report.applied)
    );
    assert_eq!(line(&editor, document), "Welcome");
    assert_eq!(
        editor.open_extension_document::<Dialogue>(SOURCE),
        Err(EditorExtensionDocumentHostError::SourceAlreadyOpen)
    );
    dispatch(
        &mut editor,
        [EditorAction::Navigate(
            EditorNavigationAction::SetActiveDocument { document: scene },
        )],
    );
    let navigated = dispatch(
        &mut editor,
        [EditorAction::Navigate(
            EditorNavigationAction::SetActiveDocument { document },
        )],
    );
    assert!(
        matches!(navigated.as_slice(), [EditorActionOutcome::Workspace(report)] if report.applied)
    );
    assert_eq!(line(&editor, document), "Welcome");
    let before = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    let invalid = EditorAction::Extension(EditorExtensionCommand::new::<Dialogue>(
        document,
        before.revision,
        SetLine {
            index: 99,
            text: "Invalid".to_owned(),
        },
    ));
    assert!(
        matches!(dispatch(&mut editor, [invalid]).as_slice(), [EditorActionOutcome::Workspace(report)] if !report.applied)
    );
    assert_eq!(
        editor.current_frame().workspace().document(document),
        Some(&before)
    );
    edit(&mut editor, document, "First edit");
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision: revision,
        })],
    );
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::Redo {
            document,
            base_revision: revision,
        })],
    );
    assert_eq!(line(&editor, document), "First edit");
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    let later = edit_action(&editor, document, "Later edit");
    let outcomes = dispatch(
        &mut editor,
        [
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            }),
            later,
        ],
    );
    assert!(matches!(
        outcomes[0],
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    assert!(
        matches!(settle(&mut editor), EditorPersistenceResult::Saved { revision: saved, .. } if saved == revision)
    );
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        fs::read(project.root.join(SOURCE)).unwrap(),
        br#"["First edit","Goodbye"]"#
    );
    acknowledge(&mut editor);

    // Both actions are admitted before a drive starts the read: no timing assumption is needed.
    let target = editor.current_frame().reopen_target();
    let later = edit_action(&editor, document, "Edit after Reopen");
    dispatch(
        &mut editor,
        [
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
            later,
        ],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Rejected {
            reason: EditorPersistenceRejection::StaleRevision,
            ..
        }
    ));
    assert_eq!(line(&editor, document), "Edit after Reopen");
    acknowledge(&mut editor);
    let stale = edit_action(&editor, document, "Stale incarnation");
    let old_revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    let target = editor.current_frame().reopen_target();
    dispatch(
        &mut editor,
        [EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target,
        })],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { .. }
    ));
    let frame = editor.current_frame();
    let facts = frame.workspace().document(document).unwrap();
    assert_ne!(facts.revision.source_id(), old_revision.source_id());
    assert!(!facts.dirty);
    assert_eq!(facts.history.undo_depth, 0);
    assert_eq!(line(&editor, document), "First edit");
    assert!(matches!(
        dispatch(&mut editor, [stale]).as_slice(),
        [EditorActionOutcome::Rejected(_)]
    ));
    acknowledge(&mut editor);

    // Undo/Redo returns to the saved state; a new branch must receive a different content identity.
    edit(&mut editor, document, "Branch A");
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        })],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Saved { .. }
    ));
    acknowledge(&mut editor);
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision: revision,
        })],
    );
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::Redo {
            document,
            base_revision: revision,
        })],
    );
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision: revision,
        })],
    );
    edit(&mut editor, document, "Branch B");
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .redo_depth,
        0
    );
    acknowledge(&mut editor);
    let before = editor.current_frame();
    fs::write(project.root.join(SOURCE), b"not dialogue JSON").unwrap();
    let target = before.reopen_target();
    dispatch(
        &mut editor,
        [EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target,
        })],
    );
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Failed {
            stage: nara_tooling::EditorPersistenceFailureStage::Decode,
            ..
        }
    ));
    assert!(
        editor
            .current_frame()
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "dialogue.decode")
    );
    assert_eq!(
        editor.current_frame().workspace().document(document),
        before.workspace().document(document)
    );
    assert_eq!(line(&editor, document), "Branch B");
}

#[test]
fn dialogue_close_cancel_discard_save_and_failed_save_keep_host_custody() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    edit(&mut editor, document, "Unsaved");
    let before = editor.current_frame();
    close_decision(&mut editor, document, EditorCloseDecision::Cancel);
    assert_eq!(
        editor.current_frame().workspace().document(document),
        before.workspace().document(document)
    );
    assert!(matches!(
        editor.current_frame().workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Cancelled { .. })
    ));
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    close_decision(&mut editor, document, EditorCloseDecision::Discard);
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .is_none()
    );
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    assert_eq!(line(&editor, document), "Welcome");
    edit(&mut editor, document, "Saved on close");
    close_decision(&mut editor, document, EditorCloseDecision::Save);
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Saved { .. }
    ));
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .is_none()
    );
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    acknowledge(&mut editor);
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    assert_eq!(line(&editor, document), "Saved on close");
    edit(&mut editor, document, "Must survive conflict");
    fs::write(project.root.join(SOURCE), br#"["External writer"]"#).unwrap();
    close_decision(&mut editor, document, EditorCloseDecision::Save);
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Rejected {
            reason: EditorPersistenceRejection::TargetChanged,
            ..
        }
    ));
    assert_eq!(line(&editor, document), "Must survive conflict");
    let prompt = editor
        .current_frame()
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            prompt.resolve(EditorCloseDecision::Cancel),
        )],
    );
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
}

fn close_decision(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    decision: EditorCloseDecision,
) {
    let outcomes = dispatch(
        editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument { document }),
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::WorkspaceIntent(
            EditorWorkspaceIntentRequestResult::Accepted
        )]
    ));
    let prompt = editor
        .current_frame()
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    assert_eq!(prompt.document(), document);
    dispatch(
        editor,
        [EditorAction::WorkspaceIntent(prompt.resolve(decision))],
    );
}

#[test]
fn dialogue_encoding_failure_keeps_dirty_content_and_returns_to_the_close_prompt() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    editor
        .register_extension_document::<OtherDialogue>()
        .unwrap();
    let document = editor
        .open_extension_document::<OtherDialogue>(SOURCE)
        .unwrap();
    let revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    let action = EditorAction::Extension(EditorExtensionCommand::new::<OtherDialogue>(
        document,
        revision,
        SetLine {
            index: 0,
            text: "Must remain dirty".to_owned(),
        },
    ));
    assert!(
        matches!(dispatch(&mut editor, [action]).as_slice(), [EditorActionOutcome::Workspace(report)] if report.applied)
    );
    let before = editor.current_frame();
    close_decision(&mut editor, document, EditorCloseDecision::Save);
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Failed {
            stage: nara_tooling::EditorPersistenceFailureStage::Encode,
            ..
        }
    ));
    assert_eq!(
        editor.current_frame().workspace().document(document),
        before.workspace().document(document)
    );
    assert!(
        editor
            .current_frame()
            .workspace_intent()
            .decision_prompt()
            .is_some()
    );
    assert!(
        editor
            .current_frame()
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "dialogue.test-encode-failure")
    );
    assert_eq!(
        fs::read(project.root.join(SOURCE)).unwrap(),
        br#"["Welcome","Goodbye"]"#
    );
}

#[test]
fn whole_session_close_retires_extension_publication_custody_after_dirty_exit_discard() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    edit(&mut editor, document, "Discard on exit");
    let before = editor.current_frame();
    assert!(before.workspace().document(document).unwrap().dirty);

    let outcomes = dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::WorkspaceIntent(
            EditorWorkspaceIntentRequestResult::Accepted
        )]
    ));
    let prompt = editor
        .current_frame()
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            prompt.resolve(EditorCloseDecision::Discard),
        )],
    );
    assert_eq!(
        editor.current_frame().workspace_intent().phase(),
        Some(nara_tooling::EditorWorkspaceIntentPhase::Prepared)
    );

    // A complete session close is the retirement receipt for every extension FilePublicationHost.
    close_session(&mut editor);
    assert_eq!(
        editor.current_frame().workspace().document(document),
        before.workspace().document(document)
    );
    assert_eq!(
        fs::read(project.root.join(SOURCE)).unwrap(),
        br#"["Welcome","Goodbye"]"#
    );
    assert_eq!(
        editor.register_extension_document::<Dialogue>(),
        Err(EditorExtensionDocumentHostError::Busy)
    );
}

#[test]
fn whole_session_close_saves_dirty_extension_before_retiring_publication_custody() {
    let project = DialogueProject::new();
    let mut editor = open(&project);
    editor.register_extension_document::<Dialogue>().unwrap();
    let document = editor.open_extension_document::<Dialogue>(SOURCE).unwrap();
    edit(&mut editor, document, "Save on exit");

    let outcomes = dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::WorkspaceIntent(
            EditorWorkspaceIntentRequestResult::Accepted
        )]
    ));
    let prompt = editor
        .current_frame()
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            prompt.resolve(EditorCloseDecision::Save),
        )],
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while editor.current_frame().workspace_intent().phase()
        != Some(nara_tooling::EditorWorkspaceIntentPhase::Prepared)
    {
        assert!(
            Instant::now() < deadline,
            "dirty exit did not finish its Save"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        fs::read(project.root.join(SOURCE)).unwrap(),
        br#"["Save on exit","Goodbye"]"#
    );
    let outcomes = dispatch(
        &mut editor,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::CommitPrepared,
        )],
    );
    assert!(matches!(
        outcomes.as_slice(),
        [EditorActionOutcome::WorkspaceIntent(
            EditorWorkspaceIntentRequestResult::Accepted
        )]
    ));
    close_session(&mut editor);
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        editor.register_extension_document::<Dialogue>(),
        Err(EditorExtensionDocumentHostError::Busy)
    );
}
