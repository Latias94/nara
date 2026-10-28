//! Native templates consumed through public release, Project Session and Browser APIs.

#![cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind,
    StableAssetId,
};
use nara_engine::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, DirectoryPublicationOutcome,
        HostCapabilityOptions, RelativeComponent, TrustMode,
    },
    project_host::{
        EditorProjectCloseProgress, EditorProjectSession, EditorSourceToolInstallation,
        EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        EditorSourceToolPrefabTemplateImportRequest, PreparedEditorSourceToolPrefabTemplateImport,
        PreparedEditorSourceToolRelease, open_explicit_product_project_root,
    },
};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    AuthoringPropertyEdit, PrefabComponentIntent, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabOverlay,
    PrefabPropertyClaim, PrefabPropertyIntent, SceneComponentRecord, SceneDocument,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
    EditorDocumentOpenResult, EditorPersistenceAction, EditorPersistenceRequestResult,
    EditorPersistenceResult, EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
    EditorProjectSourceOpenCapability, EditorSourceToolArtifactOutcome,
    EditorSourceToolArtifactPhase, EditorSourceToolAttemptId, EditorSourceToolInstallationRequest,
    EditorSourceToolReclamationMemberState, EditorSourceToolReclamationOutcome,
    EditorSourceToolReclamationPhase, EditorSourceToolReclamationRecordPhase,
    EditorSourceToolSelectionOutcome, EditorSourceToolSelectionPhase,
    EditorSourceToolTemplateImportError, EditorSourceToolTemplateImportOutcome,
    EditorSourceToolTemplateImportView, EditorSourceToolWithdrawalMode,
};
use serde_json::json;

use crate::{
    authoring_project::{
        Project, applied, assert_history, assert_value, close, replay, save, schema_bundle,
    },
    scene_runtime::id,
};

const TIMEOUT: Duration = Duration::from_secs(30);
const NAME: &str = "nara.scene.Name";
const DESCRIPTOR: &str = "samples/template.json";
const SOURCE_UNIVERSE: &str = "72011426-37e2-48ba-bc06-78841468889a";
const SOURCE_PREFABS_ROOT: &str = "template-library/prefabs";
const DOCUMENTS: [&str; 3] = [
    "main.prefab.json",
    "middle.prefab.json",
    "child.prefab.json",
];
const ORIGINAL_IDS: [&str; 3] = [
    "01104e65-cf20-4585-8d5a-fd42952ea470",
    "47cdc828-aef3-4323-9304-ecf683961135",
    "da1f01e5-fc62-4c45-a9f3-7036fb23b741",
];
const ORIGINAL_NAME: &str = "child.prefab.json";
const SAVED_NAME: &str = "Project-owned entry";

fn named(entity: &str, name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(entity)).with_component(
        ComponentTypeId::new(NAME),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(name.into()),
        ),
    )
}

fn original_keys(stable_input: bool) -> [SourceDocumentKey; 3] {
    std::array::from_fn(|index| {
        if stable_input {
            return SourceDocumentKey::stable_asset(
                StableAssetId::parse_str(ORIGINAL_IDS[index]).unwrap(),
                SourceDocumentKind::Prefab,
            );
        }
        // Source keys use the original project coordinate; lookup paths are Prefab-root-relative.
        SourceDocumentKey::project_location(
            SOURCE_UNIVERSE.parse().unwrap(),
            AssetPath::new(format!("{SOURCE_PREFABS_ROOT}/{}", DOCUMENTS[index])).unwrap(),
            SourceDocumentKind::Prefab,
        )
    })
}

