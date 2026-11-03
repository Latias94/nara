//! Real Session capture, complete Review and physical preparation retirement.

mod confirmation;
mod process_recovery;
mod publication;
mod selection;

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind,
    AssetSourcePath, StableAssetId,
};
use nara_core::ItemLimit;
use nara_fs::{ContentDigest, RelativePath};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringFieldEdit, AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding,
    PrefabInstanceHop, PrefabOverlay, SCENE_COMPONENTS_SCHEMA_OWNER_ID, SceneAuthoringContentState,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneFileEncoding, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementParent, SourceDocumentKey,
    SourceDocumentKind,
};
use nara_tasks::{TaskReturnObserver, TaskReturnState};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentDigest, EditorDocumentId,
    EditorDocumentMutation, EditorExternalReloadState, EditorNavigationAction,
    EditorPersistenceCommand, EditorPersistenceRejection, EditorPersistenceRequestKind,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorPlayAction, EditorPlayCommand,
    EditorPlayRejection, EditorPlayRequestResult, EditorPrefabExtractionDisposition,
    EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentRejection,
    EditorWorkspaceIntentRequestResult,
};

use super::{
    EditorPrefabExtractionError, EditorPrefabExtractionId, EditorPrefabExtractionOutcome,
    EditorPrefabExtractionPhase, EditorPrefabExtractionRequest,
};
use crate::{
    project_host::{
        EditorProjectCloseProgress,
        authoring::{
            AuthoringDocumentKind, AuthoringProjectAdmissionHost, AuthoringSourceOperationLimits,
            PreparedAuthoringProject, project_authoring_adapters,
        },
        composition::built_in_schema_providers,
        runtime::editor::{
            EditorProjectSession, SaveTestPhase,
            authored_upgrade::EditorAuthoredSchemaUpgradeError,
            document_open::EditorDocumentOpenRequestResult,
        },
    },
    test_host_fs,
};

const WAIT: Duration = Duration::from_secs(15);
const UNIVERSE: &str = "187810d1-5aa5-41c7-a248-2ac4abeb553c";
const P_ID: &str = "023f604e-6c55-499d-8d04-f44dff967199";
const Q_ID: &str = "451c85bf-7c48-4ab1-9706-cae84886f1ee";
const STARTUP: &str = "scenes/startup.scene.json";
const INACTIVE: &str = "scenes/inactive.scene.json";
const P: &str = "prefabs/actor.prefab.json";
const NESTED: &str = "prefabs/nested.prefab.json";
const GUARD: &str = "prefabs/guard.prefab.ron";
const CLOSED: &str = "prefabs/closed.prefab.ron";
const DORMANT: &str = "dormant/scenes/closed.scene.json";
const Q: &str = "prefabs/extracted.prefab.json";
const Q_META: &str = "prefabs/extracted.prefab.json.meta";
const BUNDLE: &str = "schema/authoring.bundle.json";
const ALIAS: &str = "actor.prefab.json";
const SOURCES: [&str; 7] = [STARTUP, INACTIVE, P, NESTED, GUARD, CLOSED, DORMANT];
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn metadata(alias: &str, stable: &str) -> AssetMetaIdentity {
    AssetMetaIdentity::new(
        StableAssetId::parse_str(stable).unwrap(),
        AssetPath::new(alias).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn name_component() -> ComponentTypeId {
    ComponentTypeId::new("nara.scene.Name")
}

fn named_entity(name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        name_component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(name.into()),
        )
        .with_owner(
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap(),
        ),
    )
}

fn instance(name: &str, alias: &str, suppress_child: bool) -> SceneEntityRecord {
    let mut entity = named_entity(name);
    let mut instance = PrefabInstance::forest(AssetRef::path(alias).unwrap());
    if suppress_child {
        let mut overlay = PrefabOverlay::new(key(P, SourceDocumentKind::Prefab));
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("child")));
        instance.overrides = Some(overlay);
    }
    entity.prefab = Some(instance);
    entity
}

