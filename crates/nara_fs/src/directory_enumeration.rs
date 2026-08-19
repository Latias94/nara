use std::fmt::{self, Debug, Formatter};

use crate::{DirectoryCapability, FileIdentity, FileKind, FsError, RelativeComponent, platform};

const DEFAULT_MAX_TOTAL_ENTRIES: usize = 4_096;
const DEFAULT_MAX_NAME_BYTES: usize = 1_024;
const DEFAULT_MAX_REJECTED_FACTS: usize = 128;

/// Cumulative limits for one direct-child directory enumeration session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryEnumerationLimits {
    max_total_entries: usize,
    max_name_bytes: usize,
    max_rejected_facts: usize,
}

impl DirectoryEnumerationLimits {
    #[must_use]
    pub const fn new(
        max_total_entries: usize,
        max_name_bytes: usize,
        max_rejected_facts: usize,
    ) -> Self {
        Self {
            max_total_entries,
            max_name_bytes,
            max_rejected_facts,
        }
    }

    #[must_use]
    pub const fn max_total_entries(self) -> usize {
        self.max_total_entries
    }

    #[must_use]
    pub const fn max_name_bytes(self) -> usize {
        self.max_name_bytes
    }

    #[must_use]
    pub const fn max_rejected_facts(self) -> usize {
        self.max_rejected_facts
    }
}

impl Default for DirectoryEnumerationLimits {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_TOTAL_ENTRIES,
            DEFAULT_MAX_NAME_BYTES,
            DEFAULT_MAX_REJECTED_FACTS,
        )
    }
}

/// A safely isolated child that could not become an openable directory observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryEntryRejectionReason {
    UnportableName,
    NameTooLong,
    ReparsePoint,
    UnsupportedKind,
    EntryChanged,
    IdentityUnproven,
    CrossVolume,
    MultipleLinks,
    EntryUnavailable,
}

/// Bounded public evidence for one isolated child rejection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntryRejectedFact {
    name: Option<RelativeComponent>,
    reason: DirectoryEntryRejectionReason,
}

impl DirectoryEntryRejectedFact {
    #[must_use]
    pub fn name(&self) -> Option<&RelativeComponent> {
        self.name.as_ref()
    }

    #[must_use]
    pub const fn reason(&self) -> DirectoryEntryRejectionReason {
        self.reason
    }
}

/// One validated direct-child observation.
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

/// One caller-bounded slice of a direct-child enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEnumerationBatch {
    entries: Vec<DirectoryEntryObservation>,
    rejected_entry_facts: Vec<DirectoryEntryRejectedFact>,
    complete: bool,
}

impl DirectoryEnumerationBatch {
    #[must_use]
    pub fn entries(&self) -> &[DirectoryEntryObservation] {
        &self.entries
    }

    #[must_use]
    pub fn rejected_entry_facts(&self) -> &[DirectoryEntryRejectedFact] {
        &self.rejected_entry_facts
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<DirectoryEntryObservation>,
        Vec<DirectoryEntryRejectedFact>,
        bool,
    ) {
        (self.entries, self.rejected_entry_facts, self.complete)
    }
}

/// Move-only, independent native cursor for one directory capability.
pub struct DirectoryEnumerationSession {
    authority: DirectoryCapability,
    cursor: Option<platform::DirectoryEnumerationCursor>,
    limits: DirectoryEnumerationLimits,
    observed_entries: usize,
    observed_rejected_facts: usize,
    complete: bool,
    failed: bool,
}

impl DirectoryEnumerationSession {
    pub(crate) fn begin(
        authority: DirectoryCapability,
        limits: DirectoryEnumerationLimits,
    ) -> Result<Self, FsError> {
        let cursor = platform::begin_directory_enumeration(&authority.inner.file)?;
        Ok(Self {
            authority,
            cursor: Some(cursor),
            limits,
            observed_entries: 0,
            observed_rejected_facts: 0,
            complete: false,
            failed: false,
        })
    }

    /// Advances the private native cursor by at most `max_entries` direct children.
    pub fn next_batch(&mut self, max_entries: usize) -> Result<DirectoryEnumerationBatch, FsError> {
        if max_entries == 0 {
            return Err(FsError::DirectoryEnumerationBatchLimitZero);
        }
        if self.failed {
            return Err(FsError::DirectoryEnumerationTerminated);
        }
        if self.complete {
            return Ok(DirectoryEnumerationBatch {
                entries: Vec::new(),
                rejected_entry_facts: Vec::new(),
                complete: true,
            });
        }

        let result = self.advance(max_entries);
        if result.is_err() {
            self.failed = true;
            self.cursor.take();
        }
        result
    }

