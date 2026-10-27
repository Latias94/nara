//! Data-only capture of the actual C# tool release, without installation or execution.

#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
    all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
))]

use std::{collections::BTreeMap, path::PathBuf};

use nara_engine::{
    fs::{CapabilityRights, ContentDigest, TrustMode},
    project_host::{
        EditorSourceToolReleaseError, EditorSourceToolReleaseMemberKind,
        PreparedEditorSourceToolRelease, open_explicit_product_project_root,
    },
};
use nara_reflect::{
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, PersistentComponentSchemaOwnerId,
};
use serde_json::json;

const TOOL_FILES: [&str; 4] = [
    "EnemyStatsTool.dll",
    "EnemyStatsTool.deps.json",
    "EnemyStatsTool.runtimeconfig.json",
    "enemy-stats.catalog.json",
];
const SAMPLE: &str = "samples/readme.txt";
const HOST_TARGET: &str = if cfg!(target_os = "windows") {
    "x86_64-pc-windows-msvc"
} else {
    "x86_64-unknown-linux-gnu"
};

struct LocalRelease {
    directory: tempfile::TempDir,
    payloads: BTreeMap<&'static str, Vec<u8>>,
    manifest: Vec<u8>,
}

impl LocalRelease {
    fn new() -> Self {
        let artifacts = PathBuf::from(
            std::env::var_os("NARA_SOURCE_TOOL_V1_DIR")
                .expect("Run python tests/fixtures/behavior-languages/run.py"),
        );
        let directory = tempfile::tempdir().unwrap();
        let mut payloads = BTreeMap::new();
        for path in TOOL_FILES {
            let bytes = std::fs::read(artifacts.join(path))
                .unwrap_or_else(|error| panic!("Missing actual C# release member {path}: {error}"));
            assert!(!bytes.is_empty(), "Empty actual C# release member {path}");
            std::fs::write(directory.path().join(path), &bytes).unwrap();
            payloads.insert(path, bytes);
        }
        std::fs::create_dir(directory.path().join("samples")).unwrap();
        let sample = b"EnemyStats sample content; not an executable tool member.\n".to_vec();
        std::fs::write(directory.path().join(SAMPLE), &sample).unwrap();
        payloads.insert(SAMPLE, sample);
        let files: Vec<_> = payloads
            .iter()
            .map(|(path, bytes)| {
                let digest = ContentDigest::of_bytes(bytes);
                let hash: String = digest
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                json!({
                    "path": path,
                    "length": digest.length(),
                    "blake3": hash,
                    "kind": if *path == SAMPLE { "content" } else { "tool" },
                })
            })
            .collect();
        let manifest = serde_json::to_vec(&json!({
            "format_version": 1,
            "package": "trial.enemy-stats",
            "release": "1",
            "host_target": HOST_TARGET,
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
        std::fs::write(directory.path().join("nara-tool.json"), &manifest).unwrap();
        Self {
            directory,
            payloads,
            manifest,
        }
    }
}

#[test]
fn csharp_local_release_captures_complete_artifacts_and_owns_immutable_payloads() {
    let release = LocalRelease::new();
    let root = open_explicit_product_project_root(release.directory.path()).unwrap();
    assert_eq!(root.options().rights(), CapabilityRights::ReadOnly);
    assert_eq!(root.options().trust(), TrustMode::Untrusted);
    let expected_manifest = ContentDigest::of_bytes(&release.manifest);
    let captured = PreparedEditorSourceToolRelease::capture(&root, expected_manifest).unwrap();

    assert_eq!(captured.package(), "trial.enemy-stats");
    assert_eq!(captured.release(), "1");
    assert_eq!(captured.host_target(), HOST_TARGET);
    assert_eq!(captured.entry(), "EnemyStatsTool.dll");
    assert_eq!(captured.component().as_str(), "trial.enemy.EnemyStats");
    assert!(captured.requires().is_empty());
    assert_eq!(captured.manifest_bytes(), release.manifest.as_slice());
    assert_eq!(captured.manifest_digest(), expected_manifest);
    let expected_owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats").unwrap(),
        ComponentSchemaCatalog::from_json_bytes(&release.payloads["enemy-stats.catalog.json"])
            .unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        captured.schema_owner().owner_id(),
        expected_owner.owner_id()
    );
    assert_eq!(captured.schema_owner().current(), expected_owner.current());
    assert!(captured.schema_owner().predecessor().is_none());

    // Mutate only this release's temporary source copies, never the runner's built artifacts.
    for path in ["EnemyStatsTool.dll", SAMPLE] {
        let original = &release.payloads[path];
        let mut changed = original.clone();
        changed[0] ^= 1;
        std::fs::write(release.directory.path().join(path), &changed).unwrap();
        assert!(matches!(
            PreparedEditorSourceToolRelease::capture(&root, expected_manifest),
            Err(EditorSourceToolReleaseError::Digest { member }) if member == path
        ));
        let member = captured
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap();
        assert_eq!(member.bytes(), original.as_slice());
        assert_eq!(member.digest(), ContentDigest::of_bytes(original));
        std::fs::write(release.directory.path().join(path), original).unwrap();
    }

    assert_eq!(captured.members().len(), release.payloads.len());
    for (&path, bytes) in &release.payloads {
        let member = captured
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap_or_else(|| panic!("Captured release lost member {path}"));
        assert_eq!(member.bytes(), bytes.as_slice(), "{path}");
        assert_eq!(member.digest(), ContentDigest::of_bytes(bytes), "{path}");
        assert_eq!(
            member.kind(),
            if path == SAMPLE {
                EditorSourceToolReleaseMemberKind::Content
            } else {
                EditorSourceToolReleaseMemberKind::Tool
            },
            "{path}",
        );
    }
}

#[test]
fn csharp_local_release_rejects_manifest_drift_from_the_selected_raw_digest() {
    let release = LocalRelease::new();
    let root = open_explicit_product_project_root(release.directory.path()).unwrap();
    let expected_manifest = ContentDigest::of_bytes(&release.manifest);
    let mut changed: serde_json::Value = serde_json::from_slice(&release.manifest).unwrap();
    changed["release"] = json!("2");
    let changed = serde_json::to_vec(&changed).unwrap();
    assert_eq!(changed.len(), release.manifest.len());
    assert_ne!(ContentDigest::of_bytes(&changed), expected_manifest);
    std::fs::write(release.directory.path().join("nara-tool.json"), changed).unwrap();

    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&root, expected_manifest),
        Err(EditorSourceToolReleaseError::Digest { member }) if member == "nara-tool.json"
    ));
}
