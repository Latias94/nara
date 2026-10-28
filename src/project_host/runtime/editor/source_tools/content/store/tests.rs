#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_fs::{HostCapabilityOptions, TrustMode};
use nara_tooling::EditorSourceToolInstalledSelection;

use super::*;
#[cfg(windows)]
use crate::project_host::file_publication::{FilePublicationOutcome, FilePublicationRejection};
use crate::project_host::{
    file_publication::FilePublicationBinding,
    runtime::editor::source_tools::content::record::{Installation, Member, Observation, Role},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
const MOVED_PATH: &str = ".nara/source-tools/original-content.json";

struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "nara-content-store-{}-{stamp}-{sequence}",
            std::process::id(),
        ));
        fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    fn capability(&self, rights: CapabilityRights) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE,
            };

            fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&self.directory)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let file = fs::File::open(&self.directory).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(rights, TrustMode::Untrusted),
        )
        .unwrap()
    }

    fn create_parents(&self) {
        fs::create_dir(self.directory.join(METADATA_DIRECTORY)).unwrap();
        fs::create_dir(self.directory.join(".nara/source-tools")).unwrap();
    }

    fn write_record(&self, bytes: &[u8]) {
        fs::write(self.directory.join(PATH), bytes).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only exclusively created, explicitly named fixture objects are removed.
        for name in [PATH, MOVED_PATH, ".nara/source-tools", ".nara"] {
            let path = self.directory.join(name);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
                    fs::remove_dir(path).unwrap();
                }
                Ok(_) => fs::remove_file(path).unwrap(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("fixture cleanup inspection failed: {error}"),
            }
        }
        fs::remove_dir(&self.directory).unwrap();
    }
}

fn ledger() -> Ledger {
    Ledger {
        installations: vec![Installation {
            installed: EditorSourceToolInstalledSelection {
                package: "test.stats".into(),
                directory: "tools/absent-installation".into(),
                installation: [1; 32],
                receipt: ContentDigest::from_parts(64, [2; 32]),
                manifest: ContentDigest::from_parts(128, [3; 32]),
            },
            members: vec![Member {
                path: "samples/readme.txt".into(),
                role: Role::Content,
                baseline: ContentDigest::of_bytes(b"original sample"),
                modified: true,
                adopted: true,
                observation: Observation::Pending,
            }],
        }],
    }
}

#[test]
fn missing_directories_and_missing_leaf_are_observed_without_creating_anything() {
    let fixture = Fixture::new();
    let root = fixture.capability(CapabilityRights::ReadOnly);
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.ledger, Ledger::default());
    assert!(loaded.host.is_none());
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 0);

    fs::create_dir(fixture.directory.join(METADATA_DIRECTORY)).unwrap();
    assert!(load(&root).unwrap().host.is_none());
    fs::create_dir(fixture.directory.join(".nara/source-tools")).unwrap();
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.ledger, Ledger::default());
    let host = loaded.host.unwrap();
    assert_eq!(host.binding(), FilePublicationBinding::Missing);
    assert_eq!(host.accepted_source_observation(), None);
    assert!(!fixture.directory.join(PATH).exists());
}

#[test]
fn load_preserves_protection_and_recovery_facts_without_reading_installed_payloads() {
    let fixture = Fixture::new();
    fixture.create_parents();
    let bytes = record::encode(&ledger()).unwrap();
    fixture.write_record(&bytes);
    let root = fixture.capability(CapabilityRights::ReadOnly);
    let loaded = load(&root).unwrap();
    assert_eq!(loaded.ledger, ledger());
    assert!(!fixture.directory.join("tools").exists());
    assert_eq!(fs::read(fixture.directory.join(PATH)).unwrap(), bytes);
}

