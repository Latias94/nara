//! Real project, file and working-copy conformance for Host-owned conversion custody.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, SceneAuthoringContentState,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};
use nara_tasks::TaskReturnObserver;
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentDigest, EditorDocumentId,
    EditorDocumentMutation, EditorDocumentOpenResult, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorSelectionSet,
    EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult,
};

use super::*;
use crate::project_host::{
    EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
    composition::built_in_schema_providers,
};

const SCENE: &str = "scenes/startup.scene.json";
const CONTAINER: &str = "prefabs/container.prefab.json";
const BASE: &str = "prefabs/base.prefab.json";
const OTHER: &str = "scenes/unrelated.scene.json";
const UNIVERSE: &str = "72b32f54-73d6-5c54-a560-0298797f6c90";
const WAIT: Duration = Duration::from_secs(15);
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn payload_type() -> ComponentTypeId {
    ComponentTypeId::new("test.HostConversionPayload")
}

fn payload_owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.host-conversion").unwrap()
}

fn stable_id() -> StableAssetId {
    StableAssetId::parse_str("e3d60518-c80d-4abc-9fe5-89c38b952626").unwrap()
}

fn payload(amount: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("amount", ComponentValue::U64(amount))]),
    )
    .with_owner(payload_owner())
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(leaves: usize, stable: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_host_prefab_conversion_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            format!(
                "schema_version = 1\n\
                 [project]\nname = 'Host Prefab conversion'\n\
                 source_universe = '{UNIVERSE}'\nnative_document_identity = 'project_location'\n\
                 [paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n\
                 [startup]\ndefault_scene = 'startup.scene.json'\n\
                 [authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n\
                 [runtime]\npreset = 'local-headless'\n\
                 [capabilities]\nrequested = ['runtime-2d']\n"
            ),
        )
        .unwrap();
        let builtins =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        let schema = ComponentSchema::new(
            payload_type(),
            "Host conversion payload",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        let bundle = AuthoringSchemaBundle::new(
            builtins
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([AuthoringSchemaBundleOwner::new(
                    payload_owner(),
                    ComponentSchemaCatalog {
                        components: vec![schema],
                        ..Default::default()
                    },
                    None,
                )]),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let project = Self { root };
        project.write_base(leaves);
        if stable {
            project.write_metadata(BASE, AssetSourceKind::Prefab);
        }
        let mut anchor = SceneEntityRecord::new(id("anchor")).with_children([id("local")]);
        anchor.prefab = Some(PrefabInstance {
            source: if stable {
                AssetRef::StableId(stable_id())
            } else {
                AssetRef::path("base.prefab.json").unwrap()
            },
            overrides: None,
        });
        let document = SceneDocument::new(
            vec![id("anchor")],
            vec![anchor, SceneEntityRecord::new(id("local"))],
        );
        project.write_source(SCENE, &document);
        project.write_source(CONTAINER, &document);
        project.write_source(
            OTHER,
            &SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("other"))]),
        );
        project
    }

    fn write_base(&self, leaves: usize) {
        fs::write(
            self.root.join(BASE),
            PrefabDocument::from_ordered_roots((0..leaves).map(|index| {
                SceneEntityRecord::new(id(&format!("leaf-{index}")))
                    .with_component(payload_type(), payload(index as u64))
            }))
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    }

    fn write_source(&self, path: &str, document: &SceneDocument) {
        let bytes = if path.ends_with(".prefab.json") {
            PrefabDocument::new(document.roots.clone(), document.entities.clone())
                .to_json_string()
                .unwrap()
        } else {
            document.to_json_string().unwrap()
        };
        fs::write(self.root.join(path), bytes).unwrap();
    }

    fn write_metadata(&self, path: &str, kind: AssetSourceKind) {
        let relative = path.split_once('/').unwrap().1;
        fs::write(
            self.root.join(format!("{path}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(stable_id(), AssetPath::new(relative).unwrap(), kind),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
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
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn editor(&self) -> EditorProjectSession {
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(self.capability(), None);
        let deadline = Instant::now() + WAIT;
        loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(mut editor) => {
                    editor.drive_editor_frame(Duration::ZERO);
                    return editor;
                }
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(error) => {
                    panic!("public project Open failed: {:?}", error.diagnostics());
                }
            }
            assert!(
                Instant::now() < deadline,
                "public project Open did not return"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // The fixture owns only this uniquely named immediate child of the temporary directory.
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
                .starts_with("nara_host_prefab_conversion_")
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let frame = editor.current_frame().revision();
    editor
        .dispatch_editor_actions(EditorActionBatch::new(frame, [action]))
        .unwrap()
        .into_outcomes()
        .into_iter()
        .next()
        .unwrap()
}

fn conversion(editor: &mut EditorProjectSession, action: EditorPrefabConversionAction) -> Outcome {
    match dispatch(editor, EditorAction::PrefabConversion(action)) {
        EditorActionOutcome::PrefabConversion(outcome) => outcome,
        other => panic!("conversion was not routed to its owner: {other:?}"),
    }
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "Host did not settle: {:?}; persistence: {:?}",
            editor.prefab_conversion_view(),
            editor.persistence_view(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_source(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    if path == SCENE {
        return editor.scene_preview_document.unwrap();
    }
    wait(editor, |editor| {
        !editor.current_frame().project_browser().has_pending_work()
    });
    let opening = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: path.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = opening
    else {
        panic!("public Browser Open refused {path}: {opening:?}");
    };
    wait(editor, |editor| {
        editor.current_frame().document_open().result().is_some()
    });
    let result = editor.current_frame().document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        panic!("public Browser Open did not publish {path}: {result:?}");
    };
    assert_eq!(completed, request);
    assert!(matches!(
        dispatch(
            editor,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
        ),
        EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }
        ) if acknowledged == request
    ));
    editor.drive_editor_frame(Duration::ZERO);
    document
}

fn navigate(editor: &mut EditorProjectSession, document: EditorDocumentId, entity: &str) {
    for action in [
        EditorNavigationAction::SetActiveDocument { document },
        EditorNavigationAction::SelectEntity {
            document,
            entity: Some(id(entity)),
        },
    ] {
        let outcome = dispatch(editor, EditorAction::Navigate(action));
        assert!(
            matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
            "{outcome:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
    }
}

#[derive(Debug, PartialEq)]
struct SourceState {
    document: SceneDocument,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    saved_content: SceneAuthoringContentState,
    saved_revision: SceneAuthoringRevision,
    saved_digest: Option<EditorDocumentDigest>,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    selection: EditorSelectionSet,
    dirty: bool,
}

fn source_state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    if let Some(slot) = editor.workspace.scene(document) {
        SourceState {
            document: slot.session().document().clone(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            saved_content: slot.saved_content_state(),
            saved_revision: slot.saved_revision(),
            saved_digest: slot.saved_digest(),
            history: slot.session().history_status(),
            usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
            dirty: slot.is_dirty(),
        }
    } else {
        let slot = editor.workspace.prefab(document).unwrap();
        SourceState {
            document: slot.session().document().instantiate(),
            revision: slot.revision(),
            content: slot.session().content_state(),
            saved_content: slot.saved_content_state(),
            saved_revision: slot.saved_revision(),
            saved_digest: slot.saved_digest(),
            history: slot.session().history_status(),
            usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
            dirty: slot.is_dirty(),
        }
    }
}

fn prepare(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> EditorPrefabConversionId {
    let basis = editor.document_query_basis(document).unwrap();
    let outcome = conversion(
        editor,
        EditorPrefabConversionAction::Prepare {
            basis,
            anchor: id("anchor"),
        },
    );
    let Outcome::Accepted { operation, phase } = outcome else {
        panic!("actual conversion preparation was refused: {outcome:?}");
    };
    assert_eq!(phase, Phase::Preparing);
    operation
}

fn review(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> EditorPrefabConversionId {
    let operation = prepare(editor, document);
    wait(editor, |editor| {
        !editor.prefab_conversion_view().phase.work_pending()
    });
    assert_eq!(
        editor.prefab_conversion_view().phase,
        Phase::Review,
        "{:?}",
        editor.prefab_conversion_view(),
    );
    operation
}

fn committed(
    editor: &mut EditorProjectSession,
    operation: EditorPrefabConversionId,
) -> Arc<nara_tooling::CommittedEditorPrefabConversion> {
    wait(editor, |editor| {
        editor.prefab_conversion_view().phase == Phase::Terminal
    });
    let result = editor.prefab_conversion_view().result.unwrap();
    let EditorPrefabConversionResult::Committed {
        operation: completed,
        receipt,
    } = result
    else {
        panic!("conversion did not commit: {result:?}");
    };
    assert_eq!(completed, operation);
    assert!(
        receipt.report().applied,
        "{:?}",
        receipt.report().diagnostics
    );
    receipt
}

fn failed(
    editor: &mut EditorProjectSession,
    operation: EditorPrefabConversionId,
    expected: Rejection,
) {
    wait(editor, |editor| {
        editor.prefab_conversion_view().phase == Phase::Terminal
    });
    assert!(
        matches!(
            editor.prefab_conversion_view().result,
            Some(EditorPrefabConversionResult::Failed { operation: completed, reason, .. })
                if completed == operation && reason == expected
        ),
        "{:?}",
        editor.prefab_conversion_view(),
    );
}

fn acknowledge(editor: &mut EditorProjectSession, operation: EditorPrefabConversionId) {
    assert_eq!(
        conversion(
            editor,
            EditorPrefabConversionAction::Acknowledge { operation }
        ),
        accepted(operation, Phase::Idle),
    );
    editor.drive_editor_frame(Duration::ZERO);
}

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, name: &str) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let action = if editor.workspace.scene(document).is_some() {
        EditorDocumentMutation::ApplyScenePatch {
            document,
            base_revision,
            patch,
        }
    } else {
        EditorDocumentMutation::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        }
    };
    let outcome = dispatch(editor, EditorAction::Mutate(action));
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
    editor.drive_editor_frame(Duration::ZERO);
}

fn replay(editor: &mut EditorProjectSession, document: EditorDocumentId, undo: bool) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let mutation = if undo {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    };
    let outcome = dispatch(editor, EditorAction::Mutate(mutation));
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
    editor.drive_editor_frame(Duration::ZERO);
}

fn persist(editor: &mut EditorProjectSession, document: EditorDocumentId, reopen: bool) {
    let action = if reopen {
        let outcome = dispatch(
            editor,
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
        );
        assert!(
            matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
            "{outcome:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        EditorPersistenceAction::Reopen {
            target: editor.current_frame().reopen_target(),
        }
    } else {
        EditorPersistenceAction::Save {
            document,
            base_revision: editor.document_query_basis(document).unwrap().revision,
        }
    };
    let outcome = dispatch(editor, EditorAction::Persistence(action));
    assert!(
        matches!(
            outcome,
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        ),
        "{outcome:?}"
    );
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    let result = editor.current_frame().persistence().result().unwrap();
    if reopen {
        assert!(
            matches!(result, EditorPersistenceResult::Opened { document: opened, .. } if opened == document),
            "{result:?}"
        );
    } else {
        assert!(
            matches!(result, EditorPersistenceResult::Saved { document: saved, .. } if saved == document),
            "{result:?}"
        );
    }
    let attempt = editor.current_frame().persistence().attempt().unwrap();
    assert!(matches!(
        dispatch(editor, EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt })),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt: acknowledged })
            if acknowledged == attempt
    ));
    editor.drive_editor_frame(Duration::ZERO);
}

