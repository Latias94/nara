use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession,
    },
    project_tooling::host_fs,
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueKind,
    },
    scene::{AuthoringPropertyEdit, SceneEntityId},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringProjection,
        EditorAuthoringQuery, EditorAuthoringQueryLimits, EditorAuthoringQueryPosition,
        EditorAuthoringQueryTarget, EditorAuthoringRow, EditorDocumentMutation,
        EditorDocumentQueryRejection, EditorNavigationAction, EditorPersistenceAction,
        EditorPersistenceRequestResult, EditorPersistenceResult, EditorPropertyRead,
        EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    },
};

use super::super::SCENE;

const TIMEOUT: Duration = Duration::from_secs(30);

pub fn move_pressure_plate(root: &Path) -> Instant {
    let source = root.join(SCENE);
    let original_bytes = fs::read(&source).unwrap();
    let mut host = EditorProjectOpenHost::new();
    let mut open = host.begin(host_fs::writable_directory(root).unwrap(), None);
    let deadline = Instant::now() + TIMEOUT;
    let mut editor = loop {
        match host.advance(open) {
            EditorProjectOpenProgress::Opened(editor) => break editor,
            EditorProjectOpenProgress::Pending(next) => open = next,
            EditorProjectOpenProgress::Rejected(failure) => panic!("{failure:#?}"),
        }
        wait_for_poll(deadline, "opening the generated project");
    };
    let document = editor.current_frame().workspace().active_document.unwrap();
    let camera = SceneEntityId::new("main-camera").unwrap();
    applied(dispatch(
        &mut editor,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document,
            entity: Some(camera.clone()),
        }),
    ));
    let selected = editor
        .current_frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .clone();
    assert_eq!(selected.top_entity(), Some(&camera));
    let assert_same_selection = |editor: &EditorProjectSession| {
        let frame = editor.current_frame();
        assert_eq!(frame.workspace().active_document, Some(document));
        assert_eq!(
            &frame.workspace().active_scene.as_ref().unwrap().selection,
            &selected
        );
    };

    let basis = editor.document_query_basis(document).unwrap();
    let entity = SceneEntityId::new("pressure-plate").unwrap();
    let component = ComponentTypeId::new("push-box.PressurePlate");
    let column = ComponentFieldId::new("column");
    let roots = editor
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: basis.clone(),
                entity: entity.clone(),
                projection: EditorAuthoringProjection::Roots {
                    component: component.clone(),
                },
            }),
            limits: EditorAuthoringQueryLimits::new(16, 8, 4096).unwrap(),
        })
        .unwrap();
    assert!(
        roots.rows.iter().any(|row| matches!(
            row,
            EditorAuthoringRow::Root {
                field,
                kind: ComponentValueKind::I64,
                edit_capability: true,
                ..
            } if *field == column
        )),
        "{roots:?}"
    );
    let request = EditorPropertyRead {
        target: EditorPropertyTarget {
            basis,
            entity,
            component,
            property: ComponentPropertyPath::new(column),
        },
        limits: EditorPropertyReadLimits::new(4096, 16, 1024).unwrap(),
    };
    let observed = editor.read_document_property(request.clone()).unwrap();
    assert_eq!(observed.target, request.target);
    let EditorPropertyValue::Present(ComponentValue::I64(before)) = observed.value else {
        panic!("expected the authored pressure-plate column: {observed:?}");
    };
    assert_eq!(before, 2);
    let after = before + 1;
    let edits = [AuthoringPropertyEdit {
        entity: observed.target.entity,
        component: observed.target.component,
        component_version: ComponentSchemaVersion::ONE,
        property: observed.target.property,
        value: ComponentValue::I64(after),
    }];
    let basis = observed.target.basis;
    let started = Instant::now();
    let prepared = editor
        .prepare_authoring_property_edit(&basis, &edits)
        .unwrap();
    let stale = editor
        .prepare_authoring_property_edit(&basis, &edits)
        .unwrap();
    assert_eq!(prepared.base_revision(), basis.revision);
    assert_eq!(
        prepared
            .preview()
            .read_property(request.clone())
            .unwrap()
            .value,
        EditorPropertyValue::Present(ComponentValue::I64(after))
    );
    assert_eq!(
        editor
            .read_document_property(request.clone())
            .unwrap()
            .value,
        EditorPropertyValue::Present(ComponentValue::I64(before))
    );
    assert_eq!(editor.document_query_basis(document).unwrap(), basis);
    assert_same_selection(&editor);
    applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.read_document_property(request.clone()).unwrap_err(),
        EditorDocumentQueryRejection::StaleRevision
    );
    let rejected = editor.commit_authoring_edit(stale);
    assert!(
        matches!(
            &rejected,
            EditorActionOutcome::Workspace(report)
                if !report.applied && report.diagnostics.iter().any(|diagnostic|
                    diagnostic.code().as_str() == "tooling.workspace-stale-document-revision")
        ),
        "{rejected:?}"
    );

    let assert_column = |editor: &EditorProjectSession, value| {
        let mut current = request.clone();
        current.target.basis = editor.document_query_basis(document).unwrap();
        assert_eq!(
            editor.read_document_property(current).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::I64(value))
        );
    };
    let assert_history = |editor: &EditorProjectSession, undo, redo, dirty| {
        let frame = editor.current_frame();
        let tab = frame.workspace().document(document).unwrap();
        assert_eq!(
            (tab.history.undo_depth, tab.history.redo_depth, tab.dirty),
            (undo, redo, dirty)
        );
    };
    assert_column(&editor, after);
    assert_same_selection(&editor);
    assert_history(&editor, 1, 0, true);

    let base_revision = editor.document_query_basis(document).unwrap().revision;
    applied(dispatch(
        &mut editor,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision,
        }),
    ));
    assert_column(&editor, before);
    assert_same_selection(&editor);
    assert_history(&editor, 0, 1, false);
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    applied(dispatch(
        &mut editor,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document,
            base_revision,
        }),
    ));
    assert_column(&editor, after);
    assert_same_selection(&editor);
    assert_history(&editor, 1, 0, true);
    assert_eq!(fs::read(&source).unwrap(), original_bytes);

    let saved_basis = editor.document_query_basis(document).unwrap();
    let saved = persist(
        &mut editor,
        EditorPersistenceAction::Save {
            document,
            base_revision: saved_basis.revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = saved
    else {
        panic!("the generated Scene Save must succeed: {saved:?}");
    };
    assert_eq!((saved_document, revision), (document, saved_basis.revision));
    let saved_bytes = fs::read(&source).unwrap();
    assert_ne!(saved_bytes, original_bytes);
    assert_eq!(
        ContentDigest::from_parts(digest.encoded_bytes(), digest.hash()),
        ContentDigest::of_bytes(&saved_bytes)
    );
    assert_same_selection(&editor);
    assert_history(&editor, 1, 0, false);

    let target = editor.current_frame().reopen_target();
    let reopened = persist(&mut editor, EditorPersistenceAction::Reopen { target });
    assert!(
        matches!(
            reopened,
            EditorPersistenceResult::Opened { document: reopened_document, revision, digest: reopened_digest }
                if reopened_document == document
                    && revision.source_id() != saved_basis.revision.source_id()
                    && reopened_digest == digest
        ),
        "{reopened:?}"
    );
    let mut previous = request.clone();
    previous.target.basis = saved_basis;
    assert_eq!(
        editor.read_document_property(previous).unwrap_err(),
        EditorDocumentQueryRejection::StaleIncarnation
    );
    assert_column(&editor, after);
    assert_history(&editor, 0, 0, false);
    let frame = editor.current_frame();
    assert_eq!(frame.workspace().active_document, Some(document));
    assert!(
        frame
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .is_empty()
    );
    close(&mut editor, &mut host);
    assert_eq!(fs::read(source).unwrap(), saved_bytes);
    println!(
        "PUSH_BOX_EDITOR_OK column={before}->{after} stale_revision=true stale_incarnation=true selection_preserved_through_save=true"
    );
    started
}

fn close(editor: &mut EditorProjectSession, host: &mut EditorProjectOpenHost) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            break;
        }
        wait_for_poll(deadline, "closing the Editor session");
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let report = host.poll_shutdown();
        if report.is_complete() {
            assert!(!report.has_cleanup_failure(), "{report:?}");
            break;
        }
        wait_for_poll(deadline, "closing the Editor open host");
    }
}

fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn persist(
    editor: &mut EditorProjectSession,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    let response = dispatch(editor, EditorAction::Persistence(action));
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        response
    else {
        panic!("{response:?}");
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.current_frame().persistence();
        if let Some(result) = view.result() {
            assert_eq!(view.attempt(), Some(attempt));
            let acknowledgement = dispatch(
                editor,
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
            );
            assert!(matches!(
                acknowledgement,
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt: acknowledged })
                    if acknowledged == attempt
            ));
            return result;
        }
        wait_for_poll(deadline, "awaiting Scene persistence");
        editor.drive_editor_frame(Duration::ZERO);
    }
}

fn wait_for_poll(deadline: Instant, operation: &str) {
    assert!(Instant::now() < deadline, "timed out while {operation}");
    thread::sleep(Duration::from_millis(1));
}