fn repeated_scene() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        instance("left", ALIAS, true),
        instance("right", ALIAS, false),
        instance("wrapped", "nested.prefab.json", false),
        // The same local spelling must not be confused with either projected child.
        named_entity("child"),
    ])
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            root: std::env::temp_dir().join(format!(
                "nara_session_prefab_extraction_{}_{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
            )),
        };
        for directory in ["assets", "scenes", "prefabs", "schema", "dormant/scenes"] {
            fs::create_dir_all(fixture.root.join(directory)).unwrap();
        }
        fs::write(
            fixture.root.join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Prefab extraction Session capture"
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
schema_bundle = "{BUNDLE}"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[profiles.dormant.paths]
scenes = "dormant/scenes"
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
        let child = |entity: &str, parent: &str| {
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id(entity)),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id(parent),
                ))),
            )
        };
        let prefab = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("trunk")),
                child("branch", "trunk"),
                child("child", "branch"),
                SceneEntityPlacement::root(id("outside")),
            ],
            ["trunk", "branch", "child", "outside"].map(named_entity),
        );
        fs::write(fixture.root.join(P), prefab.to_json_string().unwrap()).unwrap();
        fs::write(
            fixture.root.join(format!("{P}.meta")),
            AssetMeta::new(metadata(ALIAS, P_ID), 0_u32)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        for path in [STARTUP, DORMANT] {
            fs::write(
                fixture.root.join(path),
                repeated_scene().to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(
            fixture.root.join(INACTIVE),
            SceneDocument::from_ordered_roots([named_entity("target")])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            fixture.root.join(NESTED),
            PrefabDocument::from_ordered_roots([instance("inner", ALIAS, true)])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        for (path, entity) in [
            (GUARD, named_entity("target")),
            (CLOSED, instance("closed", ALIAS, true)),
        ] {
            fs::write(
                fixture.root.join(path),
                PrefabDocument::from_ordered_roots([entity])
                    .to_ron_string()
                    .unwrap(),
            )
            .unwrap();
        }
        fixture
    }

    fn open(&self) -> EditorProjectSession {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(test_host_fs::writable_directory(&self.root).unwrap(), None)
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

    fn disk(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut directories = vec![self.root.clone()];
        let mut files = Vec::new();
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    directories.push(path);
                } else {
                    files.push((
                        path.strip_prefix(&self.root).unwrap().to_owned(),
                        fs::read(path).unwrap(),
                    ));
                }
            }
        }
        files.sort_unstable();
        files
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
                .starts_with("nara_session_prefab_extraction_")
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn request(document: EditorDocumentId) -> EditorPrefabExtractionRequest {
    EditorPrefabExtractionRequest::new(
        document,
        id("branch"),
        metadata("extracted.prefab.json", Q_ID),
    )
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

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, entity: &str, value: &str) {
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
        document,
        base_revision: editor.document_query_basis(document).unwrap().revision,
        edit: AuthoringFieldEdit::single(
            id(entity),
            name_component(),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("value"),
            ComponentValue::String(value.into()),
        )
        .unwrap(),
    });
    assert!(report.applied, "{report:?}");
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "Session extraction did not settle: {:?}; {:?}",
            editor.prefab_extraction_view(),
            editor.prefab_extraction_diagnostics(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
}

fn reviewed(editor: &mut EditorProjectSession, operation: EditorPrefabExtractionId) {
    wait(editor, |editor| {
        matches!(
            editor.prefab_extraction_view().phase,
            EditorPrefabExtractionPhase::Review | EditorPrefabExtractionPhase::Terminal
        )
    });
    let view = editor.prefab_extraction_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorPrefabExtractionPhase::Review,
        "{view:?}; {:?}",
        editor.prefab_extraction_diagnostics(),
    );
    assert_eq!(view.outcome, None);
    assert!(!view.work_pending && view.writes_fenced);
    assert!(!editor.current_frame().authoring_operation_work_pending());
}

