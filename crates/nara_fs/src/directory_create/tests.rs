use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::*;
use crate::{CapabilityRights, FileFacts, FileKind, HostCapabilityOptions, TrustMode, platform};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara_fs_directory_create_{}_{}_{}",
            std::process::id(),
            stamp,
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn parent(&self, rights: CapabilityRights) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.0),
            HostCapabilityOptions::new(rights, TrustMode::TrustedLocal).with_generation(7),
        )
        .unwrap()
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn verified_exclusive_create_returns_the_created_directory_authority() {
    let root = TestRoot::new();
    let parent = root.parent(CapabilityRights::ReadWrite);
    let name = RelativeComponent::new("staging").unwrap();
    let child = parent.create_child_directory_retained(&name).unwrap();
    let opened = parent.open_child_directory(&name).unwrap();

    assert!(child.same_object(&opened).unwrap());
    assert_eq!(child.identity(), opened.identity());
    assert_ne!(child.identity(), parent.identity());
    assert_eq!(child.identity().session(), parent.identity().session());
    assert_eq!(
        child.identity().generation(),
        parent.identity().generation()
    );
    assert_eq!(child.options(), parent.options());
    let member = child
        .create_child_file(&RelativeComponent::new("member.bin").unwrap())
        .unwrap();
    assert!(root.0.join("staging/member.bin").is_file());
    drop(member);
    drop(opened);
    drop(child);
}

#[test]
fn existing_child_conflict_has_no_created_custody_and_preserves_contents() {
    let root = TestRoot::new();
    fs::create_dir(root.0.join("existing")).unwrap();
    fs::write(root.0.join("existing/keep.bin"), b"existing contents").unwrap();
    let parent = root.parent(CapabilityRights::ReadWrite);
    let failure = parent
        .create_child_directory_retained(&RelativeComponent::new("existing").unwrap())
        .unwrap_err();

    assert!(matches!(
        failure.error(),
        FsError::AlreadyExists {
            operation: FsOperation::CreateDirectory
        }
    ));
    assert!(failure.created().is_none());
    let (error, created) = failure.into_parts();
    assert!(matches!(error, FsError::AlreadyExists { .. }));
    assert!(created.is_none());
    assert_eq!(
        fs::read(root.0.join("existing/keep.bin")).unwrap(),
        b"existing contents"
    );
}

#[test]
fn read_only_rejection_has_no_creation_or_custody() {
    let root = TestRoot::new();
    let parent = root.parent(CapabilityRights::ReadOnly);
    let failure = parent
        .create_child_directory_retained(&RelativeComponent::new("forbidden").unwrap())
        .unwrap_err();

    assert!(matches!(
        failure.error(),
        FsError::ReadOnlyCapability {
            operation: FsOperation::CreateDirectory
        }
    ));
    assert!(failure.created().is_none());
    assert!(!root.0.join("forbidden").exists());
}

#[test]
fn post_create_validation_failure_retains_handle_parent_name_and_available_identity() {
    let root = TestRoot::new();
    let parent = root.parent(CapabilityRights::ReadWrite);
    for (spelling, identity_proven) in [("unknown", false), ("observed", true)] {
        let name = RelativeComponent::new(spelling).unwrap();
        // The existing hook supplies a rejected facts observation after actual native creation.
        let facts = FileFacts {
            identity: parent.inner.root_native,
            kind: FileKind::Other,
            link_count: 1,
            reparse_tag: None,
            identity_proven,
        };
        let failure = platform::with_test_facts_override(facts, || {
            parent.create_child_directory_retained(&name)
        })
        .unwrap_err();
        assert!(matches!(
            failure.error(),
            FsError::ExclusiveChildUnverified {
                operation: FsOperation::CreateDirectory,
                source,
            } if matches!(source.as_ref(), FsError::NotDirectory)
        ));
        let custody = failure.created().unwrap();
        assert_eq!(custody.name(), &name);
        assert_eq!(
            custody.identity(),
            identity_proven.then_some(parent.identity())
        );
        assert!(custody.has_handle());
        assert!(Arc::ptr_eq(&custody.parent.inner, &parent.inner));
        assert_eq!(
            platform::facts(custody.file.as_ref().unwrap())
                .unwrap()
                .kind,
            FileKind::Directory
        );
        assert!(root.0.join(spelling).is_dir());
        let (error, custody) = failure.into_parts();
        assert!(matches!(error, FsError::ExclusiveChildUnverified { .. }));
        let custody = custody.unwrap();
        drop(name);
        assert_eq!(custody.name().to_str(), Some(spelling));
        drop(custody);
        assert!(
            root.0.join(spelling).is_dir(),
            "drop must not remove the created child"
        );
    }
}

#[test]
fn created_but_unopened_failure_retains_creation_even_without_a_handle() {
    let root = TestRoot::new();
    let parent = root.parent(CapabilityRights::ReadWrite);
    let name = RelativeComponent::new("unopened").unwrap();
    let failure = platform::with_test_directory_creation_post_mutation_error(
        FsError::Unproven {
            operation: FsOperation::OpenDirectory,
            proof: "injected failure after actual exclusive directory creation",
        },
        || parent.create_child_directory_retained(&name),
    )
    .unwrap_err();

    assert!(matches!(
        failure.error(),
        FsError::ExclusiveChildUnverified {
            operation: FsOperation::CreateDirectory,
            source,
        } if matches!(source.as_ref(), FsError::Unproven {
            operation: FsOperation::OpenDirectory,
            ..
        })
    ));
    let custody = failure.created().unwrap();
    assert_eq!(custody.name(), &name);
    assert_eq!(custody.identity(), None);
    assert!(!custody.has_handle());
    assert!(Arc::ptr_eq(&custody.parent.inner, &parent.inner));
    assert!(root.0.join("unopened").is_dir());
    let (_, custody) = failure.into_parts();
    let custody = custody.unwrap();
    drop(parent);
    assert!(platform::facts(&custody.parent.inner.file).is_ok());
    drop(custody);
    assert!(
        root.0.join("unopened").is_dir(),
        "drop must not remove the created child"
    );
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }
    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}
