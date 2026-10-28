//! Actual local installation and Session adoption, without executable approval.

#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
))]

mod failures;

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

use nara_asset::AssetRef;
use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FileIdentity, HostCapabilityOptions,
    RelativeComponent, RelativePath, TrustMode,
};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentPropertyPath, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringFieldEdit, AuthoringRecordReadiness, PrefabDocument, PrefabInstance,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SceneAuthoringContentState, SceneAuthoringHistoryStatus,
    SceneAuthoringHistoryUsage, SceneAuthoringRevision, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};
use nara_tasks::{TaskReturnObserver, TaskReturnState};
use nara_tooling::{
    EditorActionOutcome, EditorAuthoringProjection, EditorAuthoringQuery,
    EditorAuthoringQueryLimits, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
    EditorAuthoringRow, EditorDocumentDigest, EditorDocumentId, EditorDocumentQueryBasis,
    EditorDocumentQueryRejection, EditorPersistenceCommand, EditorPersistenceRequestResult,
    EditorPersistenceResult, EditorPropertyRead, EditorPropertyReadLimits, EditorPropertyTarget,
    EditorPropertyValue, EditorSelectionSet, EditorWorkspaceCommand,
};
use serde_json::json;

use super::{
    EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionOutcome,
    EditorSchemaOwnerAdoptionPhase,
};
use crate::project_host::{
    EditorProjectCloseProgress,
    authoring::{
        AuthoringDocumentKind, AuthoringProjectAdmissionHost, PreparedAuthoringProject,
        project_authoring_adapters,
    },
    composition::built_in_schema_providers,
    file_publication::CleanupTestControl,
    runtime::editor::{
        EditorProjectSession, EditorSourceToolInstallationOutcome,
        EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
        document_open::EditorDocumentOpenRequestResult,
    },
};

const WAIT: Duration = Duration::from_secs(15);
const STARTUP: &str = "scenes/startup.scene.json";
const SECOND: &str = "scenes/second.scene.json";
const BUNDLE: &str = "schema/authoring.bundle.json";
const INSTALLED: &str = "tools/stats";
const OWNER: &str = "test.session-installed-stats";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.SessionInstalledStats")
}

fn entity() -> SceneEntityId {
    SceneEntityId::new("target").unwrap()
}

fn name_component() -> ComponentTypeId {
    ComponentTypeId::new("nara.scene.Name")
}

fn scene(name: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity())
        .with_component(
            name_component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(name.into()),
            )
            .with_owner(
                PersistentComponentSchemaOwnerId::try_new(
                    SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str(),
                )
                .unwrap(),
            ),
        )
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("health", ComponentValue::I64(100))]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
        )])
}

/// The project starts with built-ins only; installation never edits its authoring bundle.
struct Fixture {
    root: PathBuf,
    receipt: ContentDigest,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_session_schema_adoption_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in [
            "assets",
            "scenes",
            "prefabs",
            "schema",
            "tools",
            "release-input",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            "schema_version = 1\n\
             [project]\nname = 'Installed Schema owner Session'\n\
             source_universe = '72b32f54-73d6-5c54-a560-0298797f6c90'\n\
             native_document_identity = 'project_location'\n\
             [paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n\
             [startup]\ndefault_scene = 'startup.scene.json'\n\
             [authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n\
             [runtime]\npreset = 'local-headless'\n\
             [capabilities]\nrequested = ['runtime-2d']\n",
        )
        .unwrap();
        let bundle =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        assert!(
            bundle
                .owner(&PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
                .is_none()
        );
        fs::write(root.join(BUNDLE), bundle.to_json_string().unwrap()).unwrap();
        for (path, name) in [(STARTUP, "saved"), (SECOND, "inactive-saved")] {
            fs::write(root.join(path), scene(name).to_json_string().unwrap()).unwrap();
        }
        let receipt = install(&root);
        Self { root, receipt }
    }

    fn open(&self) -> EditorProjectSession {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(strict_directory(&self.root), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let receipt = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        let mut editor = EditorProjectSession::open(receipt, None).unwrap();
        wait(&mut editor, |editor| {
            !editor.project_source_index_work_pending()
                && !editor.editor_image_import_work_pending()
                && !editor.source_tool_view().has_pending_work()
        });
        assert!(editor.project_source_index_snapshot().is_some());
        editor
    }

    fn run(&self, test: impl FnOnce(&mut EditorProjectSession)) {
        let mut editor = self.open();
        let result = catch_unwind(AssertUnwindSafe(|| test(&mut editor)));
        close(&mut editor);
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.root.join(path)).unwrap()
    }

    fn review(&self, editor: &mut EditorProjectSession) -> EditorSchemaOwnerAdoptionId {
        let operation = editor
            .prepare_installed_schema_owner(INSTALLED, self.receipt)
            .unwrap();
        wait(editor, |editor| {
            matches!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Review
                    | EditorSchemaOwnerAdoptionPhase::Terminal
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            )
        });
        let view = editor.schema_owner_adoption_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.phase,
            EditorSchemaOwnerAdoptionPhase::Review,
            "{view:?}"
        );
        assert_eq!(view.owner.as_deref(), Some(OWNER));
        assert_eq!(view.outcome, None);
        assert!(editor.schema_owner_adoption_publication().is_none());
        operation
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
                .starts_with("nara_session_schema_adoption_")
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn strict_directory(path: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let directory = fs::File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn install(root: &Path) -> ContentDigest {
    let catalog = ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(component(), "Installed stats", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("health"),
                    "Health",
                    ComponentFieldPath::from_fields(["health"]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                .with_authoring_initial_value(ComponentValue::map([(
                    "health",
                    ComponentValue::I64(100),
                )])),
        ],
        ..Default::default()
    };
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        catalog,
        None,
    )
    .unwrap();
    let source = root.join("release-input");
    let mut members = Vec::new();
    for (path, bytes) in [
        (
            "catalog.json",
            owner.current().to_json_string().unwrap().into_bytes(),
        ),
        (
            "Tool.dll",
            b"inert metadata fixture; never executed".to_vec(),
        ),
    ] {
        fs::write(source.join(path), &bytes).unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        let hash: String = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        members.push(json!({
            "path": path, "kind": "tool", "length": digest.length(), "blake3": hash,
        }));
    }
    let manifest = serde_json::to_vec(&json!({
        "format_version": 1,
        "package": "test.session-installed-stats",
        "release": "1",
        "host_target": if cfg!(windows) { "x86_64-pc-windows-msvc" } else { "x86_64-unknown-linux-gnu" },
        "authority": "trusted_process", "protocol_version": 1,
        "owner": OWNER, "component": component().as_str(),
        "catalog": "catalog.json", "entry": "Tool.dll", "requires": [], "files": members,
    }))
    .unwrap();
    fs::write(source.join("nara-tool.json"), &manifest).unwrap();
    let release = PreparedEditorSourceToolRelease::capture(
        &strict_directory(&source),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        strict_directory(&root.join("tools")),
        RelativeComponent::new("stats").unwrap(),
        release,
    )
    .unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => panic!("real code-free installation failed: {other:?}"),
    };
    installation.verify().unwrap();
    installation.receipt_digest()
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "the installed-owner operation did not settle: {:?}",
            editor.schema_owner_adoption_view(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
}