fn close_document(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let intent = EditorWorkspaceIntent::CloseDocument { document };
    let outcome = dispatch(
        editor,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(intent)),
    );
    assert!(
        matches!(
            outcome,
            EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
        ),
        "{outcome:?}"
    );
    wait(editor, |editor| {
        editor.workspace_intent_view().result().is_some()
    });
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Applied { intent }),
    );
    assert!(editor.workspace.document_kind(document).is_none());
    dispatch(
        editor,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
    );
    editor.drive_editor_frame(Duration::ZERO);
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Host Close did not physically return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Prepare,
    Validate,
}

struct Gate {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl Gate {
    fn install(editor: &mut EditorProjectSession, stage: Stage) -> Self {
        let (entered_tx, entered) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let gate: Box<dyn FnOnce() + Send> = Box::new(move || {
            let _ = entered_tx.send(());
            // Dropping the control on assertion unwind also releases this physical worker.
            let _ = released.recv_timeout(Duration::from_secs(60));
        });
        match stage {
            Stage::Prepare => editor.prefab_conversion.next_prepare_gate = Some(gate),
            Stage::Validate => editor.prefab_conversion.next_validation_gate = Some(gate),
        }
        Self {
            entered,
            release: Some(release),
        }
    }

    fn wait_entered(&self) {
        self.entered
            .recv_timeout(WAIT)
            .expect("the actual worker must reach its gate");
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.release();
    }
}

fn observer(editor: &EditorProjectSession) -> TaskReturnObserver {
    match editor
        .prefab_conversion
        .pending
        .as_ref()
        .expect("real work must be retained")
    {
        PendingWork::Prepare(task) => task.return_observer(),
        PendingWork::Validate { task, .. } => task.return_observer(),
    }
}

#[test]
fn scene_and_prefab_conversion_publish_only_after_review_and_round_trip_one_history_entry() {
    for path in [SCENE, CONTAINER] {
        let project = Project::new(3, false);
        let mut editor = project.editor();
        let document = open_source(&mut editor, path);
        navigate(&mut editor, document, "anchor");
        let selected = editor.authoring_source_selection(document).unwrap();
        assert_eq!(
            selected.source_key,
            nara_scene::SourceDocumentKey::project_location(
                UNIVERSE.parse().unwrap(),
                AssetPath::new(path).unwrap(),
                if path == SCENE {
                    SourceDocumentKind::Scene
                } else {
                    SourceDocumentKind::Prefab
                },
            ),
        );
        let before = source_state(&editor, document);
        let original_file = fs::read(project.root.join(path)).unwrap();
        let idle_frame = editor.current_frame();
        let mut prepare_gate = Gate::install(&mut editor, Stage::Prepare);
        let operation = prepare(&mut editor, document);
        prepare_gate.wait_entered();
        let returned = observer(&editor);
        assert_eq!(returned.state(), TaskReturnState::Pending);
        assert_eq!(source_state(&editor, document), before);
        let pending_frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(pending_frame.prefab_conversion().phase, Phase::Preparing);
        assert!(pending_frame.prefab_conversion().result.is_none());
        assert_eq!(idle_frame.prefab_conversion().phase, Phase::Idle);
        prepare_gate.release();
        wait(&mut editor, |editor| {
            editor.prefab_conversion_view().phase != Phase::Preparing
        });
        let view = editor.prefab_conversion_view();
        assert_eq!(view.phase, Phase::Review, "{view:?}");
        let page = view.review.unwrap();
        assert_eq!(page.basis.document, document);
        assert_eq!(page.total_entities, 3);
        assert_eq!((page.source_count, page.working_copy_count), (1, 0));
        assert_eq!(source_state(&editor, document), before);
        let mut validation_gate = Gate::install(&mut editor, Stage::Validate);
        assert_eq!(
            conversion(
                &mut editor,
                EditorPrefabConversionAction::Commit { operation }
            ),
            accepted(operation, Phase::Validating),
        );
        validation_gate.wait_entered();
        assert_eq!(observer(&editor).state(), TaskReturnState::Pending);
        let validating_frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            validating_frame.prefab_conversion().phase,
            Phase::Validating
        );
        assert!(validating_frame.prefab_conversion().result.is_none());
        assert_eq!(source_state(&editor, document), before);
        assert_eq!(fs::read(project.root.join(path)).unwrap(), original_file);
        validation_gate.release();
        let receipt = committed(&mut editor, operation);
        assert_eq!(
            receipt
                .entities()
                .iter()
                .map(|row| row.entity())
                .collect::<Vec<_>>(),
            page.rows.iter().map(|row| &row.entity).collect::<Vec<_>>(),
        );
        let after = source_state(&editor, document);
        assert_eq!(after.usage.records, 1);
        assert_eq!((after.history.undo_depth, after.history.redo_depth), (1, 0));
        assert!(after.dirty);
        assert_eq!(after.selection, before.selection);
        assert_eq!(editor.workspace.active_document(), Some(document));
        let anchor = after
            .document
            .entities
            .iter()
            .find(|row| row.id == id("anchor"))
            .unwrap();
        assert!(anchor.prefab.is_none());
        assert_eq!(anchor.children.last(), Some(&id("local")));
        assert_eq!(
            after.document.entities.len(),
            before.document.entities.len() + 3
        );
        let mut amounts = std::collections::BTreeSet::new();
        for mapped in receipt.entities() {
            let entity = after
                .document
                .entities
                .iter()
                .find(|row| &row.id == mapped.entity())
                .unwrap();
            let record = &entity.components[&payload_type()];
            assert_eq!(record.owner.as_ref(), Some(&payload_owner()));
            let Ok(ComponentValue::U64(amount)) = record
                .value
                .get_path(&ComponentFieldPath::from_fields(["amount"]))
            else {
                panic!("the schema-only payload must remain complete: {record:?}");
            };
            assert!(amounts.insert(*amount));
        }
        assert_eq!(amounts, [0, 1, 2].into_iter().collect());
        assert_eq!(
            after
                .document
                .entities
                .iter()
                .find(|row| row.id == id("local")),
            before
                .document
                .entities
                .iter()
                .find(|row| row.id == id("local")),
        );
        assert_eq!(fs::read(project.root.join(path)).unwrap(), original_file);
        replay(&mut editor, document, true);
        let undone = source_state(&editor, document);
        assert_eq!(undone.document, before.document);
        assert_eq!(undone.content, before.content);
        assert!(!undone.dirty);
        assert_eq!(
            (undone.history.undo_depth, undone.history.redo_depth),
            (0, 1)
        );
        replay(&mut editor, document, false);
        assert_eq!(source_state(&editor, document).document, after.document);
        acknowledge(&mut editor, operation);
        persist(&mut editor, document, false);
        let saved = fs::read(project.root.join(path)).unwrap();
        let decoded = if path == SCENE {
            SceneDocumentCandidate::decode_json_bytes(&saved)
                .unwrap()
                .to_json_string()
                .unwrap()
        } else {
            PrefabDocumentCandidate::decode_json_bytes(&saved)
                .unwrap()
                .to_json_string()
                .unwrap()
        };
        let expected = if path == SCENE {
            after.document.to_json_string().unwrap()
        } else {
            PrefabDocument::new(
                after.document.roots.clone(),
                after.document.entities.clone(),
            )
            .to_json_string()
            .unwrap()
        };
        assert_eq!(decoded, expected);
        persist(&mut editor, document, true);
        let reopened = source_state(&editor, document);
        assert_eq!(reopened.document, after.document);
        assert_ne!(reopened.revision.source_id(), after.revision.source_id());
        assert_eq!(
            (reopened.history.undo_depth, reopened.history.redo_depth),
            (0, 0)
        );
        assert!(!reopened.dirty);
        assert_eq!(pending_frame.prefab_conversion().phase, Phase::Preparing);
        assert_eq!(
            validating_frame.prefab_conversion().phase,
            Phase::Validating
        );
        close(&mut editor);
    }
}

