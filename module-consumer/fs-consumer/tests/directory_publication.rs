use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[cfg(windows)]
use std::fs::OpenOptions;

use nara_fs::{
    CapabilityRights, DirectoryCapability, DirectoryPublicationBeginError,
    DirectoryPublicationOutcome, FsError, FsOperation, HostCapabilityOptions,
    PublicationIdentityEvidence, PublicationSourceBinding, PublicationTopologyEvidence,
    RelativeComponent, RelativePath, TrustMode,
};
#[cfg(windows)]
use nara_fs::{
    ConflictProtection, DirectoryPublicationCause, ParentAuthorizationTier, PublicationAtomicity,
    StageStatus,
};
use nara_fs_module_consumer::{ConformanceTreeStageError, stage_nested_tree, validate_nested_tree};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

#[test]
#[cfg(all(target_arch = "x86_64", windows))]
fn public_consumer_publishes_and_reopens_one_bounded_nested_tree() {
    let root = TestRoot::new();
    let parent = trusted_parent(&root);
    let destination = component("published-tree");
    let transaction = stage_nested_tree(&parent, &destination, &component(".staging-success"))
        .expect("the public consumer should stage a finite tree");
    let candidate = transaction.staging_identity();

    let DirectoryPublicationOutcome::Published(receipt) = transaction.publish() else {
        panic!("the supported local filesystem should publish the candidate");
    };
    assert_eq!(receipt.parent_identity(), parent.identity());
    assert_eq!(receipt.candidate_identity(), candidate);
    assert_eq!(receipt.published_identity(), candidate);
    assert_eq!(receipt.destination(), &destination);
    assert_eq!(
        receipt.parent_authorization(),
        ParentAuthorizationTier::HandleBound
    );
    assert_eq!(
        receipt.publication_atomicity(),
        PublicationAtomicity::AtomicNameSwitch
    );
    assert_eq!(
        receipt.conflict_protection(),
        ConflictProtection::StrongCompareAndSwap
    );
    assert_eq!(receipt.durability().name_published(), StageStatus::Achieved);

    #[cfg(windows)]
    {
        assert_eq!(
            receipt.source_binding(),
            PublicationSourceBinding::HandleBound
        );
        assert_eq!(
            receipt.publication_identity_evidence(),
            PublicationIdentityEvidence::HandleBoundCandidate
        );
        assert_eq!(
            receipt.topology_evidence(),
            PublicationTopologyEvidence::SameVolume
        );
        assert_eq!(
            receipt.durability().parent_directory_synced(),
            StageStatus::Unsupported
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            receipt.source_binding(),
            PublicationSourceBinding::NameBound
        );
        assert_eq!(
            receipt.publication_identity_evidence(),
            PublicationIdentityEvidence::NameBoundCandidate
        );
        assert_eq!(
            receipt.topology_evidence(),
            PublicationTopologyEvidence::SameMount
        );
    }

    let published = parent
        .open_directory(&relative("published-tree"))
        .expect("the exact destination should reopen through public authority");
    assert_eq!(published.identity(), receipt.published_identity());
    validate_nested_tree(&published)
        .expect("the reopened finite tree should match the staged plan");
    assert_directory_absent(&parent, ".staging-success");
}