fn documents(keys: &[SourceDocumentKey; 3], references: &[AssetRef; 3]) -> [PrefabDocument; 3] {
    let child = PrefabDocument::from_ordered_roots([named("child", "Child")]);
    let mut nested = named("nested", "Middle");
    nested.prefab = Some(PrefabInstance {
        source: references[2].clone(),
        overrides: None,
    });
    let middle = PrefabDocument::from_ordered_roots([nested]);
    let mut overlay = PrefabOverlay::new(keys[1].clone());
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                expected_base: keys[2].clone(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("child")),
        },
        component: ComponentTypeId::new(NAME),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("value")),
            intent: PrefabPropertyIntent::Set(ComponentValue::String("Pinned child".into())),
            bindings: Vec::new(),
        }],
    });
    let mut entry = named("entry", ORIGINAL_NAME);
    entry.prefab = Some(PrefabInstance {
        source: references[1].clone(),
        overrides: Some(overlay),
    });
    [PrefabDocument::from_ordered_roots([entry]), middle, child]
}

fn metadata(stable_id: StableAssetId, path: &str, kind: AssetSourceKind) -> Vec<u8> {
    AssetMeta::new(
        AssetMetaIdentity::new(stable_id, AssetPath::new(path).unwrap(), kind),
        (),
    )
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn descriptor(entry: &str, documents: &[(&str, &str)], stable_input: bool) -> serde_json::Value {
    json!({
        "format_version": 1,
        "kind": "prefab_template",
        "source_universe": SOURCE_UNIVERSE,
        "prefabs_root": SOURCE_PREFABS_ROOT,
        "native_document_identity": if stable_input { "stable_metadata" } else { "project_location" },
        "entry": format!("{SOURCE_PREFABS_ROOT}/{entry}"),
        "documents": documents.iter().map(|(member, path)| json!({
            "member": member, "path": format!("{SOURCE_PREFABS_ROOT}/{path}"),
        })).collect::<Vec<_>>(),
    })
}

fn native_release(stable_input: bool) -> PreparedEditorSourceToolRelease {
    let artifacts = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_V1_DIR")
            .expect("Run python tests/fixtures/behavior-languages/run.py"),
    );
    let mut payloads = BTreeMap::new();
    for member in [
        "EnemyStatsTool.dll",
        "EnemyStatsTool.deps.json",
        "EnemyStatsTool.runtimeconfig.json",
        "enemy-stats.catalog.json",
    ] {
        let bytes = fs::read(artifacts.join(member)).unwrap();
        assert!(!bytes.is_empty(), "Missing actual C# payload: {member}");
        payloads.insert(member.to_owned(), (bytes, "tool"));
    }
    let references = [
        AssetRef::path(DOCUMENTS[0]).unwrap(),
        AssetRef::path(DOCUMENTS[1]).unwrap(),
        AssetRef::StableId(StableAssetId::parse_str(ORIGINAL_IDS[2]).unwrap()),
    ];
    for ((path, document), stable_id) in DOCUMENTS
        .into_iter()
        .zip(documents(&original_keys(stable_input), &references))
        .zip(ORIGINAL_IDS)
    {
        let member = format!("samples/{path}");
        payloads.insert(
            member.clone(),
            (document.to_json_string().unwrap().into_bytes(), "content"),
        );
        payloads.insert(
            format!("{member}.meta"),
            (
                metadata(
                    StableAssetId::parse_str(stable_id).unwrap(),
                    path,
                    AssetSourceKind::Prefab,
                ),
                "content",
            ),
        );
    }
    let valid = descriptor(
        "main.prefab.json",
        &[
            ("samples/main.prefab.json", "main.prefab.json"),
            ("samples/middle.prefab.json", "middle.prefab.json"),
            ("samples/child.prefab.json", "child.prefab.json"),
        ],
        stable_input,
    );
    payloads.insert(
        DESCRIPTOR.into(),
        (serde_json::to_vec(&valid).unwrap(), "content"),
    );
    let mut missing_root = valid.clone();
    missing_root.as_object_mut().unwrap().remove("prefabs_root");
    payloads.insert(
        "samples/missing-root.json".into(),
        (serde_json::to_vec(&missing_root).unwrap(), "content"),
    );
    let mut invalid = valid;
    invalid["undeclared_option"] = json!(true);
    payloads.insert(
        "samples/invalid.json".into(),
        (serde_json::to_vec(&invalid).unwrap(), "content"),
    );
    let mut external = named("external", "External dependency");
    external.prefab = Some(PrefabInstance {
        source: AssetRef::path("unbound.prefab.json").unwrap(),
        overrides: None,
    });
    payloads.insert(
        "samples/external.prefab.json".into(),
        (
            PrefabDocument::from_ordered_roots([external])
                .to_json_string()
                .unwrap()
                .into_bytes(),
            "content",
        ),
    );
    payloads.insert(
        "samples/external.json".into(),
        (
            serde_json::to_vec(&descriptor(
                "external.prefab.json",
                &[("samples/external.prefab.json", "external.prefab.json")],
                false,
            ))
            .unwrap(),
            "content",
        ),
    );
    let files = payloads.iter().map(|(path, (bytes, kind))| {
        let digest = ContentDigest::of_bytes(bytes);
        json!({
            "path": path,
            "length": digest.length(),
            "blake3": digest.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "kind": kind,
        })
    }).collect::<Vec<_>>();
    let manifest = serde_json::to_vec(&json!({
        "format_version": 1,
        "package": "trial.enemy-stats",
        "release": "native-prefab-1",
        "host_target": "x86_64-pc-windows-msvc",
        "authority": "trusted_process",
        "protocol_version": 1,
        "owner": "trial.enemy-stats",
        "component": "trial.enemy.EnemyStats",
        "catalog": "enemy-stats.catalog.json",
        "entry": "EnemyStatsTool.dll",
        "requires": [],
        "files": files,
    }))
    .unwrap();
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("nara-tool.json"), &manifest).unwrap();
    for (member, (bytes, _)) in payloads {
        let path = source.path().join(member);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(source.path()).unwrap(),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap()
}