#[test]
fn review_paging_busy_and_terminal_actions_keep_the_same_operation_and_mapping() {
    let project = Project::new(REVIEW_ROWS + 7, false);
    let mut editor = project.editor();
    let document = open_source(&mut editor, CONTAINER);
    let before = source_state(&editor, document);
    let mut gate = Gate::install(&mut editor, Stage::Prepare);
    let operation = prepare(&mut editor, document);
    gate.wait_entered();
    let wrong = EditorPrefabConversionId::new(editor.current_frame().revision(), u64::MAX);
    assert_ne!(wrong, operation);
    let basis = editor.document_query_basis(document).unwrap();
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Prepare {
                basis: basis.clone(),
                anchor: id("anchor")
            }
        ),
        rejected(None, Rejection::Busy),
    );
    for action in [
        EditorPrefabConversionAction::Commit { operation: wrong },
        EditorPrefabConversionAction::Cancel { operation: wrong },
        EditorPrefabConversionAction::ReviewPage {
            operation: wrong,
            offset: 0,
        },
        EditorPrefabConversionAction::Acknowledge { operation: wrong },
    ] {
        assert_eq!(
            conversion(&mut editor, action),
            rejected(Some(wrong), Rejection::WrongOperation)
        );
        assert_eq!(editor.prefab_conversion_view().operation, Some(operation));
    }
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Commit { operation }
        ),
        rejected(Some(operation), Rejection::WrongPhase),
    );
    gate.release();
    wait(&mut editor, |editor| {
        editor.prefab_conversion_view().phase != Phase::Preparing
    });
    let first = editor
        .prefab_conversion_view()
        .review
        .expect("the original candidate must reach Review");
    assert_eq!(first.rows.len(), REVIEW_ROWS);
    let offset = first.next_offset.unwrap();
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::ReviewPage { operation, offset }
        ),
        accepted(operation, Phase::Review),
    );
    let last = editor.prefab_conversion_view().review.unwrap();
    assert_eq!(last.offset, offset);
    assert_eq!(last.rows.len(), 7);
    assert!(last.next_offset.is_none());
    assert_eq!(last.basis, first.basis);
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::ReviewPage {
                operation,
                offset: last.total_entities
            }
        ),
        rejected(Some(operation), Rejection::InvalidReviewPage),
    );
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::ReviewPage {
                operation,
                offset: 0
            }
        ),
        accepted(operation, Phase::Review),
    );
    assert_eq!(
        editor.prefab_conversion_view().review.as_deref(),
        Some(first.as_ref())
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let published_page = frame.prefab_conversion().review.clone().unwrap();
    let batch = EditorActionBatch::new(
        frame.revision(),
        [offset, 0].map(|offset| {
            EditorAction::PrefabConversion(EditorPrefabConversionAction::ReviewPage {
                operation,
                offset,
            })
        }),
    );
    let report = editor.dispatch_editor_actions(batch).unwrap();
    assert!(report.outcomes().iter().all(|outcome| matches!(
        outcome,
        EditorActionOutcome::PrefabConversion(Outcome::Accepted {
            operation: actual,
            phase: Phase::Review,
        }) if *actual == operation
    )));
    assert!(Arc::ptr_eq(
        editor
            .current_frame()
            .prefab_conversion()
            .review
            .as_ref()
            .unwrap(),
        &published_page,
    ));
    assert_eq!(
        editor.prefab_conversion_view().review.as_deref(),
        Some(first.as_ref()),
    );
    assert_eq!(source_state(&editor, document), before);
    let reviewed: Vec<_> = first
        .rows
        .iter()
        .chain(&last.rows)
        .map(|row| row.entity.clone())
        .collect();
    assert!(matches!(
        conversion(&mut editor, EditorPrefabConversionAction::Commit { operation }),
        Outcome::Accepted { operation: accepted, phase: Phase::Validating } if accepted == operation
    ));
    let receipt = committed(&mut editor, operation);
    assert_eq!(
        receipt
            .entities()
            .iter()
            .map(|row| row.entity().clone())
            .collect::<Vec<_>>(),
        reviewed
    );
    let after = source_state(&editor, document);
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Prepare {
                basis,
                anchor: id("anchor")
            }
        ),
        rejected(None, Rejection::ResultPending),
    );
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Commit { operation }
        ),
        rejected(Some(operation), Rejection::WrongPhase),
    );
    assert_eq!(source_state(&editor, document), after);
    acknowledge(&mut editor, operation);
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Acknowledge { operation }
        ),
        rejected(Some(operation), Rejection::WrongOperation),
    );
    close(&mut editor);
}

