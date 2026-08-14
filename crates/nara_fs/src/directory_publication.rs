use std::{
    fmt::{self, Debug, Formatter},
    sync::Arc,
};

use crate::{
    CapabilityRights, ConflictProtection, DirectoryCapability, DirectorySyncTier,
    DurabilityProgress, FileFacts, FileIdentity, FsError, FsOperation, ParentAuthorizationTier,
    PublicationAtomicity, PublicationIdentityEvidence, PublicationSourceBinding,
    PublicationTopologyEvidence, RelativeComponent, StageStatus,
    capability::validate_directory_facts, platform,
};

#[derive(Debug)]
pub enum DirectoryPublicationCause {
    Aborted,
    Filesystem(FsError),
}

impl DirectoryPublicationCause {
    #[must_use]
    pub const fn filesystem_error(&self) -> Option<&FsError> {
        match self {
            Self::Aborted => None,
            Self::Filesystem(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryCleanupEvidence {
    HandleBoundEmptyDirectoryRemoved,
}

pub struct DirectoryPublicationRejection {
    cause: DirectoryPublicationCause,
    parent: FileIdentity,
    candidate: FileIdentity,
    destination: RelativeComponent,
    cleanup: DirectoryCleanupEvidence,
}

impl DirectoryPublicationRejection {
    #[must_use]
    pub const fn cause(&self) -> &DirectoryPublicationCause {
        &self.cause
    }

    #[must_use]
    pub const fn parent_identity(&self) -> FileIdentity {
        self.parent
    }

    #[must_use]
    pub const fn candidate_identity(&self) -> FileIdentity {
        self.candidate
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }

    #[must_use]
    pub const fn cleanup_evidence(&self) -> DirectoryCleanupEvidence {
        self.cleanup
    }
}

impl Debug for DirectoryPublicationRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryPublicationRejection")
            .field("cause", &self.cause)
            .field("parent", &self.parent)
            .field("candidate", &self.candidate)
            .field("cleanup", &self.cleanup)
            .finish_non_exhaustive()
    }
}

pub struct DirectoryPublicationReceipt {
    parent: FileIdentity,
    candidate: FileIdentity,
    published: FileIdentity,
    destination: RelativeComponent,
    identity_evidence: PublicationIdentityEvidence,
    parent_authorization: ParentAuthorizationTier,
    publication: PublicationAtomicity,
    conflict: ConflictProtection,
    source_binding: PublicationSourceBinding,
    topology: PublicationTopologyEvidence,
    durability: DurabilityProgress,
}

impl DirectoryPublicationReceipt {
    #[must_use]
    pub const fn parent_identity(&self) -> FileIdentity {
        self.parent
    }

    #[must_use]
    pub const fn candidate_identity(&self) -> FileIdentity {
        self.candidate
    }

    #[must_use]
    pub const fn published_identity(&self) -> FileIdentity {
        self.published
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }

    #[must_use]
    pub const fn publication_identity_evidence(&self) -> PublicationIdentityEvidence {
        self.identity_evidence
    }

    #[must_use]
    pub const fn parent_authorization(&self) -> ParentAuthorizationTier {
        self.parent_authorization
    }

    #[must_use]
    pub const fn publication_atomicity(&self) -> PublicationAtomicity {
        self.publication
    }

    #[must_use]
    pub const fn conflict_protection(&self) -> ConflictProtection {
        self.conflict
    }

    #[must_use]
    pub const fn source_binding(&self) -> PublicationSourceBinding {
        self.source_binding
    }

    #[must_use]
    pub const fn topology_evidence(&self) -> PublicationTopologyEvidence {
        self.topology
    }

    #[must_use]
    pub const fn durability(&self) -> DurabilityProgress {
        self.durability
    }
}

impl Debug for DirectoryPublicationReceipt {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryPublicationReceipt")
            .field("parent", &self.parent)
            .field("candidate", &self.candidate)
            .field("published", &self.published)
            .field("identity_evidence", &self.identity_evidence)
            .field("parent_authorization", &self.parent_authorization)
            .field("publication", &self.publication)
            .field("conflict", &self.conflict)
            .field("source_binding", &self.source_binding)
            .field("topology", &self.topology)
            .field("durability", &self.durability)
            .finish_non_exhaustive()
    }
}

pub struct DirectoryPublicationOrphan {
    cause: DirectoryPublicationCause,
    cleanup_error: FsError,
    parent: FileIdentity,
    staging: DirectoryCapability,
    staging_name: RelativeComponent,
    destination: RelativeComponent,
}

impl DirectoryPublicationOrphan {
    #[must_use]
    pub const fn cause(&self) -> &DirectoryPublicationCause {
        &self.cause
    }