fn stable_project(release: &PreparedEditorSourceToolRelease) -> Project {
    let project = Project::new(
        &schema_bundle(release.schema_owner()),
        &SceneDocument::from_ordered_roots([named("startup", "Original scene")]),
        "json",
    );
    let manifest = project.root().join("nara.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap().replace(
            "native_document_identity = 'project_location'",
            "native_document_identity = 'stable_metadata'",
        ),
    )
    .unwrap();
    // Both the policy and startup identity exist before the first public project Open.
    fs::write(
        project.root().join("scenes/startup.scene.json.meta"),
        metadata(
            StableAssetId::parse_str("c2fddf13-9e3b-4d80-9246-587a6625aa8c").unwrap(),
            "startup.scene.json",
            AssetSourceKind::Scene,
        ),
    )
    .unwrap();
    project
}

fn project_root(project: &Project) -> DirectoryCapability {
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(project.root())
        .unwrap();
    DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn install(
    root: &DirectoryCapability,
    release: PreparedEditorSourceToolRelease,
) -> Arc<EditorSourceToolInstallation> {
    let plan = EditorSourceToolInstallationPlan::new(
        root.share(),
        RelativeComponent::new("native-prefab-templates").unwrap(),
        release,
    )
    .unwrap();
    match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => Arc::new(installation),
        other => panic!("Actual native template installation failed: {other:?}"),
    }
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        if ready(editor) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Public native template operation did not return: import={:?}; selection={:?}; artifacts={:?}; browser={:?}",
            editor.source_tool_template_import_view(),
            editor.source_tool_selection_view(),
            editor.source_tool_artifact_view(),
            editor.current_frame().project_browser()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn idle(editor: &mut EditorProjectSession) {
    wait(editor, |editor| {
        editor.source_tool_selection_view().phase == EditorSourceToolSelectionPhase::Idle
            && editor.source_tool_artifact_view().phase == EditorSourceToolArtifactPhase::Idle
            && !editor.source_tool_artifact_view().physical_work_pending
            && !editor.current_frame().project_browser().has_pending_work()
    });
    assert!(editor.source_tool_selection_view().error.is_none());
    assert!(editor.source_tool_artifact_view().error.is_none());
    assert!(editor.current_frame().project_browser().failure().is_none());
}

fn register(
    editor: &mut EditorProjectSession,
    root: &DirectoryCapability,
    owner: &Arc<EditorSourceToolInstallation>,
) -> [u8; 32] {
    idle(editor);
    let inventory = editor.source_tool_artifact_view();
    let prepared = editor
        .prepare_source_tool_artifact_registration(&inventory.basis, Arc::clone(owner))
        .unwrap();
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    idle(editor);
    let inventory = editor.source_tool_artifact_view();
    let operation = inventory.operation.unwrap();
    assert_eq!(operation.attempt, attempt);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolArtifactOutcome::Registered)
    );
    let installation = inventory
        .artifacts
        .iter()
        .find(|artifact| artifact.directory.as_ref() == owner.project_path())
        .unwrap()
        .id;
    let selection = editor.source_tool_selection_view();
    let prepared = editor
        .prepare_source_tool_installations(
            &selection.basis,
            root.share(),
            &[EditorSourceToolInstallationRequest {
                directory: owner.project_path().into(),
                receipt: owner.receipt_digest(),
                requirements: Vec::new(),
            }],
        )
        .unwrap();
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    idle(editor);
    let registered = editor.source_tool_selection_view();
    let operation = registered.operation.unwrap();
    assert_eq!(operation.attempt, attempt);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolSelectionOutcome::Registered)
    );
    assert_eq!(registered.installed.len(), 1);
    assert_eq!(registered.installed[0].installed.installation, installation);
    assert!(registered.desired.is_none() && registered.active_origin.is_none());
    assert!(editor.source_tool_view().active.is_none());
    assert!(!editor.source_tool_view().has_pending_work());
    installation
}