fn terminal(editor: &mut EditorProjectSession) -> EditorSchemaOwnerAdoptionOutcome {
    wait(editor, |editor| {
        matches!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Terminal
                | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        )
    });
    editor.schema_owner_adoption_view().outcome.unwrap()
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    loop {
        match editor.advance_close() {
            EditorProjectCloseProgress::Complete(receipt) => {
                assert!(receipt.task_shutdown().is_complete());
                assert!(!receipt.has_cleanup_failure());
                break;
            }
            progress => assert!(
                Instant::now() < deadline,
                "Session physical Close did not complete: {progress:?}"
            ),
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn startup(editor: &EditorProjectSession) -> EditorDocumentId {
    editor.workspace().active_document().unwrap()
}

fn second(editor: &mut EditorProjectSession) -> EditorDocumentId {
    let index = editor.project_source_index_snapshot().unwrap();
    match editor.open_indexed_authoring_document(index, SECOND, AuthoringDocumentKind::Scene) {
        EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("the indexed second Scene must open: {other:?}"),
    }
}

fn open_prefab(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    let index = editor.project_source_index_snapshot().unwrap();
    match editor.open_indexed_authoring_document(index, path, AuthoringDocumentKind::Prefab) {
        EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("the actual indexed Prefab must open: {other:?}"),
    }
}

fn name_edit(editor: &mut EditorProjectSession, document: EditorDocumentId, value: &str) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
        document,
        base_revision,
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

fn name_patch(value: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: entity(),
        component: name_component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String(value.into()),
    }])
}

fn read(
    editor: &EditorProjectSession,
    basis: EditorDocumentQueryBasis,
    component: ComponentTypeId,
    field: &str,
) -> Result<EditorPropertyValue, EditorDocumentQueryRejection> {
    editor
        .read_document_property(EditorPropertyRead {
            target: EditorPropertyTarget {
                basis,
                entity: entity(),
                component,
                property: ComponentPropertyPath::new(ComponentFieldId::new(field)),
            },
            limits: EditorPropertyReadLimits::new(1024, 16, 1024).unwrap(),
        })
        .map(|observation| observation.value)
}

fn readiness(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
) -> AuthoringRecordReadiness {
    let page = editor
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: editor.document_query_basis(document).unwrap(),
                entity: entity(),
                projection: EditorAuthoringProjection::Components,
            }),
            limits: EditorAuthoringQueryLimits::new(16, 16, 4096).unwrap(),
        })
        .unwrap();
    assert!(page.next.is_none());
    page.rows
        .iter()
        .find_map(|row| match row {
            EditorAuthoringRow::Component {
                component: id,
                readiness,
                ..
            } if *id == component() => Some(*readiness),
            _ => None,
        })
        .expect("the exact existing source record must remain discoverable")
}

/// Classification is intentionally excluded: adoption changes it without replacing source state.
#[derive(Debug, PartialEq)]
struct SourceState<D = SceneDocument> {
    document: Arc<D>,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    saved_revision: SceneAuthoringRevision,
    saved_content: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    selection: EditorSelectionSet,
    dirty: bool,
    source_upgrade_required: bool,
}

fn source_state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    let slot = editor.workspace().scene(document).unwrap();
    SourceState {
        document: slot.session().shared_document(),
        revision: slot.revision(),
        content: slot.session().content_state(),
        history: slot.session().history_status(),
        usage: slot.session().history_usage(),
        saved_revision: slot.saved_revision(),
        saved_content: slot.saved_content_state(),
        saved_digest: slot.saved_digest(),
        selection: slot.selection().clone(),
        dirty: slot.is_dirty(),
        source_upgrade_required: slot.session().source_upgrade_required(),
    }
}

fn prefab_source_state(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
) -> SourceState<PrefabDocument> {
    let slot = editor.workspace().prefab(document).unwrap();
    SourceState {
        document: slot.session().shared_document(),
        revision: slot.revision(),
        content: slot.session().content_state(),
        history: slot.session().history_status(),
        usage: slot.session().history_usage(),
        saved_revision: slot.saved_revision(),
        saved_content: slot.saved_content_state(),
        saved_digest: slot.saved_digest(),
        selection: slot.selection().clone(),
        dirty: slot.is_dirty(),
        source_upgrade_required: slot.session().source_upgrade_required(),
    }
}

fn save(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let revision = editor.document_query_basis(document).unwrap().revision;
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    wait(editor, |editor| {
        editor.persistence_view().result().is_some()
    });
    assert!(
        matches!(
            editor.persistence_view().result(),
            Some(EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. })
                if saved == document && saved_revision == revision
        ),
        "{:?}",
        editor.persistence_view()
    );
    assert!(!editor.workspace().scene(document).unwrap().is_dirty());
    let attempt = editor.persistence_view().attempt().unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
        EditorPersistenceRequestResult::Acknowledged { .. }
    ));
}

