//! Actual Session capture, write admission, and physical worker retirement without installed tools.

mod proposal;

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use nara_asset::AssetSourcePath;
use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions, RelativePath,
    TrustMode,
};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringFieldEdit, PrefabDocument, SCENE_COMPONENTS_SCHEMA_OWNER_ID,
    SceneAuthoringContentState, SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage,
    SceneAuthoringRevision, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneFileEncoding, ScenePatchDocument, ScenePatchOperation, SourceDocumentKey,
    SourceDocumentKind,
};
use nara_tasks::{TaskReturnObserver, TaskReturnState};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentDigest, EditorDocumentId,
    EditorDocumentMutation, EditorPersistenceCommand, EditorPersistenceRejection,
    EditorPersistenceRequestKind, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPlayAction, EditorPlayCommand, EditorPlayRejection, EditorPlayRequestResult,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorWorkspaceCommand,
    EditorWorkspaceIntent, EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
};

use super::{
    EditorAuthoredSchemaUpgradeError, EditorAuthoredSchemaUpgradeId,
    EditorAuthoredSchemaUpgradeOutcome, EditorAuthoredSchemaUpgradePhase,
    EditorAuthoredUpgradeWorkingSource,
};
use crate::project_host::{
    EditorProjectCloseProgress,
    authoring::{
        AuthoringDocumentKind, AuthoringProjectAdmissionHost, PreparedAuthoringProject,
        project_authoring_adapters,
    },
    composition::built_in_schema_providers,
    runtime::editor::{
        EditorProjectSession, SaveTestPhase, document_open::EditorDocumentOpenRequestResult,
    },
};

const WAIT: Duration = Duration::from_secs(15);
const UNIVERSE: &str = "390bdfc2-65c0-4ef9-9f24-9fe9e3764a89";
const STARTUP: &str = "scenes/startup.scene.json";
const INACTIVE: &str = "scenes/inactive.scene.json";
const CLOSED: &str = "scenes/closed.scene.json";
const PREFAB: &str = "prefabs/leaf.prefab.json";
const PROFILE_SCENE: &str = "dormant/scenes/closed.scene.json";
const PROFILE_PREFAB: &str = "dormant/prefabs/closed.prefab.json";
const BUNDLE: &str = "schema/authoring.bundle.json";
const SOURCES: [&str; 6] = [
    STARTUP,
    INACTIVE,
    CLOSED,
    PREFAB,
    PROFILE_SCENE,
    PROFILE_PREFAB,
];
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            root: std::env::temp_dir().join(format!(
                "nara_session_authored_upgrade_{}_{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
            )),
        };
        for directory in [
            "assets",
            "scenes",
            "prefabs",
            "schema",
            "dormant/scenes",
            "dormant/prefabs",
        ] {
            fs::create_dir_all(fixture.root.join(directory)).unwrap();
        }
        fs::write(
            fixture.root.join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Authored upgrade Session capture"
source_universe = "{UNIVERSE}"
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
[profiles.dormant.paths]
scenes = "dormant/scenes"
prefabs = "dormant/prefabs"
[profiles.dormant.startup]
default_scene = "closed.scene.json"
"#,
            ),
        )
        .unwrap();
        fs::write(
            fixture.root.join(BUNDLE),
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        for path in SOURCES {
            let record = named_entity(path);
            let encoded = if path.ends_with(".prefab.json") {
                PrefabDocument::from_ordered_roots([record])
                    .to_json_string()
                    .unwrap()
            } else {
                SceneDocument::from_ordered_roots([record])
                    .to_json_string()
                    .unwrap()
            };
            fs::write(fixture.root.join(path), encoded).unwrap();
        }
        fixture
    }

    fn open(&self) -> EditorProjectSession {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(directory(&self.root), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let project = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        EditorProjectSession::open(project, None).unwrap()
    }

    fn run(&self, test: impl FnOnce(&mut EditorProjectSession)) {
        let mut editor = self.open();
        let result = catch_unwind(AssertUnwindSafe(|| {
            wait(&mut editor, |editor| {
                !editor.project_source_index_work_pending()
            });
            assert!(editor.project_source_index_snapshot().is_some());
            test(&mut editor);
        }));
        close(&mut editor);
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.root.join(path)).unwrap()
    }

    fn disk(&self) -> Vec<Vec<u8>> {
        ["nara.toml", BUNDLE]
            .into_iter()
            .chain(SOURCES)
            .map(|path| self.bytes(path))
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let (Ok(root), Ok(temporary)) = (
            self.root.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) else {
            return;
        };
        if root.parent() == Some(temporary.as_path())
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_session_authored_upgrade_")
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn directory(path: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(target_os = "linux")]
    let handle = fs::File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn entity() -> SceneEntityId {
    SceneEntityId::new("target").unwrap()
}

fn name_component() -> ComponentTypeId {
    ComponentTypeId::new("nara.scene.Name")
}

fn named_entity(value: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(entity()).with_component(
        name_component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(value.into()),
        )
        .with_owner(
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap(),
        ),
    )
}

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, value: &str) {
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
        document,
        base_revision: editor.document_query_basis(document).unwrap().revision,
        edit: AuthoringFieldEdit::single(
            entity(),
            name_component(),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("value"),
            ComponentValue::String(value.into()),
        )
        .unwrap(),
    });
    assert!(report.applied, "{report:?}");
}

