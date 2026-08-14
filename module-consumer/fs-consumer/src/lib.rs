use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::Write;

use nara_fs::{
    DirectoryCapability, DirectoryPublicationBeginError, DirectoryPublicationOutcome,
    DirectoryPublicationTransaction, FsError, RelativeComponent, RelativePath,
};

pub const ROOT_MEMBER_BYTES: &[u8] = b"root-member-v1\n";
pub const NESTED_MEMBER_BYTES: &[u8] = b"nested-member-v1\n";

const ROOT_MEMBER: &str = "root.txt";
const NESTED_DIRECTORY: &str = "nested";
const NESTED_MEMBER: &str = "value.bin";
const MAX_MEMBER_BYTES: u64 = 64;

#[derive(Debug)]
pub enum ConformanceTreeContentError {
    Filesystem(FsError),
    Write(io::Error),
    ContentMismatch { member: &'static str },
}

impl Display for ConformanceTreeContentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Filesystem(error) => Display::fmt(error, formatter),
            Self::Write(error) => write!(formatter, "failed to write a staged member: {error}"),
            Self::ContentMismatch { member } => {
                write!(
                    formatter,
                    "staged member {member} did not match the finite plan"
                )
            }
        }
    }
}

impl Error for ConformanceTreeContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Filesystem(error) => Some(error),
            Self::Write(error) => Some(error),
            Self::ContentMismatch { .. } => None,
        }
    }
}

impl From<FsError> for ConformanceTreeContentError {
    fn from(error: FsError) -> Self {
        Self::Filesystem(error)
    }
}

impl From<io::Error> for ConformanceTreeContentError {
    fn from(error: io::Error) -> Self {
        Self::Write(error)
    }
}

#[derive(Debug)]
pub enum ConformanceTreeStageError {
    Begin(DirectoryPublicationBeginError),
    Candidate {
        source: ConformanceTreeContentError,
        terminal: Box<DirectoryPublicationOutcome>,
    },
}

impl Display for ConformanceTreeStageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Begin(error) => Display::fmt(error, formatter),
            Self::Candidate { source, .. } => write!(
                formatter,
                "finite tree staging failed and the publication transaction was terminated: {source}"
            ),
        }
    }
}

impl Error for ConformanceTreeStageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(error) => Some(error),
            Self::Candidate { source, .. } => Some(source),
        }
    }
}

/// Stages and validates one known finite nested tree through the public `nara_fs` Interface.
///
/// Failures after transaction creation consume `abort` and return its terminal outcome alongside
/// the member-construction error; this helper never hides a recoverable orphan.
pub fn stage_nested_tree(
    parent: &DirectoryCapability,
    destination: &RelativeComponent,
    staging_name: &RelativeComponent,
) -> Result<DirectoryPublicationTransaction, ConformanceTreeStageError> {
    stage_nested_tree_with(parent, destination, staging_name, |staging| {
        let mut root_member = staging.create_child_file(&component(ROOT_MEMBER))?;
        write_member(&mut root_member, ROOT_MEMBER_BYTES)?;

        let nested = staging.create_child_directory(&component(NESTED_DIRECTORY))?;
        let mut nested_member = nested.create_child_file(&component(NESTED_MEMBER))?;
        write_member(&mut nested_member, NESTED_MEMBER_BYTES)?;

        drop(nested_member);
        drop(nested);
        drop(root_member);
        validate_nested_tree(staging)
    })
}

fn stage_nested_tree_with(
    parent: &DirectoryCapability,
    destination: &RelativeComponent,
    staging_name: &RelativeComponent,
    stage: impl FnOnce(&DirectoryCapability) -> Result<(), ConformanceTreeContentError>,
) -> Result<DirectoryPublicationTransaction, ConformanceTreeStageError> {
    let transaction = parent
        .begin_directory_publication(destination, staging_name)
        .map_err(ConformanceTreeStageError::Begin)?;
    let staging = transaction.staging_directory();
    let staged = stage(&staging);
    drop(staging);

    match staged {
        Ok(()) => Ok(transaction),
        Err(source) => Err(ConformanceTreeStageError::Candidate {
            source,
            terminal: Box::new(transaction.abort()),
        }),
    }
}

/// Reopens and validates the finite member plan from an already-authorized directory capability.
pub fn validate_nested_tree(root: &DirectoryCapability) -> Result<(), ConformanceTreeContentError> {
    let root_member = root.open_child_file(&component(ROOT_MEMBER))?;
    require_bytes(
        ROOT_MEMBER,
        &root_member.read_to_end_bounded(MAX_MEMBER_BYTES)?,
        ROOT_MEMBER_BYTES,
    )?;

    let nested_member = root.open_file(&path("nested/value.bin"))?;
    require_bytes(
        NESTED_MEMBER,
        &nested_member.read_to_end_bounded(MAX_MEMBER_BYTES)?,
        NESTED_MEMBER_BYTES,
    )?;
    Ok(())
}

fn write_member(
    file: &mut nara_fs::FileCapability,
    bytes: &[u8],
) -> Result<(), ConformanceTreeContentError> {
    file.writer()?.write_all(bytes)?;
    file.sync()?;
    Ok(())
}

fn require_bytes(
    member: &'static str,
    actual: &[u8],
    expected: &[u8],
) -> Result<(), ConformanceTreeContentError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ConformanceTreeContentError::ContentMismatch { member })
    }
}

fn component(value: &'static str) -> RelativeComponent {
    RelativeComponent::new(value).expect("fixed conformance member names are valid")
}

fn path(value: &'static str) -> RelativePath {
    RelativePath::new(value).expect("fixed conformance member paths are valid")
}

#[cfg(all(test, target_arch = "x86_64", windows))]
mod tests {
    use std::fs::{self, File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use nara_fs::{
        CapabilityRights, DirectoryCapability, DirectoryPublicationOutcome, HostCapabilityOptions,
        TrustMode,
    };

    use super::{
        ConformanceTreeContentError, ConformanceTreeStageError, component, stage_nested_tree_with,
    };

    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn post_begin_failure_returns_the_abort_terminal_outcome() {
        let root = TestRoot::new();
        let parent = DirectoryCapability::from_host_handle(
            host_directory(root.path()),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();

        let error = stage_nested_tree_with(
            &parent,
            &component("destination"),
            &component(".staging-failure"),
            |staging| {
                let _partial = staging.create_child_file(&component("partial.bin"))?;
                Err(ConformanceTreeContentError::ContentMismatch {
                    member: "partial.bin",
                })
            },
        )
        .unwrap_err();

        let ConformanceTreeStageError::Candidate { source, terminal } = error else {
            panic!("post-begin failures must carry their terminal outcome");
        };
        assert!(matches!(
            source,
            ConformanceTreeContentError::ContentMismatch {
                member: "partial.bin"
            }
        ));
        let DirectoryPublicationOutcome::Orphaned(orphan) = *terminal else {
            panic!("a non-empty failed candidate cannot be reported as clean");
        };
        assert!(orphan.staging_handle_retained());
        assert!(orphan.staging_directory().is_some());
        assert!(!root.path().join("destination").exists());
        assert!(root.path().join(".staging-failure").is_dir());
    }

    fn host_directory(path: &Path) -> File {
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

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara_fs_module_consumer_unit_{}_{}_{}",
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
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
