use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use super::TemporaryFileCleanup;
use crate::{
    CapabilityRights, DirectoryCapability, FileFacts, FileKind, FsError, FsOperation,
    HostCapabilityOptions, RelativeComponent, RelativePath, TrustMode, platform,
};

const CONTENT: &[u8] = b"candidate bytes";

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara_fs_cleanup_comparison_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn open(
        &self,
        rights: CapabilityRights,
        trust: TrustMode,
        generation: u64,
    ) -> DirectoryCapability {
        #[cfg(unix)]
        let file = fs::File::open(&self.path).unwrap();
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
                .open(&self.path)
                .unwrap()
        };
        DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(rights, trust).with_generation(generation),
        )
        .unwrap()
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

fn candidate(parent: &DirectoryCapability) -> TemporaryFileCleanup {
    let mut temporary = parent
        .create_temp(&RelativeComponent::new("candidate.tmp").unwrap())
        .unwrap();
    temporary.write_all(CONTENT).unwrap();
    temporary.into_cleanup()
}

#[test]
fn same_object_distinguishes_equal_bytes_in_independent_host_sessions() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::Untrusted, 1);
    let cleanup = candidate(&parent);
    fs::write(root.path.join("copy.txt"), CONTENT).unwrap();
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::Untrusted, 2);
    let copy = reader_root
        .open_file_exact(&RelativePath::new("copy.txt").unwrap())
        .unwrap();

    assert_ne!(
        cleanup.identity().unwrap().session(),
        copy.identity().session()
    );
    assert_eq!(copy.read_to_end_bounded(64).unwrap(), CONTENT);
    assert!(!cleanup.same_object(&copy).unwrap());
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
}

#[test]
fn same_object_retains_renamed_original_across_host_roots_and_cleanup_drop() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::Untrusted, 1);
    let cleanup = candidate(&parent);
    fs::rename(
        root.path.join("candidate.tmp"),
        root.path.join("retained.txt"),
    )
    .unwrap();
    fs::write(root.path.join("candidate.tmp"), CONTENT).unwrap();
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::Untrusted, 2);
    let retained = reader_root
        .open_file_exact(&RelativePath::new("retained.txt").unwrap())
        .unwrap();
    let replacement = reader_root
        .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
        .unwrap();

    assert_ne!(cleanup.identity().unwrap(), retained.identity());
    assert!(cleanup.same_object(&retained).unwrap());
    assert!(!cleanup.same_object(&replacement).unwrap());
    drop(cleanup);

    assert_eq!(retained.read_to_end_bounded(64).unwrap(), CONTENT);
    assert_eq!(fs::read(root.path.join("retained.txt")).unwrap(), CONTENT);
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
}

#[test]
fn same_object_preserves_bytes_reader_position_and_independent_authority() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::Untrusted, 7);
    let cleanup = candidate(&parent);
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::TrustedLocal, 19);
    let mut file = reader_root
        .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
        .unwrap();
    let identity = file.identity();
    let parent_identity = file.parent_identity();
    let resolution = file.resolution_tier();
    let cleanup_identity = cleanup.identity();
    let cleanup_name = cleanup.name().clone();
    let mut reader = file.reader().unwrap();
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix).unwrap();
    assert_eq!(&prefix, b"cand");

    assert!(cleanup.same_object(&file).unwrap());
    assert!(cleanup.same_object(&file).unwrap());
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).unwrap();
    assert_eq!(remaining, b"idate bytes");
    drop(reader);
    assert!(matches!(
        file.writer(),
        Err(FsError::ReadOnlyCapability {
            operation: FsOperation::Write,
        })
    ));
    assert_eq!(file.identity(), identity);
    assert_eq!(file.parent_identity(), parent_identity);
    assert_eq!(file.rights(), CapabilityRights::ReadOnly);
    assert_eq!(file.trust(), TrustMode::TrustedLocal);
    assert_eq!(file.resolution_tier(), resolution);
    assert_eq!(file.identity().generation().get(), 19);
    assert_eq!(cleanup.identity(), cleanup_identity);
    assert_eq!(cleanup.name(), &cleanup_name);
    assert!(!cleanup.cleanup_on_drop);
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
    drop(cleanup);
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
    assert_eq!(file.read_to_end_bounded(64).unwrap(), CONTENT);
}

