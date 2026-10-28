//! Real extension-document edits of an installed Release content member.

use std::collections::VecDeque;

use nara_core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision};
use nara_diagnostic::{DiagnosticCode, SafeSummary};
use nara_tooling::{
    EditorActionAcknowledgement, EditorActionRejection, EditorCloseDecision, EditorDocumentId,
    EditorDocumentKind, EditorDocumentQueryRejection, EditorDocumentTabModel, EditorEntityQuery,
    EditorEntityQueryLimits, EditorEntityQueryPosition, EditorExtensionCommand,
    EditorExtensionDocument, EditorExtensionDocumentError, EditorNavigationAction,
    EditorPersistenceRejection, EditorPersistenceRequestKind, EditorReopenTarget,
    EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentRejection,
    EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
};

use super::*;
use crate::project_host::EditorExtensionDocumentHostError;

const SOURCE: &str = "installed-stats/samples/readme.txt";
const CASE_SOURCE: &str = "INSTALLED-STATS/SAMPLES/README.TXT";
const TEXT_LIMIT: usize = 1_024;
const HISTORY_LIMIT: usize = 8;

struct SetText(String);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextView {
    text: String,
    state: DocumentContentState,
}

struct SampleText {
    current: TextView,
    clock: DocumentRevision,
    undo: VecDeque<TextView>,
    redo: Vec<TextView>,
}

fn text_error() -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new("content.test-text-refused").unwrap(),
        SafeSummary::new("The bounded sample text operation was refused").unwrap(),
    )
}

impl EditorExtensionDocument for SampleText {
    type Command = SetText;
    type View = TextView;
    const TYPE_ID: &'static str = "nara.test.ManagedSampleText";

    fn title(&self) -> &str {
        "Installed sample text"
    }

    fn content_state(&self) -> DocumentContentState {
        self.current.state
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        DocumentHistoryStatus {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    fn view(&self) -> Self::View {
        self.current.clone()
    }

    fn apply(&mut self, command: &SetText) -> Result<(), EditorExtensionDocumentError> {
        if command.0.len() > TEXT_LIMIT {
            return Err(text_error());
        }
        if command.0 == self.current.text {
            return Ok(());
        }
        self.undo.push_back(self.current.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.clock = self.clock.next();
        self.current = TextView {
            text: command.0.clone(),
            state: self.clock.content_state(),
        };
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let previous = self.undo.pop_back().ok_or_else(text_error)?;
        self.redo
            .push(std::mem::replace(&mut self.current, previous));
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let next = self.redo.pop().ok_or_else(text_error)?;
        self.undo
            .push_back(std::mem::replace(&mut self.current, next));
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        Ok(self.current.text.as_bytes().to_vec())
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        if bytes.len() > TEXT_LIMIT {
            return Err(text_error());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| text_error())?;
        let clock = DocumentRevision::initial();
        Ok(Self {
            current: TextView {
                text: text.to_owned(),
                state: clock.content_state(),
            },
            clock,
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSnapshot {
    tab: EditorDocumentTabModel,
    view: TextView,
}

fn text_snapshot(editor: &EditorProjectSession, document: EditorDocumentId) -> TextSnapshot {
    let frame = editor.current_frame();
    TextSnapshot {
        tab: frame.workspace().document(document).unwrap().clone(),
        view: frame
            .workspace()
            .extension_document(document)
            .unwrap()
            .get::<SampleText>()
            .unwrap()
            .clone(),
    }
}

fn dispatch(
    editor: &mut EditorProjectSession,
    actions: impl IntoIterator<Item = EditorAction>,
) -> Vec<EditorActionOutcome> {
    let outcomes = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            actions,
        ))
        .unwrap()
        .into_outcomes();
    editor.drive_editor_frame(Duration::ZERO);
    outcomes
}

fn set_text(document: EditorDocumentId, revision: DocumentRevision, text: &str) -> EditorAction {
    EditorAction::Extension(EditorExtensionCommand::new::<SampleText>(
        document,
        revision,
        SetText(text.to_owned()),
    ))
}

fn assert_applied(
    outcome: &EditorActionOutcome,
    document: EditorDocumentId,
    revision: DocumentRevision,
    dirty: bool,
) {
    let EditorActionOutcome::Workspace(report) = outcome else {
        panic!("expected the real document owner's receipt: {outcome:?}");
    };
    assert!(report.applied, "{report:?}");
    assert_eq!(report.document, Some(document));
    assert_eq!(report.revision, Some(revision));
    assert_eq!(report.dirty, Some(dirty));
    assert!(report.diagnostics.is_empty(), "{report:?}");
}

fn edit_then_undo(editor: &mut EditorProjectSession, document: EditorDocumentId) -> TextSnapshot {
    let before = text_snapshot(editor, document);
    assert!(!before.tab.dirty);
    assert_eq!(before.tab.history, DocumentHistoryStatus::default());
    let edited_revision = before.tab.revision.next();
    // Both operations finish before a frame can discover dirty state or changed disk bytes.
    let outcomes = dispatch(
        editor,
        [
            set_text(document, before.tab.revision, "accepted extension edit"),
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: edited_revision,
            }),
        ],
    );
    assert_eq!(outcomes.len(), 2);
    assert_applied(&outcomes[0], document, edited_revision, true);
    assert_applied(&outcomes[1], document, edited_revision.next(), false);
    let mut expected = before;
    expected.tab.revision = edited_revision.next();
    expected.tab.history.redo_depth = 1;
    assert_eq!(text_snapshot(editor, document), expected);
    expected
}

fn protected_and_published(editor: &mut EditorProjectSession) -> EditorSourceToolContentView {
    wait(editor, |editor| {
        let view = editor.source_tool_content_view();
        assert!(
            sample(&view).modified,
            "accepted protection disappeared: {view:?}",
        );
        view.phase == Phase::Idle
            && !view.physical_work_pending
            && !view.protection_publication_pending
    });
    let view = editor.source_tool_content_view();
    assert!(view.error.is_none(), "{view:?}");
    view
}

fn save_text(editor: &mut EditorProjectSession, document: EditorDocumentId) -> TextSnapshot {
    let before = text_snapshot(editor, document);
    assert!(before.tab.dirty);
    let outcomes = dispatch(
        editor,
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: before.tab.revision,
        })],
    );
    let [EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt })] =
        outcomes.as_slice()
    else {
        panic!("ordinary extension Save must be admitted: {outcomes:?}");
    };
    let attempt = *attempt;
    wait(editor, |editor| {
        let persistence = editor.persistence_view();
        persistence.attempt() == Some(attempt) && persistence.result().is_some()
    });
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
            if saved == document && revision == before.tab.revision
    ));
    let mut expected = before;
    expected.tab.dirty = false;
    expected.tab.saved_revision = expected.tab.revision;
    assert_eq!(text_snapshot(editor, document), expected);
    assert_eq!(
        dispatch(
            editor,
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )],
        ),
        [EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt },
        )],
    );
    assert_eq!(text_snapshot(editor, document), expected);
    expected
}

