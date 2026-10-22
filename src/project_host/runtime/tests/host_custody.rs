use super::*;

use nara_tooling::{
    EditorDocumentId, EditorPersistenceRejection, EditorPersistenceRequestResult,
    EditorPlayCommand, EditorPlayRejection, EditorPlayRequestResult,
};
#[cfg(windows)]
use nara_tooling::{
    EditorDocumentKind, EditorPersistenceFailureStage, EditorPersistenceRequestKind,
};

// Recovery to an empty quarantine requires Windows exact-handle temporary discard. Unix
// orphan preservation is verified by nara_fs; the non-unlink admission test below stays portable.
#[cfg(windows)]
#[test]
fn terminal_failed_scene_and_prefab_saves_keep_cleanup_custody_after_session_drop() {
    for (kind, begin_close) in [
        (EditorDocumentKind::Scene, false),
        (EditorDocumentKind::Prefab, false),
        (EditorDocumentKind::Scene, true),
        (EditorDocumentKind::Prefab, true),
    ] {
        let project = seeded_project("host-custody-terminal-save");
        let guard;
        let mut editor = open_generic_editor_with_ready_index(&project);
        let (document, path) = open_kind(&project, &mut editor, kind);
        let target = fs::read(&path).unwrap();
        let entries = directory_entries(&path);
        make_authoring_name_dirty(&mut editor, document, "dirty-unsaved");
        let before = editor.drive_editor_frame(Duration::ZERO);
        guard = CleanupGuard::new(arm_failed_cleanup(&mut editor, kind));
        fail_save(&mut editor, document);
        assert_eq!(editor.current_frame().workspace(), before.workspace());
        assert_eq!(fs::read(&path).unwrap(), target);
        assert_eq!(directory_entries(&path), entries + 1);

        if begin_close {
            assert!(matches!(
                editor.advance_close(),
                editor::EditorProjectCloseProgress::Incomplete { .. }
            ));
            assert_eq!(directory_entries(&path), entries + 1);
        }
        drop(editor);
        assert_cleanup_is_retained(&guard);
        assert_eq!(fs::read(&path).unwrap(), target);
        guard.release_and_drain();
        assert_eq!(directory_entries(&path), entries);
        assert_eq!(fs::read(&path).unwrap(), target);
    }
}

#[cfg(windows)]
#[test]
fn failed_and_stale_reopen_preserve_failed_save_cleanup_through_session_drop() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for stale in [false, true] {
            let project = seeded_project("host-custody-reopen-drop");
            let guard;
            let mut editor = open_generic_editor_with_ready_index(&project);
            let (document, path) = open_kind(&project, &mut editor, kind);
            let target = fs::read(&path).unwrap();
            let entries = directory_entries(&path);
            make_authoring_name_dirty(&mut editor, document, "dirty-before-reopen");
            guard = CleanupGuard::new(arm_failed_cleanup(&mut editor, kind));
            fail_save(&mut editor, document);
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
                EditorPersistenceRequestResult::Accepted
            );
            let control = if stale {
                Some(editor.hold_next_reopen(editor::ReopenTestPhase::AfterPreparation))
            } else {
                fs::write(&path, b"{ malformed").unwrap();
                None
            };
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document)
                }),
                EditorPersistenceRequestResult::Accepted
            );
            editor.drive_editor_frame(Duration::ZERO);
            if let Some(control) = control {
                assert!(control.wait_until_started());
                make_authoring_name_dirty(&mut editor, document, "later-edit");
                editor.drive_editor_frame(Duration::ZERO);
                control.release();
            }
            let before = editor.current_frame();
            let completed = drive_persistence_to_terminal(&mut editor);
            let expected = if stale {
                EditorPersistenceResult::Rejected {
                    request: EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    reason: EditorPersistenceRejection::StaleRevision,
                }
            } else {
                EditorPersistenceResult::Failed {
                    request: EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    stage: EditorPersistenceFailureStage::Decode,
                }
            };
            assert_eq!(completed.persistence().result(), Some(expected));
            assert_eq!(completed.workspace(), before.workspace());
            assert_eq!(directory_entries(&path), entries + 1);
            if !stale {
                fs::write(&path, &target).unwrap();
            }
            assert_eq!(fs::read(&path).unwrap(), target);

            drop(editor);
            assert_cleanup_is_retained(&guard);
            guard.release_and_drain();
            assert_eq!(directory_entries(&path), entries);
            assert_eq!(fs::read(&path).unwrap(), target);
        }
    }
}

