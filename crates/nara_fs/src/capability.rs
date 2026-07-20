use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    CapabilityGeneration, CapabilitySessionId, DirectorySyncReceipt, DirectorySyncTier,
    DurabilityProgress, ExpectedTarget, FileCapability, FileFacts, FileIdentity, FileKind, FsError,
    FsOperation, NativeFileIdentity, RelativeComponent, RelativePath, ResolutionTier, StageStatus,
    TemporaryFile, platform,
};

static NEXT_CAPABILITY_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityRights {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustMode {
    TrustedLocal,
    Untrusted,
    Recovery,
}

impl TrustMode {
    pub(crate) const fn is_strict(self) -> bool {
        matches!(self, Self::Untrusted | Self::Recovery)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostCapabilityOptions {
    rights: CapabilityRights,
    trust: TrustMode,
    generation: CapabilityGeneration,
}

impl HostCapabilityOptions {
    #[must_use]
    pub const fn new(rights: CapabilityRights, trust: TrustMode) -> Self {
        Self {
            rights,
            trust,
            generation: CapabilityGeneration(0),
        }
    }

    #[must_use]
    pub const fn with_generation(mut self, generation: u64) -> Self {
        self.generation = CapabilityGeneration(generation);
        self
    }

    #[must_use]
    pub const fn rights(self) -> CapabilityRights {
        self.rights
    }

    #[must_use]
    pub const fn trust(self) -> TrustMode {
        self.trust
    }

    #[must_use]
    pub const fn generation(self) -> CapabilityGeneration {
        self.generation
    }
}

pub(crate) struct DirectoryInner {
    pub file: File,
    pub identity: FileIdentity,
    pub root_native: NativeFileIdentity,
    pub options: HostCapabilityOptions,
    pub resolution: ResolutionTier,
}

pub struct DirectoryCapability {
    pub(crate) inner: Arc<DirectoryInner>,
}

impl DirectoryCapability {
    /// Imports a directory handle issued by trusted host code.
    ///
    /// This constructor grants authority only to the object already represented
    /// by `file`; it never accepts or recovers an ambient path.
    pub fn from_host_handle(file: File, options: HostCapabilityOptions) -> Result<Self, FsError> {
        let facts = platform::facts(&file)?;
        if facts.kind != FileKind::Directory {
            return Err(FsError::NotDirectory);
        }
        reject_reparse(facts)?;
        if options.trust.is_strict() && !facts.identity_proven {
            return Err(FsError::Unproven {
                operation: FsOperation::InspectHandle,
                proof: "host filesystem does not provide the required local live-object identity",
            });
        }
        let resolution = platform::resolution_tier(&file);
        if options.trust.is_strict() {
            require_strict_resolution(resolution, FsOperation::OpenDirectory)?;
        }

        let session = next_session()?;
        let identity = FileIdentity::new(session, options.generation, facts.identity);
        Ok(Self {
            inner: Arc::new(DirectoryInner {
                file,
                identity,
                root_native: facts.identity,
                options,
                resolution,
            }),
        })
    }

    #[must_use]
    pub fn identity(&self) -> FileIdentity {
        self.inner.identity
    }

    /// Shares the same Host-issued directory authority without creating a new session or
    /// re-resolving an ambient path.
    #[must_use]
    pub fn share(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    #[must_use]
    pub fn options(&self) -> HostCapabilityOptions {
        self.inner.options
    }

    /// Returns the resolution evidence proven for this host-issued capability.
    #[must_use]
    pub fn resolution_tier(&self) -> ResolutionTier {
        self.inner.resolution
    }

    pub fn open_directory(&self, path: &RelativePath) -> Result<Self, FsError> {
        self.ensure_strict_resolution(FsOperation::OpenDirectory)?;
        let mut current = Arc::clone(&self.inner);
        for component in path.components() {
            let file = platform::open_directory(
                &current.file,
                component,
                self.inner.options.trust.is_strict(),
            )?;
            let facts = platform::facts(&file)?;
            validate_directory_facts(&self.inner, facts)?;
            let identity = FileIdentity::new(
                self.inner.identity.session(),
                self.inner.options.generation,
                facts.identity,
            );
            current = Arc::new(DirectoryInner {
                file,
                identity,
                root_native: self.inner.root_native,
                options: self.inner.options,
                resolution: self.inner.resolution,
            });
        }
        Ok(Self { inner: current })
    }

    pub fn open_file(&self, path: &RelativePath) -> Result<FileCapability, FsError> {
        self.open_file_with_observer(path, |_, _| {})
    }

    /// Resolves the authorized parent directory and leaf of a file target.
    pub fn resolve_file_parent(
        &self,
        path: &RelativePath,
    ) -> Result<(Self, RelativeComponent), FsError> {
        self.ensure_strict_resolution(FsOperation::OpenDirectory)?;
        let Some((leaf, parents)) = path.components().split_last() else {
            return Err(crate::PathValidationError::Empty.into());
        };
        let mut current = Arc::clone(&self.inner);
        for component in parents {
            let file = platform::open_directory(
                &current.file,
                component,
                self.inner.options.trust.is_strict(),
            )?;
            let facts = platform::facts(&file)?;
            validate_directory_facts(&self.inner, facts)?;
            current = Arc::new(DirectoryInner {
                file,
                identity: FileIdentity::new(
                    self.inner.identity.session(),
                    self.inner.options.generation,
                    facts.identity,
                ),
                root_native: self.inner.root_native,
                options: self.inner.options,
                resolution: self.inner.resolution,
            });
        }
        Ok((Self { inner: current }, leaf.clone()))
    }

    /// Opens one direct child file through this directory authority.
    pub fn open_child_file(&self, child: &RelativeComponent) -> Result<FileCapability, FsError> {
        self.ensure_strict_resolution(FsOperation::OpenFile)?;
        let file = platform::open_file(
            &self.inner.file,
            child,
            self.inner.options.trust.is_strict(),
        )?;
        let facts = platform::facts(&file)?;
        validate_file_facts(&self.inner, facts)?;
        Ok(FileCapability::from_opened(
            file,
            FileIdentity::new(
                self.inner.identity.session(),
                self.inner.options.generation,
                facts.identity,
            ),
            Some(self.inner.identity),
            facts.link_count,
            self.inner.options.trust,
            self.inner.resolution,
        ))
    }

    fn open_file_with_observer(
        &self,
        path: &RelativePath,
        mut after_parent_open: impl FnMut(usize, &RelativeComponent),
    ) -> Result<FileCapability, FsError> {
        self.ensure_strict_resolution(FsOperation::OpenFile)?;
        let Some((leaf, parents)) = path.components().split_last() else {
            return Err(crate::PathValidationError::Empty.into());
        };
        let mut parent = Arc::clone(&self.inner);
        for (index, component) in parents.iter().enumerate() {
            let file = platform::open_directory(
                &parent.file,
                component,
                self.inner.options.trust.is_strict(),
            )?;
            let facts = platform::facts(&file)?;
            validate_directory_facts(&self.inner, facts)?;
            parent = Arc::new(DirectoryInner {
                file,
                identity: FileIdentity::new(
                    self.inner.identity.session(),
                    self.inner.options.generation,
                    facts.identity,
                ),
                root_native: self.inner.root_native,
                options: self.inner.options,
                resolution: self.inner.resolution,
            });
            after_parent_open(index, component);
        }

        let file = platform::open_file(&parent.file, leaf, self.inner.options.trust.is_strict())?;
        let facts = platform::facts(&file)?;
        validate_file_facts(&self.inner, facts)?;
        Ok(FileCapability::from_opened(
            file,
            FileIdentity::new(
                self.inner.identity.session(),
                self.inner.options.generation,
                facts.identity,
            ),
            Some(parent.identity),
            facts.link_count,
            self.inner.options.trust,
            self.inner.resolution,
        ))
    }

    pub fn create_temp(&self, name: &RelativeComponent) -> Result<TemporaryFile, FsError> {
        self.ensure_write(FsOperation::CreateTemporary)?;
        let file = platform::create_file_exclusive(&self.inner.file, name)?;
        let facts = platform::facts(&file)?;
        validate_file_facts(&self.inner, facts)?;
        let file = FileCapability::from_opened(
            file,
            FileIdentity::new(
                self.inner.identity.session(),
                self.inner.options.generation,
                facts.identity,
            ),
            Some(self.inner.identity),
            facts.link_count,
            self.inner.options.trust,
            self.inner.resolution,
        );
        Ok(TemporaryFile::new(
            Arc::clone(&self.inner),
            name.clone(),
            file,
        ))
    }

    pub fn replace_temp(
        &self,
        mut temporary: TemporaryFile,
        target: &RelativeComponent,
        expected: ExpectedTarget,
    ) -> Result<crate::ReplaceReceipt, FsError> {
        self.ensure_write(FsOperation::Replace)?;
        if !Arc::ptr_eq(&self.inner, temporary.parent()) {
            return Err(FsError::TemporaryParentMismatch);
        }
        let matrix = platform::capability_matrix();
        if self.inner.options.trust.is_strict()
            && matrix.replace_source_binding() != crate::ReplaceSourceBinding::HandleBound
        {
            return Err(FsError::Unproven {
                operation: FsOperation::Replace,
                proof: "strict replacement cannot bind the candidate handle to the source name",
            });
        }

        let previous = self.inspect_optional_file(target)?;
        match expected {
            ExpectedTarget::Any => {}
            ExpectedTarget::Missing if previous.is_some() => {
                return Err(FsError::TargetStateMismatch);
            }
            ExpectedTarget::Identity(expected) if previous != Some(expected) => {
                return Err(FsError::TargetStateMismatch);
            }
            ExpectedTarget::Missing | ExpectedTarget::Identity(_) => {}
        }

        platform::replace_temporary(
            &self.inner.file,
            temporary.file_handle(),
            temporary.name(),
            target,
        )?;
        temporary.mark_published();
        let (observed_published, identity_evidence) = match matrix.replace_source_binding() {
            crate::ReplaceSourceBinding::HandleBound => (
                Some(temporary.identity()),
                crate::PublicationIdentityEvidence::HandleBoundCandidate,
            ),
            crate::ReplaceSourceBinding::NameBound => {
                let observed = self.inspect_optional_file(target).ok().flatten();
                let evidence = if observed.is_some() {
                    crate::PublicationIdentityEvidence::PostPublishObserved
                } else {
                    crate::PublicationIdentityEvidence::Unverified
                };
                (observed, evidence)
            }
            crate::ReplaceSourceBinding::Unsupported => {
                (None, crate::PublicationIdentityEvidence::Unverified)
            }
        };
        let parent_sync = match matrix.directory_sync_tier() {
            DirectorySyncTier::Unsupported => StageStatus::Unsupported,
            DirectorySyncTier::Naming | DirectorySyncTier::Unproven => StageStatus::Unknown,
        };
        Ok(crate::ReplaceReceipt::new(
            previous,
            temporary.identity(),
            observed_published,
            identity_evidence,
            matrix.replace_parent_authorization(),
            matrix.publication_atomicity(),
            matrix.conflict_protection(),
            matrix.replace_source_binding(),
            temporary
                .durability()
                .with_name_published()
                .with_parent_directory(parent_sync),
        ))
    }

    pub fn sync(&self) -> Result<DirectorySyncReceipt, FsError> {
        let tier = platform::sync_directory(&self.inner.file)?;
        let parent = match tier {
            DirectorySyncTier::Naming => StageStatus::Achieved,
            DirectorySyncTier::Unsupported => StageStatus::Unsupported,
            DirectorySyncTier::Unproven => StageStatus::Unknown,
        };
        Ok(DirectorySyncReceipt::new(
            tier,
            DurabilityProgress::NONE.with_parent_directory(parent),
        ))
    }

    pub fn read_directory(
        &self,
        _path: &RelativePath,
    ) -> Result<Vec<DirectoryEntryObservation>, FsError> {
        Err(FsError::Unsupported {
            operation: FsOperation::ReadDirectory,
            capability: "handle-bound directory enumeration is deferred to U11",
        })
    }

    pub fn unlink_file(
        &self,
        _path: &RelativePath,
        _expected: FileIdentity,
    ) -> Result<(), FsError> {
        Err(FsError::Unsupported {
            operation: FsOperation::Unlink,
            capability: "relative identity-guarded unlink is deferred to U29/U30",
        })
    }

    pub fn rename_file_no_replace(
        &self,
        _source: &RelativeComponent,
        _target: &RelativeComponent,
    ) -> Result<(), FsError> {
        Err(FsError::Unsupported {
            operation: FsOperation::Rename,
            capability: "same-capability non-overwrite rename is deferred to U26",
        })
    }

    pub(crate) fn inspect_optional_file(
        &self,
        component: &RelativeComponent,
    ) -> Result<Option<FileIdentity>, FsError> {
        let file = match platform::open_file(
            &self.inner.file,
            component,
            self.inner.options.trust.is_strict(),
        ) {
            Ok(file) => file,
            Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let facts = platform::facts(&file)?;
        validate_file_facts(&self.inner, facts)?;
        Ok(Some(FileIdentity::new(
            self.inner.identity.session(),
            self.inner.options.generation,
            facts.identity,
        )))
    }

    fn ensure_write(&self, operation: FsOperation) -> Result<(), FsError> {
        if self.inner.options.rights == CapabilityRights::ReadOnly {
            Err(FsError::ReadOnlyCapability { operation })
        } else {
            Ok(())
        }
    }

    fn ensure_strict_resolution(&self, operation: FsOperation) -> Result<(), FsError> {
        if !self.inner.options.trust.is_strict() {
            return Ok(());
        }
        require_strict_resolution(self.inner.resolution, operation)
    }
}

impl Debug for DirectoryCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryCapability")
            .field("identity", &self.inner.identity)
            .field("rights", &self.inner.options.rights)
            .field("trust", &self.inner.options.trust)
            .field("resolution", &self.inner.resolution)
            .finish_non_exhaustive()
    }
}

fn require_strict_resolution(
    resolution: ResolutionTier,
    operation: FsOperation,
) -> Result<(), FsError> {
    match resolution {
        ResolutionTier::HandleBound => Ok(()),
        ResolutionTier::Unsupported => Err(FsError::Unsupported {
            operation,
            capability: "strict handle-relative resolution",
        }),
        ResolutionTier::OpenedHandleOnly
        | ResolutionTier::CooperativeTrusted
        | ResolutionTier::Unproven => Err(FsError::Unproven {
            operation,
            proof: "platform cannot prove strict beneath-root no-mount resolution",
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntryObservation {
    name: RelativeComponent,
    kind: FileKind,
    identity: Option<FileIdentity>,
}

impl DirectoryEntryObservation {
    #[must_use]
    pub fn name(&self) -> &RelativeComponent {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> FileKind {
        self.kind
    }

    #[must_use]
    pub const fn identity(&self) -> Option<FileIdentity> {
        self.identity
    }
}

fn next_session() -> Result<CapabilitySessionId, FsError> {
    allocate_session(&NEXT_CAPABILITY_SESSION)
}

pub(crate) fn new_host_session() -> Result<CapabilitySessionId, FsError> {
    next_session()
}

fn allocate_session(counter: &AtomicU64) -> Result<CapabilitySessionId, FsError> {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current == u64::MAX {
            return Err(FsError::CapabilitySessionExhausted);
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Ok(CapabilitySessionId(current)),
            Err(observed) => current = observed,
        }
    }
}

fn validate_directory_facts(root: &DirectoryInner, facts: FileFacts) -> Result<(), FsError> {
    if facts.kind != FileKind::Directory {
        return Err(FsError::NotDirectory);
    }
    validate_common_facts(root, facts)
}

fn validate_file_facts(root: &DirectoryInner, facts: FileFacts) -> Result<(), FsError> {
    if facts.kind != FileKind::Regular {
        return Err(FsError::NotRegularFile);
    }
    validate_common_facts(root, facts)?;
    if root.options.trust.is_strict() && facts.link_count != 1 {
        return Err(FsError::MultipleLinks {
            links: facts.link_count,
        });
    }
    Ok(())
}

fn validate_common_facts(root: &DirectoryInner, facts: FileFacts) -> Result<(), FsError> {
    reject_reparse(facts)?;
    if !facts.identity.same_volume(root.root_native) {
        return Err(FsError::CrossVolume);
    }
    if root.options.trust.is_strict() && !facts.identity_proven {
        return Err(FsError::Unproven {
            operation: FsOperation::InspectHandle,
            proof: "live-object identity or local filesystem semantics are unavailable",
        });
    }
    Ok(())
}

fn reject_reparse(facts: FileFacts) -> Result<(), FsError> {
    if let Some(tag) = facts.reparse_tag {
        Err(FsError::ReparsePoint { tag })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(unix, windows))]
    use std::{io::Read, path::PathBuf};

    #[test]
    fn session_allocation_fails_instead_of_wrapping() {
        let counter = AtomicU64::new(u64::MAX);

        assert!(matches!(
            allocate_session(&counter),
            Err(FsError::CapabilitySessionExhausted)
        ));
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn intermediate_name_swap_cannot_redirect_the_opened_handle_chain() {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join(format!(
            "nara_fs_swap_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let original = root.join("assets");
        let moved = root.join("moved-assets");
        let replacement = root.join("replacement-assets");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::create_dir_all(&replacement).unwrap();
        std::fs::write(original.join("value.txt"), b"authorized").unwrap();
        std::fs::write(replacement.join("value.txt"), b"replacement").unwrap();

        let capability = DirectoryCapability::from_host_handle(
            test_host_directory(&root),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, test_trust_mode()),
        )
        .unwrap();
        let path = RelativePath::new("assets/value.txt").unwrap();
        let file = capability
            .open_file_with_observer(&path, |index, _| {
                if index == 0 {
                    std::fs::rename(&original, &moved).unwrap();
                    std::fs::rename(&replacement, &original).unwrap();
                }
            })
            .unwrap();
        let mut bytes = Vec::new();
        file.reader().unwrap().read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, b"authorized");
        drop(file);
        drop(capability);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn public_open_rejects_reparse_tag_fixtures_without_privileges() {
        // Stable values defined by the Windows SDK's `winnt.h`. Keeping these
        // local avoids requiring fixture privileges or broader SDK features.
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xa000_0003;
        const IO_REPARSE_TAG_SYMLINK: u32 = 0xa000_000c;

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_fs_reparse_fixture_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/value.txt"), b"authorized").unwrap();
        std::fs::write(root.join("value.txt"), b"authorized").unwrap();
        let capability = DirectoryCapability::from_host_handle(
            test_host_directory(&root),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();

        let leaf = RelativePath::new("value.txt").unwrap();
        for tag in [IO_REPARSE_TAG_SYMLINK, 0x8000_0042] {
            let facts = reparse_fixture_facts(&capability, FileKind::Regular, tag);
            let error = platform::with_test_facts_override(facts, || capability.open_file(&leaf))
                .unwrap_err();
            assert!(matches!(
                error,
                FsError::ReparsePoint { tag: rejected } if rejected == tag
            ));
        }

        let facts =
            reparse_fixture_facts(&capability, FileKind::Directory, IO_REPARSE_TAG_MOUNT_POINT);
        let nested = RelativePath::new("assets/value.txt").unwrap();
        let error = platform::with_test_facts_override(facts, || capability.open_file(&nested))
            .unwrap_err();
        assert!(matches!(
            error,
            FsError::ReparsePoint {
                tag: IO_REPARSE_TAG_MOUNT_POINT
            }
        ));

        drop(capability);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    fn reparse_fixture_facts(
        capability: &DirectoryCapability,
        kind: FileKind,
        tag: u32,
    ) -> FileFacts {
        FileFacts {
            identity: capability.inner.root_native,
            kind,
            link_count: 1,
            reparse_tag: Some(tag),
            identity_proven: true,
        }
    }

    #[cfg(any(unix, windows))]
    fn test_trust_mode() -> TrustMode {
        if cfg!(any(windows, target_os = "linux")) {
            TrustMode::Untrusted
        } else {
            TrustMode::TrustedLocal
        }
    }

    #[cfg(windows)]
    fn test_host_directory(path: &PathBuf) -> File {
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
    fn test_host_directory(path: &PathBuf) -> File {
        File::open(path).unwrap()
    }
}