fn open(
    editor: &mut EditorProjectSession,
    path: &str,
    kind: AuthoringDocumentKind,
) -> EditorDocumentId {
    let index = editor.project_source_index_snapshot().unwrap();
    match editor.open_indexed_authoring_document(index, path, kind) {
        EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("the actual source {path} must open: {other:?}"),
    }
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "Session capture did not settle: {:?}",
            editor.authored_schema_upgrade_view()
        );
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
}

fn awaiting(editor: &mut EditorProjectSession, operation: EditorAuthoredSchemaUpgradeId) {
    wait(editor, |editor| {
        matches!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::AwaitingProposal
                | EditorAuthoredSchemaUpgradePhase::Terminal
        )
    });
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorAuthoredSchemaUpgradePhase::AwaitingProposal,
        "{view:?}"
    );
    assert_eq!(view.outcome, None);
    assert!(!view.work_pending);
    assert!(view.writes_fenced);
    assert!(
        !editor
            .current_frame()
            .authored_schema_upgrade_work_pending()
    );
}

fn cancelled(editor: &EditorProjectSession, operation: EditorAuthoredSchemaUpgradeId) {
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Terminal);
    assert_eq!(
        view.outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Cancelled)
    );
    assert!(!view.work_pending);
    assert!(!view.writes_fenced);
    assert!(matches!(
        editor.authored_schema_upgrade_inputs(operation),
        Err(EditorAuthoredSchemaUpgradeError::Operation)
    ));
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    loop {
        match editor.advance_close() {
            EditorProjectCloseProgress::Complete(receipt) => {
                assert!(receipt.task_shutdown().is_complete());
                assert!(!receipt.has_cleanup_failure());
                return;
            }
            progress => assert!(
                Instant::now() < deadline,
                "Session Close did not physically finish: {progress:?}"
            ),
        }
        thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Debug, PartialEq)]
struct SourceState {
    encoded: String,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    saved_revision: SceneAuthoringRevision,
    saved_content: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    dirty: bool,
}

fn state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    if let Some(slot) = editor.workspace().scene(document) {
        SourceState {
            encoded: slot.session().document().to_json_string().unwrap(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            usage: slot.session().history_usage(),
            saved_revision: slot.saved_revision(),
            saved_content: slot.saved_content_state(),
            saved_digest: slot.saved_digest(),
            dirty: slot.is_dirty(),
        }
    } else {
        let slot = editor.workspace().prefab(document).unwrap();
        SourceState {
            encoded: slot.session().document().to_json_string().unwrap(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            usage: slot.session().history_usage(),
            saved_revision: slot.saved_revision(),
            saved_content: slot.saved_content_state(),
            saved_digest: slot.saved_digest(),
            dirty: slot.is_dirty(),
        }
    }
}

fn assert_writes_rejected(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let revision = editor.document_query_basis(document).unwrap().revision;
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: entity(),
        component: name_component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("must-not-apply".into()),
    }]);
    let batch = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [
                EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch,
                }),
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision,
                }),
                EditorAction::Play(EditorPlayAction::Play {
                    document,
                    base_revision: revision,
                }),
            ],
        ))
        .unwrap();
    for outcome in &batch.outcomes()[..2] {
        let EditorActionOutcome::Workspace(report) = outcome else {
            panic!("fenced mutations must reject at the Session owner: {outcome:?}");
        };
        assert!(!report.applied);
    }
    assert!(
        matches!(
            batch.outcomes()[2],
            EditorActionOutcome::Play(EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy))
        ),
        "{batch:?}"
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
    );
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
            EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
        );
    }
    for intent in [
        EditorWorkspaceIntent::CloseDocument { document },
        EditorWorkspaceIntent::Exit,
    ] {
        assert_eq!(
            editor.request_workspace_intent(intent),
            EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::Busy)
        );
    }
    let index = editor.project_source_index_snapshot().unwrap();
    assert_eq!(
        editor.request_indexed_authoring_document(index, CLOSED, AuthoringDocumentKind::Scene),
        EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-open-busy"
        },
    );
}