#[test]
fn reviewed_dependency_file_changes_and_new_stable_lookup_ambiguity_refuse_atomically() {
    for stable in [false, true] {
        let project = Project::new(2, stable);
        let mut editor = project.editor();
        let document = open_source(&mut editor, CONTAINER);
        let operation = review(&mut editor, document);
        let before = source_state(&editor, document);
        let original_root = fs::read(project.root.join(CONTAINER)).unwrap();
        let original_base = fs::read(project.root.join(BASE)).unwrap();
        let original_metadata =
            stable.then(|| fs::read(project.root.join(format!("{BASE}.meta"))).unwrap());
        let mut gate = Gate::install(&mut editor, Stage::Validate);
        assert_eq!(
            conversion(
                &mut editor,
                EditorPrefabConversionAction::Commit { operation }
            ),
            accepted(operation, Phase::Validating),
        );
        gate.wait_entered();
        if stable {
            project.write_metadata("assets/new-conflict.png", AssetSourceKind::Image);
            assert_eq!(fs::read(project.root.join(BASE)).unwrap(), original_base);
            assert_eq!(
                fs::read(project.root.join(format!("{BASE}.meta"))).unwrap(),
                original_metadata.unwrap(),
            );
        } else {
            project.write_base(4);
        }
        gate.release();
        failed(&mut editor, operation, Rejection::ValidationFailed);
        let Some(EditorPrefabConversionResult::Failed { diagnostics, .. }) =
            editor.prefab_conversion_view().result
        else {
            unreachable!("failed already establishes the correlated terminal result");
        };
        let expected_code = if stable {
            "project.editor.prefab-conversion-lookup-rejected"
        } else {
            "project.editor.prefab-conversion-source-changed"
        };
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == expected_code),
            "{diagnostics:?}",
        );
        assert_eq!(source_state(&editor, document), before);
        assert_eq!(
            fs::read(project.root.join(CONTAINER)).unwrap(),
            original_root
        );
        assert!(editor.prefab_conversion.prepared.is_none());
        assert!(editor.prefab_conversion.pending.is_none());
        close(&mut editor);
    }
}