fn prepare(
    editor: &EditorProjectSession,
    root: &DirectoryCapability,
    installation: [u8; 32],
    destination: &str,
) -> PreparedEditorSourceToolPrefabTemplateImport {
    editor
        .prepare_source_tool_prefab_template_import(
            &editor.source_tool_selection_view().basis,
            root.share(),
            EditorSourceToolPrefabTemplateImportRequest::new(installation, DESCRIPTOR, destination),
        )
        .unwrap()
}

struct ReviewedCopy {
    destination: String,
    entry: String,
    files: BTreeMap<String, Vec<u8>>,
    identities: [StableAssetId; 3],
}

fn review(prepared: &PreparedEditorSourceToolPrefabTemplateImport) -> ReviewedCopy {
    assert_eq!(
        prepared.entry(),
        format!("{}/main.prefab.json", prepared.destination())
    );
    assert_eq!(prepared.files().len(), DOCUMENTS.len() * 2);
    let files = prepared
        .files()
        .iter()
        .map(|file| {
            assert_eq!(ContentDigest::of_bytes(file.bytes()), file.digest());
            (file.path().to_owned(), file.bytes().to_vec())
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(files.len(), prepared.files().len());
    let logical_directory = prepared.destination().strip_prefix("prefabs/").unwrap();
    let identities = DOCUMENTS.map(|path| {
        let sidecar = format!("{path}.meta");
        let metadata = AssetMetaCandidate::<()>::decode_json_bytes(&files[&sidecar]).unwrap();
        assert_eq!(
            metadata.identity().path().as_str(),
            format!("{logical_directory}/{path}")
        );
        assert_eq!(metadata.identity().source_kind(), &AssetSourceKind::Prefab);
        let stable_id = metadata.identity().stable_id();
        assert!(
            !ORIGINAL_IDS
                .iter()
                .any(|original| StableAssetId::parse_str(original).unwrap() == stable_id)
        );
        let document = prepared
            .files()
            .iter()
            .find(|file| file.path() == path)
            .unwrap();
        assert_eq!(
            document.source(),
            Some(&SourceDocumentKey::stable_asset(
                stable_id,
                SourceDocumentKind::Prefab
            ))
        );
        assert!(
            prepared
                .files()
                .iter()
                .find(|file| file.path() == sidecar)
                .unwrap()
                .source()
                .is_none()
        );
        stable_id
    });
    assert_eq!(
        identities.iter().collect::<BTreeSet<_>>().len(),
        DOCUMENTS.len()
    );
    let keys = identities
        .map(|identity| SourceDocumentKey::stable_asset(identity, SourceDocumentKind::Prefab));
    let references = identities.map(AssetRef::StableId);
    for (path, expected) in DOCUMENTS.into_iter().zip(documents(&keys, &references)) {
        let actual = PrefabDocumentCandidate::decode_json_bytes(&files[path])
            .unwrap()
            .into_composition_source()
            .unwrap();
        // Whole-document equality covers local IDs, both lookup forms, base/hop pins and Name intent.
        assert_eq!(actual, expected, "Incorrect native copy of {path}");
    }
    ReviewedCopy {
        destination: prepared.destination().into(),
        entry: prepared.entry().into(),
        files,
        identities,
    }
}

fn settle_import(
    editor: &mut EditorProjectSession,
    attempt: &EditorSourceToolAttemptId,
) -> EditorSourceToolTemplateImportView {
    wait(editor, |editor| {
        let view = editor.source_tool_template_import_view();
        view.attempt.as_ref() == Some(attempt)
            && view.outcome.is_some()
            && !view.physical_work_pending
    });
    let view = editor.source_tool_template_import_view();
    assert!(!view.cleanup_failed, "{view:?}");
    view
}

fn publish(
    editor: &mut EditorProjectSession,
    prepared: PreparedEditorSourceToolPrefabTemplateImport,
    reviewed: &ReviewedCopy,
) -> EditorSourceToolTemplateImportView {
    let attempt = editor
        .commit_source_tool_prefab_template_import(prepared)
        .unwrap();
    let view = settle_import(editor, &attempt);
    let Some(EditorSourceToolTemplateImportOutcome::PrefabImported {
        destination,
        entry,
        members,
        parent_directory_sync,
    }) = &view.outcome
    else {
        panic!(
            "Native directory publication was not admitted: {view:?}; {:?}",
            editor.source_tool_prefab_template_publication()
        );
    };
    assert_eq!(destination.as_ref(), reviewed.destination);
    assert_eq!(entry.as_ref(), reviewed.entry);
    let publication = editor.source_tool_prefab_template_publication().unwrap();
    let DirectoryPublicationOutcome::Published(receipt) = publication.publication() else {
        panic!("Successful import lost its actual directory result: {publication:?}");
    };
    assert_eq!(
        *parent_directory_sync,
        receipt.durability().parent_directory_synced()
    );
    assert!(publication.directory().is_some());
    assert!(publication.error().is_none(), "{publication:?}");
    assert_eq!(publication.retained_files(), reviewed.files.len());
    assert_eq!(publication.created_members(), members.as_ref());
    assert_eq!(members.len(), reviewed.files.len());
    for member in members.iter() {
        assert_eq!(
            member.digest,
            ContentDigest::of_bytes(&reviewed.files[member.path.as_ref()])
        );
    }
    view
}

fn assert_copy(project: &Project, copy: &ReviewedCopy) {
    for (path, bytes) in &copy.files {
        assert_eq!(
            &fs::read(project.root().join(&copy.destination).join(path)).unwrap(),
            bytes,
            "{}/{path}",
            copy.destination
        );
    }
}

fn disk_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    result
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            editor.current_frame().revision(),
            [action],
        ))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn open_source(editor: &mut EditorProjectSession, source: &str) -> EditorDocumentId {
    idle(editor);
    assert_eq!(
        editor
            .current_frame()
            .project_browser()
            .entry(source)
            .unwrap()
            .open_capability(),
        EditorProjectSourceOpenCapability::Prefab
    );
    let opening = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: source.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = opening
    else {
        panic!("Public Browser Open refused {source}: {opening:?}");
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
        panic!("Public Browser Open failed: {result:?}");
    };
    assert_eq!(completed, request);
    assert!(
        matches!(dispatch(editor, EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request })),
        EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }) if acknowledged == request)
    );
    let frame = editor.current_frame();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.kind, EditorDocumentKind::Prefab);
    assert_eq!(tab.source_binding.as_ref().unwrap().locator(), source);
    assert!(!tab.dirty);
    assert_history(editor, document, 0, 0);
    document
}