fn activate(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let outcomes = dispatch(
        editor,
        [EditorAction::Navigate(
            EditorNavigationAction::SetActiveDocument { document },
        )],
    );
    assert!(
        matches!(outcomes.as_slice(), [EditorActionOutcome::Workspace(report)] if report.applied),
        "{outcomes:?}",
    );
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(document),
    );
}

fn expected_modified(
    managed: &EditorSourceToolContentView,
) -> Vec<EditorSourceToolContentInstallation> {
    let mut expected = managed.installations.clone();
    expected[0]
        .members
        .iter_mut()
        .find(|member| member.path.as_ref() == SAMPLE)
        .unwrap()
        .modified = true;
    expected
}

fn fail_next_content_write(editor: &EditorProjectSession) {
    editor
        .source_tools
        .content
        .host
        .as_ref()
        .unwrap()
        .try_with(|host| {
            // Fail the real temporary write without retaining a separate cleanup obligation.
            let cleanup = host.test_fail_after_write();
            cleanup.release();
        })
        .unwrap();
}

fn assert_session_close_is_repairable(
    editor: &mut EditorProjectSession,
) -> EditorProjectCloseProgress {
    let started = Instant::now();
    let progress = editor.advance_close();
    assert!(started.elapsed() < Duration::from_secs(2), "{progress:?}");
    assert!(
        matches!(
            progress,
            EditorProjectCloseProgress::Incomplete {
                file_cleanup_pending: true,
                ..
            }
        ),
        "{progress:?}",
    );
    assert!(!editor.closing);
    assert!(editor.authoring_runtime.host_custody().is_accepting());
    progress
}

fn repair_known_protection(editor: &mut EditorProjectSession) -> EditorSourceToolContentView {
    let failed = editor.source_tool_content_view();
    assert!(failed.protection_publication_pending);
    assert!(!failed.physical_work_pending);
    let attempt = editor.reconcile_source_tool_content(&failed.basis).unwrap();
    let reconciled = completed(editor, &attempt, Outcome::NeedsProtectionPublication);
    assert_eq!(reconciled.installations, failed.installations);
    let repair = editor
        .prepare_source_tool_content_protection(&reconciled.basis)
        .unwrap();
    assert_eq!(repair.installations(), reconciled.installations);
    let repaired = commit(editor, repair, Outcome::Protected);
    assert_eq!(repaired.installations, failed.installations);
    repaired
}

fn assert_persistence_is_gated(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    reason: EditorPersistenceRejection,
) {
    let before = text_snapshot(editor, document);
    for (request, action) in [
        (
            EditorPersistenceRequestKind::Save,
            EditorPersistenceAction::Save {
                document,
                base_revision: before.tab.revision,
            },
        ),
        (
            EditorPersistenceRequestKind::Reopen,
            EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: before.tab.revision,
                },
            },
        ),
    ] {
        let outcomes = dispatch(editor, [EditorAction::Persistence(action)]);
        let [
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Rejected {
                attempt,
                reason: actual,
            }),
        ] = outcomes.as_slice()
        else {
            panic!("protected source must reject {request:?}: {outcomes:?}");
        };
        let attempt = *attempt;
        assert_eq!(*actual, reason);
        assert_eq!(editor.persistence_view().attempt(), Some(attempt));
        assert_eq!(
            editor.persistence_view().result(),
            Some(EditorPersistenceResult::Rejected {
                request,
                document: Some(document),
                reason,
            }),
        );
        assert_eq!(text_snapshot(editor, document), before);
        assert_eq!(
            dispatch(
                editor,
                [EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult { attempt },
                )],
            ),
            [EditorActionOutcome::Persistence(
                EditorPersistenceRequestResult::Acknowledged { attempt },
            )],
        );
        assert_eq!(text_snapshot(editor, document), before);
    }
}

