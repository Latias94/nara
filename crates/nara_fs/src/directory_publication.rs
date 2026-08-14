use std::fmt::{self, Debug, Formatter};

use crate::{
    CapabilityRights, DirectoryCapability, FileIdentity, FsError, FsOperation,
    PublicationSourceBinding, RelativeComponent, platform,
};

/// Owns one unpublished same-parent directory candidate.
///
/// The transaction is intentionally move-only. It exposes a shared child capability for bounded
/// writes while retaining publication and terminal-resolution authority. A shared staging
/// capability never gains rename, publication or removal authority.
///
/// ```compile_fail
/// fn duplicate(transaction: nara_fs::DirectoryPublicationTransaction) {
///     let _copy = transaction.clone();
/// }
/// ```
#[must_use = "an unpublished staging directory remains until a later terminal operation consumes it"]
pub struct DirectoryPublicationTransaction {
    parent: DirectoryCapability,
    staging: DirectoryCapability,
    staging_name: RelativeComponent,
    destination: RelativeComponent,
    source_binding: PublicationSourceBinding,
}

impl DirectoryPublicationTransaction {
    /// Returns a shared capability to the unpublished staging directory.
    ///
    /// The capability keeps the exact parent session and generation while authorizing only the
    /// created child object. It never exposes a native path or publication authority.
    #[must_use]
    pub fn staging_directory(&self) -> DirectoryCapability {
        self.staging.share()
    }

    #[must_use]
    pub fn parent_identity(&self) -> FileIdentity {
        self.parent.identity()
    }

    #[must_use]
    pub fn staging_identity(&self) -> FileIdentity {
        self.staging.identity()
    }

    #[must_use]
    pub const fn source_binding(&self) -> PublicationSourceBinding {
        self.source_binding
    }

    #[must_use]
    pub fn staging_name(&self) -> &RelativeComponent {
        &self.staging_name
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }
}

impl DirectoryCapability {
    /// Creates one exclusive sibling staging directory for a future no-replace publication.
    ///
    /// `destination` must be absent at this observed parent generation, and `staging_name` must be
    /// caller-supplied bounded entropy distinct from the destination. The future publication step
    /// revalidates destination absence; this observation is not a compare-and-swap claim.
    pub fn begin_directory_publication(
        &self,
        destination: &RelativeComponent,
        staging_name: &RelativeComponent,
    ) -> Result<DirectoryPublicationTransaction, FsError> {
        if self.options().rights() == CapabilityRights::ReadOnly {
            return Err(FsError::ReadOnlyCapability {
                operation: FsOperation::BeginDirectoryPublication,
            });
        }
        if destination == staging_name {
            return Err(FsError::PublicationNamesConflict);
        }

        let source_binding = platform::directory_publication_source_binding(
            &self.inner.file,
            self.options().trust(),
        )?;
        if platform::child_exists(&self.inner.file, destination)? {
            return Err(FsError::TargetStateMismatch);
        }

        let staging = self.create_child_directory(staging_name)?;
        Ok(DirectoryPublicationTransaction {
            parent: self.share(),
            staging,
            staging_name: staging_name.clone(),
            destination: destination.clone(),
            source_binding,
        })
    }
}

