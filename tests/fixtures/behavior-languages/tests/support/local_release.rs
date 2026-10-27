//! Data-only capture, installation publication and inspection of the actual C# tool release.

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
        Self::variant("V1")
    }

    fn variant(variant: &str) -> Self {
        let artifacts = PathBuf::from(
            std::env::var_os(format!("NARA_SOURCE_TOOL_{variant}_DIR"))
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
            "release": if variant == "V2" { "2" } else { "1" },
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

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
pub(super) fn installed_variant(
    variant: &str,
) -> (
    tempfile::TempDir,
    nara_engine::project_host::EditorSourceToolInstallationInspection,
) {
    install(LocalRelease::variant(variant))
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
fn install(
    source: LocalRelease,
) -> (
    tempfile::TempDir,
    nara_engine::project_host::EditorSourceToolInstallationInspection,
) {
    use nara_engine::{
        fs::{DirectoryCapability, HostCapabilityOptions, RelativeComponent},
        project_host::{
            EditorSourceToolInstallationInspection, EditorSourceToolInstallationOutcome,
            EditorSourceToolInstallationPlan,
        },
    };
    use std::{fs, os::windows::fs::OpenOptionsExt};
    let captured = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(source.directory.path()).unwrap(),
        ContentDigest::of_bytes(&source.manifest),
    )
    .unwrap();
    let parent = tempfile::tempdir().unwrap();
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(parent.path())
        .unwrap();
    let capability = DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        capability,
        RelativeComponent::new("installed").unwrap(),
        captured,
    )
    .unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => panic!("Actual release publication failed: {other:?}"),
    };
    let inspection = EditorSourceToolInstallationInspection::read(
        installation.directory(),
        installation.receipt_digest(),
    )
    .unwrap();
    drop(installation);
    (parent, inspection)
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
#[test]
fn csharp_managed_approval_rejects_configuration_probing_native_assets_and_duplicate_keys() {
    use nara_engine::project_host::EditorSourceToolManagedError;
    let host = super::enemy_stats::approved_managed_host();
    for case in ["probing", "native", "duplicate"] {
        let mut source = LocalRelease::new();
        let (path, replacement) = if case == "native" {
            let mut value: serde_json::Value =
                serde_json::from_slice(&source.payloads["EnemyStatsTool.deps.json"]).unwrap();
            let libraries = value["targets"][".NETCoreApp,Version=v10.0"]
                .as_object_mut()
                .unwrap();
            libraries.values_mut().next().unwrap()["native"] = json!({"unknown.dll":{}});
            (
                "EnemyStatsTool.deps.json",
                serde_json::to_vec(&value).unwrap(),
            )
        } else if case == "duplicate" {
            ("EnemyStatsTool.runtimeconfig.json", br#"{"runtimeOptions":{"tfm":"net10.0","framework":{"name":"Microsoft.NETCore.App","version":"10.0.0"},"configProperties":{"System.Runtime.Serialization.EnableUnsafeBinaryFormatterSerialization":true,"System.Runtime.Serialization.EnableUnsafeBinaryFormatterSerialization":false}}}"#.to_vec())
        } else {
            let mut value: serde_json::Value =
                serde_json::from_slice(&source.payloads["EnemyStatsTool.runtimeconfig.json"])
                    .unwrap();
            value["runtimeOptions"]["additionalProbingPaths"] = json!(["external-packages"]);
            (
                "EnemyStatsTool.runtimeconfig.json",
                serde_json::to_vec(&value).unwrap(),
            )
        };
        std::fs::write(source.directory.path().join(path), &replacement).unwrap();
        let digest = ContentDigest::of_bytes(&replacement);
        let mut manifest: serde_json::Value = serde_json::from_slice(&source.manifest).unwrap();
        let member = manifest["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|member| member["path"] == path)
            .unwrap();
        member["length"] = json!(digest.length());
        member["blake3"] = json!(
            digest
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        source.manifest = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(
            source.directory.path().join("nara-tool.json"),
            &source.manifest,
        )
        .unwrap();
        let (_directory, inspected) = install(source);
        assert!(
            matches!(
                host.approve_installation(inspected),
                Err(EditorSourceToolManagedError::Contract(_))
            ),
            "{case}: unsupported configuration became an executable definition"
        );
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

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
#[test]
fn csharp_local_release_installs_frozen_bytes_and_fresh_inspection_rejects_content_drift() {
    use std::{fs, os::windows::fs::OpenOptionsExt};

    use nara_engine::{
        fs::{
            DirectoryCapability, HostCapabilityOptions, RelativeComponent, RelativePath,
            StageStatus,
        },
        project_host::{
            EditorSourceToolInstallationError, EditorSourceToolInstallationInspection,
            EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        },
    };

    let release = LocalRelease::new();
    let source = open_explicit_product_project_root(release.directory.path()).unwrap();
    let expected_manifest = ContentDigest::of_bytes(&release.manifest);
    let captured = PreparedEditorSourceToolRelease::capture(&source, expected_manifest).unwrap();
    let parent_directory = tempfile::tempdir().unwrap();
    let parent_handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(parent_directory.path())
        .unwrap();
    let parent = DirectoryCapability::from_host_handle(
        parent_handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap();
    let destination = RelativeComponent::new("installed").unwrap();
    let preview =
        EditorSourceToolInstallationPlan::new(parent.share(), destination.clone(), captured)
            .unwrap();
    assert_eq!(preview.release().manifest_digest(), expected_manifest);
    assert_eq!(
        preview.release().manifest_bytes(),
        release.manifest.as_slice()
    );
    assert_eq!(fs::read_dir(parent_directory.path()).unwrap().count(), 0);
    let captured = preview.cancel();
    assert_eq!(captured.manifest_digest(), expected_manifest);
    assert_eq!(fs::read_dir(parent_directory.path()).unwrap().count(), 0);

    // Only the temporary source copy changes; publication must use the captured actual DLL.
    let mut changed_dll = release.payloads["EnemyStatsTool.dll"].clone();
    changed_dll[0] ^= 1;
    fs::write(
        release.directory.path().join("EnemyStatsTool.dll"),
        &changed_dll,
    )
    .unwrap();
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&source, expected_manifest),
        Err(EditorSourceToolReleaseError::Digest { member }) if member == "EnemyStatsTool.dll"
    ));
    let plan = EditorSourceToolInstallationPlan::new(parent, destination, captured).unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => panic!("Expected actual C# release publication: {other:?}"),
    };
    assert_eq!(installation.package(), "trial.enemy-stats");
    assert_eq!(installation.release(), "1");
    assert_eq!(installation.receipt().manifest_digest(), expected_manifest);
    let installation_id = *installation.receipt().installation_id();
    assert_ne!(installation_id, [0; 32]);
    let expected_receipt = installation.receipt_digest();
    let receipt_bytes = installation
        .directory()
        .open_file_exact(&RelativePath::new(".nara-installation.json").unwrap())
        .unwrap()
        .read_to_end_bounded(expected_receipt.length())
        .unwrap();
    assert_eq!(ContentDigest::of_bytes(&receipt_bytes), expected_receipt);
    let mut expected_files = release.payloads.clone();
    assert_eq!(expected_files.len(), 5);
    expected_files.insert("nara-tool.json", release.manifest.clone());
    expected_files.insert(".nara-installation.json", receipt_bytes);
    assert_eq!(installation.members().len(), 7);
    for (&path, expected) in &expected_files {
        let member = installation
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap_or_else(|| panic!("Installation lost member {path}"));
        assert_eq!(member.digest(), ContentDigest::of_bytes(expected), "{path}");
        assert_eq!(
            member.kind(),
            match path {
                "nara-tool.json" | ".nara-installation.json" => None,
                SAMPLE => Some(EditorSourceToolReleaseMemberKind::Content),
                _ => Some(EditorSourceToolReleaseMemberKind::Tool),
            },
            "{path}",
        );
        assert_eq!(
            member.sync_receipt().progress().data_synced(),
            StageStatus::Achieved,
            "{path}",
        );
        let installed_bytes = installation
            .directory()
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap()
            .read_to_end_bounded(member.digest().length())
            .unwrap();
        assert_eq!(&installed_bytes, expected, "{path}");
    }
    assert_eq!(
        installation
            .publication()
            .durability()
            .parent_directory_synced(),
        StageStatus::Unsupported,
    );
    installation.verify().unwrap();
    drop(installation);
    drop(source);

    // Fresh inspection recovers persisted facts, not the closed owner's live-file provenance.
    let reopened_parent = open_explicit_product_project_root(parent_directory.path()).unwrap();
    let reopened = reopened_parent
        .open_directory_exact(&RelativePath::new("installed").unwrap())
        .unwrap();
    let inspected =
        EditorSourceToolInstallationInspection::read(&reopened, expected_receipt).unwrap();
    assert_eq!(inspected.receipt().installation_id(), &installation_id);
    assert_eq!(inspected.receipt().manifest_digest(), expected_manifest);
    assert_eq!(inspected.release().package(), "trial.enemy-stats");
    assert_eq!(inspected.release().release(), "1");
    assert_eq!(inspected.release().manifest_digest(), expected_manifest);
    assert_eq!(
        inspected.release().manifest_bytes(),
        release.manifest.as_slice()
    );
    assert_eq!(inspected.release().members().len(), release.payloads.len());
    for (&path, expected) in &release.payloads {
        let member = inspected
            .release()
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap_or_else(|| panic!("Fresh inspection lost member {path}"));
        assert_eq!(member.bytes(), expected.as_slice(), "{path}");
        assert_eq!(member.digest(), ContentDigest::of_bytes(expected), "{path}");
    }

    let mut changed_sample = release.payloads[SAMPLE].clone();
    changed_sample[0] ^= 1;
    fs::write(
        parent_directory.path().join("installed").join(SAMPLE),
        changed_sample,
    )
    .unwrap();
    assert!(matches!(
        EditorSourceToolInstallationInspection::read(&reopened, expected_receipt),
        Err(EditorSourceToolInstallationError::Inspection(
            EditorSourceToolReleaseError::Digest { member }
        )) if member == SAMPLE
    ));
    assert_eq!(
        reopened
            .open_file_exact(&RelativePath::new(".nara-installation.json").unwrap())
            .unwrap()
            .digest(expected_receipt.length())
            .unwrap(),
        expected_receipt,
    );
}