fn cancelled(editor: &EditorProjectSession, operation: EditorPrefabExtractionId) {
    let view = editor.prefab_extraction_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.phase, EditorPrefabExtractionPhase::Terminal);
    assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Cancelled));
    assert!(!view.work_pending && !view.writes_fenced);
    assert!(matches!(
        editor.prefab_extraction_review(operation),
        Err(EditorPrefabExtractionError::Operation)
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
    source: usize,
    basis: Option<AuthoringSchemaBasis>,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    saved_revision: SceneAuthoringRevision,
    saved_content: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    dirty: bool,
    reload: EditorExternalReloadState,
}

fn state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    if let Some(slot) = editor.workspace().scene(document) {
        let session = slot.session();
        SourceState {
            encoded: session.document().to_json_string().unwrap(),
            source: std::ptr::from_ref(session.document()) as usize,
            basis: session.authoring_schema_basis(),
            revision: slot.revision(),
            content: session.content_state(),
            history: session.history_status(),
            usage: session.history_usage(),
            saved_revision: slot.saved_revision(),
            saved_content: slot.saved_content_state(),
            saved_digest: slot.saved_digest(),
            dirty: slot.is_dirty(),
            reload: slot.external_reload(),
        }
    } else {
        let slot = editor.workspace().prefab(document).unwrap();
        let session = slot.session();
        SourceState {
            encoded: session.document().to_json_string().unwrap(),
            source: std::ptr::from_ref(session.document()) as usize,
            basis: session.authoring_schema_basis(),
            revision: slot.revision(),
            content: session.content_state(),
            history: session.history_status(),
            usage: session.history_usage(),
            saved_revision: slot.saved_revision(),
            saved_content: slot.saved_content_state(),
            saved_digest: slot.saved_digest(),
            dirty: slot.is_dirty(),
            reload: slot.external_reload(),
        }
    }
}

fn set_name_patch(entity: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id(entity),
        component: name_component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("must-not-apply".into()),
    }])
}

fn assert_writes_rejected(
    editor: &mut EditorProjectSession,
    scene: EditorDocumentId,
    prefab: EditorDocumentId,
) {
    let scene_revision = editor.document_query_basis(scene).unwrap().revision;
    let prefab_revision = editor.document_query_basis(prefab).unwrap().revision;
    let batch = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [
                EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                    document: scene,
                    base_revision: scene_revision,
                    patch: set_name_patch("left"),
                }),
                EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabPatch {
                    document: prefab,
                    base_revision: prefab_revision,
                    patch: set_name_patch("branch"),
                }),
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document: prefab,
                    base_revision: prefab_revision,
                }),
                EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document: prefab,
                    base_revision: prefab_revision,
                }),
                EditorAction::Play(EditorPlayAction::Play {
                    document: scene,
                    base_revision: scene_revision,
                }),
            ],
        ))
        .unwrap();
    for outcome in &batch.outcomes()[..4] {
        let EditorActionOutcome::Workspace(report) = outcome else {
            panic!("the Session must refuse fenced mutations: {outcome:?}");
        };
        assert!(!report.applied);
    }
    assert!(matches!(
        batch.outcomes()[4],
        EditorActionOutcome::Play(EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy))
    ));
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
    );
    assert_eq!(
        editor.begin_authored_schema_upgrade(),
        Err(EditorAuthoredSchemaUpgradeError::Busy)
    );
    assert_eq!(
        editor.begin_prefab_extraction(request(prefab)),
        Err(EditorPrefabExtractionError::Busy)
    );
    for command in [
        EditorPersistenceCommand::Save {
            document: Some(prefab),
        },
        EditorPersistenceCommand::Reopen {
            document: Some(prefab),
        },
    ] {
        assert_eq!(
            editor.request_persistence(command),
            EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
        );
    }
    for intent in [
        EditorWorkspaceIntent::CloseDocument { document: prefab },
        EditorWorkspaceIntent::Exit,
    ] {
        assert_eq!(
            editor.request_workspace_intent(intent),
            EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::Busy)
        );
    }
    let index = editor.project_source_index_snapshot().unwrap();
    assert_eq!(
        editor.request_indexed_authoring_document(index, CLOSED, AuthoringDocumentKind::Prefab),
        EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-open-busy",
        }
    );
}

fn assert_routed_suppression(entities: &[SceneEntityRecord], anchor: &str) {
    let entity = entities
        .iter()
        .find(|entity| entity.id == id(anchor))
        .unwrap();
    let overlay = entity.prefab.as_ref().unwrap().overrides.as_ref().unwrap();
    assert_eq!(overlay.base, key(P, SourceDocumentKind::Prefab));
    assert_eq!(
        overlay.suppressions,
        [PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("branch")),
                expected_base: key(Q, SourceDocumentKind::Prefab),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("child")),
        }]
    );
}