#[test]
fn capture_work_exhaustion_returns_no_baseline_and_does_not_change_a_later_operation_policy() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "dirty-before-capacity-refusal");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = editor
            .begin_authored_schema_upgrade_with_limits(
                crate::project_host::AuthoredSchemaUpgradeHostLimits::default()
                    .with_host_work(nara_core::ItemLimit::new(1).unwrap()),
            )
            .unwrap();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.outcome,
            Some(EditorAuthoredSchemaUpgradeOutcome::Failed(
                EditorAuthoredSchemaUpgradeError::WorkLimit
            ))
        );
        assert!(!view.work_pending && !view.writes_fenced);
        assert!(matches!(
            editor.authored_schema_upgrade_inputs(operation),
            Err(EditorAuthoredSchemaUpgradeError::Operation)
        ));
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
        let next = editor.begin_authored_schema_upgrade().unwrap();
        assert_ne!(next, operation);
        awaiting(editor, next);
        editor.cancel_authored_schema_upgrade(next).unwrap();
        cancelled(editor, next);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn queued_browser_refresh_sleeps_behind_the_upgrade_fence_and_resumes_after_cancellation() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let original = editor.project_source_index_snapshot().unwrap().generation();
        let dispatch = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                editor.current_frame().revision(),
                [EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Refresh,
                )],
            ))
            .unwrap();
        assert!(matches!(
            dispatch.outcomes(),
            [EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::RefreshRequested
            )]
        ));
        let project = editor.authoring_project().unwrap();
        assert!(project.source_index_has_pending_work());
        assert!(!project.source_index_scan_pending());

        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        for _ in 0..3 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(!frame.project_browser().has_pending_work());
            assert!(!frame.authored_schema_upgrade_work_pending());
            assert_eq!(
                editor.project_source_index_snapshot().unwrap().generation(),
                original
            );
            assert!(
                editor
                    .authoring_project()
                    .unwrap()
                    .source_index_has_pending_work()
            );
        }

        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
        wait(editor, |editor| {
            editor.project_source_index_snapshot().unwrap().generation() != original
                && !editor.project_source_index_work_pending()
        });
        assert!(!editor.current_frame().project_browser().has_pending_work());
    });
}