    fn advance(&mut self, max_entries: usize) -> Result<DirectoryEnumerationBatch, FsError> {
        let mut entries = Vec::with_capacity(max_entries.min(32));
        let mut rejected_entry_facts = Vec::new();
        let mut consumed = 0_usize;

        while consumed < max_entries {
            let native = {
                let Some(cursor) = self.cursor.as_mut() else {
                    return Err(FsError::DirectoryEnumerationTerminated);
                };
                platform::next_directory_entry(cursor)?
            };
            let Some(native) = native else {
                self.complete = true;
                self.cursor.take();
                break;
            };
            consumed += 1;
            self.observed_entries = self.observed_entries.checked_add(1).ok_or(
                FsError::DirectoryEnumerationEntryLimitExceeded {
                    limit: self.limits.max_total_entries,
                },
            )?;
            if self.observed_entries > self.limits.max_total_entries {
                return Err(FsError::DirectoryEnumerationEntryLimitExceeded {
                    limit: self.limits.max_total_entries,
                });
            }

            self.classify_entry(native, &mut entries, &mut rejected_entry_facts)?;
        }

        Ok(DirectoryEnumerationBatch {
            entries,
            rejected_entry_facts,
            complete: self.complete,
        })
    }

    fn classify_entry(
        &mut self,
        native: platform::NativeDirectoryEntry,
        entries: &mut Vec<DirectoryEntryObservation>,
        rejected: &mut Vec<DirectoryEntryRejectedFact>,
    ) -> Result<(), FsError> {
        if native.name_bytes > self.limits.max_name_bytes {
            return self.push_rejection(None, DirectoryEntryRejectionReason::NameTooLong, rejected);
        }
        let Some(portable_name) = native.name.to_str() else {
            return self.push_rejection(
                None,
                DirectoryEntryRejectionReason::UnportableName,
                rejected,
            );
        };
        let name = match RelativeComponent::new(portable_name) {
            Ok(name) => name,
            Err(_) => {
                return self.push_rejection(
                    None,
                    DirectoryEntryRejectionReason::UnportableName,
                    rejected,
                );
            }
        };
        if native.reparse {
            return self.push_rejection(
                Some(name),
                DirectoryEntryRejectionReason::ReparsePoint,
                rejected,
            );
        }
        if native.kind == FileKind::Other {
            return self.push_rejection(
                Some(name),
                DirectoryEntryRejectionReason::UnsupportedKind,
                rejected,
            );
        }

        let identity = match native.kind {
            FileKind::Directory => self
                .authority
                .open_child_directory(&name)
                .map(|child| child.identity()),
            FileKind::Regular => self
                .authority
                .open_child_file(&name)
                .map(|child| child.identity()),
            FileKind::Other => {
                return self.push_rejection(
                    Some(name),
                    DirectoryEntryRejectionReason::UnsupportedKind,
                    rejected,
                );
            }
        };
        match identity {
            Ok(identity) => entries.push(DirectoryEntryObservation {
                name,
                kind: native.kind,
                identity: Some(identity),
            }),
            Err(error) => {
                let reason = rejection_reason(&error);
                self.push_rejection(Some(name), reason, rejected)?;
            }
        }
        Ok(())
    }

    fn push_rejection(
        &mut self,
        name: Option<RelativeComponent>,
        reason: DirectoryEntryRejectionReason,
        rejected: &mut Vec<DirectoryEntryRejectedFact>,
    ) -> Result<(), FsError> {
        self.observed_rejected_facts = self.observed_rejected_facts.checked_add(1).ok_or(
            FsError::DirectoryEnumerationRejectedFactLimitExceeded {
                limit: self.limits.max_rejected_facts,
            },
        )?;
        if self.observed_rejected_facts > self.limits.max_rejected_facts {
            return Err(FsError::DirectoryEnumerationRejectedFactLimitExceeded {
                limit: self.limits.max_rejected_facts,
            });
        }
        rejected.push(DirectoryEntryRejectedFact { name, reason });
        Ok(())
    }
}

impl Debug for DirectoryEnumerationSession {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectoryEnumerationSession")
            .field("directory", &self.authority.identity())
            .field("limits", &self.limits)
            .field("observed_entries", &self.observed_entries)
            .field("observed_rejected_facts", &self.observed_rejected_facts)
            .field("complete", &self.complete)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

fn rejection_reason(error: &FsError) -> DirectoryEntryRejectionReason {
    match error {
        FsError::ReparsePoint { .. } | FsError::SymbolicLinkTraversal => {
            DirectoryEntryRejectionReason::ReparsePoint
        }
        FsError::CrossVolume => DirectoryEntryRejectionReason::CrossVolume,
        FsError::MultipleLinks { .. } => DirectoryEntryRejectionReason::MultipleLinks,
        FsError::Unproven { .. } | FsError::IdentityUnavailable => {
            DirectoryEntryRejectionReason::IdentityUnproven
        }
        FsError::NotDirectory | FsError::NotRegularFile | FsError::IdentityMismatch { .. } => {
            DirectoryEntryRejectionReason::EntryChanged
        }
        FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            DirectoryEntryRejectionReason::EntryChanged
        }
        _ => DirectoryEntryRejectionReason::EntryUnavailable,
    }
}
