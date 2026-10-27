use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{CapabilityRights, HostCapabilityOptions};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentValue, ComponentValueKind,
};
use serde_json::{Value, json};

use super::*;

static NEXT_RELEASE: AtomicU64 = AtomicU64::new(1);

struct Release {
    directory: PathBuf,
    manifest: Value,
}

impl Release {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "nara_local_tool_release_{}_{}",
            std::process::id(),
            NEXT_RELEASE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        fs::create_dir(directory.join("samples")).unwrap();
        let catalog = ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.release.Stats"),
                    "Stats",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("health"),
                    "Health",
                    ComponentFieldPath::from_fields(["wire_health"]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                .with_authoring_initial_value(ComponentValue::map([(
                    "wire_health",
                    ComponentValue::I64(100),
                )])),
            ],
            ..Default::default()
        };
        let files = [
            (
                "catalog.json",
                catalog.to_json_string().unwrap().into_bytes(),
                "tool",
            ),
            (
                "Tool.dll",
                b"not an executable; capture must only read".to_vec(),
                "tool",
            ),
            ("samples/readme.txt", b"editable sample".to_vec(), "content"),
        ];
        let mut members = Vec::new();
        for (path, bytes, kind) in files {
            fs::write(directory.join(path), &bytes).unwrap();
            let digest = ContentDigest::of_bytes(&bytes);
            members.push(
                json!({"path":path,"kind":kind,"length":digest.length(),"blake3":hex(digest)}),
            );
        }
        Self {
            directory,
            manifest: json!({
                "format_version":1,"package":"test.stats","release":"1",
                "host_target":current_host_target().unwrap(),"authority":"trusted_process",
                "protocol_version":1,"owner":"test.stats-schema","component":"test.release.Stats",
                "catalog":"catalog.json","entry":"Tool.dll","requires":[],"files":members,
            }),
        }
    }

    fn root(&self, trust: TrustMode) -> DirectoryCapability {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.directory)
                .unwrap()
        };
        #[cfg(not(windows))]
        let directory = fs::File::open(&self.directory).unwrap();
        DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust),
        )
        .unwrap()
    }

    fn write_manifest(&self) -> ContentDigest {
        let bytes = serde_json::to_vec(&self.manifest).unwrap();
        fs::write(self.directory.join(MANIFEST_NAME), &bytes).unwrap();
        ContentDigest::of_bytes(&bytes)
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn hex(digest: ContentDigest) -> String {
    digest
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn capture_freezes_complete_declared_bytes_and_schema_without_execution_or_source_mutation() {
    let release = Release::new();
    let digest = release.write_manifest();
    // Unlisted files are not part of the artifact closure, even if present beside the entry.
    fs::write(release.directory.join("unlisted.secret"), "not a payload").unwrap();
    let root = release.root(TrustMode::Untrusted);
    let prepared = PreparedEditorSourceToolRelease::capture(&root, digest).unwrap();
    assert_eq!(prepared.package(), "test.stats");
    assert_eq!(prepared.release(), "1");
    assert_eq!(prepared.manifest_digest(), digest);
    assert_eq!(ContentDigest::of_bytes(prepared.manifest_bytes()), digest);
    assert_eq!(prepared.source_root_identity(), root.identity());
    assert_eq!(
        prepared.schema_owner().owner_id().as_str(),
        "test.stats-schema"
    );
    assert!(
        prepared
            .schema_owner()
            .schema(prepared.component())
            .is_some()
    );
    assert_eq!(prepared.members().len(), 3);
    let sample = prepared
        .members()
        .iter()
        .find(|file| file.path() == "samples/readme.txt")
        .unwrap();
    assert_eq!(sample.kind(), EditorSourceToolReleaseMemberKind::Content);
    assert_eq!(sample.bytes(), b"editable sample");
    assert_eq!(
        fs::read(release.directory.join(sample.path())).unwrap(),
        sample.bytes()
    );
    fs::write(release.directory.join(sample.path()), b"changed sample").unwrap();
    assert_eq!(sample.bytes(), b"editable sample");
    assert!(PreparedEditorSourceToolRelease::capture(&root, digest).is_err());
}

#[test]
fn source_digest_and_strict_capability_are_required_before_manifest_admission() {
    let release = Release::new();
    let digest = release.write_manifest();
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::TrustedLocal), digest),
        Err(EditorSourceToolReleaseError::SourceTrust)
    ));
    let root = release.root(TrustMode::Untrusted);
    let wrong = ContentDigest::of_bytes(&vec![b'!'; digest.length() as usize]);
    assert!(
        matches!(PreparedEditorSourceToolRelease::capture(&root, wrong),
        Err(EditorSourceToolReleaseError::Digest { member }) if member == MANIFEST_NAME)
    );
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(
            &root,
            ContentDigest::from_parts(MAX_MANIFEST_BYTES + 1, [0; 32])
        ),
        Err(EditorSourceToolReleaseError::ManifestLimit)
    ));
}

#[test]
fn incompatible_target_and_payload_cost_reject_before_opening_any_payload() {
    let mut release = Release::new();
    release.manifest["host_target"] = json!(if cfg!(windows) {
        "x86_64-unknown-linux-gnu"
    } else {
        "x86_64-pc-windows-msvc"
    });
    let digest = release.write_manifest();
    fs::remove_file(release.directory.join("catalog.json")).unwrap();
    let root = release.root(TrustMode::Untrusted);
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&root, digest),
        Err(EditorSourceToolReleaseError::HostTarget)
    ));
    release.manifest["host_target"] = json!(current_host_target().unwrap());
    release.manifest["files"][0]["length"] = json!(manifest::MAX_MEMBER_BYTES + 1);
    let digest = release.write_manifest();
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&root, digest),
        Err(EditorSourceToolReleaseError::Manifest(_))
    ));
}

#[test]
fn matching_payload_digest_does_not_bypass_schema_or_tool_component_admission() {
    let mut release = Release::new();
    let invalid = b"{}";
    fs::write(release.directory.join("catalog.json"), invalid).unwrap();
    let digest = ContentDigest::of_bytes(invalid);
    release.manifest["files"][0]["length"] = json!(digest.length());
    release.manifest["files"][0]["blake3"] = json!(hex(digest));
    let manifest = release.write_manifest();
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::Untrusted), manifest),
        Err(EditorSourceToolReleaseError::Schema)
    ));
}

#[test]
fn strict_capture_rejects_a_declared_hard_link_even_with_matching_content() {
    let release = Release::new();
    let digest = release.write_manifest();
    fs::hard_link(
        release.directory.join("Tool.dll"),
        release.directory.join("alias.dll"),
    )
    .unwrap();
    assert!(
        matches!(PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::Untrusted), digest),
        Err(EditorSourceToolReleaseError::Filesystem { member, .. }) if member == "Tool.dll")
    );
}