#[derive(Clone, Copy, Debug)]
enum ParticipantChange {
    Open,
    Edit,
    Save,
    Reopen,
    Close,
}

#[test]
fn reviewed_file_or_working_copy_selection_rejects_open_edit_save_reopen_and_close() {
    for change in [
        ParticipantChange::Open,
        ParticipantChange::Edit,
        ParticipantChange::Save,
        ParticipantChange::Reopen,
        ParticipantChange::Close,
    ] {
        let project = Project::new(2, false);
        let mut editor = project.editor();
        let root = open_source(&mut editor, CONTAINER);
        let participant = if matches!(change, ParticipantChange::Open) {
            None
        } else {
            Some(open_source(&mut editor, BASE))
        };
        if matches!(change, ParticipantChange::Save) {
            edit(&mut editor, participant.unwrap(), "saved-participant-edit");
        }
        let operation = review(&mut editor, root);
        let page = editor.prefab_conversion_view().review.unwrap();
        assert_eq!(page.working_copy_count, usize::from(participant.is_some()));
        match change {
            ParticipantChange::Open => {
                open_source(&mut editor, BASE);
            }
            ParticipantChange::Edit => {
                edit(&mut editor, participant.unwrap(), "late-participant-edit")
            }
            ParticipantChange::Save => persist(&mut editor, participant.unwrap(), false),
            ParticipantChange::Reopen => persist(&mut editor, participant.unwrap(), true),
            ParticipantChange::Close => close_document(&mut editor, participant.unwrap()),
        }
        let before = source_state(&editor, root);
        let active = editor.workspace.active_document();
        let outcome = conversion(
            &mut editor,
            EditorPrefabConversionAction::Commit { operation },
        );
        assert!(
            matches!(outcome, Outcome::Accepted { operation: accepted, .. } if accepted == operation),
            "{change:?}: {outcome:?}"
        );
        failed(&mut editor, operation, Rejection::StaleSource);
        assert_eq!(source_state(&editor, root), before, "{change:?}");
        assert_eq!(editor.workspace.active_document(), active, "{change:?}");
        close(&mut editor);
    }
}