    #[must_use]
    pub const fn cleanup_error(&self) -> &FsError {
        &self.cleanup_error
    }

    #[must_use]
    pub const fn parent_identity(&self) -> FileIdentity {
        self.parent
    }

    #[must_use]
    pub fn staging_directory(&self) -> DirectoryCapability {
        self.staging.share()
    }

    #[must_use]
    pub fn staging_identity(&self) -> FileIdentity {
        self.staging.identity()
    }

    /// Returns the originally allocated sibling name.
    ///
    /// This is diagnostic evidence, not name-based authority. Under the Linux cooperative tier a
    /// concurrent writer may have moved or replaced the name before rejection.
    #[must_use]
    pub fn staging_name(&self) -> &RelativeComponent {
        &self.staging_name
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }
}

impl Debug for DirectoryPublicationOrphan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryPublicationOrphan")
            .field("cause", &self.cause)
            .field("cleanup_error", &self.cleanup_error)
            .field("parent", &self.parent)
            .field("staging_identity", &self.staging.identity())
            .finish_non_exhaustive()
    }
}

#[must_use = "the publication terminal outcome must be handled"]
#[derive(Debug)]
pub enum DirectoryPublicationOutcome {
    Published(DirectoryPublicationReceipt),
    RejectedClean(DirectoryPublicationRejection),
    Orphaned(DirectoryPublicationOrphan),
}

