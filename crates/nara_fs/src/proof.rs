#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofStatus {
    Proven,
    Unproven,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionTier {
    HandleBound,
    OpenedHandleOnly,
    CooperativeTrusted,
    Unsupported,
    Unproven,
}

impl ResolutionTier {
    #[must_use]
    pub const fn status(self) -> ProofStatus {
        match self {
            Self::HandleBound => ProofStatus::Proven,
            Self::OpenedHandleOnly | Self::CooperativeTrusted | Self::Unproven => {
                ProofStatus::Unproven
            }
            Self::Unsupported => ProofStatus::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentAuthorizationTier {
    HandleBound,
    CooperativeTrusted,
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationAtomicity {
    AtomicNameSwitch,
    NonAtomic,
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictProtection {
    StrongCompareAndSwap,
    CooperativeLocked,
    DetectOnly,
    None,
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceSourceBinding {
    HandleBound,
    NameBound,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationIdentityEvidence {
    HandleBoundCandidate,
    PostPublishObserved,
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileSyncTier {
    DataAndMetadata,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectorySyncTier {
    Naming,
    Unproven,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageStatus {
    Achieved,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurabilityProgress {
    data_synced: StageStatus,
    file_metadata_synced: StageStatus,
    name_published: StageStatus,
    parent_directory_synced: StageStatus,
}

impl DurabilityProgress {
    pub const NONE: Self = Self::new(
        StageStatus::Unknown,
        StageStatus::Unknown,
        StageStatus::Unknown,
        StageStatus::Unknown,
    );

    pub const fn new(
        data_synced: StageStatus,
        file_metadata_synced: StageStatus,
        name_published: StageStatus,
        parent_directory_synced: StageStatus,
    ) -> Self {
        Self {
            data_synced,
            file_metadata_synced,
            name_published,
            parent_directory_synced,
        }
    }

    #[must_use]
    pub const fn data_synced(self) -> StageStatus {
        self.data_synced
    }

    #[must_use]
    pub const fn file_metadata_synced(self) -> StageStatus {
        self.file_metadata_synced
    }

    #[must_use]
    pub const fn name_published(self) -> StageStatus {
        self.name_published
    }

    #[must_use]
    pub const fn parent_directory_synced(self) -> StageStatus {
        self.parent_directory_synced
    }

    pub(crate) const fn with_name_published(mut self) -> Self {
        self.name_published = StageStatus::Achieved;
        self
    }

    pub(crate) const fn with_parent_directory(mut self, status: StageStatus) -> Self {
        self.parent_directory_synced = status;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCapabilityMatrix {
    pub(crate) resolution: ResolutionTier,
    pub(crate) single_link: ProofStatus,
    pub(crate) replace_parent: ParentAuthorizationTier,
    pub(crate) publication: PublicationAtomicity,
    pub(crate) conflict: ConflictProtection,
    pub(crate) replace_source: ReplaceSourceBinding,
    pub(crate) file_sync: FileSyncTier,
    pub(crate) directory_sync: DirectorySyncTier,
    pub(crate) advisory_lock: ProofStatus,
    pub(crate) read_directory: ProofStatus,
    pub(crate) relative_unlink: ProofStatus,
    pub(crate) non_overwrite_rename: ProofStatus,
}

impl PlatformCapabilityMatrix {
    #[must_use]
    pub const fn resolution_tier(self) -> ResolutionTier {
        self.resolution
    }

    #[must_use]
    pub const fn strict_relative_open(self) -> ProofStatus {
        self.resolution.status()
    }

    #[must_use]
    pub const fn single_link_proof(self) -> ProofStatus {
        self.single_link
    }

    #[must_use]
    pub const fn replace_parent_authorization(self) -> ParentAuthorizationTier {
        self.replace_parent
    }

    #[must_use]
    pub const fn publication_atomicity(self) -> PublicationAtomicity {
        self.publication
    }

    #[must_use]
    pub const fn conflict_protection(self) -> ConflictProtection {
        self.conflict
    }

    #[must_use]
    pub const fn replace_source_binding(self) -> ReplaceSourceBinding {
        self.replace_source
    }

    #[must_use]
    pub const fn file_sync_tier(self) -> FileSyncTier {
        self.file_sync
    }

    #[must_use]
    pub const fn directory_sync_tier(self) -> DirectorySyncTier {
        self.directory_sync
    }

    #[must_use]
    pub const fn advisory_lock(self) -> ProofStatus {
        self.advisory_lock
    }

    #[must_use]
    pub const fn read_directory(self) -> ProofStatus {
        self.read_directory
    }

    #[must_use]
    pub const fn relative_unlink(self) -> ProofStatus {
        self.relative_unlink
    }

    #[must_use]
    pub const fn non_overwrite_rename(self) -> ProofStatus {
        self.non_overwrite_rename
    }
}