#[test]
fn root_reopen_rejects_the_original_incarnation_before_and_after_validation_dispatch() {
    for path in [SCENE, CONTAINER] {
        for during_validation in [false, true] {
            let project = Project::new(2, false);
            let mut editor = project.editor();
            let root = open_source(&mut editor, path);
            let operation = review(&mut editor, root);
            let original = source_state(&editor, root);
            let mut gate = during_validation.then(|| Gate::install(&mut editor, Stage::Validate));
            if let Some(gate) = &gate {
                assert_eq!(
                    conversion(
                        &mut editor,
                        EditorPrefabConversionAction::Commit { operation }
                    ),
                    accepted(operation, Phase::Validating),
                );
                gate.wait_entered();
            }
            persist(&mut editor, root, true);
            let reopened = source_state(&editor, root);
            assert_eq!(reopened.document, original.document);
            assert_ne!(reopened.revision.source_id(), original.revision.source_id());
            if let Some(gate) = &mut gate {
                gate.release();
            } else {
                assert_eq!(
                    conversion(
                        &mut editor,
                        EditorPrefabConversionAction::Commit { operation }
                    ),
                    accepted(operation, Phase::Terminal),
                );
            }
            failed(&mut editor, operation, Rejection::StaleSource);
            assert_eq!(source_state(&editor, root), reopened);
            close(&mut editor);
        }
    }
}