#[test]
fn begin_fences_writes_but_keeps_navigation_and_complete_all_profile_review_without_installing() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
        let guard = open(editor, GUARD, AuthoringDocumentKind::Prefab);
        let documents = [startup, selected, inactive, guard];
        for (document, entity, value) in [
            (startup, "left", "dirty-startup"),
            (selected, "branch", "dirty-branch"),
            (inactive, "target", "dirty-inactive"),
            (guard, "target", "dirty-guard"),
        ] {
            edit(editor, document, entity, value);
        }
        edit(editor, guard, "target", "redo-guard");
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document: guard,
                    base_revision: editor.document_query_basis(guard).unwrap().revision,
                })
                .applied
        );
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        // Play actions bind to the actual displayed revision; direct source edits above do not
        // publish a frame. Start from current observation before testing the operation fence.
        editor.drive_editor_frame(Duration::ZERO);
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        let operation = editor.begin_prefab_extraction(extraction).unwrap();
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(view.phase, EditorPrefabExtractionPhase::Draining);
        assert!(view.work_pending && view.writes_fenced);
        assert!(matches!(
            editor.prefab_extraction_review(operation),
            Err(EditorPrefabExtractionError::Operation)
        ));
        assert_writes_rejected(editor, startup, selected);
        let navigation = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                editor.current_frame().revision(),
                [
                    EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                        document: inactive,
                    }),
                    EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                        document: inactive,
                        entity: Some(id("target")),
                    }),
                    EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                        document: selected,
                    }),
                    EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                        document: selected,
                        entity: Some(id("outside")),
                    }),
                    EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                        document: inactive,
                    }),
                ],
            ))
            .unwrap();
        for outcome in navigation.outcomes() {
            let EditorActionOutcome::Workspace(report) = outcome else {
                panic!("navigation must remain available under the fence: {outcome:?}");
            };
            assert!(report.applied, "{report:?}");
        }
        let tabs = editor.workspace().authoring_model().documents;
        reviewed(editor, operation);
        assert_eq!(editor.prefab_extraction_view().sources, SOURCES.len());
        assert_eq!(editor.prefab_extraction_view().documents, documents.len());
        {
            let review = editor.prefab_extraction_review(operation).unwrap();
            assert_eq!(review.source_count(), SOURCES.len());
            assert_eq!(
                review.documents().collect::<Vec<_>>(),
                [
                    (startup, EditorPrefabExtractionDisposition::ReplaceSource),
                    (selected, EditorPrefabExtractionDisposition::ReplaceSource),
                    (inactive, EditorPrefabExtractionDisposition::SaveCurrent),
                    (guard, EditorPrefabExtractionDisposition::Guard),
                ]
            );
            let mut paths = review
                .outputs()
                .map(|output| output.project_path().to_owned())
                .collect::<Vec<_>>();
            paths.sort_unstable();
            let mut expected = [STARTUP, P, NESTED, CLOSED, DORMANT, INACTIVE, Q, Q_META];
            expected.sort_unstable();
            assert_eq!(paths, expected);
            for output in review.outputs() {
                if output.project_path() == Q_META {
                    assert_eq!(output.source_key(), None);
                    assert_eq!(output.source_encoding(), None);
                    assert_eq!(
                        AssetMetaCandidate::<()>::decode_json_bytes(output.bytes())
                            .unwrap()
                            .identity(),
                        &metadata("extracted.prefab.json", Q_ID)
                    );
                    continue;
                }
                let scene = [STARTUP, INACTIVE, DORMANT].contains(&output.project_path());
                assert_eq!(
                    output.source_key(),
                    Some(&key(
                        output.project_path(),
                        if scene {
                            SourceDocumentKind::Scene
                        } else {
                            SourceDocumentKind::Prefab
                        }
                    ))
                );
                assert_eq!(
                    output.source_encoding(),
                    Some(if output.project_path() == CLOSED {
                        SceneFileEncoding::Ron
                    } else {
                        SceneFileEncoding::Json
                    })
                );
                if scene {
                    let document = SceneDocumentCandidate::decode_json_bytes(output.bytes())
                        .unwrap()
                        .into_composition_source()
                        .unwrap();
                    if output.project_path() == INACTIVE {
                        assert_eq!(
                            document.entities[0].components[&name_component()]
                                .value
                                .as_str(),
                            Some("dirty-inactive")
                        );
                    } else {
                        assert_routed_suppression(&document.entities, "left");
                        assert_eq!(
                            document
                                .entities
                                .iter()
                                .filter(|entity| entity.prefab.is_some())
                                .count(),
                            3
                        );
                        assert!(
                            document
                                .entities
                                .iter()
                                .any(|entity| entity.id == id("child"))
                        );
                    }
                } else {
                    let candidate = if output.source_encoding() == Some(SceneFileEncoding::Ron) {
                        PrefabDocumentCandidate::decode_ron_bytes(output.bytes())
                    } else {
                        PrefabDocumentCandidate::decode_json_bytes(output.bytes())
                    }
                    .unwrap();
                    let document = candidate.into_composition_source().unwrap();
                    match output.project_path() {
                        Q => {
                            assert_eq!(document.entities.len(), 2);
                            let branch = document
                                .entities
                                .iter()
                                .find(|entity| entity.id == id("branch"))
                                .unwrap();
                            assert_eq!(
                                branch.components[&name_component()].value.as_str(),
                                Some("dirty-branch")
                            );
                            assert!(
                                document
                                    .placements
                                    .contains(&SceneEntityPlacement::root(id("branch")))
                            );
                        }
                        P => {
                            assert_eq!(document.entities.len(), 3);
                            assert!(
                                !document
                                    .entities
                                    .iter()
                                    .any(|entity| entity.id == id("child"))
                            );
                            let branch = document
                                .entities
                                .iter()
                                .find(|entity| entity.id == id("branch"))
                                .unwrap();
                            let instance = branch.prefab.as_ref().unwrap();
                            assert_eq!(instance.source, AssetRef::StableId(Q_ID.parse().unwrap()));
                            assert_eq!(
                                instance.binding,
                                PrefabInstanceBinding::Root {
                                    base: key(Q, SourceDocumentKind::Prefab),
                                    root: PrefabEntityTarget::source(id("branch")),
                                }
                            );
                        }
                        NESTED => assert_routed_suppression(&document.entities, "inner"),
                        CLOSED => assert_routed_suppression(&document.entities, "closed"),
                        other => panic!("unexpected reviewed source {other}"),
                    }
                }
            }
        }
        for _ in 0..3 {
            assert!(
                !editor
                    .drive_editor_frame(Duration::ZERO)
                    .authoring_operation_work_pending()
            );
            assert!(editor.prefab_extraction_view().writes_fenced);
        }
        assert_writes_rejected(editor, startup, selected);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(editor.workspace().active_document(), Some(inactive));
        assert_eq!(
            editor
                .workspace()
                .scene(inactive)
                .unwrap()
                .selection()
                .top_entity(),
            Some(&id("target"))
        );
        assert_eq!(
            editor
                .workspace()
                .prefab(selected)
                .unwrap()
                .selection()
                .top_entity(),
            Some(&id("outside"))
        );
        assert_eq!(fixture.disk(), disk);
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
        editor.cancel_prefab_extraction(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(editor.workspace().authoring_model().documents, tabs);
        assert_eq!(editor.workspace().active_document(), Some(inactive));
        assert_eq!(fixture.disk(), disk);
        for (index, document) in documents.into_iter().enumerate() {
            for undo in [true, false] {
                let base_revision = editor.document_query_basis(document).unwrap().revision;
                let command = if undo {
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision,
                    }
                } else {
                    EditorWorkspaceCommand::Redo {
                        document,
                        base_revision,
                    }
                };
                assert!(editor.apply_workspace_command(command).applied);
                if undo {
                    assert_eq!(editor.workspace().document_is_dirty(document), Some(false));
                }
            }
            let restored = state(editor, document);
            assert_eq!(restored.encoded, before[index].encoded);
            assert_eq!(restored.content, before[index].content);
            assert_eq!(restored.history, before[index].history);
            assert_eq!(restored.saved_digest, before[index].saved_digest);
        }
        assert_eq!(fixture.disk(), disk);
    });
}