#[test]
fn mixed_scene_and_demanded_dirty_prefabs_adopt_one_basis_without_replacing_history_or_saved_state()
{
    const OUTER: &str = "prefabs/outer.prefab.json";
    const LEAF: &str = "prefabs/leaf.prefab.json";
    let fixture = Fixture::new();
    let anchor = |id: &str, source: &str| {
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path(source).unwrap(),
            overrides: None,
        });
        anchor
    };
    let saved_scenes = ["startup-saved", "second-saved"].map(|name| {
        SceneDocument::from_ordered_roots(
            scene(name)
                .entities
                .into_iter()
                .chain([anchor("outer", "outer.prefab.json")]),
        )
    });
    let saved_prefabs = [
        PrefabDocument::from_ordered_roots(
            scene("outer-saved")
                .entities
                .into_iter()
                .chain([anchor("nested", "leaf.prefab.json")]),
        ),
        PrefabDocument::from_ordered_roots(scene("leaf-saved").entities),
    ];
    for (path, source) in [STARTUP, SECOND].into_iter().zip(&saved_scenes) {
        fs::write(fixture.root.join(path), source.to_json_string().unwrap()).unwrap();
    }
    for (path, source) in [OUTER, LEAF].into_iter().zip(&saved_prefabs) {
        fs::write(fixture.root.join(path), source.to_json_string().unwrap()).unwrap();
    }
    let disk = [STARTUP, SECOND, OUTER, LEAF].map(|path| fixture.bytes(path));
    let original_bundle = fixture.bytes(BUNDLE);
    fixture.run(|editor| {
        let scenes = [startup(editor), second(editor)];
        let prefabs = [open_prefab(editor, OUTER), open_prefab(editor, LEAF)];
        let documents = [scenes[0], scenes[1], prefabs[0], prefabs[1]];
        assert_eq!(editor.workspace().len(), 4);
        let replay = |editor: &mut EditorProjectSession, document, redo| {
            let base_revision = editor.document_query_basis(document).unwrap().revision;
            let command = if redo {
                EditorWorkspaceCommand::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorWorkspaceCommand::Undo {
                    document,
                    base_revision,
                }
            };
            let report = editor.apply_workspace_command(command);
            assert!(report.applied, "{report:?}");
        };
        for (index, &document) in documents.iter().enumerate() {
            assert_eq!(
                readiness(editor, document),
                AuthoringRecordReadiness::UnknownSchema
            );
            name_edit(editor, document, &format!("working-{index}"));
            name_edit(editor, document, &format!("redo-{index}"));
            replay(editor, document, false);
            assert!(
                editor
                    .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(entity()),
                    })
                    .applied
            );
        }
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: scenes[1],
                })
                .applied
        );
        let before_scenes = scenes.map(|document| source_state(editor, document));
        let before_prefabs = prefabs.map(|document| prefab_source_state(editor, document));
        let bases = documents.map(|document| editor.document_query_basis(document).unwrap());
        for &document in &documents {
            let tab = editor.workspace_model().document(document).unwrap().clone();
            assert!(tab.dirty);
            assert_eq!(tab.history.undo_depth, 1);
            assert_eq!(tab.history.redo_depth, 1);
            assert_eq!(tab.active, document == scenes[1]);
        }
        let stale_edits = bases
            .iter()
            .map(|basis| {
                editor
                    .prepare_authoring_edit(basis, &name_patch("must-not-cross-basis"))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let operation = fixture.review(editor);
        assert_eq!(editor.schema_owner_adoption_view().documents, 4);
        assert_eq!(editor.schema_owner_adoption_view().degraded_roots, 0);
        for (&document, before) in scenes.iter().zip(&before_scenes) {
            assert_eq!(source_state(editor, document), *before);
            assert!(
                editor
                    .schema_owner_adoption_diagnostics(Some(document))
                    .is_some()
            );
            assert!(
                editor
                    .schema_owner_adoption_composition_failure(Some(document))
                    .is_none()
            );
        }
        for (&document, before) in prefabs.iter().zip(&before_prefabs) {
            assert_eq!(prefab_source_state(editor, document), *before);
        }
        assert!(
            editor
                .schema_owner_adoption_composition_failure(None)
                .is_none()
        );
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        for (path, original) in [STARTUP, SECOND, OUTER, LEAF].into_iter().zip(&disk) {
            assert_eq!(fixture.bytes(path), *original);
        }
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);
        assert_eq!(editor.workspace().active_document(), Some(scenes[1]));
        let basis = editor
            .document_query_basis(scenes[1])
            .unwrap()
            .schema
            .unwrap();
        assert_eq!(
            editor
                .authoring_project()
                .unwrap()
                .fork_startup_scene()
                .schema_basis(),
            basis
        );
        assert_eq!(
            editor
                .authoring_project()
                .unwrap()
                .fork_startup_scene()
                .document(),
            &saved_scenes[0]
        );
        for (&document, old) in documents.iter().zip(&bases) {
            let current = editor.document_query_basis(document).unwrap();
            assert_eq!(current.revision, old.revision);
            assert_eq!(current.schema.as_ref(), Some(&basis));
            assert_ne!(current.schema, old.schema);
            assert_eq!(
                read(editor, old.clone(), name_component(), "value"),
                Err(EditorDocumentQueryRejection::StaleSchema)
            );
            assert_eq!(
                readiness(editor, document),
                AuthoringRecordReadiness::Available
            );
        }
        for (&document, before) in scenes.iter().zip(&before_scenes) {
            let after = source_state(editor, document);
            assert_eq!(&after, before);
            assert!(Arc::ptr_eq(&after.document, &before.document));
        }
        for (&document, before) in prefabs.iter().zip(&before_prefabs) {
            let after = prefab_source_state(editor, document);
            assert_eq!(&after, before);
            assert!(Arc::ptr_eq(&after.document, &before.document));
        }

        // Both current roots and the saved startup select the actual dirty nested closure.
        let project = editor.authoring_project().unwrap();
        assert_eq!(project.prefab_sources().count(), 2);
        for (index, (path, before)) in [OUTER, LEAF].into_iter().zip(&before_prefabs).enumerate() {
            let selected = project
                .prefab_sources()
                .find(|source| source.source().project_path().as_str() == path)
                .unwrap();
            assert_eq!(selected.session_revision(), Some(before.revision));
            assert!(std::ptr::eq(selected.document(), before.document.as_ref()));
            assert_eq!(
                selected.source().digest(),
                ContentDigest::of_bytes(&disk[index + 2])
            );
        }
        assert_eq!(editor.schema_owner_adoption.captures.len(), 2);
        for (root, capture) in &mut editor.schema_owner_adoption.captures {
            let index = scenes
                .iter()
                .position(|document| *document == *root)
                .unwrap();
            assert_eq!(capture.root_revision(), Some(before_scenes[index].revision));
            assert!(capture.composition_failure().is_none());
            assert!(!capture.diagnostics().has_errors());
            assert_eq!(capture.prefabs().len(), 2);
            for (path, before) in [OUTER, LEAF].into_iter().zip(&before_prefabs) {
                let selected = capture
                    .prefabs()
                    .iter()
                    .find(|source| source.source().project_path().as_str() == path)
                    .unwrap();
                assert_eq!(selected.session_revision(), Some(before.revision));
                assert!(std::ptr::eq(selected.document(), before.document.as_ref()));
            }
            let preview = capture
                .take_preview()
                .expect("the current selected closure must compose");
            for expected in [
                format!("working-{index}"),
                "working-2".into(),
                "working-3".into(),
            ] {
                assert!(
                    preview.candidate.document().entities.iter().any(|entity| {
                        entity
                            .components
                            .get(&name_component())
                            .and_then(|record| record.value.as_str())
                            == Some(expected.as_str())
                    }),
                    "the compiled Scene must use dirty participant {expected}"
                );
            }
        }
        for prepared in stale_edits {
            let EditorActionOutcome::Workspace(report) = editor.commit_authoring_edit(prepared)
            else {
                panic!("the original Scene/Prefab edit must reject through its existing owner");
            };
            assert!(!report.applied);
        }
        editor.acknowledge_schema_owner_adoption(operation).unwrap();

        // Exercise both retained edges through each actual source owner after classification.
        for &document in &documents {
            replay(editor, document, false);
            assert_eq!(editor.workspace().document_is_dirty(document), Some(false));
        }
        for (index, &document) in scenes.iter().enumerate() {
            let restored = source_state(editor, document);
            assert_eq!(restored.document.as_ref(), &saved_scenes[index]);
            assert_eq!(restored.content, before_scenes[index].saved_content);
        }
        for (index, &document) in prefabs.iter().enumerate() {
            let restored = prefab_source_state(editor, document);
            assert_eq!(restored.document.as_ref(), &saved_prefabs[index]);
            assert_eq!(restored.content, before_prefabs[index].saved_content);
        }
        for (index, &document) in documents.iter().enumerate() {
            replay(editor, document, true);
            replay(editor, document, true);
            assert_eq!(
                read(
                    editor,
                    editor.document_query_basis(document).unwrap(),
                    name_component(),
                    "value"
                )
                .unwrap(),
                EditorPropertyValue::Present(ComponentValue::String(format!("redo-{index}")))
            );
            replay(editor, document, false);
        }
        for (&document, before) in scenes.iter().zip(&before_scenes) {
            let after = source_state(editor, document);
            assert_ne!(after.revision, before.revision);
            assert_eq!(after.document, before.document);
            assert_eq!(after.content, before.content);
            assert_eq!(after.history, before.history);
            assert_eq!(after.usage, before.usage);
            assert_eq!(after.saved_revision, before.saved_revision);
            assert_eq!(after.saved_content, before.saved_content);
            assert_eq!(after.saved_digest, before.saved_digest);
            assert_eq!(after.selection, before.selection);
            assert_eq!(after.dirty, before.dirty);
            assert_eq!(
                after.source_upgrade_required,
                before.source_upgrade_required
            );
        }
        for (&document, before) in prefabs.iter().zip(&before_prefabs) {
            let after = prefab_source_state(editor, document);
            assert_ne!(after.revision, before.revision);
            assert_eq!(after.document, before.document);
            assert_eq!(after.content, before.content);
            assert_eq!(after.history, before.history);
            assert_eq!(after.usage, before.usage);
            assert_eq!(after.saved_revision, before.saved_revision);
            assert_eq!(after.saved_content, before.saved_content);
            assert_eq!(after.saved_digest, before.saved_digest);
            assert_eq!(after.selection, before.selection);
            assert_eq!(after.dirty, before.dirty);
            assert_eq!(
                after.source_upgrade_required,
                before.source_upgrade_required
            );
        }
        for (path, original) in [STARTUP, SECOND, OUTER, LEAF].into_iter().zip(&disk) {
            assert_eq!(fixture.bytes(path), *original);
        }
        assert_eq!(editor.workspace().active_document(), Some(scenes[1]));
        assert!(editor.source_tool_view().active.is_none());
    });
}