fn set_name(editor: &mut EditorProjectSession, document: EditorDocumentId, value: &str) {
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_authoring_property_edit(
            &basis,
            &[AuthoringPropertyEdit {
                entity: id("entry"),
                component: ComponentTypeId::new(NAME),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new("value")),
                value: ComponentValue::String(value.into()),
            }],
        )
        .unwrap();
    applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
}

fn assert_name(editor: &EditorProjectSession, document: EditorDocumentId, value: &str) {
    assert_value(
        editor,
        document,
        "entry",
        NAME,
        "value",
        ComponentValue::String(value.into()),
    );
}

fn reopen(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let target = editor.current_frame().reopen_target();
    let outcome = dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
    );
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        outcome
    else {
        panic!("Native Prefab Reopen was refused: {outcome:?}");
    };
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    let frame = editor.current_frame();
    assert_eq!(frame.persistence().attempt(), Some(attempt));
    assert!(
        matches!(frame.persistence().result(), Some(EditorPersistenceResult::Opened { document: opened, .. }) if opened == document),
        "{:?}",
        frame.persistence()
    );
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    assert_history(editor, document, 0, 0);
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
}

fn close_preserving_import(
    editor: &mut EditorProjectSession,
    expected: &EditorSourceToolTemplateImportView,
) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            assert_eq!(receipt.template_import(), Some(expected));
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Native template physical Close did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn csharp_native_prefab_templates_are_independent_editable_project_sources_after_reclamation() {
    let release = native_release(true);
    let project = stable_project(&release);
    let root = project_root(&project);
    let owner = install(&root, release);
    let mut editor = project.open();
    let installation = register(&mut editor, &root, &owner);
    let startup = editor.current_frame().workspace().active_document.unwrap();
    let initial_frame = editor.current_frame();
    let initial_import = editor.source_tool_template_import_view();
    let initial_disk = disk_snapshot(project.root());
    let cancelled = prepare(&editor, &root, installation, "prefabs/dropped");
    review(&cancelled);
    drop(cancelled);
    assert!(!project.root().join("prefabs/dropped").exists());
    assert_eq!(disk_snapshot(project.root()), initial_disk);
    assert_eq!(
        editor.current_frame().workspace().documents,
        initial_frame.workspace().documents
    );
    assert_eq!(editor.source_tool_template_import_view(), initial_import);
    assert_history(&editor, startup, 0, 0);

    let prepared = prepare(&editor, &root, installation, "prefabs/first");
    let mut first = review(&prepared);
    assert!(!project.root().join(&first.destination).exists());
    publish(&mut editor, prepared, &first);
    assert_copy(&project, &first);
    idle(&mut editor);
    let prepared = prepare(&editor, &root, installation, "prefabs/second");
    let second = review(&prepared);
    assert!(!project.root().join(&second.destination).exists());
    assert!(
        first
            .identities
            .iter()
            .all(|identity| !second.identities.contains(identity))
    );
    let imported = publish(&mut editor, prepared, &second);
    assert_copy(&project, &first);
    assert_copy(&project, &second);
    assert_eq!(
        editor.current_frame().workspace().documents,
        initial_frame.workspace().documents
    );

    let document = open_source(&mut editor, &first.entry);
    assert_name(&editor, document, ORIGINAL_NAME);
    set_name(&mut editor, document, SAVED_NAME);
    assert_name(&editor, document, SAVED_NAME);
    assert_history(&editor, document, 1, 0);
    replay(&mut editor, document, false);
    assert_name(&editor, document, ORIGINAL_NAME);
    assert_history(&editor, document, 0, 1);
    replay(&mut editor, document, true);
    assert_name(&editor, document, SAVED_NAME);
    assert_copy(&project, &first);
    save(&mut editor, document);
    let saved = fs::read(project.root().join(&first.entry)).unwrap();
    let mut expected = PrefabDocumentCandidate::decode_json_bytes(&first.files[DOCUMENTS[0]])
        .unwrap()
        .into_composition_source()
        .unwrap();
    expected.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(NAME))
        .unwrap()
        .value = ComponentValue::String(SAVED_NAME.into());
    assert_eq!(
        PrefabDocumentCandidate::decode_json_bytes(&saved)
            .unwrap()
            .into_composition_source()
            .unwrap(),
        expected
    );
    first.files.insert(DOCUMENTS[0].into(), saved);
    assert_copy(&project, &first);
    assert_copy(&project, &second);
    let saved_revision = editor.document_query_basis(document).unwrap().revision;
    replay(&mut editor, document, false);
    assert_name(&editor, document, ORIGINAL_NAME);
    reopen(&mut editor, document);
    assert_name(&editor, document, SAVED_NAME);
    assert_ne!(
        editor
            .document_query_basis(document)
            .unwrap()
            .revision
            .source_id(),
        saved_revision.source_id()
    );

    // Keep an ordinary redo entry alive while the original installed content is withdrawn/unlinked.
    set_name(&mut editor, document, "History after artifact reclamation");
    replay(&mut editor, document, false);
    assert_history(&editor, document, 0, 1);
    let before_reclamation = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    let selection = editor.source_tool_selection_view();
    let withdrawal = editor
        .prepare_source_tool_withdrawal(
            &selection.basis,
            &installation,
            EditorSourceToolWithdrawalMode::RejectDependents,
        )
        .unwrap();
    let attempt = editor
        .withdraw_source_tool_installations(withdrawal)
        .unwrap();
    idle(&mut editor);
    let selection = editor.source_tool_selection_view();
    assert!(selection.installed.is_empty());
    let operation = selection.operation.as_ref().unwrap();
    assert_eq!(operation.attempt, attempt);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolSelectionOutcome::Withdrawn)
    );
    for member in owner.members() {
        let bytes = fs::read(
            project
                .root()
                .join(owner.project_path())
                .join(member.path()),
        )
        .unwrap();
        assert_eq!(
            ContentDigest::of_bytes(&bytes),
            member.digest(),
            "Import or source editing changed the installed member {}",
            member.path()
        );
    }
    let inventory = editor.source_tool_artifact_view();
    let prepared = editor
        .prepare_source_tool_reclamation(
            &inventory.basis,
            &selection.basis,
            root.share(),
            &installation,
        )
        .unwrap();
    assert!(prepared.creation_custody());
    let reviewed = prepared.record().clone();
    assert_eq!(reviewed.members.len(), owner.members().len());
    let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
    wait(&mut editor, |editor| {
        let view = editor.source_tool_reclamation_view();
        view.attempt.as_ref() == Some(&attempt)
            && view.phase == EditorSourceToolReclamationPhase::Terminal
            && !view.physical_work_pending
            && !view.record_publication_pending
    });
    let reclaimed = editor.source_tool_reclamation_view();
    assert_eq!(
        reclaimed.outcome,
        Some(EditorSourceToolReclamationOutcome::Complete)
    );
    assert!(reclaimed.error.is_none(), "{reclaimed:?}");
    let record = reclaimed.record.unwrap();
    let mut expected_record = reviewed;
    expected_record.phase = EditorSourceToolReclamationRecordPhase::Results;
    for member in &mut expected_record.members {
        member.state = EditorSourceToolReclamationMemberState::Removed;
    }
    assert_eq!(record, expected_record);
    assert_eq!(record.artifact, installation);
    for member in &record.members {
        assert_eq!(
            member.state,
            EditorSourceToolReclamationMemberState::Removed
        );
        assert!(
            !project
                .root()
                .join(owner.project_path())
                .join(member.path.as_ref())
                .exists()
        );
    }
    assert_eq!(
        editor.current_frame().workspace().document(document),
        Some(&before_reclamation)
    );
    replay(&mut editor, document, true);
    assert_name(&editor, document, "History after artifact reclamation");
    replay(&mut editor, document, false);
    assert_name(&editor, document, SAVED_NAME);
    assert_copy(&project, &first);
    assert_copy(&project, &second);
    assert_history(&editor, startup, 0, 0);
    let mut startup_tab = initial_frame.workspace().document(startup).unwrap().clone();
    startup_tab.active = false;
    assert_eq!(
        editor.current_frame().workspace().document(startup),
        Some(&startup_tab)
    );
    assert_eq!(
        fs::read(project.scene_path()).unwrap(),
        initial_disk[Path::new("scenes/startup.scene.json")]
    );
    close_preserving_import(&mut editor, &imported);
    drop(editor);

    let mut fresh = project.open();
    idle(&mut fresh);
    assert!(fresh.source_tool_selection_view().installed.is_empty());
    assert!(
        fresh
            .source_tool_artifact_view()
            .reclamations
            .contains(&record)
    );
    let document = open_source(&mut fresh, &first.entry);
    assert_name(&fresh, document, SAVED_NAME);
    let document = open_source(&mut fresh, &second.entry);
    assert_name(&fresh, document, ORIGINAL_NAME);
    assert_copy(&project, &first);
    assert_copy(&project, &second);
    close(&mut fresh);
}

