use crate::{DirectorySyncTier, DurabilityProgress, FileSyncTier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSyncReceipt {
    tier: FileSyncTier,
    progress: DurabilityProgress,
}

impl FileSyncReceipt {
    pub(crate) const fn new(tier: FileSyncTier, progress: DurabilityProgress) -> Self {
        Self { tier, progress }
    }

    #[must_use]
    pub const fn tier(self) -> FileSyncTier {
        self.tier
    }

    #[must_use]
    pub const fn progress(self) -> DurabilityProgress {
        self.progress
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectorySyncReceipt {
    tier: DirectorySyncTier,
    progress: DurabilityProgress,
}

impl DirectorySyncReceipt {
    pub(crate) const fn new(tier: DirectorySyncTier, progress: DurabilityProgress) -> Self {
        Self { tier, progress }
    }

    #[must_use]
    pub const fn tier(self) -> DirectorySyncTier {
        self.tier
    }

    #[must_use]
    pub const fn progress(self) -> DurabilityProgress {
        self.progress
    }
}