#[test]
fn loaded_host_retains_the_exact_decoded_leaf_after_namespace_replacement_and_unlink() {
    let fixture = Fixture::new();
    fixture.create_parents();
    let bytes = record::encode(&ledger()).unwrap();
    fixture.write_record(&bytes);
    let root = fixture.capability(CapabilityRights::ReadOnly);
    let observed = root.open_file_exact(&fixed_path(PATH)).unwrap();
    let identity = observed.identity();
    let loaded = load(&root).unwrap();
    let host = loaded.host.unwrap();
    drop(observed);
    assert_eq!(
        host.binding(),
        FilePublicationBinding::Existing {
            identity,
            digest: ContentDigest::of_bytes(&bytes)
        },
    );
    let accepted = host.accepted_source_observation();
    fs::rename(
        fixture.directory.join(PATH),
        fixture.directory.join(MOVED_PATH),
    )
    .unwrap();
    fixture.write_record(b"replacement leaf");
    fs::remove_file(fixture.directory.join(MOVED_PATH)).unwrap();
    assert_eq!(host.test_accepted_source_bytes().unwrap(), bytes);
    assert_eq!(host.accepted_source_observation(), accepted);
    assert_eq!(
        fs::read(fixture.directory.join(PATH)).unwrap(),
        b"replacement leaf"
    );
}

#[test]
fn corrupt_unsupported_and_oversized_records_never_become_empty_or_get_overwritten() {
    let fixture = Fixture::new();
    fixture.create_parents();
    let root = fixture.capability(CapabilityRights::ReadWrite);
    for (bytes, expected) in [
        (b"not JSON".to_vec(), Error::InvalidRecord),
        (
            br#"{"format_version":2,"installations":[]}"#.to_vec(),
            Error::UnsupportedFormat,
        ),
        (
            br#"{"format_version":1,"installations":[],"approved":true}"#.to_vec(),
            Error::InvalidRecord,
        ),
        (vec![b'!'; BYTE_LIMIT as usize + 1], Error::TooLarge),
    ] {
        fixture.write_record(&bytes);
        assert_eq!(load(&root).unwrap_err(), expected);
        assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
        assert_eq!(fs::read(fixture.directory.join(PATH)).unwrap(), bytes);
    }
}

#[test]
fn explicit_parent_creation_never_publishes_and_refuses_even_equal_existing_bytes() {
    let fixture = Fixture::new();
    let root = fixture.capability(CapabilityRights::ReadWrite);
    let host = create_missing_host(&root).unwrap();
    assert_eq!(host.binding(), FilePublicationBinding::Missing);
    assert!(fixture.directory.join(".nara/source-tools").is_dir());
    assert!(!fixture.directory.join(PATH).exists());
    let bytes = record::encode(&ledger()).unwrap();
    fixture.write_record(&bytes);
    assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
    assert_eq!(fs::read(fixture.directory.join(PATH)).unwrap(), bytes);
    assert!(!host.has_cleanup());
    #[cfg(windows)]
    {
        let mut host = host;
        assert!(matches!(
            host.publish(&bytes),
            FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged),
        ));
        assert_eq!(fs::read(fixture.directory.join(PATH)).unwrap(), bytes);
        assert!(!host.has_cleanup());
    }
}

#[test]
fn read_only_mutation_refuses_before_parent_creation_and_nonregular_paths_are_not_missing() {
    let fixture = Fixture::new();
    let root = fixture.capability(CapabilityRights::ReadOnly);
    assert_eq!(create_missing_host(&root).unwrap_err(), Error::ReadOnly);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 0);
    fs::write(fixture.directory.join(METADATA_DIRECTORY), b"keep").unwrap();
    assert!(load(&root).is_err());
    assert_eq!(
        fs::read(fixture.directory.join(METADATA_DIRECTORY)).unwrap(),
        b"keep"
    );
    fs::remove_file(fixture.directory.join(METADATA_DIRECTORY)).unwrap();
    fixture.create_parents();
    fs::create_dir(fixture.directory.join(PATH)).unwrap();
    assert!(load(&root).is_err());
    assert_eq!(
        create_missing_host(&fixture.capability(CapabilityRights::ReadWrite)).unwrap_err(),
        Error::Changed,
    );
    assert!(fixture.directory.join(PATH).is_dir());
}

#[cfg(target_os = "linux")]
#[test]
fn dangling_symlink_does_not_become_a_missing_record() {
    let fixture = Fixture::new();
    fixture.create_parents();
    std::os::unix::fs::symlink("absent", fixture.directory.join(PATH)).unwrap();
    let root = fixture.capability(CapabilityRights::ReadWrite);
    assert!(load(&root).is_err());
    assert!(create_missing_host(&root).is_err());
    assert!(
        fs::symlink_metadata(fixture.directory.join(PATH))
            .unwrap()
            .is_symlink()
    );
}
