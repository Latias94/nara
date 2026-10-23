//! Owner-conformance fixtures exercise retained lanes without changing public Open policy.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use nara_scene::{
    PrefabDocument, SceneDocument, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePlacement, ScenePlacementOwner,
};

use super::*;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_document_custody_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nname = \"Document custody\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        let bundle = nara_reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let file = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            nara_fs::HostCapabilityOptions::new(
                nara_fs::CapabilityRights::ReadWrite,
                nara_fs::TrustMode::TrustedLocal,
            ),
        )
        .unwrap()
    }

    fn editor(&self) -> EditorProjectSession {
        EditorProjectSession::open_trusted_compiled(
            self.capability(),
            super::super::TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap()
    }

    fn add(
        &self,
        editor: &mut EditorProjectSession,
        kind: EditorDocumentKind,
        name: &str,
    ) -> EditorDocumentId {
        let registry = editor
            .trusted_compiled()
            .unwrap()
            .plan
            .schema_validation()
            .registry();
        match kind {
            EditorDocumentKind::Scene => {
                fs::write(
                    self.root.join("scenes").join(name),
                    SceneDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("scenes").unwrap())
                    .unwrap();
                let opened = ScenePersistenceHost::open(&directory, name, registry).unwrap();
                let document = editor
                    .workspace
                    .open_scene_session(name, opened.session)
                    .unwrap()
                    .opened_document
                    .unwrap();
                assert!(
                    editor
                        .persistence_authority
                        .bind_opened_source_digest(&mut editor.workspace, document, opened.digest)
                        .applied
                );
                assert!(
                    editor
                        .scene_lanes
                        .insert(
                            document,
                            EditorScenePersistenceLane {
                                project_path: Arc::from(format!("scenes/{name}")),
                                document,
                                host: editor
                                    .authoring_runtime
                                    .host_custody()
                                    .retain_scene(opened.host)
                                    .unwrap(),
                            }
                        )
                        .is_none()
                );
                document
            }
            EditorDocumentKind::Prefab => {
                fs::write(
                    self.root.join("prefabs").join(name),
                    PrefabDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("prefabs").unwrap())
                    .unwrap();
                let binding = EditorDocumentSourceBinding::new(name.to_owned());
                let opened =
                    PrefabPersistenceHost::open(&directory, binding.clone(), registry).unwrap();
                let document = editor
                    .workspace
                    .open_prefab_session(name, binding, opened.session)
                    .unwrap()
                    .opened_document
                    .unwrap();
                assert!(
                    editor
                        .persistence_authority
                        .bind_opened_source_digest(&mut editor.workspace, document, opened.digest)
                        .applied
                );
                assert!(
                    editor
                        .prefab_lanes
                        .insert(
                            document,
                            EditorPrefabPersistenceLane {
                                project_path: Arc::from(format!("prefabs/{name}")),
                                document,
                                host: editor
                                    .authoring_runtime
                                    .host_custody()
                                    .retain_prefab(opened.host)
                                    .unwrap(),
                            }
                        )
                        .is_none()
                );
                document
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
    }

    fn rebind(&self, editor: &mut EditorProjectSession, document: EditorDocumentId) {
        let registry = editor
            .trusted_compiled()
            .unwrap()
            .plan
            .schema_validation()
            .registry();
        match editor.capture_persistence_lane(document).unwrap() {
            PersistenceLane::Scene(mut lane) => {
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("scenes").unwrap())
                    .unwrap();
                let name = lane.project_path.strip_prefix("scenes/").unwrap();
                let opened = ScenePersistenceHost::open(&directory, name, registry).unwrap();
                lane.host = editor
                    .authoring_runtime
                    .host_custody()
                    .retain_scene(opened.host)
                    .unwrap();
                editor.scene_lanes.insert(document, lane);
            }
            PersistenceLane::Prefab(mut lane) => {
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("prefabs").unwrap())
                    .unwrap();
                let name = lane.project_path.strip_prefix("prefabs/").unwrap();
                let opened = PrefabPersistenceHost::open(
                    &directory,
                    EditorDocumentSourceBinding::new(name.to_owned()),
                    registry,
                )
                .unwrap();
                lane.host = editor
                    .authoring_runtime
                    .host_custody()
                    .retain_prefab(opened.host)
                    .unwrap();
                editor.prefab_lanes.insert(document, lane);
            }
            PersistenceLane::Extension(_) => unreachable!(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_document_custody_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, name: &str) {
    let base_revision = editor.workspace.document_revision(document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new(name).unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let command = match editor.workspace.document_kind(document).unwrap() {
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
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    let report = editor.apply_workspace_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn settle(editor: &mut EditorProjectSession) -> EditorPersistenceResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        editor.advance_persistence();
        if let Some(result) = editor.persistence_result {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "persistence did not physically return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn acknowledge(editor: &mut EditorProjectSession) {
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            return;
        }
        assert!(Instant::now() < deadline, "custody did not retire");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn non_active_scene_and_prefab_persistence_keeps_each_captured_owner() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let preview = editor.scene_preview_document;
        let a = fixture.add(&mut editor, kind, "a.json");
        let b = fixture.add(&mut editor, kind, "b.json");
        editor.workspace.activate_document(a);
        edit(&mut editor, b, "captured-b");
        let captured_revision = editor.workspace.document_revision(b).unwrap();
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted
        );
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        edit(&mut editor, a, "later-a");
        edit(&mut editor, b, "later-b");
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, revision, .. } if document == b && revision == captured_revision)
        );
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        let path = fixture.root.join(directory).join("b.json");
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("captured-b"));
        assert!(!saved.contains("later-b"));
        assert!(
            !fs::read_to_string(fixture.root.join(directory).join("a.json"))
                .unwrap()
                .contains("later-a")
        );
        assert_eq!(editor.workspace.document_is_dirty(b), Some(true));
        let a_revision = editor.workspace.document_revision(a);
        acknowledge(&mut editor);
        let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted
        );
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        edit(&mut editor, a, "reopen-unrelated-a");
        let later_a_revision = editor.workspace.document_revision(a);
        assert_ne!(later_a_revision, a_revision);
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Opened { document, .. } if document == b)
        );
        assert_eq!(editor.workspace.document_is_dirty(b), Some(false));
        assert_eq!(editor.workspace.document_revision(a), later_a_revision);
        assert_eq!(editor.workspace.document_is_dirty(a), Some(true));
        assert_eq!(editor.workspace.active_document(), Some(a));
        assert_eq!(editor.scene_preview_document, preview);
        acknowledge(&mut editor);
        edit(&mut editor, b, "after-reopen-b");
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted
        );
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == b)
        );
        close(&mut editor);
    }
}