#[cfg(windows)]
#[test]
fn document_replacement_rejects_until_failed_cleanup_retires_then_opens_the_new_lane() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let project = seeded_project("host-custody-replacement");
        let guard;
        let mut editor = open_generic_editor_with_ready_index(&project);
        let (document, path) = open_kind(&project, &mut editor, kind);
        let target = fs::read(&path).unwrap();
        let entries = directory_entries(&path);
        make_authoring_name_dirty(&mut editor, document, "retained-dirty");
        guard = CleanupGuard::new(arm_failed_cleanup(&mut editor, kind));
        fail_save(&mut editor, document);
        let index = editor.project_source_index_snapshot().unwrap();
        let (source, source_kind) = successor(kind);
        assert!(
            matches!(editor.open_indexed_authoring_document(Arc::clone(&index), source, source_kind),
            editor::EditorDocumentOpenRequestResult::AwaitingClose { document: closing } if closing == document)
        );
        let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
        assert_eq!(
            editor.resolve_workspace_intent(prompt, nara_tooling::EditorCloseDecision::Discard),
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        );
        let blocked = editor.drive_editor_frame(Duration::ZERO);
        assert!(matches!(
            blocked.workspace_intent().result(),
            Some(nara_tooling::EditorWorkspaceIntentResult::Rejected {
                reason: nara_tooling::EditorWorkspaceIntentRejection::PersistenceRejected(
                    EditorPersistenceRejection::CleanupPending
                ),
                ..
            })
        ));
        assert_eq!(editor.workspace().active_document(), Some(document));
        assert!(editor.workspace().document_is_dirty(document).unwrap());
        assert_eq!(fs::read(&path).unwrap(), target);
        assert_eq!(directory_entries(&path), entries + 1);

        guard.release();
        assert!(editor.acknowledge_workspace_intent_result());
        assert!(
            matches!(editor.open_indexed_authoring_document(index, source, source_kind),
            editor::EditorDocumentOpenRequestResult::AwaitingClose { document: closing } if closing == document)
        );
        let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
        assert_eq!(
            editor.resolve_workspace_intent(prompt, nara_tooling::EditorCloseDecision::Discard),
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert_ne!(editor.workspace().active_document(), Some(document));
        assert!(editor.workspace().document_kind(document).is_none());
        assert_eq!(directory_entries(&path), entries);
        assert_eq!(fs::read(&path).unwrap(), target);
        assert!(matches!(
            drive_editor_close_to_completion(&mut editor),
            editor::EditorProjectCloseProgress::Complete(_)
        ));
    }
}

#[test]
fn session_close_rejects_new_file_and_play_admission_before_and_after_physical_retirement() {
    let project = seeded_project("host-custody-closed-admission");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "never-published");
    let index = editor.project_source_index_snapshot().unwrap();
    let before = editor.drive_editor_frame(Duration::ZERO);
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::BeforeRead);
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    assert!(matches!(
        editor.advance_close(),
        editor::EditorProjectCloseProgress::Incomplete { .. }
    ));
    assert_closed_admission(&mut editor, document, Arc::clone(&index));
    assert!(editor.persistence_view().result().is_none());
    assert_eq!(editor.workspace_model(), before.workspace().clone());
    control.release();
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert_closed_admission(&mut editor, document, index);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert!(!editor.workspace().scene(document).unwrap().is_dirty());
}

