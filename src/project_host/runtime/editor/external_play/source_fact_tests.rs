use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::{PrefabDocument, PrefabInstance, SceneDocument, SceneEntityId, SceneEntityRecord};

use crate::project_content::ProjectContentLoader;
use crate::project_host::composition::{
    built_in_schema_providers, project_runtime_plugins, resolve_runtime_plan,
};
use crate::project_host::{
    ingest_project_manifest, runtime_agreement::prepare_runtime_schema_agreement,
};

use super::super::EditorProjectCloseProgress;
use super::*;

const SCENE_METADATA: &str = "scenes/startup.scene.json.meta";
const PREFAB_METADATA: &str = "prefabs/shared.prefab.json.meta";
const PREFAB_ID: &str = "71e9bc93-21e8-4c8f-b1d3-a508b726da70";
static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

#[test]
fn supervised_receipt_includes_stable_scene_metadata_in_strict_startup_digest() {
    let project = SourceReceiptProject::new(&[]);

    assert_receipt_matches_strict_startup(&project, &[SCENE_METADATA]);
}

#[test]
fn supervised_receipt_includes_shared_stable_prefab_metadata_once() {
    let path = AssetRef::path("shared.prefab.json").unwrap();
    for references in [
        [path.clone(), path.clone()],
        [
            path,
            AssetRef::StableId(StableAssetId::parse_str(PREFAB_ID).unwrap()),
        ],
    ] {
        // The second case also admits this sidecar through stable-reference catalogue lookup.
        let project = SourceReceiptProject::new(&references);

        assert_receipt_matches_strict_startup(&project, &[PREFAB_METADATA, SCENE_METADATA]);
    }
}

fn assert_receipt_matches_strict_startup(project: &SourceReceiptProject, metadata_paths: &[&str]) {
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(project.capability(), Some("desktop"))
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
    let encoded = editor.supervised_desktop_source_receipt().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "the source-receipt Editor did not close"
        );
        std::thread::yield_now();
    }
    drop(editor);

    let receipt = EditorExternalPlaySourceReceipt::decode_canonical(&encoded).unwrap();
    assert_eq!(
        receipt
            .source_facts()
            .iter()
            .filter(|fact| fact.kind() == EditorExternalPlaySourceKind::AssetMetadata)
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>(),
        metadata_paths
    );
    for path in metadata_paths {
        let fact = receipt
            .source_facts()
            .iter()
            .find(|fact| fact.project_path().as_str() == *path)
            .unwrap();
        assert_eq!(
            fact.digest(),
            ContentDigest::of_bytes(&fs::read(project.root.join(path)).unwrap())
        );
    }

    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, Some("desktop")).unwrap();
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
    let agreement = prepare_runtime_schema_agreement(&root, &candidate, &plan).unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let runtime = loader
        .load_with_runtime_agreement(&candidate, &plan, agreement)
        .unwrap();
    let startup = runtime.startup_content_closure().unwrap();
    assert!(runtime.has_runtime_schema_agreement());
    assert_eq!(receipt.content_digest(), startup.content_digest());
    assert_eq!(receipt.source_facts().len(), startup.members().len() + 2);
    for member in startup.members() {
        let fact = receipt
            .source_facts()
            .iter()
            .find(|fact| fact.project_path() == member.path())
            .unwrap();
        assert_eq!(fact.digest(), member.digest());
    }
    assert_eq!(
        startup
            .members()
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::AssetMetadata)
            .map(|member| member.path().as_str())
            .collect::<Vec<_>>(),
        metadata_paths
    );
}

struct SourceReceiptProject {
    root: PathBuf,
}

impl SourceReceiptProject {
    fn new(prefab_references: &[AssetRef]) -> Self {
        let project = Self {
            root: std::env::temp_dir().join(format!(
                "nara_supervised_source_facts_{}_{}",
                std::process::id(),
                NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
            )),
        };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(project.root.join(directory)).unwrap();
        }
        fs::write(
            project.root.join("nara.toml"),
            r#"schema_version = 1
[project]
source_universe = "305b2522-2728-47cb-b44e-e888c7d2e9c7"
native_document_identity = "stable_metadata"
stable_id = "source-fact-regression"
name = "Source fact regression"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[profiles.desktop.execution]
contract = "desktop-supervised-play/v1"
"#,
        )
        .unwrap();
        fs::write(
            project.root.join("Cargo.toml"),
            "[package]\nname = 'source-fact-regression'\nversion = '0.1.0'\nedition = '2024'\n",
        )
        .unwrap();
        fs::write(project.root.join("Cargo.lock"), "version = 4\n").unwrap();
        let bundle =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let entities = prefab_references
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let mut entity = SceneEntityRecord::new(
                    SceneEntityId::new(format!("instance-{index}")).unwrap(),
                );
                entity.prefab = Some(PrefabInstance {
                    source: reference.clone(),
                    overrides: Default::default(),
                });
                entity
            });
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(entities)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project.write_metadata(
            SCENE_METADATA,
            "startup.scene.json",
            AssetSourceKind::Scene,
            "844974a5-b882-4f09-8419-9e74c955c6ab",
        );
        if !prefab_references.is_empty() {
            fs::write(
                project.root.join("prefabs/shared.prefab.json"),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
                    SceneEntityId::new("leaf").unwrap(),
                )])
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            project.write_metadata(
                PREFAB_METADATA,
                "shared.prefab.json",
                AssetSourceKind::Prefab,
                PREFAB_ID,
            );
        }
        project
    }

    fn write_metadata(&self, path: &str, source: &str, kind: AssetSourceKind, id: &str) {
        fs::write(
            self.root.join(path),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str(id).unwrap(),
                    AssetPath::new(source).unwrap(),
                    kind,
                ),
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
}

impl Drop for SourceReceiptProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