/// Owns one unpublished same-parent directory candidate.
///
/// The transaction is intentionally move-only. It exposes a shared child capability for bounded
/// writes while retaining publication and terminal-resolution authority. A shared staging
/// capability never gains rename, publication or removal authority.
/// Dropping an active transaction performs no namespace mutation; callers use `abort` when they
/// need a typed cleanup or orphan outcome.
///
/// ```compile_fail
/// fn duplicate(transaction: nara_fs::DirectoryPublicationTransaction) {
///     let _copy = transaction.clone();
/// }
/// ```
///
/// ```compile_fail
/// fn publish_twice(transaction: nara_fs::DirectoryPublicationTransaction) {
///     let _first = transaction.publish();
///     let _second = transaction.publish();
/// }
/// ```
#[must_use = "an unpublished staging directory remains until a later terminal operation consumes it"]
pub struct DirectoryPublicationTransaction {
    parent: DirectoryCapability,
    staging: DirectoryCapability,
    staging_name: RelativeComponent,
    destination: RelativeComponent,
    proof: platform::DirectoryPublicationProof,
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
        self.proof.source_binding()
    }

    #[must_use]
    pub fn staging_name(&self) -> &RelativeComponent {
        &self.staging_name
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }

    /// Atomically publishes the staged directory only when the destination remains absent.
    ///
    /// Callers must quiesce staging writers and validate the finite tree before publication. The
    /// receipt proves the namespace transition and recorded identity facts; it does not revoke
    /// outstanding member handles or claim that the published contents are immutable.
    ///
    /// Every failure attempts only exact handle-bound empty-directory cleanup. The terminal result
    /// never claims a clean rejection when the platform cannot prove that cleanup.
    pub fn publish(self) -> DirectoryPublicationOutcome {
        if let Err(error) = self.revalidate_for_publication() {
            return self.reject(DirectoryPublicationCause::Filesystem(error));
        }
        if let Err(error) = platform::publish_directory_no_replace(
            &self.parent.inner.file,
            &self.staging.inner.file,
            &self.staging_name,
            &self.destination,
        ) {
            return self.reject(DirectoryPublicationCause::Filesystem(error));
        }
        self.published()
    }

    /// Abandons the candidate and attempts exact empty-directory cleanup.
    ///
    /// A non-empty tree or a platform without exact object-bound cleanup returns `Orphaned`; this
    /// method never falls back to a name-based recursive deletion.
    pub fn abort(self) -> DirectoryPublicationOutcome {
        self.reject(DirectoryPublicationCause::Aborted)
    }

    fn revalidate_for_publication(&self) -> Result<(), FsError> {
        let current_proof = platform::directory_publication_proof(
            &self.parent.inner.file,
            self.parent.options().trust(),
            FsOperation::PublishDirectory,
        )?;
        if current_proof != self.proof {
            return Err(FsError::Unproven {
                operation: FsOperation::PublishDirectory,
                proof: "the live directory-publication proof changed after staging",
            });
        }

        revalidate_live_directory(&self.parent, FsOperation::PublishDirectory)?;
        revalidate_live_directory(&self.staging, FsOperation::PublishDirectory)?;
        platform::validate_directory_publication_topology(
            &self.staging.inner.file,
            self.proof,
            FsOperation::PublishDirectory,
        )?;
        if self.proof.source_binding() == PublicationSourceBinding::NameBound {
            self.revalidate_named_source()?;
        }
        if platform::child_exists(
            &self.parent.inner.file,
            &self.destination,
            FsOperation::PublishDirectory,
        )? {
            return Err(FsError::AlreadyExists {
                operation: FsOperation::PublishDirectory,
            });
        }
        Ok(())
    }

    fn revalidate_named_source(&self) -> Result<(), FsError> {
        let opened = platform::open_directory(&self.parent.inner.file, &self.staging_name, false)?;
        let facts = platform::facts(&opened)?;
        validate_directory_facts(&self.parent.inner, facts)?;
        platform::validate_directory_publication_topology(
            &opened,
            self.proof,
            FsOperation::PublishDirectory,
        )?;
        if facts.identity != self.staging.identity().native() {
            return Err(FsError::CapabilityIdentityDrift {
                operation: FsOperation::PublishDirectory,
            });
        }
        Ok(())
    }

    fn published(self) -> DirectoryPublicationOutcome {
        let identity_evidence = match self.proof.binding() {
            platform::DirectoryPublicationBinding::HandleBound => {
                PublicationIdentityEvidence::HandleBoundCandidate
            }
            platform::DirectoryPublicationBinding::NameBound => {
                PublicationIdentityEvidence::NameBoundCandidate
            }
        };
        let parent_sync = parent_sync_status(&self.parent);
        DirectoryPublicationOutcome::Published(DirectoryPublicationReceipt {
            parent: self.parent.identity(),
            candidate: self.staging.identity(),
            published: self.staging.identity(),
            destination: self.destination,
            identity_evidence,
            parent_authorization: ParentAuthorizationTier::HandleBound,
            publication: PublicationAtomicity::AtomicNameSwitch,
            conflict: ConflictProtection::StrongCompareAndSwap,
            source_binding: self.proof.source_binding(),
            topology: self.proof.topology(),
            durability: DurabilityProgress::NONE
                .with_name_published()
                .with_parent_directory(parent_sync),
        })
    }

    fn reject(self, cause: DirectoryPublicationCause) -> DirectoryPublicationOutcome {
        if Arc::strong_count(&self.staging.inner) != 1 {
            return self.orphan(
                cause,
                FsError::OutstandingDirectoryAuthority {
                    operation: FsOperation::RemoveDirectory,
                },
            );
        }
        match platform::discard_directory_exact(&self.staging.inner.file) {
            Ok(()) => {
                let parent = self.parent.identity();
                let candidate = self.staging.identity();
                let destination = self.destination;
                drop(self.staging);
                DirectoryPublicationOutcome::RejectedClean(DirectoryPublicationRejection {
                    cause,
                    parent,
                    candidate,
                    destination,
                    cleanup: DirectoryCleanupEvidence::HandleBoundEmptyDirectoryRemoved,
                })
            }
            Err(cleanup_error) => self.orphan(cause, cleanup_error),
        }
    }

    fn orphan(
        self,
        cause: DirectoryPublicationCause,
        cleanup_error: FsError,
    ) -> DirectoryPublicationOutcome {
        DirectoryPublicationOutcome::Orphaned(DirectoryPublicationOrphan {
            cause,
            cleanup_error,
            parent: self.parent.identity(),
            staging: self.staging,
            staging_name: self.staging_name,
            destination: self.destination,
        })
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

        let proof = platform::directory_publication_proof(
            &self.inner.file,
            self.options().trust(),
            FsOperation::BeginDirectoryPublication,
        )?;
        if platform::child_exists(
            &self.inner.file,
            destination,
            FsOperation::BeginDirectoryPublication,
        )? {
            return Err(FsError::TargetStateMismatch);
        }

        let staging = self.create_child_directory(staging_name)?;
        platform::validate_directory_publication_topology(
            &staging.inner.file,
            proof,
            FsOperation::BeginDirectoryPublication,
        )
        .map_err(|error| {
            FsError::exclusive_child_unverified(FsOperation::CreateDirectory, error)
        })?;
        Ok(DirectoryPublicationTransaction {
            parent: self.share(),
            staging,
            staging_name: staging_name.clone(),
            destination: destination.clone(),
            proof,
        })
    }
}