fn close_text_document(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> EditorWorkspaceIntentResult {
    let dirty = text_snapshot(editor, document).tab.dirty;
    assert_eq!(
        dispatch(
            editor,
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                    document,
                }),
            )],
        ),
        [EditorActionOutcome::WorkspaceIntent(
            EditorWorkspaceIntentRequestResult::Accepted,
        )],
    );
    if dirty {
        let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
        assert_eq!(
            editor.workspace_intent_view().decision_document(),
            Some(document),
        );
        assert_eq!(
            dispatch(
                editor,
                [EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::Resolve {
                        prompt,
                        decision: EditorCloseDecision::Discard,
                    },
                )],
            ),
            [EditorActionOutcome::WorkspaceIntent(
                EditorWorkspaceIntentRequestResult::Accepted,
            )],
        );
    }
    wait(editor, |editor| {
        editor.workspace_intent_view().result().is_some()
    });
    let result = editor.workspace_intent_view().result().unwrap();
    assert_eq!(
        dispatch(
            editor,
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::AcknowledgeResult,
            )],
        ),
        [EditorActionOutcome::Acknowledged(
            EditorActionAcknowledgement::WorkspaceIntent(true),
        )],
    );
    result
}

#[test]
fn extension_edit_then_undo_protects_release_content_through_save_close_and_fresh_open() {
    for source in [SOURCE, CASE_SOURCE] {
        let project = Project::new();
        let scene_bytes = project.persisted();
        let mut persisted = None;
        let mut previous_source = None;
        project.run(|editor| {
            loaded(editor);
            let scene = editor.workspace.active_document().unwrap();
            let scene_before = source_snapshot(editor);
            let installation = install(editor);
            let files = file_snapshot(&project, &installation);
            let baseline = files.get(SOURCE).unwrap();
            let managed = manage(editor, &installation);
            let record = fs::read(project.root.join(RECORD)).unwrap();
            editor.register_extension_document::<SampleText>().unwrap();
            let document = editor
                .open_extension_document::<SampleText>(source)
                .unwrap();
            let opened = text_snapshot(editor, document);
            assert_eq!(opened.view.text.as_bytes(), baseline);
            assert_eq!(
                opened.tab.source_binding.as_ref().unwrap().locator(),
                source
            );
            assert_eq!(
                opened.tab.kind,
                EditorDocumentKind::Extension(SampleText::TYPE_ID),
            );
            assert!(!opened.tab.dirty);
            assert_eq!(opened.tab.history, DocumentHistoryStatus::default());
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            for alias in [SOURCE, CASE_SOURCE] {
                assert_eq!(
                    editor.open_extension_document::<SampleText>(alias),
                    Err(EditorExtensionDocumentHostError::SourceAlreadyOpen),
                );
            }
            let id = *installation.receipt().installation_id();
            let obsolete = editor
                .prepare_source_tool_content_observation(&managed.basis, &id)
                .unwrap();
            assert_eq!(obsolete.installations(), managed.installations);
            let restored = edit_then_undo(editor, document);
            assert_eq!(restored.view, opened.view);
            assert_files(&project, &files);
            let protected = protected_and_published(editor);
            assert_eq!(protected.installations, expected_modified(&managed));
            assert_eq!(protected.operation, managed.operation);
            assert_record(&project, &protected);
            assert_eq!(
                editor.commit_source_tool_content(obsolete),
                Err(Error::StaleBasis),
            );
            assert_eq!(text_snapshot(editor, document), restored);
            let published = fs::read(project.root.join(RECORD)).unwrap();

            let outcomes = dispatch(
                editor,
                [EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document,
                    base_revision: restored.tab.revision,
                })],
            );
            assert_applied(&outcomes[0], document, restored.tab.revision.next(), true);
            let redone = text_snapshot(editor, document);
            assert_eq!(redone.view.text, "accepted extension edit");
            assert_ne!(redone.view.state, opened.view.state);
            assert_eq!(
                redone.tab.history,
                DocumentHistoryStatus {
                    undo_depth: 1,
                    redo_depth: 0,
                },
            );
            let saved = save_text(editor, document);
            assert_eq!(
                fs::read(project.root.join(SOURCE)).unwrap(),
                saved.view.text.as_bytes(),
            );
            let outcomes = dispatch(
                editor,
                [set_text(document, saved.tab.revision, &opened.view.text)],
            );
            assert_applied(&outcomes[0], document, saved.tab.revision.next(), true);
            let restored = save_text(editor, document);
            assert_eq!(restored.view.text, opened.view.text);
            assert_ne!(restored.view.state, opened.view.state);
            assert_eq!(
                restored.tab.history,
                DocumentHistoryStatus {
                    undo_depth: 2,
                    redo_depth: 0,
                },
            );
            let final_content = protected_and_published(editor);
            assert_eq!(final_content.installations, protected.installations);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), published);
            assert_record(&project, &final_content);
            assert_files(&project, &files);
            activate(editor, scene);
            assert_eq!(source_snapshot(editor), scene_before);
            activate(editor, document);
            assert_eq!(text_snapshot(editor, document), restored);
            assert_eq!(project.persisted(), scene_bytes);
            assert_no_execution(editor);
            previous_source = Some(restored.tab.revision.source_id());
            let mut expected = final_content.installations;
            expected[0].creation_custody = false;
            persisted = Some((expected, published, files));
        });
        let (expected, record, files) = persisted.unwrap();
        project.run(|editor| {
            let reopened = loaded(editor);
            assert_eq!(reopened.installations, expected);
            assert_record(&project, &reopened);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            editor.register_extension_document::<SampleText>().unwrap();
            let document = editor
                .open_extension_document::<SampleText>(SOURCE)
                .unwrap();
            let fresh = text_snapshot(editor, document);
            assert_eq!(fresh.view.text.as_bytes(), files.get(SOURCE).unwrap());
            assert_eq!(fresh.tab.revision.generation(), 0);
            assert_ne!(Some(fresh.tab.revision.source_id()), previous_source);
            assert_eq!(fresh.tab.saved_revision, fresh.tab.revision);
            assert_eq!(fresh.tab.history, DocumentHistoryStatus::default());
            assert!(!fresh.tab.dirty);
            assert_eq!(editor.source_tool_content_view(), reopened);
            assert_files(&project, &files);
            assert_eq!(project.persisted(), scene_bytes);
            assert_no_execution(editor);
        });
    }
}

