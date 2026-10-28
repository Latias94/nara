//! Accepted edits of a real installed sample through an independent public document owner.

use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_engine::{
    core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision},
    diagnostic::{DiagnosticCode, SafeSummary},
    fs::{ContentDigest, RelativePath},
    project_host::EditorProjectSession,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
    EditorDocumentMutation, EditorDocumentTabModel, EditorExtensionCommand,
    EditorExtensionDocument, EditorExtensionDocumentError, EditorPersistenceAction,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorSourceToolContentOutcome,
    EditorSourceToolContentPhase, EditorWorkspaceCommandReport,
};

use super::{
    PACKAGE_A, SAMPLE, TIMEOUT, assert_content_installations, assert_content_outcome,
    assert_installed_bytes, assert_no_execution, content_installation, install_owned, original,
    project_root, settle_content, settle_selection,
};
use crate::{
    authoring_project::{Project, assert_history, close, schema_bundle},
    local_release::installed_variant,
};

const CONTENT_LIMIT: usize = 4096;
const HISTORY_LIMIT: usize = 8;

struct SetText(String);

type Snapshot = (DocumentContentState, String);

struct SampleText {
    text: String,
    clock: DocumentRevision,
    state: DocumentContentState,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

fn text_error() -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new("trial.sample-text.invalid-operation").unwrap(),
        SafeSummary::new("the sample text operation exceeds its limits or is unavailable").unwrap(),
    )
}

impl EditorExtensionDocument for SampleText {
    type Command = SetText;
    type View = String;

    const TYPE_ID: &'static str = "trial.installed-sample-text";

    fn title(&self) -> &str {
        "Installed sample"
    }

    fn content_state(&self) -> DocumentContentState {
        self.state
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        DocumentHistoryStatus {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    fn view(&self) -> Self::View {
        self.text.clone()
    }

    fn apply(&mut self, command: &SetText) -> Result<(), EditorExtensionDocumentError> {
        if command.0.len() > CONTENT_LIMIT {
            return Err(text_error());
        }
        if self.text == command.0 {
            return Ok(());
        }
        self.undo.push_back((
            self.state,
            std::mem::replace(&mut self.text, command.0.clone()),
        ));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.clock = self.clock.next();
        self.state = self.clock.content_state();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, text) = self.undo.pop_back().ok_or_else(text_error)?;
        self.redo
            .push((self.state, std::mem::replace(&mut self.text, text)));
        self.state = state;
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, text) = self.redo.pop().ok_or_else(text_error)?;
        self.undo
            .push_back((self.state, std::mem::replace(&mut self.text, text)));
        self.state = state;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        Ok(self.text.as_bytes().to_vec())
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        if bytes.len() > CONTENT_LIMIT {
            return Err(text_error());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| text_error())?;
        let clock = DocumentRevision::initial();
        Ok(Self {
            text: text.to_owned(),
            clock,
            state: clock.content_state(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }
}

fn applied_text(
    document: EditorDocumentId,
    revision: DocumentRevision,
    dirty: bool,
) -> EditorActionOutcome {
    EditorActionOutcome::Workspace(EditorWorkspaceCommandReport {
        applied: true,
        document: Some(document),
        active_document: Some(document),
        dirty: Some(dirty),
        revision: Some(revision),
        ..EditorWorkspaceCommandReport::default()
    })
}

fn assert_text_frame(editor: &EditorProjectSession, expected: &EditorDocumentTabModel, text: &str) {
    let frame = editor.current_frame();
    assert_eq!(frame.workspace().active_document, Some(expected.document));
    assert_eq!(
        frame.workspace().document(expected.document),
        Some(expected)
    );
    let view = frame
        .workspace()
        .extension_document(expected.document)
        .unwrap();
    assert_eq!(view.document(), expected.document);
    assert_eq!(view.type_id(), SampleText::TYPE_ID);
    assert_eq!(view.revision(), expected.revision);
    assert_eq!(view.get::<SampleText>().unwrap().as_str(), text);
}

fn save_sample(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    revision: DocumentRevision,
) {
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            })],
        ))
        .unwrap();
    let [EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt })] =
        report.outcomes()
    else {
        panic!("The extension source Save was not accepted: {report:?}");
    };
    let attempt = *attempt;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if frame.persistence().attempt() == Some(attempt)
            && let Some(result) = frame.persistence().result()
        {
            assert!(
                matches!(
                    result,
                    EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. }
                        if saved == document && saved_revision == revision
                ),
                "{result:?}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The extension source Save did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt }
        ),]
    );
    editor.drive_editor_frame(Duration::ZERO);
}