#[test]
fn installed_missing_owner_adoption_preserves_whole_live_source_history_and_save_close_open() {
    let fixture = Fixture::new();
    let original_bundle = fixture.bytes(BUNDLE);
    let original_startup = fixture.bytes(STARTUP);
    let original_second = fixture.bytes(SECOND);
    fixture.run(|editor| {
        let document = startup(editor);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        name_edit(editor, document, "working");
        name_edit(editor, document, "retained-redo");
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision
                })
                .applied
        );
        let inactive = second(editor);
        name_edit(editor, inactive, "inactive-working");
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document })
                .applied
        );
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(entity())
                })
                .applied
        );
        let before = source_state(editor, document);
        let inactive_before = source_state(editor, inactive);
        assert_eq!(before.history.undo_depth, 1);
        assert_eq!(before.history.redo_depth, 1);
        assert!(before.dirty && inactive_before.dirty);
        let old_basis = editor.document_query_basis(document).unwrap();
        let inactive_basis = editor.document_query_basis(inactive).unwrap();
        let old_edit = editor
            .prepare_authoring_edit(&old_basis, &name_patch("must-not-commit"))
            .unwrap();

        let operation = fixture.review(editor);
        assert_eq!(editor.schema_owner_adoption_view().documents, 2);
        assert!(editor.schema_owner_adoption_diagnostics(None).is_some());
        for id in [document, inactive] {
            assert!(editor.schema_owner_adoption_diagnostics(Some(id)).is_some());
            assert!(
                editor
                    .schema_owner_adoption_composition_failure(Some(id))
                    .is_none()
            );
            assert_eq!(
                readiness(editor, id),
                AuthoringRecordReadiness::UnknownSchema
            );
        }
        assert!(
            editor
                .schema_owner_adoption_composition_failure(None)
                .is_none()
        );
        assert_eq!(source_state(editor, document), before);
        assert_eq!(source_state(editor, inactive), inactive_before);
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        assert_eq!(fixture.bytes(STARTUP), original_startup);
        assert_eq!(fixture.bytes(SECOND), original_second);
        assert!(editor.source_tool_view().active.is_none());
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);

        let after = source_state(editor, document);
        assert_eq!(after, before);
        assert!(Arc::ptr_eq(&after.document, &before.document));
        let inactive_after = source_state(editor, inactive);
        assert_eq!(inactive_after, inactive_before);
        assert!(Arc::ptr_eq(
            &inactive_after.document,
            &inactive_before.document
        ));
        assert_eq!(editor.workspace().active_document(), Some(document));
        let basis = editor.document_query_basis(document).unwrap();
        assert_eq!(basis.revision, old_basis.revision);
        assert_ne!(basis.schema, old_basis.schema);
        assert_eq!(
            basis.schema,
            editor.document_query_basis(inactive).unwrap().schema
        );
        assert_eq!(
            read(editor, old_basis, name_component(), "value"),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        assert_eq!(
            read(editor, inactive_basis, name_component(), "value"),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        let EditorActionOutcome::Workspace(rejected) = editor.commit_authoring_edit(old_edit)
        else {
            panic!("the retained source edit must reject through the ordinary workspace owner");
        };
        assert!(!rejected.applied);
        assert_eq!(source_state(editor, document), before);
        for id in [document, inactive] {
            assert_eq!(readiness(editor, id), AuthoringRecordReadiness::Available);
            assert_eq!(
                read(
                    editor,
                    editor.document_query_basis(id).unwrap(),
                    component(),
                    "health"
                )
                .unwrap(),
                EditorPropertyValue::Present(ComponentValue::I64(100))
            );
        }
        assert_eq!(fixture.bytes(STARTUP), original_startup);
        assert_eq!(fixture.bytes(SECOND), original_second);
        let bundle = fixture.bytes(BUNDLE);
        assert_ne!(bundle, original_bundle);
        assert!(
            AuthoringSchemaBundle::from_json_bytes(&bundle)
                .unwrap()
                .owner(&PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
                .is_some()
        );
        assert!(editor.schema_owner_adoption_publication().is_some());
        assert!(editor.source_tool_view().active.is_none());
        editor.acknowledge_schema_owner_adoption(operation).unwrap();

        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision
                })
                .applied
        );
        assert_eq!(
            editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .document(),
            &scene("saved")
        );
        assert!(!editor.workspace().scene(document).unwrap().is_dirty());
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Redo {
                    document,
                    base_revision
                })
                .applied
        );
        assert_eq!(
            editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .document(),
            before.document.as_ref()
        );
        assert_eq!(source_state(editor, inactive), inactive_before);

        let basis = editor.document_query_basis(document).unwrap();
        let prepared = editor
            .prepare_authoring_edit(
                &basis,
                &ScenePatchDocument::new([ScenePatchOperation::SetField {
                    entity: entity(),
                    component: component(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("health"),
                    value: ComponentValue::I64(125),
                }]),
            )
            .unwrap();
        let EditorActionOutcome::Workspace(committed) = editor.commit_authoring_edit(prepared)
        else {
            panic!("the newly understood data must edit through the ordinary workspace owner");
        };
        assert!(committed.applied, "{committed:?}");
        save(editor, document);
        save(editor, inactive);
    });

    let adopted_bundle = fixture.bytes(BUNDLE);
    fixture.run(|editor| {
        let document = startup(editor);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::Available
        );
        let basis = editor.document_query_basis(document).unwrap();
        assert_eq!(
            read(editor, basis.clone(), component(), "health").unwrap(),
            EditorPropertyValue::Present(ComponentValue::I64(125))
        );
        assert_eq!(
            read(editor, basis, name_component(), "value").unwrap(),
            EditorPropertyValue::Present(ComponentValue::String("working".into()))
        );
        assert!(!editor.workspace().scene(document).unwrap().is_dirty());
        assert_eq!(
            editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .history_status()
                .undo_depth,
            0
        );
        let inactive = second(editor);
        assert_eq!(
            readiness(editor, inactive),
            AuthoringRecordReadiness::Available
        );
        assert_eq!(
            read(
                editor,
                editor.document_query_basis(inactive).unwrap(),
                name_component(),
                "value"
            )
            .unwrap(),
            EditorPropertyValue::Present(ComponentValue::String("inactive-working".into()))
        );
        assert!(!editor.workspace().scene(inactive).unwrap().is_dirty());
        assert!(editor.source_tool_view().active.is_none());
        let operation = editor
            .prepare_installed_schema_owner(INSTALLED, fixture.receipt)
            .unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::AlreadyAdmitted
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(fixture.bytes(BUNDLE), adopted_bundle);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Idle
        );
    });
}