#[test]
fn begin_fences_writes_and_retains_all_profile_originals_and_dirty_working_history_until_cancel() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Reopen {
                document: Some(startup)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        wait(editor, |editor| {
            editor.persistence_view().result().is_some()
        });
        let opened = editor.persistence_view().result().unwrap();
        assert!(matches!(
            opened,
            EditorPersistenceResult::Opened { document, .. } if document == startup
        ));
        // An earlier Opened terminal is not an accepted Save for this operation to drain.
        let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
        let prefab = open(editor, PREFAB, AuthoringDocumentKind::Prefab);
        let documents = [startup, inactive, prefab];
        for (index, document) in documents.into_iter().enumerate() {
            edit(editor, document, &format!("working-{index}"));
        }
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: startup
                })
                .applied
        );
        editor.drive_editor_frame(Duration::ZERO);
        #[cfg(feature = "presentation-2d")]
        editor.configure_editor_preview(nara_render::Extent2d::new(320, 180));
        let before = documents.map(|document| state(editor, document));
        let scene_sources = [startup, inactive].map(|document| {
            editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .shared_document()
        });
        let prefab_source = editor
            .workspace()
            .prefab(prefab)
            .unwrap()
            .session()
            .shared_document();
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Draining);
        assert!(view.work_pending && view.writes_fenced);
        assert_writes_rejected(editor, startup);

        for command in [
            EditorWorkspaceCommand::SetActiveDocument { document: inactive },
            EditorWorkspaceCommand::SelectEntity {
                document: inactive,
                entity: Some(entity()),
            },
            EditorWorkspaceCommand::SetActiveDocument { document: startup },
        ] {
            assert!(editor.apply_workspace_command(command).applied);
        }
        assert!(matches!(
            editor.authored_schema_upgrade_inputs(operation),
            Err(EditorAuthoredSchemaUpgradeError::Operation)
        ));
        awaiting(editor, operation);
        assert_eq!(
            editor.authored_schema_upgrade_view().documents,
            documents.len()
        );
        assert_eq!(editor.authored_schema_upgrade_view().sources, SOURCES.len());
        {
            let inputs = editor.authored_schema_upgrade_inputs(operation).unwrap();
            let expected_bundle = AuthoringSchemaBundle::from_trusted_provider_definitions(
                built_in_schema_providers(),
            )
            .unwrap();
            assert_eq!(inputs.schema_bundle(), &expected_bundle);
            assert_eq!(inputs.source_count(), SOURCES.len());
            assert_eq!(inputs.sources().len(), SOURCES.len());
            let mut paths: Vec<_> = inputs.sources().map(|source| source.project_path).collect();
            paths.sort_unstable();
            let mut expected_paths = SOURCES;
            expected_paths.sort_unstable();
            assert_eq!(paths, expected_paths);
            for source in inputs.sources() {
                let kind = if source.project_path.ends_with(".prefab.json") {
                    SourceDocumentKind::Prefab
                } else {
                    SourceDocumentKind::Scene
                };
                assert_eq!(
                    source.key,
                    &SourceDocumentKey::project_location(
                        UNIVERSE.parse().unwrap(),
                        AssetSourcePath::new(source.project_path).unwrap(),
                        kind,
                    )
                );
                assert_eq!(source.encoding, SceneFileEncoding::Json);
                assert_eq!(source.original_disk, fixture.bytes(source.project_path));
                match source.working {
                    EditorAuthoredUpgradeWorkingSource::Scene(working) => {
                        let index = [STARTUP, INACTIVE]
                            .iter()
                            .position(|path| *path == source.project_path)
                            .unwrap();
                        assert_eq!(source.document, Some(documents[index]));
                        assert!(std::ptr::eq(working, scene_sources[index].as_ref()));
                        assert_eq!(
                            working.entities[0].components[&name_component()]
                                .value
                                .as_str(),
                            Some(format!("working-{index}").as_str())
                        );
                        assert_ne!(
                            working.to_json_string().unwrap().as_bytes(),
                            source.original_disk
                        );
                    }
                    EditorAuthoredUpgradeWorkingSource::Prefab(working) => {
                        assert_eq!(source.project_path, PREFAB);
                        assert_eq!(source.document, Some(prefab));
                        assert!(std::ptr::eq(working, prefab_source.as_ref()));
                        assert_eq!(
                            working.entities[0].components[&name_component()]
                                .value
                                .as_str(),
                            Some("working-2")
                        );
                        assert_ne!(
                            working.to_json_string().unwrap().as_bytes(),
                            source.original_disk
                        );
                    }
                    EditorAuthoredUpgradeWorkingSource::Saved(saved) => {
                        assert!(
                            [CLOSED, PROFILE_SCENE, PROFILE_PREFAB].contains(&source.project_path)
                        );
                        assert_eq!(source.document, None);
                        assert!(std::ptr::eq(saved, source.original_disk));
                    }
                }
            }
        }
        for _ in 0..3 {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(
                !editor
                    .current_frame()
                    .authored_schema_upgrade_work_pending()
            );
            assert!(editor.authored_schema_upgrade_view().writes_fenced);
        }
        assert_writes_rejected(editor, startup);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        assert_eq!(editor.persistence_view().result(), Some(opened));
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert!(Arc::ptr_eq(
            &editor
                .workspace()
                .prefab(prefab)
                .unwrap()
                .session()
                .shared_document(),
            &prefab_source
        ));
        for (index, document) in documents.into_iter().enumerate() {
            if index < 2 {
                assert!(Arc::ptr_eq(
                    &editor
                        .workspace()
                        .scene(document)
                        .unwrap()
                        .session()
                        .shared_document(),
                    &scene_sources[index]
                ));
            }
            let report = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: editor.document_query_basis(document).unwrap().revision,
            });
            assert!(report.applied, "{report:?}");
            assert_eq!(editor.workspace().document_is_dirty(document), Some(false));
            let report = editor.apply_workspace_command(EditorWorkspaceCommand::Redo {
                document,
                base_revision: editor.document_query_basis(document).unwrap().revision,
            });
            assert!(report.applied, "{report:?}");
            let restored = state(editor, document);
            assert_eq!(restored.encoded, before[index].encoded);
            assert_eq!(restored.history, before[index].history);
            assert_eq!(restored.content, before[index].content);
            assert_eq!(restored.saved_digest, before[index].saved_digest);
        }
        assert_eq!(fixture.disk(), disk);
    });
}