#[test]
fn same_object_rechecks_unknown_initial_identity_without_upgrading_cleanup() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::TrustedLocal, 1);
    let name = RelativeComponent::new("candidate.tmp").unwrap();
    let failed_facts = FileFacts {
        kind: FileKind::Other,
        identity_proven: false,
        ..platform::facts(&parent.inner.file).unwrap()
    };
    let failure =
        platform::with_test_facts_override(failed_facts, || parent.create_temp(&name)).unwrap_err();
    assert!(matches!(failure.error(), FsError::NotRegularFile));
    let cleanup = failure.into_parts().1.unwrap();
    assert_eq!(cleanup.identity(), None);
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::Untrusted, 2);
    let file = reader_root
        .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
        .unwrap();
    let unproven = FileFacts {
        identity_proven: false,
        ..platform::facts(&cleanup.file).unwrap()
    };

    let result = platform::with_test_facts_override(unproven, || cleanup.same_object(&file));
    assert!(matches!(
        result,
        Err(FsError::Unproven {
            operation: FsOperation::InspectHandle,
            ..
        })
    ));
    assert_eq!(cleanup.identity(), None);
    assert!(root.path.join("candidate.tmp").exists());
    assert!(cleanup.same_object(&file).unwrap());
    assert_eq!(cleanup.identity(), None);
    assert!(!cleanup.cleanup_on_drop);
    drop(cleanup);
    assert!(root.path.join("candidate.tmp").exists());
    assert!(file.read_to_end_bounded(64).unwrap().is_empty());
}

#[test]
fn same_object_rejects_invalid_or_unproven_cleanup_facts() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::TrustedLocal, 1);
    let cleanup = candidate(&parent);
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::TrustedLocal, 2);
    let file = reader_root
        .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
        .unwrap();
    let facts = platform::facts(&cleanup.file).unwrap();
    let identity = cleanup.identity();

    let nonregular = FileFacts {
        kind: FileKind::Other,
        ..facts
    };
    let result = platform::with_test_facts_override(nonregular, || cleanup.same_object(&file));
    assert!(matches!(result, Err(FsError::NotRegularFile)));
    let reparse = FileFacts {
        reparse_tag: Some(0xA000_000C),
        ..facts
    };
    let result = platform::with_test_facts_override(reparse, || cleanup.same_object(&file));
    assert!(matches!(
        result,
        Err(FsError::ReparsePoint { tag: 0xA000_000C })
    ));
    let unproven = FileFacts {
        identity_proven: false,
        ..facts
    };
    let result = platform::with_test_facts_override(unproven, || cleanup.same_object(&file));
    assert!(matches!(
        result,
        Err(FsError::Unproven {
            operation: FsOperation::InspectHandle,
            ..
        })
    ));
    assert_eq!(cleanup.identity(), identity);
    assert!(!cleanup.cleanup_on_drop);
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
}

#[test]
fn same_object_rechecks_strict_single_link_policy_on_each_owner() {
    for (cleanup_trust, reader_trust) in [
        (TrustMode::Untrusted, TrustMode::TrustedLocal),
        (TrustMode::TrustedLocal, TrustMode::Untrusted),
    ] {
        let root = TestRoot::new();
        let parent = root.open(CapabilityRights::ReadWrite, cleanup_trust, 1);
        let cleanup = candidate(&parent);
        let reader_root = root.open(CapabilityRights::ReadOnly, reader_trust, 2);
        let file = reader_root
            .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
            .unwrap();
        assert!(cleanup.same_object(&file).unwrap());
        fs::hard_link(root.path.join("candidate.tmp"), root.path.join("alias.txt")).unwrap();

        assert!(matches!(
            cleanup.same_object(&file),
            Err(FsError::MultipleLinks { links: 2 })
        ));
        assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
        assert_eq!(fs::read(root.path.join("alias.txt")).unwrap(), CONTENT);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn same_object_does_not_enable_unsupported_temporary_discard() {
    let root = TestRoot::new();
    let parent = root.open(CapabilityRights::ReadWrite, TrustMode::Untrusted, 1);
    let cleanup = candidate(&parent);
    let reader_root = root.open(CapabilityRights::ReadOnly, TrustMode::Untrusted, 2);
    let file = reader_root
        .open_file_exact(&RelativePath::new("candidate.tmp").unwrap())
        .unwrap();
    assert!(cleanup.same_object(&file).unwrap());

    let failure = cleanup.discard().unwrap_err();
    assert!(matches!(
        failure.error(),
        FsError::Unsupported {
            operation: FsOperation::RemoveTemporary,
            ..
        }
    ));
    assert!(failure.cleanup().is_some());
    drop(failure);
    assert_eq!(fs::read(root.path.join("candidate.tmp")).unwrap(), CONTENT);
    assert_eq!(file.read_to_end_bounded(64).unwrap(), CONTENT);
}