fn saved_before_begin(phase: SaveTestPhase) {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "saved-before-capture");
        let before = state(editor, selected);
        let original = fixture.bytes(P);
        let root = editor.source_project_root();
        let path = RelativePath::new(P).unwrap();
        let predecessor = root.open_file_exact(&path).unwrap();
        let gate = editor.hold_next_save(phase);
        let EditorPersistenceRequestResult::Accepted { attempt } =
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(selected),
            })
        else {
            panic!("the original Save must be accepted");
        };
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        assert!(gate.wait_until_started());
        for _ in 0..3 {
            assert!(
                editor
                    .drive_editor_frame(Duration::ZERO)
                    .authoring_operation_work_pending()
            );
            let view = editor.prefab_extraction_view();
            assert_eq!(view.phase, EditorPrefabExtractionPhase::Draining);
            assert!(view.work_pending && view.writes_fenced);
            assert!(editor.prefab_extraction.pending.is_none());
            assert_eq!(state(editor, selected), before);
            assert!(editor.persistence_view().result().is_none());
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
            );
        }
        if phase == SaveTestPhase::BeforePublication {
            assert_eq!(fixture.bytes(P), original);
        } else {
            assert_ne!(fixture.bytes(P), original);
            assert_ne!(
                root.open_file_exact(&path).unwrap().identity(),
                predecessor.identity()
            );
        }
        gate.release();
        // Poll Save alone to observe installation before extraction records the terminal attempt.
        let deadline = Instant::now() + WAIT;
        while editor.persistence_view().result().is_none() {
            editor.advance_persistence();
            assert!(
                Instant::now() < deadline,
                "the accepted Save did not physically return"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let saved = editor.persistence_view().result().unwrap();
        assert!(
            matches!(
                saved,
                EditorPersistenceResult::Saved { document, revision, .. }
                    if document == selected && revision == before.revision
            ),
            "{saved:?}"
        );
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
            EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
        );
        assert_eq!(
            editor.prefab_extraction_view().phase,
            EditorPrefabExtractionPhase::Draining
        );
        assert!(editor.prefab_extraction.pending.is_none());
        let saved_state = state(editor, selected);
        assert!(!saved_state.dirty);
        assert_eq!(saved_state.encoded, before.encoded);
        assert_eq!(saved_state.source, before.source);
        assert_eq!(saved_state.history, before.history);
        assert_eq!(saved_state.usage, before.usage);
        assert_eq!(saved_state.saved_content, before.content);
        let published = root.open_file_exact(&path).unwrap();
        let saved_bytes = fixture.bytes(P);
        assert_ne!(published.identity(), predecessor.identity());
        assert_ne!(saved_bytes, original);
        let installed = editor.authoring_source_selection(selected).unwrap();
        let receipt = installed.receipt.as_ref().unwrap();
        assert_eq!(receipt.identity(), published.identity());
        assert_eq!(receipt.digest(), ContentDigest::of_bytes(&saved_bytes));
        let disk = fixture.disk();
        reviewed(editor, operation);
        let captured = editor
            .prefab_extraction
            .sources
            .iter()
            .find(|source| source.document == selected)
            .unwrap();
        assert_eq!(
            captured.receipt.as_ref().unwrap().identity(),
            published.identity()
        );
        assert_eq!(
            captured.receipt.as_ref().unwrap().digest(),
            receipt.digest()
        );
        let review = editor.prefab_extraction_review(operation).unwrap();
        let output = review
            .outputs()
            .find(|output| output.project_path() == Q)
            .unwrap();
        let extracted = PrefabDocumentCandidate::decode_json_bytes(output.bytes())
            .unwrap()
            .into_composition_source()
            .unwrap();
        assert_eq!(
            extracted
                .entities
                .iter()
                .find(|entity| entity.id == id("branch"))
                .unwrap()
                .components[&name_component()]
                .value
                .as_str(),
            Some("saved-before-capture")
        );
        assert_eq!(editor.persistence_view().result(), Some(saved));
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
            EditorPersistenceRequestResult::Acknowledged { attempt }
        );
        assert!(editor.prefab_extraction_view().writes_fenced);
        editor.cancel_prefab_extraction(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(state(editor, selected), saved_state);
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn accepted_save_installs_its_new_leaf_before_extraction_preparation() {
    saved_before_begin(SaveTestPhase::BeforePublication);
}

#[test]
fn published_save_held_before_return_cannot_supply_a_baseline_or_lose_its_result() {
    saved_before_begin(SaveTestPhase::AfterPublication);
}

#[test]
fn rejected_accepted_save_terminates_without_review_or_preparation() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "must-stay-dirty");
        let before = state(editor, selected);
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        let EditorPersistenceRequestResult::Accepted { attempt } =
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(selected),
            })
        else {
            panic!("Save admission precedes the external change");
        };
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        assert!(gate.wait_until_started());
        let mut external = fixture.bytes(P);
        external.push(b'\n');
        fs::write(fixture.root.join(P), &external).unwrap();
        let disk = fixture.disk();
        gate.release();
        wait(editor, |editor| {
            editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
        });
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.outcome,
            Some(EditorPrefabExtractionOutcome::Failed(
                EditorPrefabExtractionError::Persistence
            ))
        );
        assert!(!view.work_pending && !view.writes_fenced);
        assert_eq!(view.sources, 0);
        assert!(editor.prefab_extraction.pending.is_none());
        assert!(matches!(
            editor.prefab_extraction_review(operation),
            Err(EditorPrefabExtractionError::Operation)
        ));
        assert_eq!(editor.persistence_view().attempt(), Some(attempt));
        assert_eq!(
            editor.persistence_view().result(),
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(selected),
                reason: EditorPersistenceRejection::TargetChanged,
            })
        );
        assert!(editor.prefab_extraction_diagnostics().unwrap().has_errors());
        assert!(!editor.current_frame().authoring_operation_work_pending());
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.bytes(P), external);
        assert_eq!(fixture.disk(), disk);
    });
}