#[test]
fn unrelated_working_copy_edits_and_navigation_do_not_retarget_or_invalidate_review() {
    let project = Project::new(2, false);
    let mut editor = project.editor();
    let root = open_source(&mut editor, CONTAINER);
    let unrelated = open_source(&mut editor, OTHER);
    let operation = review(&mut editor, root);
    let page = editor.prefab_conversion_view().review.unwrap();
    edit(&mut editor, unrelated, "independent-edit");
    navigate(&mut editor, unrelated, "independent-edit");
    let other_before = source_state(&editor, unrelated);
    let root_before = source_state(&editor, root);
    let mut gate = Gate::install(&mut editor, Stage::Validate);
    assert_eq!(
        conversion(
            &mut editor,
            EditorPrefabConversionAction::Commit { operation }
        ),
        accepted(operation, Phase::Validating),
    );
    gate.wait_entered();
    edit(&mut editor, unrelated, "second-independent-edit");
    navigate(&mut editor, unrelated, "second-independent-edit");
    let other_after = source_state(&editor, unrelated);
    assert_ne!(other_before.document, other_after.document);
    assert_eq!(source_state(&editor, root), root_before);
    gate.release();
    let receipt = committed(&mut editor, operation);
    assert_eq!(
        receipt
            .entities()
            .iter()
            .map(|row| row.entity())
            .collect::<Vec<_>>(),
        page.rows.iter().map(|row| &row.entity).collect::<Vec<_>>()
    );
    assert_eq!(source_state(&editor, unrelated), other_after);
    assert_eq!(editor.workspace.active_document(), Some(unrelated));
    assert_eq!(source_state(&editor, root).usage.records, 1);
    assert!(
        source_state(&editor, root)
            .document
            .entities
            .iter()
            .find(|row| row.id == id("anchor"))
            .unwrap()
            .prefab
            .is_none()
    );
    close(&mut editor);
}

