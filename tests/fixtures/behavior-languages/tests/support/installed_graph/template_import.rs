//! Explicit template import and ordinary editing through an independent public document owner.

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
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorSourceToolArtifactOutcome,
    EditorSourceToolArtifactPhase, EditorSourceToolAttemptId,
    EditorSourceToolReclamationError as ReclamationError,
    EditorSourceToolReclamationMemberState as ReclamationMemberState,
    EditorSourceToolReclamationOutcome as ReclamationOutcome,
    EditorSourceToolReclamationPhase as ReclamationPhase,
    EditorSourceToolReclamationRecordPhase as ReclamationRecordPhase,
    EditorSourceToolReclamationView, EditorSourceToolSelectionError,
    EditorSourceToolSelectionOutcome, EditorSourceToolSelectionPhase,
    EditorSourceToolSelectionView, EditorSourceToolTemplateImportOutcome,
    EditorSourceToolTemplateImportView, EditorSourceToolWithdrawalMode,
    EditorWorkspaceCommandReport,
};

use super::{
    ARTIFACT_PATH, PACKAGE_A, PACKAGE_B, RECORD_PATH, SAMPLE, TIMEOUT, artifact_installation,
    assert_artifact_installations, assert_artifact_outcome, assert_installations,
    assert_installed_bytes, assert_no_execution, binding, install_owned, original, project_root,
    request, settle_artifacts,
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

    const TYPE_ID: &'static str = "trial.imported-sample-text";

    fn title(&self) -> &str {
        "Imported sample"
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

fn settle_import(
    editor: &mut EditorProjectSession,
    attempt: &nara_tooling::EditorSourceToolAttemptId,
) -> EditorSourceToolTemplateImportView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        let view = editor.source_tool_template_import_view();
        if view.attempt.as_ref() == Some(attempt)
            && view.outcome.is_some()
            && !view.physical_work_pending
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Template import did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_selection(editor: &mut EditorProjectSession) -> EditorSourceToolSelectionView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        let view = editor.source_tool_selection_view();
        if view.phase == EditorSourceToolSelectionPhase::Idle
            && !editor.source_tool_view().has_pending_work()
            && view
                .operation
                .as_ref()
                .is_some_and(|operation| operation.outcome.is_some())
        {
            assert!(view.error.is_none(), "{view:?}");
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Installed graph did not physically return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_reclamation(
    editor: &mut EditorProjectSession,
    attempt: &EditorSourceToolAttemptId,
) -> EditorSourceToolReclamationView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        let view = editor.source_tool_reclamation_view();
        if view.attempt.as_ref() == Some(attempt)
            && view.phase == ReclamationPhase::Terminal
            && !view.physical_work_pending
            && !view.record_publication_pending
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Reviewed artifact reclamation did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn csharp_template_import_history_survives_reviewed_artifact_reclamation_and_fresh_open() {
    let (_v1_directory, v1) = installed_variant("V1");
    let (_v2_directory, v2) = installed_variant("V2");
    let project = Project::new(
        &schema_bundle(v1.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let (a1, owner1) = install_owned(&root, v1.release(), "sample-v1", PACKAGE_A, None);
    let (a2, owner2) = install_owned(&root, v2.release(), "sample-v2", PACKAGE_A, None);
    assert_ne!(a1.installed.manifest, a2.installed.manifest);
    assert_ne!(a1.installed.installation, a2.installed.installation);
    let (mut dependent, dependent_owner) = install_owned(
        &root,
        v1.release(),
        "sample-dependent",
        PACKAGE_B,
        Some((PACKAGE_A, a1.installed.manifest)),
    );
    dependent.requirements.push(binding(&a1));
    let expected_inventory = [
        artifact_installation(&a1, &owner1),
        artifact_installation(&a2, &owner2),
        artifact_installation(&dependent, &dependent_owner),
    ];
    let mut installed_bytes = BTreeMap::new();
    for owner in [&owner1, &owner2, &dependent_owner] {
        for member in owner.members() {
            let path = format!("{}/{}", owner.project_path(), member.path());
            let bytes = fs::read(project.root().join(&path)).unwrap();
            assert_eq!(ContentDigest::of_bytes(&bytes), member.digest());
            installed_bytes.insert(path, bytes);
        }
    }
    let scene_bytes = fs::read(project.scene_path()).unwrap();
    let source = "assets/enemy-stats.txt";
    let sample_path = RelativePath::new(source).unwrap();
    let baseline = std::str::from_utf8(
        v1.release()
            .members()
            .iter()
            .find(|member| member.path() == SAMPLE)
            .unwrap()
            .bytes(),
    )
    .unwrap();
    assert_ne!(
        baseline.as_bytes(),
        v2.release()
            .members()
            .iter()
            .find(|member| member.path() == SAMPLE)
            .unwrap()
            .bytes(),
    );
    let mut editor = project.open();
    let scene = editor.current_frame().workspace().active_document.unwrap();
    let scene_basis = editor.document_query_basis(scene).unwrap();
    let mut inventory = settle_artifacts(&mut editor);
    assert_eq!(inventory.phase, EditorSourceToolArtifactPhase::Idle);
    for owner in [&owner1, &owner2, &dependent_owner] {
        let prepared = editor
            .prepare_source_tool_artifact_registration(&inventory.basis, Arc::clone(owner))
            .unwrap();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        inventory = settle_artifacts(&mut editor);
        assert_artifact_outcome(
            &inventory,
            &attempt,
            EditorSourceToolArtifactOutcome::Registered,
        );
    }
    assert_artifact_installations(&inventory.installations, &expected_inventory);
    let inventory_bytes = fs::read(project.root().join(ARTIFACT_PATH)).unwrap();
    let selection = settle_selection(&mut editor);
    let prepared = editor
        .prepare_source_tool_installations(
            &selection.basis,
            root.share(),
            &[request(&a1), request(&a2)],
        )
        .unwrap();
    editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    assert_installations(&registered.installed, &[a1.clone(), a2.clone()]);
    assert_no_execution(&editor);

    let initial_import = editor.source_tool_template_import_view();
    let frame = editor.current_frame();
    let graph_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    let cancelled = editor
        .prepare_source_tool_template_import(
            &registered.basis,
            root.share(),
            &a1.installed.installation,
            SAMPLE,
            source,
        )
        .unwrap();
    assert!(!project.root().join(source).exists());
    drop(cancelled);
    assert_eq!(editor.source_tool_template_import_view(), initial_import);
    assert_eq!(
        editor.current_frame().workspace().documents,
        frame.workspace().documents
    );
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        inventory_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);

    fs::write(
        project.root().join("assets/keep.txt"),
        b"existing project source",
    )
    .unwrap();
    for (member, destination) in [
        (SAMPLE, "assets/keep.txt"),
        (SAMPLE, "assets/missing-parent/sample.txt"),
        (SAMPLE, ".nara/source-tools/artifacts/import.txt"),
        (SAMPLE, "assets/import.scene.json"),
        ("EnemyStatsTool.dll", "assets/tool.txt"),
    ] {
        assert!(
            editor
                .prepare_source_tool_template_import(
                    &registered.basis,
                    root.share(),
                    &a1.installed.installation,
                    member,
                    destination,
                )
                .is_err(),
            "invalid template import admitted: {member} -> {destination}"
        );
    }
    assert_eq!(
        fs::read(project.root().join("assets/keep.txt")).unwrap(),
        b"existing project source"
    );
    assert!(!project.root().join(source).exists());
    assert_eq!(editor.source_tool_template_import_view(), initial_import);

    let stale = editor
        .prepare_source_tool_template_import(
            &registered.basis,
            root.share(),
            &a1.installed.installation,
            SAMPLE,
            source,
        )
        .unwrap();
    let prepared = editor
        .prepare_source_tool_installations(&registered.basis, root.share(), &[request(&dependent)])
        .unwrap();
    editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    assert_installations(
        &registered.installed,
        &[a1.clone(), a2.clone(), dependent.clone()],
    );
    assert!(editor.commit_source_tool_template_import(stale).is_err());
    assert!(!project.root().join(source).exists());
    assert_eq!(editor.source_tool_template_import_view(), initial_import);

    let race_path = "assets/import-race.txt";
    let raced = editor
        .prepare_source_tool_template_import(
            &registered.basis,
            root.share(),
            &a1.installed.installation,
            SAMPLE,
            race_path,
        )
        .unwrap();
    fs::write(project.root().join(race_path), b"concurrent project source").unwrap();
    if let Ok(attempt) = editor.commit_source_tool_template_import(raced) {
        let failed = settle_import(&mut editor, &attempt);
        assert!(matches!(
            failed.outcome,
            Some(EditorSourceToolTemplateImportOutcome::Failed(_))
        ));
    }
    assert_eq!(
        fs::read(project.root().join(race_path)).unwrap(),
        b"concurrent project source"
    );
    assert!(!project.root().join(source).exists());

    let prepared = editor
        .prepare_source_tool_template_import(
            &registered.basis,
            root.share(),
            &a1.installed.installation,
            SAMPLE,
            source,
        )
        .unwrap();
    assert!(!project.root().join(source).exists());
    let attempt = editor.commit_source_tool_template_import(prepared).unwrap();
    let imported = settle_import(&mut editor, &attempt);
    assert!(matches!(
        imported.outcome,
        Some(EditorSourceToolTemplateImportOutcome::Imported { .. })
    ));
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        baseline
    );
    assert!(
        editor
            .prepare_source_tool_template_import(
                &registered.basis,
                root.share(),
                &a2.installed.installation,
                SAMPLE,
                source,
            )
            .is_err()
    );
    assert_eq!(editor.source_tool_artifact_view(), inventory);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        inventory_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    editor.register_extension_document::<SampleText>().unwrap();
    assert!(editor.open_extension_document::<SampleText>(
        &format!("{}/{}", owner1.project_path(), SAMPLE),
    ).is_err());
    let document = editor
        .open_extension_document::<SampleText>(source)
        .unwrap();
    let opened = editor.current_frame();
    let mut tab = opened.workspace().document(document).unwrap().clone();
    assert_eq!(tab.kind, EditorDocumentKind::Extension(SampleText::TYPE_ID));
    assert_eq!(tab.title, "Imported sample");
    assert_eq!(tab.source_binding.as_ref().unwrap().locator(), source);
    assert!(!tab.dirty);
    assert_eq!(tab.saved_revision, tab.revision);
    assert_eq!(tab.history, DocumentHistoryStatus::default());
    assert_text_frame(&editor, &tab, baseline);
    let scene_tab = opened.workspace().document(scene).unwrap().clone();
    let changed = "An EnemyStats sample owned and edited by this project.\n";
    let revision = tab.revision;
    let frame_revision = editor.current_frame().revision();
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
    editor.drive_editor_frame(Duration::ZERO);
    tab.revision = revision.next().next();
    tab.history.redo_depth = 1;
    assert_text_frame(&editor, &tab, baseline);
    assert_eq!(editor.source_tool_artifact_view(), inventory);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        inventory_bytes
    );

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
    let original_file = root.open_file(&sample_path).unwrap();
    save_sample(&mut editor, document, tab.revision);
    let saved_file = root.open_file(&sample_path).unwrap();
    assert!(!original_file.same_object(&saved_file).unwrap());
    tab.saved_revision = tab.revision;
    tab.dirty = false;
    assert_text_frame(&editor, &tab, changed);
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        changed
    );
    drop(original_file);
    drop(saved_file);

    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: tab.revision,
            })],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[applied_text(document, tab.revision.next(), true)]
    );
    editor.drive_editor_frame(Duration::ZERO);
    tab = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert_text_frame(&editor, &tab, baseline);
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        changed
    );
    let frame = editor.current_frame();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: frame.reopen_target(),
            })],
        ))
        .unwrap();
    let [EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt })] =
        report.outcomes()
    else {
        panic!("The imported project source could not Reopen: {report:?}");
    };
    let attempt = *attempt;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if frame.persistence().attempt() == Some(attempt)
            && let Some(result) = frame.persistence().result()
        {
            assert!(
                matches!(result, EditorPersistenceResult::Opened {
                document: opened, ..
            } if opened == document),
                "{result:?}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Imported source Reopen did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )],
        ))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    let before_reopen = tab.revision;
    tab = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert_ne!(tab.revision.source_id(), before_reopen.source_id());
    assert_eq!(tab.saved_revision, tab.revision);
    assert_eq!(tab.history, DocumentHistoryStatus::default());
    assert!(!tab.dirty);
    assert_text_frame(&editor, &tab, changed);
    assert_eq!(editor.source_tool_artifact_view(), inventory);

    // Retain a real redo entry across withdrawal and reclamation of the original template.
    let revision = tab.revision;
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [
                EditorAction::Extension(EditorExtensionCommand::new::<SampleText>(
                    document,
                    revision,
                    SetText("Project-owned history after template import.\n".into()),
                )),
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision.next(),
                }),
            ],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[
            applied_text(document, revision.next(), true),
            applied_text(document, revision.next().next(), false),
        ]
    );
    editor.drive_editor_frame(Duration::ZERO);
    tab = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert_eq!(
        tab.history,
        DocumentHistoryStatus {
            undo_depth: 0,
            redo_depth: 1,
        }
    );
    assert_text_frame(&editor, &tab, changed);

    // An unknown sibling is retained; it does not extend the bounded artifact inventory.
    let unknown = format!("{}/samples/unlisted.txt", owner1.project_path());
    fs::write(project.root().join(&unknown), b"unlisted artifact sibling").unwrap();
    installed_bytes.insert(unknown.clone(), b"unlisted artifact sibling".to_vec());
    let initial_reclamation = editor.source_tool_reclamation_view();
    assert_eq!(initial_reclamation.phase, ReclamationPhase::Idle);
    assert!(inventory.reclamations.is_empty());
    // Nonselected and disabled graph members are still installed, including the dependent.
    for entry in [&a1, &a2, &dependent] {
        assert!(matches!(
            editor.prepare_source_tool_reclamation(
                &inventory.basis,
                &registered.basis,
                root.share(),
                &entry.installed.installation,
            ),
            Err(ReclamationError::StillInstalled)
        ));
    }
    assert!(matches!(
        editor.prepare_source_tool_withdrawal(
            &registered.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::RejectDependents,
        ),
        Err(EditorSourceToolSelectionError::RequiredDependents)
    ));
    let cancelled = editor
        .prepare_source_tool_withdrawal(
            &registered.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::Cascade,
        )
        .unwrap();
    assert_installations(cancelled.installations(), &[a1.clone(), dependent.clone()]);
    drop(cancelled);
    assert_eq!(editor.source_tool_selection_view(), registered);
    let prepared = editor
        .prepare_source_tool_withdrawal(
            &registered.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::Cascade,
        )
        .unwrap();
    let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
    let withdrawn = settle_selection(&mut editor);
    assert!(matches!(withdrawn.operation.as_ref(), Some(operation)
        if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Withdrawn)));
    assert_installations(&withdrawn.installed, &[a2.clone()]);
    assert!(withdrawn.desired.is_none() && withdrawn.active_origin.is_none());
    assert_text_frame(&editor, &tab, changed);
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        changed
    );
    // Graph withdrawal has no physical reclamation semantics.
    assert_installed_bytes(&project, &installed_bytes);
    assert_eq!(editor.source_tool_artifact_view(), inventory);
    assert_eq!(editor.source_tool_selection_view(), withdrawn);
    assert_eq!(editor.document_query_basis(scene).unwrap(), scene_basis);
    assert_eq!(
        editor.current_frame().workspace().document(scene),
        Some(&scene_tab)
    );
    assert_history(&editor, scene, 0, 0);
    assert_eq!(project.read(), original());
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_no_execution(&editor);

    let graph_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    let prepared = editor
        .prepare_source_tool_reclamation(
            &inventory.basis,
            &withdrawn.basis,
            root.share(),
            &a1.installed.installation,
        )
        .unwrap();
    assert!(prepared.creation_custody());
    let review = prepared.record().clone();
    assert_ne!(review.id, [0; 32]);
    assert_eq!(review.installation, a1.installed.installation);
    assert_eq!(review.phase, ReclamationRecordPhase::Intent);
    assert_eq!(review.members.len(), owner1.members().len());
    assert_eq!(
        review
            .members
            .iter()
            .map(|member| (member.path.to_string(), member.state))
            .collect::<BTreeMap<_, _>>(),
        owner1
            .members()
            .iter()
            .map(|member| (member.path().to_owned(), ReclamationMemberState::Pending))
            .collect::<BTreeMap<_, _>>()
    );
    drop(prepared);
    assert_eq!(editor.source_tool_reclamation_view(), initial_reclamation);
    assert_eq!(editor.source_tool_artifact_view(), inventory);
    assert_eq!(editor.source_tool_selection_view(), withdrawn);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        inventory_bytes
    );
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    assert_text_frame(&editor, &tab, changed);

    // Even an identical graph publication invalidates the original graph observation.
    let stale = editor
        .prepare_source_tool_reclamation(
            &inventory.basis,
            &withdrawn.basis,
            root.share(),
            &a1.installed.installation,
        )
        .unwrap();
    let prepared = editor
        .prepare_source_tool_installations(&withdrawn.basis, root.share(), &[request(&a2)])
        .unwrap();
    let original_graph_basis = withdrawn.basis.clone();
    editor.register_source_tool_installations(prepared).unwrap();
    let withdrawn = settle_selection(&mut editor);
    assert_ne!(withdrawn.basis, original_graph_basis);
    assert_installations(&withdrawn.installed, &[a2.clone()]);
    assert!(matches!(
        editor.confirm_source_tool_reclamation(stale),
        Err(ReclamationError::StaleBasis)
    ));
    assert_eq!(editor.source_tool_reclamation_view(), initial_reclamation);
    assert_eq!(editor.source_tool_artifact_view(), inventory);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);

    // A fresh inventory observation also cannot silently refresh an older removal review.
    let stale = editor
        .prepare_source_tool_reclamation(
            &inventory.basis,
            &withdrawn.basis,
            root.share(),
            &a1.installed.installation,
        )
        .unwrap();
    let prepared = editor
        .prepare_source_tool_artifact_observation(&inventory.basis, &a1.installed.installation)
        .unwrap();
    let original_artifact_basis = inventory.basis.clone();
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let inventory = settle_artifacts(&mut editor);
    assert_ne!(inventory.basis, original_artifact_basis);
    assert_artifact_outcome(
        &inventory,
        &attempt,
        EditorSourceToolArtifactOutcome::Observed,
    );
    assert!(matches!(
        editor.confirm_source_tool_reclamation(stale),
        Err(ReclamationError::StaleBasis)
    ));
    assert_eq!(editor.source_tool_reclamation_view(), initial_reclamation);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        inventory_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    assert_text_frame(&editor, &tab, changed);

    let prepared = editor
        .prepare_source_tool_reclamation(
            &inventory.basis,
            &withdrawn.basis,
            root.share(),
            &a1.installed.installation,
        )
        .unwrap();
    let reviewed = prepared.record().clone();
    assert_eq!(reviewed.members, review.members);
    assert_installed_bytes(&project, &installed_bytes);
    let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
    let reclaimed = settle_reclamation(&mut editor, &attempt);
    assert_eq!(reclaimed.outcome, Some(ReclamationOutcome::Complete));
    assert!(reclaimed.error.is_none(), "{reclaimed:?}");
    let results = reclaimed.record.unwrap();
    assert_eq!(results.id, reviewed.id);
    assert_eq!(results.installation, reviewed.installation);
    assert_eq!(results.phase, ReclamationRecordPhase::Results);
    let mut expected_results = reviewed;
    expected_results.phase = ReclamationRecordPhase::Results;
    for member in &mut expected_results.members {
        member.state = ReclamationMemberState::Removed;
        let path = format!("{}/{}", owner1.project_path(), member.path);
        assert!(
            !project.root().join(&path).exists(),
            "Selected artifact survived: {path}"
        );
        installed_bytes.remove(&path).unwrap();
    }
    assert_eq!(results, expected_results);
    let inventory = settle_artifacts(&mut editor);
    assert_eq!(inventory.reclamations, [results.clone()]);
    let reclaimed_bytes = fs::read(project.root().join(ARTIFACT_PATH)).unwrap();
    assert_ne!(reclaimed_bytes, inventory_bytes);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_eq!(
        fs::read(project.root().join(&unknown)).unwrap(),
        b"unlisted artifact sibling"
    );
    assert_installed_bytes(&project, &installed_bytes);
    assert_text_frame(&editor, &tab, changed);
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        changed
    );
    assert_eq!(editor.source_tool_selection_view(), withdrawn);
    assert_eq!(editor.document_query_basis(scene).unwrap(), scene_basis);
    assert_eq!(
        editor.current_frame().workspace().document(scene),
        Some(&scene_tab)
    );
    assert_history(&editor, scene, 0, 0);
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_no_execution(&editor);
    close(&mut editor);
    drop(editor);

    let mut reopened = project.open();
    let observed = settle_selection(&mut reopened);
    assert_installations(&observed.installed, &[a2]);
    assert!(observed.desired.is_none() && observed.active_origin.is_none());
    let inventory = settle_artifacts(&mut reopened);
    assert_eq!(inventory.phase, EditorSourceToolArtifactPhase::Idle);
    assert!(inventory.error.is_none());
    assert_eq!(inventory.reclamations, [results.clone()]);
    assert_eq!(inventory.installations.len(), expected_inventory.len());
    for expected in &expected_inventory {
        assert!(
            inventory
                .installations
                .iter()
                .any(|entry| entry.installed == expected.installed)
        );
    }
    let fresh_reclamation = reopened.source_tool_reclamation_view();
    assert_eq!(fresh_reclamation.phase, ReclamationPhase::Idle);
    assert!(fresh_reclamation.attempt.is_none() && fresh_reclamation.outcome.is_none());
    assert!(!fresh_reclamation.physical_work_pending);
    assert!(!fresh_reclamation.record_publication_pending);
    reopened
        .register_extension_document::<SampleText>()
        .unwrap();
    let fresh_document = reopened
        .open_extension_document::<SampleText>(source)
        .unwrap();
    let fresh = reopened.current_frame();
    let fresh_tab = fresh.workspace().document(fresh_document).unwrap();
    assert_ne!(fresh_tab.revision.source_id(), tab.revision.source_id());
    assert_eq!(fresh_tab.saved_revision, fresh_tab.revision);
    assert!(!fresh_tab.dirty);
    assert_eq!(fresh_tab.history, DocumentHistoryStatus::default());
    assert_eq!(fresh_tab.source_binding.as_ref().unwrap().locator(), source);
    assert_text_frame(&reopened, fresh_tab, changed);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        reclaimed_bytes
    );
    for member in &results.members {
        assert!(
            !project
                .root()
                .join(owner1.project_path())
                .join(member.path.as_ref())
                .exists()
        );
    }
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_eq!(
        fs::read(project.root().join(&unknown)).unwrap(),
        b"unlisted artifact sibling"
    );
    assert_installed_bytes(&project, &installed_bytes);
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_no_execution(&reopened);
    assert!(reopened.source_tool_view().attempt.is_none());
    close(&mut reopened);
    assert_eq!(
        fs::read_to_string(project.root().join(source)).unwrap(),
        changed
    );
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        reclaimed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
}