#[test]
fn cancelling_installed_owner_review_keeps_the_original_bundle_classification_and_history() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "cancelled-review");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        editor.cancel_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(
            editor.adopt_schema_owner(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Idle
        );
        name_edit(editor, document, "editing-resumed");
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
    });
}

#[test]
fn changed_installed_payload_refuses_review_acceptance_before_bundle_publication() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retain-on-installation-change");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        fs::write(
            fixture.root.join(INSTALLED).join("Tool.dll"),
            b"externally changed inert payload",
        )
        .unwrap();
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Failed(EditorSchemaOwnerAdoptionError::Installation)
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.source_tool_view().active.is_none());
    });
}

#[test]
fn changed_bundle_refuses_old_review_without_rebasing_or_overwriting_external_bytes() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retain-on-bundle-change");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        let mut changed_bundle = fixture.bytes(BUNDLE);
        changed_bundle.extend_from_slice(b"\n ");
        assert!(AuthoringSchemaBundle::from_json_bytes(&changed_bundle).is_ok());
        fs::write(fixture.root.join(BUNDLE), &changed_bundle).unwrap();
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Failed(EditorSchemaOwnerAdoptionError::Stale)
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(fixture.bytes(BUNDLE), changed_bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.source_tool_view().active.is_none());
    });
}