struct HeldPreparation {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl HeldPreparation {
    fn arm(editor: &mut EditorProjectSession) -> Self {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let held = Self {
            entered,
            release: Some(release),
        };
        assert!(
            editor
                .prefab_extraction
                .next_preparation_gate
                .replace(Box::new(move || {
                    let _ = entered_tx.send(());
                    release_rx
                        .recv_timeout(WAIT * 2)
                        .expect("release the actual prepared result before worker return");
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
                    panic!("preparation did not enter its completion gate")
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            assert!(
                Instant::now() < deadline,
                "the actual preparation did not finish: {:?}; {:?}",
                editor.prefab_extraction_view(),
                editor.prefab_extraction_diagnostics()
            );
            thread::sleep(Duration::from_millis(1));
        }
        editor
            .prefab_extraction
            .pending
            .as_ref()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .pending
            .task
            .as_ref()
            .unwrap()
            .return_observer()
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }

    fn assert_pending(&self, editor: &EditorProjectSession, returned: &TaskReturnObserver) {
        assert!(self.release.is_some());
        assert_eq!(returned.state(), TaskReturnState::Pending);
        let view = editor.prefab_extraction_view();
        assert_eq!(view.phase, EditorPrefabExtractionPhase::Preparing);
        assert!(view.work_pending && view.writes_fenced);
        assert_eq!(view.outcome, None);
        assert!(matches!(
            editor.prefab_extraction_review(view.operation.unwrap()),
            Err(EditorPrefabExtractionError::Operation)
        ));
    }
}

impl Drop for HeldPreparation {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn cancelling_started_preparation_keeps_inputs_and_fence_until_physical_return() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "dirty-at-cancel");
        let before = state(editor, selected);
        let disk = fixture.disk();
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        let mut held = HeldPreparation::arm(editor);
        let returned = held.wait_until_entered(editor);
        held.assert_pending(editor, &returned);
        editor.cancel_prefab_extraction(operation).unwrap();
        for _ in 0..3 {
            let started = Instant::now();
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Cancel must not wait for the preparation worker"
            );
            assert!(frame.authoring_operation_work_pending());
            held.assert_pending(editor, &returned);
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(selected)
                }),
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
            );
            assert_eq!(state(editor, selected), before);
            assert_eq!(fixture.disk(), disk);
        }
        held.release();
        wait(editor, |editor| {
            editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
        });
        assert_eq!(returned.state(), TaskReturnState::Returned);
        cancelled(editor, operation);
        assert!(!editor.current_frame().authoring_operation_work_pending());
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
        edit(editor, selected, "branch", "editing-resumed");
    });
}