#[test]
fn open_queries_navigation_rejected_stale_and_noop_extension_actions_do_not_modify_content() {
    let project = Project::new();
    let mut expected_reopened = None;
    project.run(|editor| {
        loaded(editor);
        let scene = editor.workspace.active_document().unwrap();
        let scene_before = source_snapshot(editor);
        let installation = install(editor);
        let files = file_snapshot(&project, &installation);
        let managed = manage(editor, &installation);
        let record = fs::read(project.root.join(RECORD)).unwrap();
        editor.register_extension_document::<SampleText>().unwrap();
        let document = editor
            .open_extension_document::<SampleText>(SOURCE)
            .unwrap();
        let opened = text_snapshot(editor, document);
        for _ in 0..3 {
            assert_eq!(text_snapshot(editor, document), opened);
            assert_eq!(
                editor.document_query_basis(document),
                Err(EditorDocumentQueryRejection::UnsupportedDocumentKind {
                    kind: EditorDocumentKind::Extension(SampleText::TYPE_ID),
                }),
            );
        }
        let basis = editor.document_query_basis(scene).unwrap();
        let outcomes = dispatch(
            editor,
            [EditorAction::QueryDocumentEntities(EditorEntityQuery {
                position: EditorEntityQueryPosition::Start(basis),
                limits: EditorEntityQueryLimits::new(8, 8).unwrap(),
            })],
        );
        let [EditorActionOutcome::DocumentEntities(Ok(page))] = outcomes.as_slice() else {
            panic!("ordinary source query must remain read-only: {outcomes:?}");
        };
        assert_eq!(page.basis.document, scene);
        activate(editor, scene);
        assert_eq!(source_snapshot(editor), scene_before);
        activate(editor, document);
        assert_eq!(text_snapshot(editor, document), opened);
        for action in [
            set_text(document, opened.tab.revision, &"x".repeat(TEXT_LIMIT + 1)),
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: opened.tab.revision,
            }),
            EditorAction::Mutate(EditorDocumentMutation::Redo {
                document,
                base_revision: opened.tab.revision,
            }),
        ] {
            let outcomes = dispatch(editor, [action]);
            assert!(
                matches!(outcomes.as_slice(), [EditorActionOutcome::Workspace(report)]
                    if !report.applied && report.document == Some(document)),
                "{outcomes:?}",
            );
            assert_eq!(text_snapshot(editor, document), opened);
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        }

        let outcomes = dispatch(
            editor,
            [set_text(document, opened.tab.revision, &opened.view.text)],
        );
        assert_applied(&outcomes[0], document, opened.tab.revision.next(), false);
        let mut noop = opened.clone();
        noop.tab.revision = noop.tab.revision.next();
        assert_eq!(text_snapshot(editor, document), noop);
        for action in [
            set_text(document, opened.tab.revision, "stale extension edit"),
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: opened.tab.revision,
            }),
        ] {
            assert_eq!(
                dispatch(editor, [action]),
                [EditorActionOutcome::Rejected(
                    EditorActionRejection::StaleDocumentFact {
                        document,
                        expected: opened.tab.revision,
                        actual: Some(noop.tab.revision),
                    },
                )],
            );
            assert_eq!(text_snapshot(editor, document), noop);
            assert_eq!(editor.source_tool_content_view(), managed);
        }
        assert_record(&project, &managed);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert_files(&project, &files);
        activate(editor, scene);
        assert_eq!(source_snapshot(editor), scene_before);
        assert_no_execution(editor);
        let mut expected = managed.installations;
        expected[0].creation_custody = false;
        expected_reopened = Some((expected, record, files));
    });
    let (expected, record, files) = expected_reopened.unwrap();
    project.run(|editor| {
        let reopened = loaded(editor);
        assert_eq!(reopened.installations, expected);
        assert!(!sample(&reopened).modified);
        assert!(!sample(&reopened).adopted);
        assert_record(&project, &reopened);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert_files(&project, &files);
        assert_no_execution(editor);
    });
}

#[test]
fn accepted_edit_of_unmanaged_same_named_source_does_not_protect_the_installed_member() {
    const UNMANAGED: &str = "project-owned/samples/readme.txt";
    let project = Project::new();
    project.run(|editor| {
        loaded(editor);
        let scene = editor.workspace.active_document().unwrap();
        let scene_before = source_snapshot(editor);
        let installation = install(editor);
        let files = file_snapshot(&project, &installation);
        let managed = manage(editor, &installation);
        let record = fs::read(project.root.join(RECORD)).unwrap();
        let path = project.root.join(UNMANAGED);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, files.get(SOURCE).unwrap()).unwrap();
        editor.register_extension_document::<SampleText>().unwrap();
        let document = editor
            .open_extension_document::<SampleText>(UNMANAGED)
            .unwrap();
        let restored = edit_then_undo(editor, document);
        assert_eq!(
            restored.tab.source_binding.as_ref().unwrap().locator(),
            UNMANAGED,
        );
        assert_eq!(restored.view.text.as_bytes(), files.get(SOURCE).unwrap());
        assert_eq!(fs::read(path).unwrap(), files[SOURCE]);
        assert_eq!(editor.source_tool_content_view(), managed);
        assert_record(&project, &managed);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert_files(&project, &files);
        activate(editor, scene);
        assert_eq!(source_snapshot(editor), scene_before);
        assert_no_execution(editor);
    });
}

