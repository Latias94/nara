#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_fs::{HostCapabilityOptions, TrustMode};
use nara_tooling::{
    EditorSourceToolArtifactInstallation, EditorSourceToolArtifactMember,
    EditorSourceToolInstalledSelection,
};

use super::*;
use crate::project_host::file_publication::FilePublicationBinding;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
const MOVED: &str = ".nara/source-tools/observed-artifacts.json";
const OLD: &str = ".nara/source-tools/content.json";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "nara-artifact-inventory-{}-{stamp}-{sequence}",
            std::process::id(),
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn root(&self, rights: CapabilityRights) -> DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.0)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let handle = fs::File::open(&self.0).unwrap();
        DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(rights, TrustMode::Untrusted),
        )
        .unwrap()
    }

    fn parents(&self) {
        fs::create_dir(self.0.join(".nara")).unwrap();
        fs::create_dir(self.0.join(PARENT)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Remove only exclusively created, enumerated fixture names, never an artifact subtree.
        for name in [PATH, MOVED, OLD, PARENT, ".nara"] {
            let path = self.0.join(name);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
                    fs::remove_dir(path).unwrap()
                }
                Ok(_) => fs::remove_file(path).unwrap(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("fixture inspection failed: {error}"),
            }
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

fn inventory() -> Inventory {
    let installed = EditorSourceToolInstalledSelection {
        package: "test.stats".into(),
        directory: ".nara/source-tools/artifacts/absent".into(),
        installation: [1; 32],
        receipt: ContentDigest::from_parts(64, [2; 32]),
        manifest: ContentDigest::from_parts(128, [3; 32]),
    };
    Inventory {
        installations: vec![EditorSourceToolArtifactInstallation {
            members: vec![
                EditorSourceToolArtifactMember {
                    path: ".nara-installation.json".into(),
                    role: record::Role::Metadata,
                    baseline: installed.receipt,
                    observation: record::Observation::Missing,
                },
                EditorSourceToolArtifactMember {
                    path: "nara-tool.json".into(),
                    role: record::Role::Metadata,
                    baseline: installed.manifest,
                    observation: record::Observation::Changed,
                },
                EditorSourceToolArtifactMember {
                    path: "tool.dll".into(),
                    role: record::Role::Tool,
                    baseline: ContentDigest::of_bytes(b"tool"),
                    observation: record::Observation::Failed,
                },
            ],
            installed,
        }],
    }
}

#[test]
fn absent_inventory_read_and_old_record_do_not_create_or_import_anything() {
    let fixture = Fixture::new();
    let root = fixture.root(CapabilityRights::ReadOnly);
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.inventory, Inventory::default());
    assert!(loaded.host.is_none());
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    fixture.parents();
    fs::write(fixture.0.join(OLD), b"old protection is not read").unwrap();
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.inventory, Inventory::default());
    assert_eq!(
        loaded.host.unwrap().binding(),
        FilePublicationBinding::Missing
    );
    assert!(!fixture.0.join(PATH).exists());
    assert_eq!(
        fs::read(fixture.0.join(OLD)).unwrap(),
        b"old protection is not read"
    );
}

#[test]
fn loading_exact_facts_does_not_inspect_or_reconstruct_installation_objects() {
    let fixture = Fixture::new();
    fixture.parents();
    let bytes = record::encode(&inventory()).unwrap();
    fs::write(fixture.0.join(PATH), &bytes).unwrap();
    let root = fixture.root(CapabilityRights::ReadOnly);
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.inventory, inventory());
    assert!(!fixture.0.join(".nara/source-tools/artifacts").exists());
    assert_eq!(
        loaded.host.unwrap().test_accepted_source_bytes().unwrap(),
        bytes
    );
}

#[test]
fn host_keeps_the_actual_decoded_leaf_after_rename_and_equal_replacement() {
    let fixture = Fixture::new();
    fixture.parents();
    let bytes = record::encode(&inventory()).unwrap();
    fs::write(fixture.0.join(PATH), &bytes).unwrap();
    let root = fixture.root(CapabilityRights::ReadWrite);
    let loaded = load(&root).unwrap();
    let mut host = loaded.host.unwrap();
    let original = host.accepted_source_observation();
    fs::rename(fixture.0.join(PATH), fixture.0.join(MOVED)).unwrap();
    fs::write(fixture.0.join(PATH), &bytes).unwrap();
    let retained = root.open_file_exact(&path(MOVED)).unwrap();
    let replacement = root.open_file_exact(&path(PATH)).unwrap();
    assert!(host.same_source_object(&retained).unwrap());
    assert!(!host.same_source_object(&replacement).unwrap());
    assert_eq!(host.accepted_source_observation(), original);
    #[cfg(windows)]
    assert!(matches!(
        host.publish(&bytes),
        crate::project_host::file_publication::FilePublicationOutcome::Rejected(
            crate::project_host::file_publication::FilePublicationRejection::TargetChanged,
        )
    ));
    #[cfg(target_os = "linux")]
    host.mark_uncertain();
    assert_eq!(fs::read(fixture.0.join(PATH)).unwrap(), bytes);
}

#[test]
fn corrupt_unsupported_oversized_and_nonregular_records_never_become_empty() {
    let fixture = Fixture::new();
    fixture.parents();
    let root = fixture.root(CapabilityRights::ReadWrite);
    for (bytes, expected) in [
        (b"not JSON".to_vec(), Error::InvalidRecord),
        (
            br#"{"format_version":2,"installations":[]}"#.to_vec(),
            Error::UnsupportedFormat,
        ),
        (vec![b'!'; BYTE_LIMIT as usize + 1], Error::TooLarge),
    ] {
        fs::write(fixture.0.join(PATH), &bytes).unwrap();
        assert_eq!(load(&root).unwrap_err(), expected);
        assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
        assert_eq!(fs::read(fixture.0.join(PATH)).unwrap(), bytes);
    }
    fs::remove_file(fixture.0.join(PATH)).unwrap();
    fs::create_dir(fixture.0.join(PATH)).unwrap();
    assert!(load(&root).is_err());
    assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
}

#[test]
fn only_explicit_writable_preparation_creates_fixed_parents_without_publishing() {
    let fixture = Fixture::new();
    assert_eq!(
        create_missing_host(&fixture.root(CapabilityRights::ReadOnly)).unwrap_err(),
        Error::ReadOnly
    );
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    let root = fixture.root(CapabilityRights::ReadWrite);
    let host = create_missing_host(&root).unwrap();
    assert_eq!(host.binding(), FilePublicationBinding::Missing);
    assert!(fixture.0.join(PARENT).is_dir());
    assert!(!fixture.0.join(PATH).exists());
    let bytes = record::encode(&Inventory::default()).unwrap();
    fs::write(fixture.0.join(PATH), &bytes).unwrap();
    assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
    assert_eq!(fs::read(fixture.0.join(PATH)).unwrap(), bytes);
}

#[cfg(target_os = "linux")]
#[test]
fn dangling_symlink_never_becomes_an_absent_inventory() {
    let fixture = Fixture::new();
    fixture.parents();
    std::os::unix::fs::symlink("absent", fixture.0.join(PATH)).unwrap();
    let root = fixture.root(CapabilityRights::ReadWrite);
    assert!(load(&root).is_err());
    assert!(create_missing_host(&root).is_err());
}