#[test]
fn close_polls_the_actual_preparation_return_before_releasing_its_inputs() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "dirty-at-close");
        let before = state(editor, selected);
        let disk = fixture.disk();
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        let mut held = HeldPreparation::arm(editor);
        let returned = held.wait_until_entered(editor);
        for _ in 0..3 {
            let started = Instant::now();
            let progress = editor.advance_close();
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Close must poll actual worker retirement"
            );
            assert!(matches!(
                progress,
                EditorProjectCloseProgress::Incomplete { .. }
            ));
            assert!(!progress.task_shutdown().is_complete());
            held.assert_pending(editor, &returned);
            assert_eq!(state(editor, selected), before);
            assert_eq!(fixture.disk(), disk);
        }
        held.release();
        close(editor);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        cancelled(editor, operation);
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
        assert_eq!(
            editor.begin_prefab_extraction(request(selected)),
            Err(EditorPrefabExtractionError::Closing)
        );
    });
}

#[test]
fn stale_operation_ids_cannot_cancel_or_observe_a_later_review() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        let before = state(editor, selected);
        let disk = fixture.disk();
        let previous = editor.begin_prefab_extraction(request(selected)).unwrap();
        editor.cancel_prefab_extraction(previous).unwrap();
        cancelled(editor, previous);
        let current = editor.begin_prefab_extraction(request(selected)).unwrap();
        assert_ne!(current, previous);
        assert_eq!(
            editor.cancel_prefab_extraction(previous),
            Err(EditorPrefabExtractionError::Operation)
        );
        reviewed(editor, current);
        assert!(matches!(
            editor.prefab_extraction_review(previous),
            Err(EditorPrefabExtractionError::Operation)
        ));
        assert_eq!(
            editor.cancel_prefab_extraction(previous),
            Err(EditorPrefabExtractionError::Operation)
        );
        assert_eq!(editor.prefab_extraction_view().operation, Some(current));
        assert!(editor.prefab_extraction_review(current).is_ok());
        editor.cancel_prefab_extraction(current).unwrap();
        cancelled(editor, current);
        assert_eq!(
            editor.cancel_prefab_extraction(current),
            Err(EditorPrefabExtractionError::Operation)
        );
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn selected_host_work_and_handle_limits_report_typed_failures_without_changing_incumbents() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "dirty-before-budget-refusal");
        let before = state(editor, selected);
        let disk = fixture.disk();
        for (limits, error) in [
            (
                AuthoringSourceOperationLimits::default()
                    .with_host_work(ItemLimit::new(1).unwrap()),
                EditorPrefabExtractionError::WorkLimit,
            ),
            (
                AuthoringSourceOperationLimits::default()
                    .with_open_handles(ItemLimit::new(1).unwrap()),
                EditorPrefabExtractionError::OpenHandleLimit,
            ),
        ] {
            let operation = editor
                .begin_prefab_extraction_with_limits(request(selected), limits)
                .unwrap();
            wait(editor, |editor| {
                editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
            });
            let view = editor.prefab_extraction_view();
            assert_eq!(view.operation, Some(operation));
            assert_eq!(view.limits, limits);
            assert_eq!(
                view.outcome,
                Some(EditorPrefabExtractionOutcome::Failed(error))
            );
            assert!(!view.work_pending && !view.writes_fenced);
            assert_eq!(view.sources, 0);
            assert!(matches!(
                editor.prefab_extraction_review(operation),
                Err(EditorPrefabExtractionError::Operation)
            ));
            assert!(editor.prefab_extraction_diagnostics().unwrap().has_errors());
            assert!(!editor.current_frame().authoring_operation_work_pending());
            assert_eq!(state(editor, selected), before);
            assert_eq!(fixture.disk(), disk);
        }
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        assert_eq!(
            editor.prefab_extraction_view().limits,
            AuthoringSourceOperationLimits::default()
        );
        reviewed(editor, operation);
        editor.cancel_prefab_extraction(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
    });
}