#[cfg(all(windows, feature = "runtime-ui"))]
#[test]
fn content_runtime_replacement_transfers_failed_cleanup_and_keeps_original_hosts_usable() {
    let project = TestProject::new("host-custody-content-runtime-replacement");
    let source_path = project.root.join("scenes/startup.scene.json");
    let prefab_path = project.root.join("prefabs/panel.prefab.json");
    let anchor = nara_identity::SceneEntityId::new("prefab-anchor").unwrap();
    let mut instance = SceneEntityRecord::new(anchor.clone());
    instance.prefab = Some(nara_scene::PrefabInstance {
        source: AssetRef::path("panel.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    fs::write(
        &source_path,
        SceneDocument::from_ordered_roots([editable_name_entity("original-scene"), instance])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        &prefab_path,
        editable_authoring_prefab("original-prefab")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let scene_bytes = fs::read(&source_path).unwrap();
    let scene_entries = directory_entries(&source_path);
    let guard;
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let scene_document = editor.workspace().active_document().unwrap();
    let frame = editor.current_frame();
    let scene_revision = editor.workspace().scene(scene_document).unwrap().revision();
    let opened = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: anchor,
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(report) = &opened.outcomes()[1] else {
        panic!("the compiled consumer must open its Prefab source");
    };
    assert!(report.applied, "{:?}", report.diagnostics);
    let prefab_document = report.opened_document.unwrap();
    edit_native_name(&mut editor, scene_document, "unsaved-scene");
    guard = CleanupGuard::new(editor.test_fail_next_save_after_write());
    fail_save(&mut editor, scene_document);
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(directory_entries(&source_path), scene_entries + 1);
    let old_content = editor.test_project_content_revision();
    edit_native_name(&mut editor, prefab_document, "saved-prefab");
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(prefab_document)
        }),
        EditorPersistenceRequestResult::Accepted
    );
    let saved = drive_persistence_to_terminal(&mut editor);
    assert!(
        matches!(saved.persistence().result(), Some(EditorPersistenceResult::Saved { document, .. })
        if document == prefab_document)
    );
    assert!(matches!(
        saved.project_content(),
        EditorProjectContentView::RefreshPending { .. }
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !editor.current_frame().project_content().is_current() {
        assert!(
            Instant::now() < deadline,
            "the compiled content runtime did not replace"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_ne!(editor.test_project_content_revision(), old_content);
    assert_eq!(directory_entries(&source_path), scene_entries + 1);
    assert_eq!(fs::read(&source_path).unwrap(), scene_bytes);
    assert!(
        fs::read_to_string(&prefab_path)
            .unwrap()
            .contains("saved-prefab")
    );
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(scene_document)
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(
        matches!(drive_persistence_to_terminal(&mut editor).persistence().result(),
        Some(EditorPersistenceResult::Opened { document, .. }) if document == scene_document)
    );
    assert_eq!(scene_name_value(&editor, scene_document), "original-scene");
    assert_eq!(directory_entries(&source_path), scene_entries + 1);
    edit_native_name(&mut editor, scene_document, "post-rebuild-edit");
    assert!(editor.workspace().scene(scene_document).unwrap().is_dirty());

    drop(editor);
    assert_cleanup_is_retained(&guard);
    guard.release_and_drain();
    assert_eq!(directory_entries(&source_path), scene_entries);
    assert_eq!(fs::read(&source_path).unwrap(), scene_bytes);
}

#[cfg(all(windows, feature = "runtime-ui"))]
fn edit_native_name(
    editor: &mut editor::EditorProjectSession,
    document: EditorDocumentId,
    value: &str,
) {
    let base_revision = editor.workspace().document_revision(document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: nara_identity::SceneEntityId::new("editable-name").unwrap(),
        component: ComponentTypeId::new("nara.scene.Name"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String(value.to_owned()),
    }]);
    let command = match editor.workspace().document_kind(document).unwrap() {
        EditorDocumentKind::Scene => EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Prefab => EditorWorkspaceCommand::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Extension(_) => {
            unreachable!("fixture contains only Scene and Prefab documents")
        }
    };
    let report = editor.apply_workspace_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn assert_closed_admission(
    editor: &mut editor::EditorProjectSession,
    document: EditorDocumentId,
    index: Arc<crate::project_host::authoring::ProjectSourceIndexSnapshot>,
) {
    for command in [
        EditorPersistenceCommand::Save {
            document: Some(document),
        },
        EditorPersistenceCommand::Reopen {
            document: Some(document),
        },
    ] {
        assert_eq!(
            editor.request_persistence(command),
            EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy)
        );
    }
    for command in [
        EditorPlayCommand::Play,
        EditorPlayCommand::Restart,
        EditorPlayCommand::RunLastGood,
    ] {
        assert_eq!(
            editor.request_play(command),
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
        );
    }
    assert!(matches!(
        editor.open_indexed_authoring_document(
            index,
            "scenes/successor.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene
        ),
        editor::EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-open-busy"
        }
    ));
}

fn seeded_project(label: &str) -> TestProject {
    let project = TestProject::new(label);
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        editable_authoring_scene("scene-original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/successor.scene.json"),
        editable_authoring_scene("scene-successor")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/first.prefab.json"),
        editable_authoring_prefab("prefab-original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/second.prefab.json"),
        editable_authoring_prefab("prefab-successor")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    project
}

#[cfg(windows)]
fn open_kind(
    project: &TestProject,
    editor: &mut editor::EditorProjectSession,
    kind: EditorDocumentKind,
) -> (EditorDocumentId, PathBuf) {
    match kind {
        EditorDocumentKind::Scene => (
            editor.workspace().active_document().unwrap(),
            project.root.join("scenes/startup.scene.json"),
        ),
        EditorDocumentKind::Prefab => {
            let index = editor.project_source_index_snapshot().unwrap();
            let document = opened_document(editor.open_indexed_authoring_document(
                index,
                "prefabs/first.prefab.json",
                crate::project_host::authoring::AuthoringDocumentKind::Prefab,
            ));
            (document, project.root.join("prefabs/first.prefab.json"))
        }
        EditorDocumentKind::Extension(_) => {
            unreachable!("fixture contains only Scene and Prefab sources")
        }
    }
}

#[cfg(windows)]
fn successor(
    kind: EditorDocumentKind,
) -> (
    &'static str,
    crate::project_host::authoring::AuthoringDocumentKind,
) {
    match kind {
        EditorDocumentKind::Scene => (
            "scenes/successor.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
        EditorDocumentKind::Prefab => (
            "prefabs/second.prefab.json",
            crate::project_host::authoring::AuthoringDocumentKind::Prefab,
        ),
        EditorDocumentKind::Extension(_) => {
            unreachable!("fixture contains only Scene and Prefab sources")
        }
    }
}

#[cfg(windows)]
fn arm_failed_cleanup(
    editor: &mut editor::EditorProjectSession,
    kind: EditorDocumentKind,
) -> crate::project_host::persistence::CleanupTestControl {
    match kind {
        EditorDocumentKind::Scene => editor.test_fail_next_save_after_write(),
        EditorDocumentKind::Prefab => editor.test_fail_next_prefab_save_after_write(),
        EditorDocumentKind::Extension(_) => {
            unreachable!("fixture contains only Scene and Prefab sources")
        }
    }
}

#[cfg(windows)]
fn fail_save(editor: &mut editor::EditorProjectSession, document: EditorDocumentId) {
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        drive_persistence_to_terminal(editor).persistence().result(),
        Some(EditorPersistenceResult::Failed {
            request: EditorPersistenceRequestKind::Save,
            document: Some(document),
            stage: EditorPersistenceFailureStage::WriteTemporary,
        })
    );
}

#[cfg(windows)]
fn directory_entries(path: &Path) -> usize {
    fs::read_dir(path.parent().unwrap()).unwrap().count()
}

#[cfg(windows)]
fn assert_cleanup_is_retained(guard: &CleanupGuard) {
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        guard.retained_before + 1
    );
    for _ in 0..3 {
        assert_eq!(
            drive_runtime_quarantine().current_thread_retained(),
            guard.retained_before + 1
        );
    }
}

#[cfg(windows)]
struct CleanupGuard {
    control: crate::project_host::persistence::CleanupTestControl,
    retained_before: usize,
}

#[cfg(windows)]
impl CleanupGuard {
    fn new(control: crate::project_host::persistence::CleanupTestControl) -> Self {
        Self {
            control,
            retained_before: runtime_quarantine_status().current_thread_retained(),
        }
    }
    fn release(&self) {
        self.control.release();
    }
    fn release_and_drain(&self) {
        self.release();
        assert!(
            self.drain(),
            "retained file Host did not release its cleanup after retry"
        );
        assert_eq!(
            runtime_quarantine_status().current_thread_retained(),
            self.retained_before
        );
    }
    fn drain(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime_quarantine_status().current_thread_retained() > self.retained_before {
            let _ = drive_runtime_quarantine();
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        true
    }
}

#[cfg(windows)]
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        self.release();
        let _ = self.drain();
    }
}