#[test]
fn prepared_reopen_rejects_rebound_or_retired_exact_host() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for retired in [false, true] {
            let fixture = Fixture::new();
            let mut editor = fixture.editor();
            let document = fixture.add(&mut editor, kind, "original.json");
            edit(&mut editor, document, "working");
            let revision = editor.workspace.document_revision(document);
            let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document)
                }),
                EditorPersistenceRequestResult::Accepted
            );
            editor.advance_persistence();
            assert!(gate.wait_until_started());
            if retired {
                match editor.capture_persistence_lane(document).unwrap() {
                    PersistenceLane::Scene(lane) => assert!(lane.host.retire()),
                    PersistenceLane::Prefab(lane) => assert!(lane.host.retire()),
                    PersistenceLane::Extension(_) => unreachable!(),
                }
            } else {
                fixture.rebind(&mut editor, document);
            }
            gate.release();
            assert_eq!(
                settle(&mut editor),
                EditorPersistenceResult::Rejected {
                    request: EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    reason: EditorPersistenceRejection::SourceBindingMismatch
                }
            );
            assert_eq!(editor.workspace.document_revision(document), revision);
            assert_eq!(editor.workspace.document_is_dirty(document), Some(true));
            close(&mut editor);
        }
    }
}

#[test]
fn queued_save_cannot_acquire_a_replacement_host_after_admission() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "queued.json");
        edit(&mut editor, document, "must-not-publish");
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted
        );
        match editor.capture_persistence_lane(document).unwrap() {
            PersistenceLane::Scene(lane) => assert!(lane.host.retire()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.retire()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        fixture.rebind(&mut editor, document);
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason: EditorPersistenceRejection::Busy,
            }
        );
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        assert!(
            !fs::read_to_string(fixture.root.join(directory).join("queued.json"))
                .unwrap()
                .contains("must-not-publish")
        );
        assert_eq!(editor.workspace.document_is_dirty(document), Some(true));
        close(&mut editor);
    }
}

fn direct_save(
    lane: &PersistenceLane,
    checkpoint: nara_tooling::EditorPersistenceCheckpoint,
) -> PersistenceSaveOutcome {
    match lane {
        PersistenceLane::Scene(lane) => lane
            .host
            .try_with(|host| host.save_owned(SceneSaveCandidate { checkpoint }, None))
            .unwrap(),
        PersistenceLane::Prefab(lane) => lane
            .host
            .try_with(|host| host.save_owned(PrefabSaveCandidate { checkpoint }, None))
            .unwrap(),
        PersistenceLane::Extension(_) => unreachable!(),
    }
}