fn revalidate_live_directory(
    capability: &DirectoryCapability,
    operation: FsOperation,
) -> Result<FileFacts, FsError> {
    let facts = platform::facts(&capability.inner.file)?;
    validate_directory_facts(&capability.inner, facts)?;
    if facts.identity != capability.identity().native() {
        return Err(FsError::CapabilityIdentityDrift { operation });
    }
    Ok(facts)
}

fn parent_sync_status(parent: &DirectoryCapability) -> StageStatus {
    match platform::sync_directory(&parent.inner.file) {
        Ok(DirectorySyncTier::Naming) => StageStatus::Achieved,
        Ok(DirectorySyncTier::Unsupported) | Err(FsError::Unsupported { .. }) => {
            StageStatus::Unsupported
        }
        Ok(DirectorySyncTier::Unproven) | Err(_) => StageStatus::Unknown,
    }
}

impl Debug for DirectoryPublicationTransaction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryPublicationTransaction")
            .field("parent_identity", &self.parent.identity())
            .field("staging_identity", &self.staging.identity())
            .field("source_binding", &self.proof.source_binding())
            .field("topology", &self.proof.topology())
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
                platform::directory_publication_proof(
                    &parent.inner.file,
                    TrustMode::Untrusted,
                    FsOperation::BeginDirectoryPublication,
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

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn publish_preserves_a_finite_nested_tree_and_reports_exact_evidence() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-success"),
                )
                .unwrap()
        });
        let candidate_identity = transaction.staging_identity();
        let staging = transaction.staging_directory();
        let assets = staging
            .create_child_directory(&component("assets"))
            .unwrap();
        let mut member = assets
            .create_child_file(&component("manifest.bin"))
            .unwrap();
        member.writer().unwrap().write_all(b"candidate").unwrap();
        member.sync().unwrap();
        drop(member);
        drop(assets);
        drop(staging);

        let outcome = with_support(supported_test_tier(), || transaction.publish());
        let DirectoryPublicationOutcome::Published(receipt) = outcome else {
            panic!("expected published directory, got {outcome:?}");
        };

        assert_eq!(receipt.parent_identity(), parent.identity());
        assert_eq!(receipt.candidate_identity(), candidate_identity);
        assert_eq!(receipt.published_identity(), candidate_identity);
        assert_eq!(receipt.destination(), &component("published"));
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
        assert_eq!(receipt.source_binding(), expected_source_binding());
        assert_eq!(receipt.topology_evidence(), expected_topology_evidence());
        assert_eq!(receipt.durability().name_published(), StageStatus::Achieved);
        if cfg!(windows) {
            assert_eq!(
                receipt.publication_identity_evidence(),
                PublicationIdentityEvidence::HandleBoundCandidate
            );
            assert_eq!(
                receipt.durability().parent_directory_synced(),
                StageStatus::Unsupported
            );
        } else {
            assert_eq!(
                receipt.publication_identity_evidence(),
                PublicationIdentityEvidence::NameBoundCandidate
            );
        }
        assert!(!root.path().join(".published-staging-success").exists());
        assert_eq!(
            std::fs::read(root.path().join("published/assets/manifest.bin")).unwrap(),
            b"candidate"
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn native_no_replace_never_changes_an_existing_destination() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-native-collision"),
                )
                .unwrap()
        });
        std::fs::create_dir(root.path().join("published")).unwrap();
        std::fs::write(root.path().join("published/marker.bin"), b"existing").unwrap();

        let error = platform::publish_directory_no_replace(
            &transaction.parent.inner.file,
            &transaction.staging.inner.file,
            transaction.staging_name(),
            transaction.destination(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            FsError::AlreadyExists {
                operation: FsOperation::PublishDirectory
            }
        ));
        assert_eq!(
            std::fs::read(root.path().join("published/marker.bin")).unwrap(),
            b"existing"
        );
        assert!(
            root.path()
                .join(".published-staging-native-collision")
                .exists()
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn transaction_collision_is_terminal_without_changing_the_existing_destination() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-collision"),
                )
                .unwrap()
        });
        std::fs::create_dir(root.path().join("published")).unwrap();
        std::fs::write(root.path().join("published/marker.bin"), b"existing").unwrap();

        let outcome = with_support(supported_test_tier(), || transaction.publish());
        assert_terminal_filesystem_error(&outcome, |error| {
            matches!(
                error,
                FsError::AlreadyExists {
                    operation: FsOperation::PublishDirectory
                }
            )
        });
        assert_eq!(
            std::fs::read(root.path().join("published/marker.bin")).unwrap(),
            b"existing"
        );
        assert_eq!(
            root.path().join(".published-staging-collision").exists(),
            !cfg!(windows)
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn abort_reports_clean_only_when_exact_empty_cleanup_succeeds() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-abort"),
                )
                .unwrap()
        });

        let outcome = transaction.abort();
        assert!(matches!(
            terminal_cause(&outcome),
            DirectoryPublicationCause::Aborted
        ));
        if cfg!(windows) {
            let DirectoryPublicationOutcome::RejectedClean(rejection) = &outcome else {
                panic!("Windows empty-directory cleanup should be exact");
            };
            assert_eq!(rejection.parent_identity(), parent.identity());
            assert_eq!(
                rejection.cleanup_evidence(),
                DirectoryCleanupEvidence::HandleBoundEmptyDirectoryRemoved
            );
            assert_eq!(rejection.destination(), &component("published"));
        }
        assert_eq!(
            matches!(outcome, DirectoryPublicationOutcome::RejectedClean(_)),
            cfg!(windows)
        );
        assert_eq!(
            root.path().join(".published-staging-abort").exists(),
            !cfg!(windows)
        );
    }

    #[cfg(windows)]
    #[test]
    fn outstanding_staging_authority_prevents_a_false_clean_rejection() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-shared"),
                )
                .unwrap()
        });
        let outstanding = transaction.staging_directory();

        let DirectoryPublicationOutcome::Orphaned(orphan) = transaction.abort() else {
            panic!("shared staging authority must prevent a clean terminal claim");
        };

        assert!(matches!(
            orphan.cleanup_error(),
            FsError::OutstandingDirectoryAuthority {
                operation: FsOperation::RemoveDirectory
            }
        ));
        assert!(root.path().join(".published-staging-shared").exists());
        drop(outstanding);
        drop(orphan);
        assert!(root.path().join(".published-staging-shared").exists());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn nonempty_abort_retains_a_capability_to_the_recoverable_orphan() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-orphan"),
                )
                .unwrap()
        });
        let staging = transaction.staging_directory();
        let mut member = staging.create_child_file(&component("member.bin")).unwrap();
        member.writer().unwrap().write_all(b"recoverable").unwrap();
        member.sync().unwrap();
        drop(member);
        drop(staging);

        let DirectoryPublicationOutcome::Orphaned(orphan) = transaction.abort() else {
            panic!("non-empty directory cleanup must remain a recoverable orphan");
        };

        assert!(matches!(orphan.cause(), DirectoryPublicationCause::Aborted));
        assert!(!root.path().join("published").exists());
        assert!(root.path().join(".published-staging-orphan").exists());
        assert_eq!(
            orphan
                .staging_directory()
                .open_child_file(&component("member.bin"))
                .unwrap()
                .read_to_end_bounded(64)
                .unwrap(),
            b"recoverable"
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn changed_platform_support_rejects_before_publication() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-support-drift"),
                )
                .unwrap()
        });

        let outcome = with_support(DirectoryPublicationSupport::Unsupported, || {
            transaction.publish()
        });

        assert_terminal_filesystem_error(&outcome, |error| {
            matches!(
                error,
                FsError::Unsupported {
                    operation: FsOperation::PublishDirectory,
                    ..
                }
            )
        });
        assert!(!root.path().join("published").exists());
        assert_eq!(
            root.path()
                .join(".published-staging-support-drift")
                .exists(),
            !cfg!(windows)
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn changed_volume_or_mount_proof_rejects_before_publication() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-topology-drift"),
                )
                .unwrap()
        });

        let outcome = with_support(supported_test_tier(), || {
            platform::with_test_directory_publication_platform_identity_override(u64::MAX, || {
                transaction.publish()
            })
        });

        assert_terminal_filesystem_error(&outcome, |error| {
            matches!(
                error,
                FsError::Unproven {
                    operation: FsOperation::PublishDirectory,
                    ..
                }
            )
        });
        assert!(!root.path().join("published").exists());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn parent_identity_drift_rejects_before_publication() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-parent-drift"),
                )
                .unwrap()
        });
        let drifted_parent = FileFacts {
            identity: different_object(parent.identity().native()),
            kind: FileKind::Directory,
            link_count: 1,
            reparse_tag: None,
            identity_proven: true,
        };

        let outcome = with_support(supported_test_tier(), || {
            platform::with_test_facts_override(drifted_parent, || transaction.publish())
        });

        assert_terminal_filesystem_error(&outcome, |error| {
            matches!(
                error,
                FsError::CapabilityIdentityDrift {
                    operation: FsOperation::PublishDirectory
                }
            )
        });
        assert!(!root.path().join("published").exists());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn parent_volume_drift_rejects_before_publication() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-volume-drift"),
                )
                .unwrap()
        });
        let drifted_parent = FileFacts {
            identity: different_volume(parent.identity().native()),
            kind: FileKind::Directory,
            link_count: 1,
            reparse_tag: None,
            identity_proven: true,
        };

        let outcome = with_support(supported_test_tier(), || {
            platform::with_test_facts_override(drifted_parent, || transaction.publish())
        });

        assert_terminal_filesystem_error(&outcome, |error| matches!(error, FsError::CrossVolume));
        assert!(!root.path().join("published").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn name_bound_source_swap_rejects_the_replacement_object() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-source-drift"),
                )
                .unwrap()
        });
        std::fs::rename(
            root.path().join(".published-staging-source-drift"),
            root.path().join(".moved-staging"),
        )
        .unwrap();
        std::fs::create_dir(root.path().join(".published-staging-source-drift")).unwrap();

        let outcome = with_support(supported_test_tier(), || transaction.publish());

        assert_terminal_filesystem_error(&outcome, |error| {
            matches!(
                error,
                FsError::CapabilityIdentityDrift {
                    operation: FsOperation::PublishDirectory
                }
            )
        });
        assert!(!root.path().join("published").exists());
        assert!(root.path().join(".moved-staging").exists());
        assert!(root.path().join(".published-staging-source-drift").exists());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn dropping_an_active_transaction_performs_no_namespace_cleanup() {
        let root = TestRoot::new();
        let parent = read_write_parent(&root);
        let transaction = with_support(supported_test_tier(), || {
            parent
                .begin_directory_publication(
                    &component("published"),
                    &component(".published-staging-dropped"),
                )
                .unwrap()
        });

        drop(transaction);

        assert!(!root.path().join("published").exists());
        assert!(root.path().join(".published-staging-dropped").exists());
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

    #[cfg(windows)]
    fn different_object(identity: NativeFileIdentity) -> NativeFileIdentity {
        let NativeFileIdentity::Windows {
            volume_serial,
            mut file_id,
        } = identity;
        file_id[0] = file_id[0].wrapping_add(1);
        NativeFileIdentity::Windows {
            volume_serial,
            file_id,
        }
    }

    #[cfg(unix)]
    fn different_object(identity: NativeFileIdentity) -> NativeFileIdentity {
        let NativeFileIdentity::Unix { device, inode } = identity;
        NativeFileIdentity::Unix {
            device,
            inode: inode.wrapping_add(1),
        }
    }

    fn terminal_cause(outcome: &DirectoryPublicationOutcome) -> &DirectoryPublicationCause {
        match outcome {
            DirectoryPublicationOutcome::RejectedClean(rejection) => rejection.cause(),
            DirectoryPublicationOutcome::Orphaned(orphan) => orphan.cause(),
            DirectoryPublicationOutcome::Published(receipt) => {
                panic!("expected rejection, got publication receipt {receipt:?}")
            }
        }
    }

    fn assert_terminal_filesystem_error(
        outcome: &DirectoryPublicationOutcome,
        predicate: impl FnOnce(&FsError) -> bool,
    ) {
        let DirectoryPublicationCause::Filesystem(error) = terminal_cause(outcome) else {
            panic!("expected a filesystem rejection, got {outcome:?}");
        };
        assert!(predicate(error), "unexpected filesystem rejection: {error}");
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

    fn expected_topology_evidence() -> PublicationTopologyEvidence {
        if cfg!(windows) {
            PublicationTopologyEvidence::SameVolume
        } else {
            PublicationTopologyEvidence::SameMount
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