#[test]
fn unavailable_initial_record_retains_clean_document_touch_until_reconciliation() {
    let project = Project::new();
    let mut original_record = Vec::new();
    let mut files = BTreeMap::new();
    project.run(|editor| {
        loaded(editor);
        let installation = install(editor);
        let managed = manage(editor, &installation);
        assert!(!sample(&managed).modified);
        original_record = fs::read(project.root.join(RECORD)).unwrap();
        files = file_snapshot(&project, &installation);
    });
    fs::write(project.root.join(RECORD), b"{unavailable content record").unwrap();
    project.run(|editor| {
        let unavailable = loaded(editor);
        assert_eq!(unavailable.phase, Phase::Unavailable);
        assert!(unavailable.installations.is_empty());
        editor.register_extension_document::<SampleText>().unwrap();
        let document = editor
            .open_extension_document::<SampleText>(SOURCE)
            .unwrap();
        let restored = edit_then_undo(editor, document);
        assert!(!restored.tab.dirty);
        assert_eq!(editor.source_tool_content_view(), unavailable);
        assert_persistence_is_gated(
            editor,
            document,
            EditorPersistenceRejection::RequiresReconcile,
        );
        assert_eq!(
            close_text_document(editor, document),
            EditorWorkspaceIntentResult::Rejected {
                intent: EditorWorkspaceIntent::CloseDocument { document },
                reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                    EditorPersistenceRejection::RequiresReconcile,
                ),
            },
        );
        assert_session_close_is_repairable(editor);
        assert_eq!(text_snapshot(editor, document), restored);
        fs::write(project.root.join(RECORD), &original_record).unwrap();
        let attempt = editor
            .reconcile_source_tool_content(&unavailable.basis)
            .unwrap();
        wait(editor, |editor| {
            let view = editor.source_tool_content_view();
            view.phase == Phase::Idle
                && !view.protection_publication_pending
                && !view.physical_work_pending
        });
        let protected = editor.source_tool_content_view();
        assert_eq!(
            protected.operation,
            Some(EditorSourceToolContentAttempt {
                attempt,
                outcome: Some(Outcome::Reconciled),
            }),
        );
        assert!(sample(&protected).modified);
        assert!(!protected.installations[0].creation_custody);
        assert_record(&project, &protected);
        assert_eq!(text_snapshot(editor, document), restored);
        assert_eq!(
            close_text_document(editor, document),
            EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document },
            },
        );
        assert_files(&project, &files);
    });
    project.run(|editor| {
        let reopened = loaded(editor);
        assert!(sample(&reopened).modified);
        assert!(!sample(&reopened).adopted);
        assert_record(&project, &reopened);
        assert_files(&project, &files);
    });
}

#[test]
fn queued_save_polls_unknown_classification_and_protection_before_publishing_its_original_snapshot()
{
    let project = Project::new();
    let mut original_record = Vec::new();
    project.run(|editor| {
        loaded(editor);
        let installation = install(editor);
        manage(editor, &installation);
        original_record = fs::read(project.root.join(RECORD)).unwrap();
    });
    fs::write(
        project.root.join(RECORD),
        br#"{"format_version":1,"installations":[]}"#,
    )
    .unwrap();
    project.run(|editor| {
        let initial = loaded(editor);
        assert!(initial.installations.is_empty());
        editor.register_extension_document::<SampleText>().unwrap();
        let document = editor
            .open_extension_document::<SampleText>(SOURCE)
            .unwrap();
        let opened = text_snapshot(editor, document);
        let baseline = fs::read(project.root.join(SOURCE)).unwrap();
        fs::write(project.root.join(RECORD), &original_record).unwrap();
        let (gate, control) = IoGate::new();
        editor.source_tools.content.next_io_gate = Some(gate);
        let read = editor
            .reconcile_source_tool_content(&initial.basis)
            .unwrap();
        control.wait_until_entered();
        let first = "the captured source checkpoint";
        let second = "a later accepted source change";
        let outcomes = dispatch(editor, [set_text(document, opened.tab.revision, first)]);
        assert_applied(&outcomes[0], document, opened.tab.revision.next(), true);
        let captured = text_snapshot(editor, document);
        let outcomes = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                editor.current_frame().revision(),
                [EditorAction::Persistence(EditorPersistenceAction::Save {
                    document,
                    base_revision: captured.tab.revision,
                })],
            ))
            .unwrap()
            .into_outcomes();
        let [
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }),
        ] = outcomes.as_slice()
        else {
            panic!("unknown classification must retain the owned Save request: {outcomes:?}");
        };
        let attempt = *attempt;
        let pending = editor.persistence_view();
        for _ in 0..3 {
            editor.advance_persistence();
            assert_eq!(editor.persistence_view(), pending);
            assert!(editor.source_tool_content_view().installations.is_empty());
            assert_eq!(fs::read(project.root.join(SOURCE)).unwrap(), baseline);
        }
        let outcomes = dispatch(editor, [set_text(document, captured.tab.revision, second)]);
        assert_applied(&outcomes[0], document, captured.tab.revision.next(), true);
        let later = text_snapshot(editor, document);
        control.release();
        let deadline = Instant::now() + super::super::WAIT;
        loop {
            // This owner poll must progress its own classification/protection prerequisite.
            editor.advance_persistence();
            if let Some(result) = editor.persistence_view().result() {
                assert_eq!(editor.persistence_view().attempt(), Some(attempt));
                assert!(
                    matches!(
                        result,
                        EditorPersistenceResult::Saved { document: saved, revision, .. }
                            if saved == document && revision == captured.tab.revision
                    ),
                    "{result:?}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the owned Save dependency did not return"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let protected = editor.source_tool_content_view();
        assert_eq!(protected.phase, Phase::Idle);
        assert!(!protected.protection_publication_pending);
        assert!(!protected.physical_work_pending);
        assert!(sample(&protected).modified);
        assert_eq!(
            protected.operation,
            Some(EditorSourceToolContentAttempt {
                attempt: read,
                outcome: Some(Outcome::Reconciled),
            })
        );
        assert_record(&project, &protected);
        assert_eq!(
            fs::read(project.root.join(SOURCE)).unwrap(),
            first.as_bytes()
        );
        editor.drive_editor_frame(Duration::ZERO);
        let current = text_snapshot(editor, document);
        assert_eq!(current.view, later.view);
        assert_eq!(current.tab.revision, later.tab.revision);
        assert_eq!(current.tab.saved_revision, captured.tab.revision);
        assert!(current.tab.dirty);
        assert_eq!(current.tab.history, later.tab.history);
    });
    project.run(|editor| {
        let reopened = loaded(editor);
        assert!(sample(&reopened).modified);
        editor.register_extension_document::<SampleText>().unwrap();
        let document = editor
            .open_extension_document::<SampleText>(SOURCE)
            .unwrap();
        assert_eq!(
            text_snapshot(editor, document).view.text,
            "the captured source checkpoint"
        );
    });
}