#[test]
fn csharp_installed_sample_edits_stay_protected_after_clean_undo_save_and_fresh_open() {
    let (_template_directory, template) = installed_variant("V1");
    let project = Project::new(
        &schema_bundle(template.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let (entry, installation) = install_owned(
        &root,
        template.release(),
        "installed-edited-sample",
        PACKAGE_A,
        None,
    );
    let mut expected = content_installation(&entry, &installation);
    let source = format!("{}/{}", entry.installed.directory, SAMPLE);
    let sample_path = RelativePath::new(&source).unwrap();
    let baseline = fs::read_to_string(project.root().join(&source)).unwrap();
    let mut installed_bytes: BTreeMap<_, _> = installation
        .members()
        .iter()
        .map(|member| {
            let path = format!("{}/{}", entry.installed.directory, member.path());
            let bytes = fs::read(project.root().join(&path)).unwrap();
            assert_eq!(ContentDigest::of_bytes(&bytes), member.digest());
            (path, bytes)
        })
        .collect();
    let unknown = format!("{}/samples/user-notes.txt", entry.installed.directory);
    let notes = b"Project notes outside the installed release inventory.\n".to_vec();
    fs::write(project.root().join(&unknown), &notes).unwrap();
    installed_bytes.insert(unknown, notes);
    let scene_bytes = fs::read(project.scene_path()).unwrap();

    let mut editor = project.open();
    let scene = editor.current_frame().workspace().active_document.unwrap();
    let scene_basis = editor.document_query_basis(scene).unwrap();
    let selection = settle_selection(&mut editor);
    let initial = settle_content(&mut editor);
    assert_eq!(initial.phase, EditorSourceToolContentPhase::Idle);
    assert!(initial.error.is_none(), "{initial:?}");
    assert!(!initial.protection_publication_pending);
    assert!(initial.installations.is_empty());
    let prepared = editor
        .prepare_source_tool_content_management(
            &initial.basis,
            entry.installed.directory.as_ref(),
            Arc::clone(&installation),
        )
        .unwrap();
    let attempt = editor.commit_source_tool_content(prepared).unwrap();
    let managed = settle_content(&mut editor);
    assert_content_outcome(&managed, &attempt, EditorSourceToolContentOutcome::Managed);
    assert!(!managed.protection_publication_pending);
    assert_content_installations(&managed.installations, std::slice::from_ref(&expected));

    editor.register_extension_document::<SampleText>().unwrap();
    let document = editor
        .open_extension_document::<SampleText>(&source)
        .unwrap();
    let opened = editor.current_frame();
    let mut tab = opened.workspace().document(document).unwrap().clone();
    assert_eq!(tab.kind, EditorDocumentKind::Extension(SampleText::TYPE_ID));
    assert_eq!(tab.title, "Installed sample");
    assert_eq!(tab.source_binding.as_ref().unwrap().locator(), source);
    assert!(!tab.dirty);
    assert_eq!(tab.saved_revision, tab.revision);
    assert_eq!(tab.history, DocumentHistoryStatus::default());
    assert_text_frame(&editor, &tab, &baseline);
    assert_eq!(editor.source_tool_content_view(), managed);
    assert_eq!(opened.source_tool().content.as_ref(), Some(&managed));
    let scene_tab = opened.workspace().document(scene).unwrap().clone();
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.source_tool_content_view(), managed);
    assert_installed_bytes(&project, &installed_bytes);
    assert_no_execution(&editor);

    let changed = "A project-authored installed sample.\n";
    assert_ne!(changed, baseline);
    let revision = tab.revision;
    let frame_revision = editor.current_frame().revision();
    // No frame drive between these actions: the final clean state must retain the first edit.
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame_revision,
            [
                EditorAction::Extension(EditorExtensionCommand::new::<SampleText>(
                    document,
                    revision,
                    SetText(changed.into()),
                )),
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision.next(),
                }),
            ],
        ))
        .unwrap();
    assert_eq!(report.frame_revision(), frame_revision);
    assert_eq!(
        report.outcomes(),
        &[
            applied_text(document, revision.next(), true),
            applied_text(document, revision.next().next(), false),
        ]
    );
    expected
        .members
        .iter_mut()
        .find(|member| member.path.as_ref() == SAMPLE)
        .unwrap()
        .modified = true;
    let immediate = editor.source_tool_content_view();
    assert_content_installations(&immediate.installations, std::slice::from_ref(&expected));
    assert_ne!(immediate.basis, managed.basis);
    assert_eq!(
        immediate.phase,
        EditorSourceToolContentPhase::ProtectionPending
    );
    assert!(immediate.protection_publication_pending);
    assert!(!immediate.physical_work_pending);
    assert!(immediate.error.is_none());
    assert_eq!(immediate.operation, managed.operation);
    assert_installed_bytes(&project, &installed_bytes);

    let deadline = Instant::now() + TIMEOUT;
    let protected = loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let view = frame.source_tool().content.as_ref().unwrap();
        assert_eq!(view, &editor.source_tool_content_view());
        assert!(view.error.is_none(), "{view:?}");
        assert_eq!(view.operation, managed.operation);
        assert_content_installations(&view.installations, std::slice::from_ref(&expected));
        if view.phase == EditorSourceToolContentPhase::Idle
            && !view.protection_publication_pending
            && !view.physical_work_pending
        {
            break view.clone();
        }
        assert!(
            Instant::now() < deadline,
            "Accepted sample edit protection did not settle: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_content_outcome(
        &protected,
        &attempt,
        EditorSourceToolContentOutcome::Managed,
    );
    tab.revision = revision.next().next();
    tab.history.redo_depth = 1;
    assert_text_frame(&editor, &tab, &baseline);
    assert_eq!(
        editor.current_frame().workspace().document(scene),
        Some(&scene_tab)
    );
    assert_history(&editor, scene, 0, 0);

    let revision = tab.revision;
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Redo {
                document,
                base_revision: revision,
            })],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[applied_text(document, revision.next(), true)]
    );
    editor.drive_editor_frame(Duration::ZERO);
    tab.revision = revision.next();
    tab.dirty = true;
    tab.history = DocumentHistoryStatus {
        undo_depth: 1,
        redo_depth: 0,
    };
    assert_text_frame(&editor, &tab, changed);
    assert_eq!(editor.source_tool_content_view(), protected);
    assert_installed_bytes(&project, &installed_bytes);

    let original_file = root.open_file(&sample_path).unwrap();
    save_sample(&mut editor, document, tab.revision);
    let saved_file = root.open_file(&sample_path).unwrap();
    assert!(!original_file.same_object(&saved_file).unwrap());
    installed_bytes.insert(source.clone(), changed.as_bytes().to_vec());
    tab.saved_revision = tab.revision;
    tab.dirty = false;
    assert_text_frame(&editor, &tab, changed);
    assert_installed_bytes(&project, &installed_bytes);
    assert_eq!(editor.source_tool_content_view(), protected);
    assert_eq!(
        editor.current_frame().workspace().document(scene),
        Some(&scene_tab)
    );
    drop(original_file);
    drop(saved_file);

    let revision = tab.revision;
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: revision,
            })],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[applied_text(document, revision.next(), true)]
    );
    editor.drive_editor_frame(Duration::ZERO);
    tab.revision = revision.next();
    tab.dirty = true;
    tab.history = DocumentHistoryStatus {
        undo_depth: 0,
        redo_depth: 1,
    };
    assert_text_frame(&editor, &tab, &baseline);
    assert_eq!(editor.source_tool_content_view(), protected);
    save_sample(&mut editor, document, tab.revision);
    installed_bytes.insert(source.clone(), baseline.as_bytes().to_vec());
    tab.saved_revision = tab.revision;
    tab.dirty = false;
    assert_text_frame(&editor, &tab, &baseline);
    assert_installed_bytes(&project, &installed_bytes);
    assert_eq!(editor.source_tool_content_view(), protected);
    assert_eq!(editor.source_tool_selection_view(), selection);
    assert_eq!(editor.document_query_basis(scene).unwrap(), scene_basis);
    assert_eq!(
        editor.current_frame().workspace().document(scene),
        Some(&scene_tab)
    );
    assert_history(&editor, scene, 0, 0);
    assert_eq!(project.read(), original());
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_no_execution(&editor);
    close(&mut editor);
    drop(editor);
    drop(installation);

    let mut reopened = project.open();
    let scene = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    let reopened_selection = settle_selection(&mut reopened);
    assert!(reopened_selection.installed.is_empty());
    assert!(reopened_selection.desired.is_none());
    assert!(reopened_selection.active_origin.is_none());
    let content = settle_content(&mut reopened);
    assert_eq!(content.phase, EditorSourceToolContentPhase::Idle);
    assert!(content.error.is_none(), "{content:?}");
    assert!(!content.protection_publication_pending);
    assert!(!content.physical_work_pending);
    assert!(matches!(
        content.operation.as_ref().unwrap().outcome,
        Some(EditorSourceToolContentOutcome::Observed)
    ));
    expected.creation_custody = false;
    assert_content_installations(&content.installations, std::slice::from_ref(&expected));
    reopened
        .register_extension_document::<SampleText>()
        .unwrap();
    let fresh_document = reopened
        .open_extension_document::<SampleText>(&source)
        .unwrap();
    let fresh = reopened.current_frame();
    let fresh_tab = fresh.workspace().document(fresh_document).unwrap();
    assert_ne!(fresh_tab.revision.source_id(), tab.revision.source_id());
    assert_eq!(fresh_tab.saved_revision, fresh_tab.revision);
    assert!(!fresh_tab.dirty);
    assert_eq!(fresh_tab.history, DocumentHistoryStatus::default());
    assert_eq!(
        fresh_tab.kind,
        EditorDocumentKind::Extension(SampleText::TYPE_ID)
    );
    assert_eq!(fresh_tab.source_binding.as_ref().unwrap().locator(), source);
    assert_text_frame(&reopened, fresh_tab, &baseline);
    assert_eq!(fresh.source_tool().content.as_ref(), Some(&content));
    assert_eq!(reopened.source_tool_content_view(), content);
    assert_history(&reopened, scene, 0, 0);
    assert_eq!(project.read(), original());
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_installed_bytes(&project, &installed_bytes);
    assert_no_execution(&reopened);
    close(&mut reopened);
    assert_installed_bytes(&project, &installed_bytes);
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
}