fn saved_before_begin(phase: SaveTestPhase) {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "saved-before-capture");
        let before = state(editor, document);
        let original = fixture.bytes(STARTUP);
        let project_root = editor.source_project_root();
        let path = RelativePath::new(STARTUP).unwrap();
        let predecessor = project_root.open_file_exact(&path).unwrap();
        let gate = editor.hold_next_save(phase);
        let EditorPersistenceRequestResult::Accepted { attempt } = editor.request_persistence(EditorPersistenceCommand::Save { document: Some(document) }) else {
            panic!("the original Save must be accepted");
        };
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        assert!(gate.wait_until_started());
        for _ in 0..3 {
            editor.drive_editor_frame(Duration::ZERO);
            let view = editor.authored_schema_upgrade_view();
            assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Draining);
            assert!(view.work_pending && view.writes_fenced);
            assert!(editor.authored_schema_upgrade.pending.is_none());
            assert!(editor.persistence_view().result().is_none());
            assert_eq!(state(editor, document), before);
            assert_eq!(editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }), EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy));
        }
        if phase == SaveTestPhase::BeforePublication {
            assert_eq!(fixture.bytes(STARTUP), original);
        } else {
            assert_ne!(fixture.bytes(STARTUP), original);
            assert_ne!(project_root.open_file_exact(&path).unwrap().identity(), predecessor.identity());
        }
        gate.release();

        // Observe the real Save installation before the upgrade consumes its exact attempt.
        // A full maintenance frame would consume it in the same turn and hide this Busy window.
        let deadline = Instant::now() + WAIT;
        while editor.persistence_view().result().is_none() {
            editor.advance_persistence();
            assert!(Instant::now() < deadline, "the accepted Save did not physically return");
            thread::sleep(Duration::from_millis(1));
        }
        let saved = editor.persistence_view().result().unwrap();
        assert!(matches!(saved, EditorPersistenceResult::Saved { document: saved_document, revision, .. } if saved_document == document && revision == before.revision), "{saved:?}");
        assert_eq!(editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }), EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy));
        assert_eq!(editor.persistence_view().result(), Some(saved));
        assert_eq!(editor.workspace().document_is_dirty(document), Some(false));
        let saved_state = state(editor, document);
        assert_eq!(saved_state.encoded, before.encoded);
        assert_eq!(saved_state.history, before.history);
        assert_eq!(saved_state.usage, before.usage);
        assert_eq!(saved_state.saved_content, before.content);
        let published = project_root.open_file_exact(&path).unwrap();
        let saved_bytes = fixture.bytes(STARTUP);
        assert_ne!(published.identity(), predecessor.identity());
        assert_ne!(saved_bytes, original);
        let installed = editor.authoring_source_selection(document).unwrap();
        let source = installed.receipt.as_ref().unwrap();
        assert_eq!(source.identity(), published.identity());
        assert_eq!(source.digest(), ContentDigest::of_bytes(&saved_bytes));
        awaiting(editor, operation);
        {
            let inputs = editor.authored_schema_upgrade_inputs(operation).unwrap();
            let captured = inputs.sources().find(|source| source.project_path == STARTUP).unwrap();
            assert_eq!(captured.original_disk, saved_bytes);
            assert_eq!(captured.document, Some(document));
            let EditorAuthoredUpgradeWorkingSource::Scene(working) = captured.working else {
                panic!("the saved open Scene keeps its actual working document");
            };
            assert_eq!(working, editor.workspace().scene(document).unwrap().session().document());
        }
        assert_eq!(editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }), EditorPersistenceRequestResult::Acknowledged { attempt });
        assert!(editor.authored_schema_upgrade_view().writes_fenced);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(state(editor, document), saved_state);
        assert_eq!(fixture.bytes(STARTUP), saved_bytes);
    });
}

#[test]
fn save_accepted_before_begin_finishes_publication_and_installs_its_new_leaf_before_capture() {
    saved_before_begin(SaveTestPhase::BeforePublication);
}

#[test]
fn published_save_held_before_return_cannot_supply_a_baseline_or_lose_its_acknowledgement() {
    saved_before_begin(SaveTestPhase::AfterPublication);
}