#[test]
#[cfg(all(target_arch = "x86_64", windows))]
fn publication_collision_preserves_the_existing_destination_and_returns_the_candidate() {
    let root = TestRoot::new();
    let parent = trusted_parent(&root);
    let destination = component("occupied-tree");
    let transaction = stage_nested_tree(&parent, &destination, &component(".staging-collision"))
        .expect("the public consumer should stage a finite tree");
    let candidate_identity = transaction.staging_identity();

    let existing = parent
        .create_child_directory(&destination)
        .expect("the competing destination should be created without overwrite");
    let existing_identity = existing.identity();
    let mut sentinel = existing
        .create_child_file(&component("sentinel.txt"))
        .expect("the existing destination should own its sentinel");
    {
        use std::io::Write;
        sentinel.writer().unwrap().write_all(b"existing\n").unwrap();
    }
    sentinel.sync().unwrap();
    drop(sentinel);
    drop(existing);

    let DirectoryPublicationOutcome::Orphaned(orphan) = transaction.publish() else {
        panic!("a non-empty rejected candidate cannot be reported as cleaned");
    };
    assert!(matches!(
        orphan.cause(),
        DirectoryPublicationCause::Filesystem(FsError::AlreadyExists {
            operation: FsOperation::PublishDirectory
        })
    ));
    assert_eq!(orphan.destination(), &destination);
    assert!(orphan.staging_handle_retained());
    assert_eq!(orphan.staging_identity(), Some(candidate_identity));
    let candidate = orphan
        .staging_directory()
        .expect("the recoverable orphan must retain its candidate capability");
    assert_eq!(candidate.identity(), candidate_identity);
    validate_nested_tree(&candidate).expect("the rejected candidate must remain complete");

    let existing = parent
        .open_directory(&relative("occupied-tree"))
        .expect("the pre-existing destination must remain authoritative");
    assert_eq!(existing.identity(), existing_identity);
    assert!(matches!(
        existing.open_child_file(&component("root.txt")),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(matches!(
        existing.open_directory(&relative("nested")),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound
    ));
    let existing = parent
        .open_file(&relative("occupied-tree/sentinel.txt"))
        .expect("the pre-existing destination must remain authoritative");
    assert_eq!(existing.read_to_end_bounded(32).unwrap(), b"existing\n");
}

#[test]
#[cfg(all(target_arch = "x86_64", any(windows, target_os = "linux")))]
fn read_only_parent_rejects_before_creating_any_staging_name() {
    let root = TestRoot::new();
    let parent = DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();

    let error = stage_nested_tree(
        &parent,
        &component("read-only-tree"),
        &component(".staging-read-only"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ConformanceTreeStageError::Begin(DirectoryPublicationBeginError::BeforeStaging(
            FsError::ReadOnlyCapability {
                operation: FsOperation::BeginDirectoryPublication
            }
        ))
    ));
    assert_directory_empty(root.path());
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn linux_consumer_reports_the_runtime_publication_tier_without_overclaiming() {
    let root = TestRoot::new();
    let parent = trusted_parent(&root);
    let destination = component("published-tree");

    match stage_nested_tree(&parent, &destination, &component(".staging-linux")) {
        Ok(transaction) => {
            let DirectoryPublicationOutcome::Published(receipt) = transaction.publish() else {
                panic!("a candidate admitted on the proven Linux tier must publish");
            };
            assert_eq!(
                receipt.source_binding(),
                PublicationSourceBinding::NameBound
            );
            assert_eq!(
                receipt.publication_identity_evidence(),
                PublicationIdentityEvidence::NameBoundCandidate
            );
            assert_eq!(
                receipt.topology_evidence(),
                PublicationTopologyEvidence::SameMount
            );
            let published = parent
                .open_directory(&relative("published-tree"))
                .expect("the admitted Linux destination must reopen");
            validate_nested_tree(&published)
                .expect("the admitted Linux destination must remain complete");
            assert_directory_absent(&parent, ".staging-linux");
        }
        Err(ConformanceTreeStageError::Begin(DirectoryPublicationBeginError::BeforeStaging(
            FsError::Unsupported {
                operation: FsOperation::BeginDirectoryPublication,
                ..
            }
            | FsError::Unproven {
                operation: FsOperation::BeginDirectoryPublication,
                ..
            },
        ))) => assert_directory_empty(root.path()),
        Err(error) => panic!("unexpected Linux publication admission failure: {error}"),
    }
}

fn trusted_parent(root: &TestRoot) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .expect("the host should issue a supported local directory capability")
}

fn component(value: &str) -> RelativeComponent {
    RelativeComponent::new(value).unwrap()
}

fn relative(value: &str) -> RelativePath {
    RelativePath::new(value).unwrap()
}

fn assert_directory_absent(parent: &DirectoryCapability, path: &str) {
    let error = parent
        .open_directory(&relative(path))
        .expect_err("the staging name must be absent after publication");
    assert!(matches!(
        error,
        FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        } if source.kind() == std::io::ErrorKind::NotFound
    ));
}

fn assert_directory_empty(path: &Path) {
    let entries = fs::read_dir(path)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        entries.is_empty(),
        "the parent directory changed: {entries:?}"
    );
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
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

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara_fs_module_consumer_{}_{}_{}",
            std::process::id(),
            timestamp,
            sequence
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0)
            .unwrap_or_else(|error| panic!("failed to remove {}: {error}", self.0.display()));
    }
}