#[test]
fn rejected_acknowledgement_marks_only_the_original_published_host_uncertain() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "uncertain.json");
        edit(&mut editor, document, "published");
        let original = editor.capture_persistence_lane(document).unwrap();
        let gate = editor.hold_next_save(SaveTestPhase::AfterPublication);
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted
        );
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        fixture.rebind(&mut editor, document);
        let replacement = editor.capture_persistence_lane(document).unwrap();
        let original_retry = editor
            .persistence_authority
            .capture(&editor.workspace, document)
            .unwrap();
        let replacement_retry = editor
            .persistence_authority
            .capture(&editor.workspace, document)
            .unwrap();
        assert!(editor.workspace.__discard_document(document).applied);
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::PersistenceUncertain { document: target, .. } if target == document)
        );
        assert!(matches!(
            direct_save(&original, original_retry),
            PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
        ));
        assert!(matches!(
            direct_save(&replacement, replacement_retry),
            PersistenceSaveOutcome::Saved { .. }
        ));
        close(&mut editor);
    }
}

#[test]
fn close_releases_only_the_exact_document_lane_and_preserves_preview_binding() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let preview = editor.scene_preview_document;
        let a = fixture.add(&mut editor, kind, "remaining.json");
        let b = fixture.add(&mut editor, kind, "closing.json");
        let remaining = editor.capture_persistence_lane(a).unwrap();
        let closing = editor.capture_persistence_lane(b).unwrap();
        editor.workspace.activate_document(a);
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: b }),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        editor.advance_workspace_retirement();
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: b }
            })
        );
        assert!(editor.capture_persistence_lane(b).is_err());
        assert!(editor.workspace.document_kind(b).is_none());
        match closing {
            PersistenceLane::Scene(lane) => assert!(lane.host.try_with(|_| ()).is_none()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.try_with(|_| ()).is_none()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        match remaining {
            PersistenceLane::Scene(lane) => assert!(lane.host.try_with(|_| ()).is_some()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.try_with(|_| ()).is_some()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        assert_eq!(editor.workspace.active_document(), Some(a));
        assert_eq!(editor.scene_preview_document, preview);
        edit(&mut editor, a, "still-usable");
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(a) }),
            EditorPersistenceRequestResult::Accepted
        );
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == a)
        );
        close(&mut editor);
    }
}

#[cfg(windows)]
struct ReleaseCleanup(crate::project_host::persistence::CleanupTestControl);

#[cfg(windows)]
impl Drop for ReleaseCleanup {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[cfg(windows)]
#[test]
fn held_cleanup_keeps_only_its_document_open_while_another_owner_can_save() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let a = fixture.add(&mut editor, kind, "usable.json");
        let b = fixture.add(&mut editor, kind, "held.json");
        editor.workspace.activate_document(a);
        edit(&mut editor, b, "failed-save-b");
        let cleanup = ReleaseCleanup(match kind {
            EditorDocumentKind::Scene => editor.test_fail_next_save_after_write(b),
            EditorDocumentKind::Prefab => editor.test_fail_next_prefab_save_after_write(b),
            EditorDocumentKind::Extension(_) => unreachable!(),
        });
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted
        );
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Failed { document: Some(document), .. } if document == b)
        );
        acknowledge(&mut editor);
        let discard = |editor: &mut EditorProjectSession| {
            assert_eq!(
                editor
                    .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: b }),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
            assert_eq!(
                editor.resolve_workspace_intent(prompt, EditorCloseDecision::Discard),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            editor.advance_workspace_retirement();
        };
        discard(&mut editor);
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Rejected {
                intent: EditorWorkspaceIntent::CloseDocument { document: b },
                reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                    EditorPersistenceRejection::CleanupPending
                ),
            })
        );
        assert!(editor.acknowledge_workspace_intent_result());
        assert_eq!(editor.workspace.document_kind(b), Some(kind));
        assert!(editor.capture_persistence_lane(b).is_ok());
        edit(&mut editor, a, "saved-a");
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(a) }),
            EditorPersistenceRequestResult::Accepted
        );
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == a)
        );
        assert_eq!(editor.workspace.active_document(), Some(a));
        acknowledge(&mut editor);
        cleanup.0.release();
        discard(&mut editor);
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: b }
            })
        );
        assert!(editor.capture_persistence_lane(b).is_err());
        assert_eq!(editor.workspace.document_kind(b), None);
        close(&mut editor);
    }
}