#[test]
fn rejected_accepted_save_finishes_as_persistence_failure_without_capturing_a_baseline() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "must-stay-dirty");
        let before = state(editor, document);
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        let EditorPersistenceRequestResult::Accepted { attempt } =
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document),
            })
        else {
            panic!("Save admission precedes the external change");
        };
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        assert!(gate.wait_until_started());
        let mut external = fixture.bytes(STARTUP);
        external.push(b'\n');
        fs::write(fixture.root.join(STARTUP), &external).unwrap();
        gate.release();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.outcome,
            Some(EditorAuthoredSchemaUpgradeOutcome::Failed(
                EditorAuthoredSchemaUpgradeError::Persistence
            ))
        );
        assert!(!view.work_pending && !view.writes_fenced);
        assert_eq!(view.sources, 0);
        assert!(matches!(
            editor.authored_schema_upgrade_inputs(operation),
            Err(EditorAuthoredSchemaUpgradeError::Operation)
        ));
        assert_eq!(
            editor.persistence_view().result(),
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason: EditorPersistenceRejection::TargetChanged,
            })
        );
        assert_eq!(editor.persistence_view().attempt(), Some(attempt));
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.bytes(STARTUP), external);
        assert!(
            editor
                .authored_schema_upgrade_diagnostics()
                .unwrap()
                .has_errors()
        );
    });
}

struct HeldCapture {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl HeldCapture {
    fn arm(editor: &mut EditorProjectSession) -> Self {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let held = Self {
            entered,
            release: Some(release),
        };
        assert!(
            editor
                .authored_schema_upgrade
                .next_capture_gate
                .replace(Box::new(move || {
                    let _ = entered_tx.send(());
                    release_rx
                        .recv_timeout(WAIT * 2)
                        .expect("release the actual completed capture before worker return");
                }))
                .is_none()
        );
        held
    }

    fn wait_until_entered(&self, editor: &mut EditorProjectSession) -> TaskReturnObserver {
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            match self.entered.try_recv() {
                Ok(()) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("the capture did not enter its completion gate")
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            assert!(
                Instant::now() < deadline,
                "the actual capture did not finish: {:?}",
                editor.authored_schema_upgrade_view()
            );
            thread::sleep(Duration::from_millis(1));
        }
        let pending = editor
            .authored_schema_upgrade
            .pending
            .as_ref()
            .expect("capture retains its worker");
        pending.return_observer()
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }

    fn assert_pending(&self, editor: &EditorProjectSession, returned: &TaskReturnObserver) {
        assert!(self.release.is_some());
        assert_eq!(returned.state(), TaskReturnState::Pending);
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Capturing);
        assert!(view.work_pending && view.writes_fenced);
        assert_eq!(view.outcome, None);
        assert!(matches!(
            editor.authored_schema_upgrade_inputs(view.operation.unwrap()),
            Err(EditorAuthoredSchemaUpgradeError::Operation)
        ));
    }
}

impl Drop for HeldCapture {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn cancelling_a_started_capture_keeps_work_and_writes_fenced_until_physical_return() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "dirty-at-cancel");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        let mut held = HeldCapture::arm(editor);
        let returned = held.wait_until_entered(editor);
        held.assert_pending(editor, &returned);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        for _ in 0..3 {
            let started = Instant::now();
            editor.drive_editor_frame(Duration::ZERO);
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Cancel must not wait on the capture worker"
            );
            held.assert_pending(editor, &returned);
            assert!(
                editor
                    .current_frame()
                    .authored_schema_upgrade_work_pending()
            );
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(document)
                }),
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
            );
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);
        }
        held.release();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        assert_eq!(returned.state(), TaskReturnState::Returned);
        cancelled(editor, operation);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
        edit(editor, document, "editing-resumed");
    });
}

#[test]
fn owner_close_waits_for_the_actual_capture_return_without_publishing_or_installing_its_inputs() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "dirty-at-close");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        let mut held = HeldCapture::arm(editor);
        let returned = held.wait_until_entered(editor);
        for _ in 0..3 {
            let started = Instant::now();
            let progress = editor.advance_close();
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Close must poll actual capture retirement"
            );
            assert!(matches!(
                progress,
                EditorProjectCloseProgress::Incomplete { .. }
            ));
            assert!(!progress.task_shutdown().is_complete());
            held.assert_pending(editor, &returned);
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);
        }
        held.release();
        close(editor);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        cancelled(editor, operation);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
        assert_eq!(
            editor.begin_authored_schema_upgrade(),
            Err(EditorAuthoredSchemaUpgradeError::Closing)
        );
    });
}