#[test]
fn cancellation_waits_for_physical_prepare_or_validation_return_and_discards_late_output() {
    for stage in [Stage::Prepare, Stage::Validate] {
        let project = Project::new(2, false);
        let mut editor = project.editor();
        let root = open_source(&mut editor, CONTAINER);
        let before = source_state(&editor, root);
        let mut gate;
        let operation = match stage {
            Stage::Prepare => {
                gate = Gate::install(&mut editor, stage);
                prepare(&mut editor, root)
            }
            Stage::Validate => {
                let operation = review(&mut editor, root);
                gate = Gate::install(&mut editor, stage);
                assert_eq!(
                    conversion(
                        &mut editor,
                        EditorPrefabConversionAction::Commit { operation }
                    ),
                    accepted(operation, Phase::Validating),
                );
                operation
            }
        };
        gate.wait_entered();
        let returned = observer(&editor);
        assert_eq!(
            conversion(
                &mut editor,
                EditorPrefabConversionAction::Cancel { operation }
            ),
            accepted(operation, Phase::Cancelling),
        );
        for _ in 0..3 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(returned.state(), TaskReturnState::Pending);
            assert_eq!(frame.prefab_conversion().phase, Phase::Cancelling);
            assert!(frame.prefab_conversion().result.is_none());
            assert_eq!(source_state(&editor, root), before);
        }
        assert_eq!(
            conversion(
                &mut editor,
                EditorPrefabConversionAction::Acknowledge { operation }
            ),
            rejected(Some(operation), Rejection::WrongPhase),
        );
        let basis = editor.document_query_basis(root).unwrap();
        assert_eq!(
            conversion(
                &mut editor,
                EditorPrefabConversionAction::Prepare {
                    basis,
                    anchor: id("anchor")
                }
            ),
            rejected(None, Rejection::Busy),
        );
        gate.release();
        wait(&mut editor, |editor| {
            editor.prefab_conversion_view().phase == Phase::Terminal
        });
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert_eq!(
            editor.prefab_conversion_view().result,
            Some(EditorPrefabConversionResult::Cancelled { operation }),
        );
        assert_eq!(source_state(&editor, root), before);
        assert!(editor.prefab_conversion.pending.is_none());
        assert!(editor.prefab_conversion.prepared.is_none());
        acknowledge(&mut editor, operation);
        close(&mut editor);
    }
}

#[test]
fn whole_session_close_retains_physical_prepare_and_validation_custody_until_return() {
    for stage in [Stage::Prepare, Stage::Validate] {
        let project = Project::new(2, false);
        let mut editor = project.editor();
        let root = open_source(&mut editor, CONTAINER);
        let before = source_state(&editor, root);
        let mut gate;
        let operation = match stage {
            Stage::Prepare => {
                gate = Gate::install(&mut editor, stage);
                prepare(&mut editor, root)
            }
            Stage::Validate => {
                let operation = review(&mut editor, root);
                gate = Gate::install(&mut editor, stage);
                assert_eq!(
                    conversion(
                        &mut editor,
                        EditorPrefabConversionAction::Commit { operation }
                    ),
                    accepted(operation, Phase::Validating),
                );
                operation
            }
        };
        gate.wait_entered();
        let returned = observer(&editor);
        for _ in 0..3 {
            assert!(
                matches!(
                    editor.advance_close(),
                    EditorProjectCloseProgress::Incomplete { .. }
                ),
                "{stage:?}"
            );
            assert_eq!(returned.state(), TaskReturnState::Pending);
            assert_eq!(editor.prefab_conversion_view().phase, Phase::Cancelling);
            assert!(editor.prefab_conversion_view().result.is_none());
            assert_eq!(source_state(&editor, root), before);
        }
        editor.prefab_conversion.next_close_poll_gate = Some(Box::new(move |editor| {
            assert_eq!(editor.prefab_conversion_view().phase, Phase::Cancelling);
            assert!(editor.prefab_conversion.pending.is_some());
            gate.release();
            let deadline = Instant::now() + WAIT;
            while returned.state() == TaskReturnState::Pending {
                assert!(
                    Instant::now() < deadline,
                    "the worker must physically return"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            loop {
                if matches!(
                    editor.authoring_runtime.shutdown_tasks(),
                    super::super::authoring_runtime::EditorAuthoringRuntimeCloseProgress::Complete(
                        _
                    )
                ) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "the task owner must finish shutdown"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(editor.prefab_conversion.pending.is_some());
        }));
        let returned = observer(&editor);
        close(&mut editor);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert_eq!(
            editor.prefab_conversion_view().result,
            Some(EditorPrefabConversionResult::Cancelled { operation }),
        );
        assert_eq!(source_state(&editor, root), before);
        assert!(editor.prefab_conversion.pending.is_none());
        assert!(editor.prefab_conversion.prepared.is_none());
    }
}