/// Holds the actual publication result in its existing worker, before owner observation.
struct HeldPublication {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl HeldPublication {
    fn arm(editor: &mut EditorProjectSession) -> Self {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let held = Self {
            entered,
            release: Some(release),
        };
        let gate = Box::new(move || {
            entered_tx.send(()).unwrap();
            release_rx
                .recv_timeout(WAIT * 2)
                .expect("release the actual publication result before physical return");
        });
        assert!(
            editor
                .schema_owner_adoption
                .next_publication_gate
                .replace(gate)
                .is_none()
        );
        held
    }

    fn wait_until_entered(&self) {
        self.entered
            .recv_timeout(WAIT)
            .expect("the actual publication must reach the post-publication gate");
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }

    fn assert_pending(&self, editor: &EditorProjectSession, returned: &TaskReturnObserver) {
        assert!(self.release.is_some());
        assert_eq!(returned.state(), TaskReturnState::Pending);
        let Some(super::Work::Publish(pending)) = &editor.schema_owner_adoption.work else {
            panic!("the owner must retain its actual publication worker");
        };
        assert!(matches!(
            pending.job.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
    }

    fn poll<T>(&self, poll: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = poll();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the Session owner blocked on a held publication result"
        );
        result
    }
}

impl Drop for HeldPublication {
    fn drop(&mut self) {
        self.release();
    }
}

fn publication_returned(editor: &EditorProjectSession) -> TaskReturnObserver {
    let Some(super::Work::Publish(pending)) = &editor.schema_owner_adoption.work else {
        panic!("the Session must own its admitted publication task");
    };
    pending.task.as_ref().unwrap().return_observer()
}

fn assert_publication_evidence(
    editor: &EditorProjectSession,
    previous: FileIdentity,
    published: FileIdentity,
    digest: ContentDigest,
) {
    let receipt = editor
        .schema_owner_adoption_publication()
        .expect("retain the actual published receipt");
    assert_eq!(receipt.previous_identity(), Some(previous));
    assert_eq!(receipt.candidate_identity(), published);
    assert_eq!(receipt.published_identity(), Some(published));
    assert_eq!(receipt.verified_content_digest(), Some(digest));
}

#[test]
fn cancelled_held_publication_retains_evidence_and_reconciles_the_current_dirty_set_without_rewrite()
 {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "dirty-before-publication");
        name_edit(editor, document, "retained-redo");
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision
                })
                .applied
        );
        let inactive = second(editor);
        name_edit(editor, inactive, "inactive-before-publication");
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document })
                .applied
        );
        let before = source_state(editor, document);
        let inactive_before = source_state(editor, inactive);
        let basis = editor.document_query_basis(document).unwrap();
        let inactive_basis = editor.document_query_basis(inactive).unwrap();
        let original_source = fixture.bytes(STARTUP);
        let original_second = fixture.bytes(SECOND);
        let original_bundle = fixture.bytes(BUNDLE);
        let project_root = editor.source_project_root();
        let predecessor = project_root
            .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        let operation = fixture.review(editor);
        let mut held = HeldPublication::arm(editor);
        editor.adopt_schema_owner(operation).unwrap();
        let returned = publication_returned(editor);
        held.wait_until_entered();
        held.assert_pending(editor, &returned);

        let published_bytes = fixture.bytes(BUNDLE);
        let published_digest = ContentDigest::of_bytes(&published_bytes);
        assert_ne!(published_bytes, original_bundle);
        let published = project_root
            .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        assert_ne!(published.identity(), predecessor.identity());
        assert!(
            AuthoringSchemaBundle::from_json_bytes(&published_bytes)
                .unwrap()
                .owner(&PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
                .is_some()
        );
        held.poll(|| editor.cancel_schema_owner_adoption(operation))
            .unwrap();
        for _ in 0..3 {
            let frame = held.poll(|| editor.drive_editor_frame(Duration::ZERO));
            held.assert_pending(editor, &returned);
            assert!(frame.schema_owner_adoption_work_pending());
            assert_eq!(
                editor.schema_owner_adoption_view().operation,
                Some(operation)
            );
            assert_eq!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Publishing
            );
            assert_eq!(editor.schema_owner_adoption_view().outcome, None);
            assert!(editor.schema_owner_adoption_publication().is_none());
            assert_eq!(source_state(editor, document), before);
            assert_eq!(source_state(editor, inactive), inactive_before);
            assert_eq!(editor.document_query_basis(document).unwrap(), basis);
            assert_eq!(
                readiness(editor, document),
                AuthoringRecordReadiness::UnknownSchema
            );
        }
        held.release();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption
        );
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        );
        assert_publication_evidence(
            editor,
            predecessor.identity(),
            published.identity(),
            published_digest,
        );
        assert_eq!(source_state(editor, document), before);
        assert_eq!(source_state(editor, inactive), inactive_before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            editor.document_query_basis(inactive).unwrap(),
            inactive_basis
        );
        assert_eq!(
            editor.acknowledge_schema_owner_adoption(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        assert_eq!(
            editor.prepare_installed_schema_owner(INSTALLED, fixture.receipt),
            Err(EditorSchemaOwnerAdoptionError::RequiresReconcile)
        );
        editor.cancel_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            editor.schema_owner_adoption_view().outcome,
            Some(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption)
        );

        // Recovery must classify the actual current set, including changes made after publication.
        name_edit(editor, document, "edited-after-publication");
        name_edit(editor, inactive, "inactive-after-publication");
        let current = source_state(editor, document);
        let inactive_current = source_state(editor, inactive);
        let old_edit = editor
            .prepare_authoring_edit(
                &editor.document_query_basis(document).unwrap(),
                &name_patch("stale-reconcile-edit"),
            )
            .unwrap();
        let reconciliation = editor.reconcile_schema_owner_adoption(operation).unwrap();
        assert_ne!(reconciliation, operation);
        wait(editor, |editor| {
            matches!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Review
                    | EditorSchemaOwnerAdoptionPhase::Terminal
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            )
        });
        let review = editor.schema_owner_adoption_view();
        assert_eq!(review.operation, Some(reconciliation));
        assert_eq!(
            review.phase,
            EditorSchemaOwnerAdoptionPhase::Review,
            "{review:?}"
        );
        assert!(review.reconciliation);
        assert_eq!(review.documents, 2);
        assert_eq!(source_state(editor, document), current);
        assert_eq!(source_state(editor, inactive), inactive_current);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), published_bytes);
        assert_publication_evidence(
            editor,
            predecessor.identity(),
            published.identity(),
            published_digest,
        );
        assert_eq!(
            editor.adopt_schema_owner(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        assert_eq!(
            editor.cancel_schema_owner_adoption(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        editor.adopt_schema_owner(reconciliation).unwrap();
        assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);
        assert_publication_evidence(
            editor,
            predecessor.identity(),
            published.identity(),
            published_digest,
        );
        let after = source_state(editor, document);
        assert_eq!(after, current);
        assert!(Arc::ptr_eq(&after.document, &current.document));
        let inactive_after = source_state(editor, inactive);
        assert_eq!(inactive_after, inactive_current);
        assert!(Arc::ptr_eq(
            &inactive_after.document,
            &inactive_current.document
        ));
        assert_ne!(
            editor.document_query_basis(document).unwrap().schema,
            basis.schema
        );
        for id in [document, inactive] {
            assert_eq!(readiness(editor, id), AuthoringRecordReadiness::Available);
        }
        let EditorActionOutcome::Workspace(rejected) = editor.commit_authoring_edit(old_edit)
        else {
            panic!("the predecessor-basis edit must reject after actual reconciliation");
        };
        assert!(!rejected.applied);
        assert_eq!(source_state(editor, document), current);
        let observed = project_root
            .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        assert_eq!(observed.identity(), published.identity());
        assert_eq!(fixture.bytes(BUNDLE), published_bytes);
        assert_eq!(
            observed.digest(published_digest.length()).unwrap(),
            published_digest
        );
        assert_eq!(fixture.bytes(STARTUP), original_source);
        assert_eq!(fixture.bytes(SECOND), original_second);
        assert!(editor.source_tool_view().active.is_none());
        editor
            .acknowledge_schema_owner_adoption(reconciliation)
            .unwrap();
    });
}