impl Debug for DirectoryPublicationTransaction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryPublicationTransaction")
            .field("parent_identity", &self.parent.identity())
            .field("staging_identity", &self.staging.identity())
            .field("source_binding", &self.source_binding)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::Write,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::{
        FileFacts, FileKind, HostCapabilityOptions, NativeFileIdentity, TrustMode,
        platform::DirectoryPublicationSupport,
    };

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "nara_fs_directory_publication_{}_{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn begin_retains_capability_lineage_and_bounded_member_authority() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-7f4a"),
                )
                .unwrap()
        });

        assert_eq!(
            transaction.parent_identity().session(),
            transaction.staging_identity().session()
        );
        assert_eq!(
            transaction.parent_identity().generation(),
            transaction.staging_identity().generation()
        );
        assert_ne!(
            transaction.parent_identity(),
            transaction.staging_identity()
        );
        assert_eq!(transaction.source_binding(), expected_source_binding());

        let staging = transaction.staging_directory();
        let assets = staging
            .create_child_directory(&component("assets"))
            .unwrap();
        let mut member = assets
            .create_child_file(&component("manifest.bin"))
            .unwrap();
        member.writer().unwrap().write_all(b"candidate").unwrap();
        member.sync().unwrap();

        let bytes = assets
            .open_child_file(&component("manifest.bin"))
            .unwrap()
            .read_to_end_bounded(64)
            .unwrap();
        assert_eq!(bytes, b"candidate");
        assert!(!format!("{transaction:?}").contains(&root.path().display().to_string()));
    }

    #[test]
    fn begin_rejects_before_creation_without_write_authority() {
        let root = TestRoot::new();
        let parent = DirectoryCapability::from_host_handle(
            host_directory(root.path()),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap();

        assert!(matches!(
            parent.begin_directory_publication(
                &component("published"),
                &component(".published-staging")
            ),
            Err(FsError::ReadOnlyCapability {
                operation: FsOperation::BeginDirectoryPublication
            })
        ));
        assert!(!root.path().join(".published-staging").exists());
    }

    #[test]
    fn begin_rejects_equal_names_and_existing_destinations_before_staging() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);

        assert!(matches!(
            parent.begin_directory_publication(&component("same"), &component("same")),
            Err(FsError::PublicationNamesConflict)
        ));
        std::fs::write(root.path().join("published"), b"existing").unwrap();
        let error = with_support(supported_test_tier(), || {
            parent.begin_directory_publication(
                &component("published"),
                &component(".published-staging"),
            )
        })
        .unwrap_err();

        assert!(matches!(error, FsError::TargetStateMismatch));
        assert!(!root.path().join(".published-staging").exists());
    }

    #[test]
    fn unsupported_parent_rejects_before_staging() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let error = with_support(DirectoryPublicationSupport::Unsupported, || {
            parent.begin_directory_publication(
                &component("published"),
                &component(".published-staging"),
            )
        })
        .unwrap_err();

        assert!(matches!(error, FsError::Unsupported { .. }));
        assert!(!root.path().join(".published-staging").exists());
    }

    #[test]
    fn unproven_parent_rejects_before_staging() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let error = with_support(DirectoryPublicationSupport::Unproven, || {
            parent.begin_directory_publication(
                &component("published"),
                &component(".published-staging"),
            )
        })
        .unwrap_err();

        assert!(matches!(error, FsError::Unproven { .. }));
        assert!(!root.path().join(".published-staging").exists());
    }

    #[test]
    fn name_bound_proof_requires_the_cooperative_trust_tier() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let error = with_support(
            DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound),
            || {
                platform::directory_publication_source_binding(
                    &parent.inner.file,
                    TrustMode::Untrusted,
                )
            },
        )
        .unwrap_err();

        assert!(matches!(error, FsError::Unproven { .. }));
    }

    #[test]
    fn duplicate_begin_reports_the_exclusive_staging_collision() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let first = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published-a"),
                    &component(".shared-staging"),
                )
                .unwrap()
        });
        let error = with_support(supported_test_tier(), || {
            parent.begin_directory_publication(
                &component("published-b"),
                &component(".shared-staging"),
            )
        })
        .unwrap_err();

        assert!(matches!(
            error,
            FsError::AlreadyExists {
                operation: FsOperation::CreateDirectory
            }
        ));
        drop(first);
    }

    #[test]
    fn invalid_created_directory_facts_report_a_recoverable_orphan() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        for facts in [
            invalid_facts(&parent, FileKind::Regular, None, false),
            invalid_facts(&parent, FileKind::Directory, Some(0xa000_000c), false),
            invalid_facts(&parent, FileKind::Directory, None, true),
        ] {
            let staging_name = component(&format!(
                ".invalid-{}",
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let error = with_support(supported_test_tier(), || {
                platform::with_test_facts_override(facts, || {
                    parent.begin_directory_publication(&component("published"), &staging_name)
                })
            })
            .unwrap_err();

            assert!(matches!(
                error,
                FsError::ExclusiveChildUnverified {
                    operation: FsOperation::CreateDirectory,
                    ..
                }
            ));
            assert!(
                root.path()
                    .join(staging_component_for_test(&staging_name))
                    .exists()
            );
        }
    }

    fn invalid_facts(
        parent: &DirectoryCapability,
        kind: FileKind,
        reparse_tag: Option<u32>,
        cross_volume: bool,
    ) -> FileFacts {
        FileFacts {
            identity: if cross_volume {
                different_volume(parent.inner.root_native)
            } else {
                parent.inner.root_native
            },
            kind,
            link_count: 1,
            reparse_tag,
            identity_proven: true,
        }
    }

    #[cfg(windows)]
    fn different_volume(identity: NativeFileIdentity) -> NativeFileIdentity {
        let NativeFileIdentity::Windows {
            volume_serial,
            file_id,
        } = identity;
        NativeFileIdentity::Windows {
            volume_serial: volume_serial.wrapping_add(1),
            file_id,
        }
    }

    #[cfg(unix)]
    fn different_volume(identity: NativeFileIdentity) -> NativeFileIdentity {
        let NativeFileIdentity::Unix { device, inode } = identity;
        NativeFileIdentity::Unix {
            device: device.wrapping_add(1),
            inode,
        }
    }

    fn with_support<T>(support: DirectoryPublicationSupport, operation: impl FnOnce() -> T) -> T {
        platform::with_test_directory_publication_support_override(support, operation)
    }

    fn supported_test_tier() -> DirectoryPublicationSupport {
        if cfg!(windows) {
            DirectoryPublicationSupport::Proven(PublicationSourceBinding::HandleBound)
        } else {
            DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound)
        }
    }

    fn expected_source_binding() -> PublicationSourceBinding {
        if cfg!(windows) {
            PublicationSourceBinding::HandleBound
        } else {
            PublicationSourceBinding::NameBound
        }
    }

    fn read_write_parent(root: &TestRoot) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(root.path()),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal)
                .with_generation(11),
        )
        .unwrap()
    }

    fn component(value: &str) -> RelativeComponent {
        RelativeComponent::new(value).unwrap()
    }

    fn staging_component_for_test(component: &RelativeComponent) -> &std::ffi::OsStr {
        component.as_os_str()
    }

    #[cfg(windows)]
    fn host_directory(path: &Path) -> File {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    fn host_directory(path: &Path) -> File {
        File::open(path).unwrap()
    }
}