#[test]
fn csharp_native_prefab_templates_refuse_invalid_closures_and_destination_competition() {
    let release = native_release(false);
    let project = stable_project(&release);
    let root = project_root(&project);
    let owner = install(&root, release);
    let mut editor = project.open();
    let installation = register(&mut editor, &root, &owner);
    let initial_frame = editor.current_frame();
    let initial_import = editor.source_tool_template_import_view();
    let initial_disk = disk_snapshot(project.root());
    let selection = editor.source_tool_selection_view();
    for (descriptor, expected) in [
        (
            "samples/invalid.json",
            EditorSourceToolTemplateImportError::InvalidDescriptor,
        ),
        (
            "samples/missing-root.json",
            EditorSourceToolTemplateImportError::InvalidDescriptor,
        ),
        (
            "samples/external.json",
            EditorSourceToolTemplateImportError::InvalidPrefab,
        ),
        (
            "EnemyStatsTool.dll",
            EditorSourceToolTemplateImportError::UnsupportedTemplate,
        ),
    ] {
        assert_eq!(
            editor
                .prepare_source_tool_prefab_template_import(
                    &selection.basis,
                    root.share(),
                    EditorSourceToolPrefabTemplateImportRequest::new(
                        installation,
                        descriptor,
                        "prefabs/refused"
                    ),
                )
                .unwrap_err(),
            expected,
            "{descriptor}"
        );
        assert!(!project.root().join("prefabs/refused").exists());
        assert_eq!(disk_snapshot(project.root()), initial_disk);
        assert_eq!(editor.source_tool_template_import_view(), initial_import);
        assert_eq!(
            editor.current_frame().workspace().documents,
            initial_frame.workspace().documents
        );
    }
    let prepared = prepare(&editor, &root, installation, "prefabs/competed");
    review(&prepared);
    let competitor = project.root().join("prefabs/competed");
    fs::create_dir(&competitor).unwrap();
    fs::write(competitor.join("keep.txt"), b"concurrent project source").unwrap();
    let before_commit = disk_snapshot(project.root());
    match editor.commit_source_tool_prefab_template_import(prepared) {
        Ok(attempt) => {
            let failed = settle_import(&mut editor, &attempt);
            assert_eq!(
                failed.outcome,
                Some(EditorSourceToolTemplateImportOutcome::Failed(
                    EditorSourceToolTemplateImportError::TargetExists
                ))
            );
        }
        Err(error) => assert_eq!(error, EditorSourceToolTemplateImportError::TargetExists),
    }
    assert_eq!(disk_snapshot(project.root()), before_commit);
    assert_eq!(
        fs::read(competitor.join("keep.txt")).unwrap(),
        b"concurrent project source"
    );
    assert_eq!(
        editor.current_frame().workspace().documents,
        initial_frame.workspace().documents
    );
    let result = editor.source_tool_template_import_view();
    if result.attempt.is_some() {
        close_preserving_import(&mut editor, &result);
    } else {
        close(&mut editor);
    }
}