#[test]
fn close_waits_for_held_publication_return_and_retains_known_publication_without_late_adoption() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "dirty-at-close");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let original_source = fixture.bytes(STARTUP);
        let project_root = editor.source_project_root();
        let predecessor = project_root
            .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        let operation = fixture.review(editor);
        let mut held = HeldPublication::arm(editor);
        editor.adopt_schema_owner(operation).unwrap();
        let returned = publication_returned(editor);
        held.wait_until_entered();
        let published_bytes = fixture.bytes(BUNDLE);
        let published_digest = ContentDigest::of_bytes(&published_bytes);
        let published = project_root
            .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        assert_ne!(published.identity(), predecessor.identity());
        for _ in 0..3 {
            let progress = held.poll(|| editor.advance_close());
            assert!(matches!(
                progress,
                EditorProjectCloseProgress::Incomplete { .. }
            ));
            assert!(!progress.task_shutdown().is_complete());
            held.assert_pending(editor, &returned);
            assert_eq!(
                editor.schema_owner_adoption_view().operation,
                Some(operation)
            );
            assert_eq!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Publishing
            );
            assert_eq!(editor.schema_owner_adoption_view().outcome, None);
            assert!(editor.schema_owner_adoption_publication().is_none());
            assert_eq!(source_state(editor, document), before);
            assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        }
        held.release();
        close(editor);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        for _ in 0..3 {
            let progress = editor.advance_close();
            assert!(matches!(progress, EditorProjectCloseProgress::Complete(_)));
            assert_eq!(
                editor.schema_owner_adoption_view().operation,
                Some(operation)
            );
            assert_eq!(
                editor.schema_owner_adoption_view().outcome,
                Some(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption)
            );
            assert_eq!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            );
            assert_publication_evidence(
                editor,
                predecessor.identity(),
                published.identity(),
                published_digest,
            );
            assert_eq!(source_state(editor, document), before);
            assert_eq!(editor.document_query_basis(document).unwrap(), basis);
            assert_eq!(
                readiness(editor, document),
                AuthoringRecordReadiness::UnknownSchema
            );
            assert_eq!(fixture.bytes(BUNDLE), published_bytes);
            assert_eq!(fixture.bytes(STARTUP), original_source);
            assert!(editor.source_tool_view().active.is_none());
        }
        assert_eq!(
            editor.reconcile_schema_owner_adoption(operation),
            Err(EditorSchemaOwnerAdoptionError::Closing)
        );
    });
}

struct ReleaseCleanup(CleanupTestControl);