#[test]
fn held_older_content_read_or_publication_cannot_erase_accepted_extension_edit_protection() {
    for publish in [false, true] {
        let project = Project::new();
        let mut persisted = None;
        let mut returned = None;
        project.run(|editor| {
            loaded(editor);
            let scene = editor.workspace.active_document().unwrap();
            let scene_before = source_snapshot(editor);
            let installation = install(editor);
            let files = file_snapshot(&project, &installation);
            let managed = manage(editor, &installation);
            let id = *installation.receipt().installation_id();
            editor.register_extension_document::<SampleText>().unwrap();
            let document = editor
                .open_extension_document::<SampleText>(SOURCE)
                .unwrap();
            let older = publish.then(|| {
                editor
                    .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                    .unwrap()
            });
            let (gate, control) = IoGate::new();
            assert!(
                editor
                    .source_tools
                    .content
                    .next_io_gate
                    .replace(gate)
                    .is_none(),
            );
            let attempt = if let Some(older) = older {
                let candidate = older.installations();
                let member = candidate[0]
                    .members
                    .iter()
                    .find(|member| member.path.as_ref() == SAMPLE)
                    .unwrap();
                assert!(!member.modified);
                editor.commit_source_tool_content(older).unwrap()
            } else {
                editor
                    .reconcile_source_tool_content(&managed.basis)
                    .unwrap()
            };
            let observer = editor
                .source_tools
                .content
                .pending
                .as_ref()
                .unwrap()
                .task
                .as_ref()
                .unwrap()
                .return_observer();
            control.wait_until_entered();
            assert_eq!(observer.state(), TaskReturnState::Pending);
            let disk: Value =
                serde_json::from_slice(&fs::read(project.root.join(RECORD)).unwrap()).unwrap();
            let older_sample = disk["installations"][0]["members"]
                .as_array()
                .unwrap()
                .iter()
                .find(|member| member["path"] == SAMPLE)
                .unwrap();
            assert_eq!(older_sample["modified"], false);
            let restored = edit_then_undo(editor, document);
            for _ in 0..3 {
                editor.drive_editor_frame(Duration::ZERO);
                let held = editor.source_tool_content_view();
                assert!(held.physical_work_pending, "{held:?}");
                assert!(sample(&held).modified, "{held:?}");
                assert_eq!(held.operation.as_ref().unwrap().attempt, attempt);
                assert_eq!(observer.state(), TaskReturnState::Pending);
                assert_eq!(text_snapshot(editor, document), restored);
                assert_files(&project, &files);
            }
            control.release();
            wait(editor, |editor| {
                let view = editor.source_tool_content_view();
                assert!(sample(&view).modified, "{view:?}");
                view.operation.as_ref().is_some_and(|operation| {
                    operation.attempt == attempt && operation.outcome.is_some()
                })
            });
            let returned_view = editor.source_tool_content_view();
            let protected = if returned_view.phase == Phase::RequiresReconcile {
                assert!(returned_view.error.is_none(), "{returned_view:?}");
                let repair = editor
                    .prepare_source_tool_content_protection(&returned_view.basis)
                    .unwrap();
                commit(editor, repair, Outcome::Protected)
            } else {
                let protected = protected_and_published(editor);
                assert_eq!(protected.operation, returned_view.operation);
                protected
            };
            assert_eq!(observer.state(), TaskReturnState::Returned);
            let mut expected = expected_modified(&managed);
            if publish {
                expected[0]
                    .members
                    .iter_mut()
                    .find(|member| member.path.as_ref() == SAMPLE)
                    .unwrap()
                    .adopted = true;
            }
            assert_eq!(protected.installations, expected);
            assert_eq!(text_snapshot(editor, document), restored);
            assert_record(&project, &protected);
            assert_files(&project, &files);
            activate(editor, scene);
            assert_eq!(source_snapshot(editor), scene_before);
            assert_no_execution(editor);
            expected[0].creation_custody = false;
            persisted = Some((
                expected,
                fs::read(project.root.join(RECORD)).unwrap(),
                files,
            ));
            returned = Some(observer);
        });
        assert_eq!(returned.unwrap().state(), TaskReturnState::Returned);
        let (expected, record, files) = persisted.unwrap();
        project.run(|editor| {
            let reopened = loaded(editor);
            assert_eq!(reopened.installations, expected);
            assert_record(&project, &reopened);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }
}

#[test]
fn failed_confirmed_adoption_stays_known_and_close_preserves_explicit_repair() {
    let project = Project::new();
    let mut persisted = None;
    project.run(|editor| {
        loaded(editor);
        let source = source_snapshot(editor);
        let installation = install(editor);
        let files = file_snapshot(&project, &installation);
        let managed = manage(editor, &installation);
        let record = fs::read(project.root.join(RECORD)).unwrap();
        let id = *installation.receipt().installation_id();
        let adoption = editor
            .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
            .unwrap();
        let confirmed = adoption.installations();
        fail_next_content_write(editor);
        let failed = commit(editor, adoption, Outcome::Failed(Error::Write));
        assert_eq!(failed.installations, confirmed);
        assert!(sample(&failed).adopted);
        assert!(!sample(&failed).modified);
        assert_eq!(failed.phase, Phase::RequiresReconcile);
        assert!(failed.protection_publication_pending);
        assert!(!failed.physical_work_pending);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert!(matches!(
            editor.prepare_source_tool_content_protection(&failed.basis),
            Err(Error::RequiresReconcile),
        ));
        assert_session_close_is_repairable(editor);
        assert_eq!(editor.source_tool_content_view(), failed);
        assert_eq!(source_snapshot(editor), source);
        assert_files(&project, &files);

        let repaired = repair_known_protection(editor);
        assert_eq!(repaired.installations, confirmed);
        assert_record(&project, &repaired);
        assert_eq!(source_snapshot(editor), source);
        assert_files(&project, &files);
        assert_no_execution(editor);
        let mut expected = repaired.installations;
        expected[0].creation_custody = false;
        persisted = Some((
            expected,
            fs::read(project.root.join(RECORD)).unwrap(),
            files,
        ));
    });
    let (expected, record, files) = persisted.unwrap();
    project.run(|editor| {
        let reopened = loaded(editor);
        assert_eq!(reopened.installations, expected);
        assert!(sample(&reopened).adopted);
        assert!(!sample(&reopened).modified);
        assert_record(&project, &reopened);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert_files(&project, &files);
        assert_no_execution(editor);
    });
}

#[test]
fn held_or_failed_edit_protection_gates_save_reopen_and_clean_or_discard_close() {
    for fail in [false, true] {
        for dirty in [false, true] {
            let project = Project::new();
            let mut persisted = None;
            project.run(|editor| {
                loaded(editor);
                let scene = editor.workspace.active_document().unwrap();
                let scene_before = source_snapshot(editor);
                let installation = install(editor);
                let files = file_snapshot(&project, &installation);
                let managed = manage(editor, &installation);
                let record = fs::read(project.root.join(RECORD)).unwrap();
                let explicit = managed.operation.clone();
                editor.register_extension_document::<SampleText>().unwrap();
                let document = editor
                    .open_extension_document::<SampleText>(SOURCE)
                    .unwrap();
                let opened = text_snapshot(editor, document);
                let lane = editor.extension_documents.lane(document).unwrap();
                let original = lane
                    .host
                    .try_with(|host| host.read_source().open())
                    .unwrap()
                    .unwrap();
                if fail {
                    fail_next_content_write(editor);
                }
                let (gate, control) = IoGate::new();
                assert!(
                    editor
                        .source_tools
                        .content
                        .next_io_gate
                        .replace(gate)
                        .is_none(),
                );
                let mut actions = vec![set_text(
                    document,
                    opened.tab.revision,
                    "accepted protected sample edit",
                )];
                if !dirty {
                    actions.push(EditorAction::Mutate(EditorDocumentMutation::Undo {
                        document,
                        base_revision: opened.tab.revision.next(),
                    }));
                }
                let outcomes = editor
                    .dispatch_editor_actions(EditorActionBatch::new(
                        editor.current_frame().revision(),
                        actions,
                    ))
                    .unwrap()
                    .into_outcomes();
                assert_eq!(outcomes.len(), if dirty { 1 } else { 2 });
                assert_applied(&outcomes[0], document, opened.tab.revision.next(), true);
                if !dirty {
                    assert_applied(
                        &outcomes[1],
                        document,
                        opened.tab.revision.next().next(),
                        false,
                    );
                }
                let queued = editor.source_tool_content_view();
                assert_eq!(queued.phase, Phase::ProtectionPending);
                assert_eq!(queued.installations, expected_modified(&managed));
                assert!(queued.protection_publication_pending);
                assert!(!queued.physical_work_pending);
                assert_eq!(queued.operation, explicit);
                assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
                editor.drive_editor_frame(Duration::ZERO);
                let accepted = text_snapshot(editor, document);
                assert_eq!(accepted.tab.dirty, dirty);
                assert_eq!(accepted.tab.saved_revision, opened.tab.saved_revision);
                assert_eq!(
                    accepted.tab.history,
                    DocumentHistoryStatus {
                        undo_depth: usize::from(dirty),
                        redo_depth: usize::from(!dirty),
                    },
                );
                if dirty {
                    assert_eq!(accepted.view.text, "accepted protected sample edit");
                    assert_ne!(accepted.view.state, opened.view.state);
                } else {
                    assert_eq!(accepted.view, opened.view);
                }
                control.wait_until_entered();
                let observer = editor
                    .source_tools
                    .content
                    .pending
                    .as_ref()
                    .unwrap()
                    .task
                    .as_ref()
                    .unwrap()
                    .return_observer();
                assert_eq!(observer.state(), TaskReturnState::Pending);
                let held = editor.source_tool_content_view();
                assert_eq!(held.phase, Phase::Publishing);
                assert!(held.protection_publication_pending);
                assert!(held.physical_work_pending);
                assert!(held.error.is_none());
                assert_eq!(held.operation, explicit);
                let disk: Value =
                    serde_json::from_slice(&fs::read(project.root.join(RECORD)).unwrap()).unwrap();
                let disk_sample = disk["installations"][0]["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|member| member["path"] == SAMPLE)
                    .unwrap();
                assert_eq!(disk_sample["modified"], !fail);

                assert_persistence_is_gated(editor, document, EditorPersistenceRejection::Busy);
                assert_eq!(
                    close_text_document(editor, document),
                    EditorWorkspaceIntentResult::Rejected {
                        intent: EditorWorkspaceIntent::CloseDocument { document },
                        reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                            EditorPersistenceRejection::Busy,
                        ),
                    },
                );
                assert_eq!(text_snapshot(editor, document), accepted);
                assert!(editor.extension_documents.lane(document).is_some());
                assert!(
                    lane.host
                        .try_with(|host| host.read_source().open().unwrap().same_object(&original))
                        .unwrap()
                        .unwrap(),
                );
                let close = assert_session_close_is_repairable(editor);
                assert!(!close.task_shutdown().is_complete(), "{close:?}");
                assert_eq!(observer.state(), TaskReturnState::Pending);
                assert_eq!(editor.source_tool_content_view(), held);
                assert_files(&project, &files);

                control.release();
                let protected = if fail {
                    wait(editor, |editor| {
                        let view = editor.source_tool_content_view();
                        assert!(sample(&view).modified, "{view:?}");
                        assert_eq!(view.operation, explicit);
                        !view.physical_work_pending && view.error.is_some()
                    });
                    let failed = editor.source_tool_content_view();
                    assert_eq!(failed.phase, Phase::RequiresReconcile);
                    assert_eq!(failed.error, Some(Error::Write));
                    assert!(failed.protection_publication_pending);
                    assert_eq!(failed.operation, explicit);
                    assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
                    assert_persistence_is_gated(
                        editor,
                        document,
                        EditorPersistenceRejection::RequiresReconcile,
                    );
                    assert_eq!(
                        close_text_document(editor, document),
                        EditorWorkspaceIntentResult::Rejected {
                            intent: EditorWorkspaceIntent::CloseDocument { document },
                            reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                                EditorPersistenceRejection::RequiresReconcile,
                            ),
                        },
                    );
                    assert_eq!(text_snapshot(editor, document), accepted);
                    assert!(editor.extension_documents.lane(document).is_some());
                    assert!(
                        lane.host
                            .try_with(|host| {
                                host.read_source().open().unwrap().same_object(&original)
                            })
                            .unwrap()
                            .unwrap(),
                    );
                    assert_session_close_is_repairable(editor);
                    assert_eq!(editor.source_tool_content_view(), failed);
                    repair_known_protection(editor)
                } else {
                    let protected = protected_and_published(editor);
                    assert_eq!(protected.operation, explicit);
                    protected
                };
                assert_eq!(observer.state(), TaskReturnState::Returned);
                assert_eq!(protected.installations, expected_modified(&managed));
                assert_record(&project, &protected);
                assert_eq!(text_snapshot(editor, document), accepted);
                assert_eq!(
                    close_text_document(editor, document),
                    EditorWorkspaceIntentResult::Applied {
                        intent: EditorWorkspaceIntent::CloseDocument { document },
                    },
                );
                assert!(editor.extension_documents.lane(document).is_none());
                assert!(lane.host.try_with(|_| ()).is_none());
                assert!(
                    editor
                        .current_frame()
                        .workspace()
                        .document(document)
                        .is_none()
                );
                activate(editor, scene);
                assert_eq!(source_snapshot(editor), scene_before);
                assert_files(&project, &files);
                assert_no_execution(editor);
                let mut expected = protected.installations;
                expected[0].creation_custody = false;
                persisted = Some((
                    expected,
                    fs::read(project.root.join(RECORD)).unwrap(),
                    files,
                ));
            });
            let (expected, record, files) = persisted.unwrap();
            project.run(|editor| {
                let reopened = loaded(editor);
                assert_eq!(reopened.installations, expected);
                assert!(sample(&reopened).modified);
                assert!(!sample(&reopened).adopted);
                assert_record(&project, &reopened);
                assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
                editor.register_extension_document::<SampleText>().unwrap();
                let document = editor
                    .open_extension_document::<SampleText>(SOURCE)
                    .unwrap();
                let fresh = text_snapshot(editor, document);
                assert_eq!(fresh.view.text.as_bytes(), files.get(SOURCE).unwrap());
                assert_eq!(fresh.tab.history, DocumentHistoryStatus::default());
                assert!(!fresh.tab.dirty);
                assert_files(&project, &files);
                assert_no_execution(editor);
            });
        }
    }
}