impl Drop for ReleaseCleanup {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[test]
fn cancelled_failed_publication_keeps_cleanup_pending_after_physical_return_until_explicit_release()
{
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "dirty-with-cleanup-debt");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let original_bundle = fixture.bytes(BUNDLE);
        let original_source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        let publisher = editor
            .schema_owner_adoption
            .candidate
            .as_ref()
            .unwrap()
            .metadata
            .publisher
            .as_ref()
            .unwrap()
            .clone();
        let cleanup = ReleaseCleanup(
            publisher
                .try_with(|host| host.test_fail_after_write())
                .unwrap(),
        );
        let mut held = HeldPublication::arm(editor);
        editor.adopt_schema_owner(operation).unwrap();
        let returned = publication_returned(editor);
        held.wait_until_entered();
        held.assert_pending(editor, &returned);
        assert!(publisher.try_with(|host| host.has_cleanup()).unwrap());
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        held.poll(|| editor.cancel_schema_owner_adoption(operation))
            .unwrap();
        held.poll(|| editor.drive_editor_frame(Duration::ZERO));
        held.assert_pending(editor, &returned);
        assert!(editor.schema_owner_adoption_publication().is_none());
        held.release();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        assert_eq!(returned.state(), TaskReturnState::Returned);
        for _ in 0..3 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(frame.schema_owner_adoption_work_pending());
            assert!(publisher.try_with(|host| host.has_cleanup()).unwrap());
            assert!(editor.schema_owner_adoption.work.is_none());
            assert!(editor.schema_owner_adoption.pending());
            assert_eq!(
                editor.acknowledge_schema_owner_adoption(operation),
                Err(EditorSchemaOwnerAdoptionError::Operation)
            );
            assert_eq!(
                editor.prepare_installed_schema_owner(INSTALLED, fixture.receipt),
                Err(EditorSchemaOwnerAdoptionError::Busy)
            );
            assert_eq!(source_state(editor, document), before);
            assert_eq!(editor.document_query_basis(document).unwrap(), basis);
            assert_eq!(fixture.bytes(BUNDLE), original_bundle);
            assert!(editor.schema_owner_adoption_publication().is_none());
        }
        let deadline = Instant::now() + WAIT;
        loop {
            let progress = held.poll(|| editor.advance_close());
            assert!(matches!(
                progress,
                EditorProjectCloseProgress::Incomplete {
                    file_cleanup_pending: true,
                    ..
                }
            ));
            if progress.task_shutdown().is_complete() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "failed publication task did not physically retire: {progress:?}"
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert!(publisher.try_with(|host| host.has_cleanup()).unwrap());
        cleanup.0.release();
        close(editor);
        assert!(publisher.try_with(|_| ()).is_none());
        assert!(!editor.schema_owner_adoption.pending());
        assert_eq!(
            editor.schema_owner_adoption_view().outcome,
            Some(EditorSchemaOwnerAdoptionOutcome::Cancelled)
        );
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        assert_eq!(fixture.bytes(STARTUP), original_source);
        assert!(editor.schema_owner_adoption_publication().is_none());
    });
}

#[cfg(feature = "presentation-2d")]
#[test]
fn schema_review_masks_drained_preview_rechecks_and_fences_direct_preview_configuration() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "capture-before-schema-review");
        let before = source_state(editor, document);
        let original_bundle = fixture.bytes(BUNDLE);
        let original_source = fixture.bytes(STARTUP);

        // The real preview driver admits work for the dirty root before adoption quiesces it.
        editor.configure_editor_preview(nara_render::Extent2d::new(320, 180));
        assert!(editor.scene_capture.has_pending_work());
        let operation = fixture.review(editor);
        assert!(!editor.scene_capture.has_pending_work());
        assert!(!editor.scene_capture.schema_adoption_pending());
        assert_eq!(editor.scene_capture.recheck_after(), Some(Duration::ZERO));
        assert_eq!(editor.current_frame().scene_preview_recheck_after(), None);
        assert!(!editor.current_frame().scene_preview_work_pending());

        for (width, height) in [(640, 360), (800, 450), (320, 180)] {
            // This public entry drives preview directly, independently of the frame driver.
            editor.configure_editor_preview(nara_render::Extent2d::new(width, height));
            assert!(!editor.scene_capture.has_pending_work());
            assert!(!editor.scene_capture.schema_adoption_pending());
            assert_eq!(editor.scene_capture.recheck_after(), Some(Duration::ZERO));
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(frame.scene_preview_recheck_after(), None);
            assert!(!frame.scene_preview_work_pending());
            assert!(!editor.scene_capture.has_pending_work());
            assert_eq!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Review
            );
            assert_eq!(source_state(editor, document), before);
        }

        editor.cancel_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        editor.configure_editor_preview(nara_render::Extent2d::new(640, 360));
        assert!(editor.scene_capture.has_pending_work());
        wait(editor, |editor| {
            !editor.scene_capture.has_pending_work()
                && editor
                    .current_frame()
                    .scene_preview_recheck_after()
                    .is_some()
        });
        assert!(!editor.current_frame().scene_preview_work_pending());
        assert_eq!(source_state(editor, document), before);
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        assert_eq!(fixture.bytes(STARTUP), original_source);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
    });
}

#[test]
fn interrupted_publication_requires_actual_disk_reconciliation_without_inventing_a_receipt() {
    let fixture = Fixture::new();
    let mut editor = fixture.open();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let document = startup(&editor);
        name_edit(&mut editor, document, "dirty-before-interruption");
        let before = source_state(&editor, document);
        let original = fixture.bytes(BUNDLE);
        let operation = fixture.review(&mut editor);
        editor.schema_owner_adoption.next_publication_gate = Some(Box::new(|| {
            panic!("injected interruption after actual publication before result handoff");
        }));
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(
            terminal(&mut editor),
            EditorSchemaOwnerAdoptionOutcome::Uncertain
        );
        let published = fixture.bytes(BUNDLE);
        assert_ne!(published, original);
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(source_state(&editor, document), before);
        assert_eq!(
            readiness(&editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        name_edit(&mut editor, document, "dirty-after-interruption");
        let current = source_state(&editor, document);
        let recovery = editor.reconcile_schema_owner_adoption(operation).unwrap();
        wait(&mut editor, |editor| {
            matches!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Review
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            )
        });
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Review
        );
        editor.adopt_schema_owner(recovery).unwrap();
        assert_eq!(
            terminal(&mut editor),
            EditorSchemaOwnerAdoptionOutcome::Adopted
        );
        assert_eq!(source_state(&editor, document), current);
        assert_eq!(
            readiness(&editor, document),
            AuthoringRecordReadiness::Available
        );
        assert_eq!(fixture.bytes(BUNDLE), published);
        assert!(editor.schema_owner_adoption_publication().is_none());
    }));
    let deadline = Instant::now() + WAIT;
    loop {
        match editor.advance_close() {
            EditorProjectCloseProgress::Complete(receipt) => {
                assert!(receipt.task_shutdown().is_complete());
                assert!(receipt.task_shutdown().has_cleanup_failure());
                break;
            }
            progress => assert!(
                Instant::now() < deadline,
                "interrupted worker did not physically close: {progress:?}"
            ),
        }
        thread::sleep(Duration::from_millis(1));
    }
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}
