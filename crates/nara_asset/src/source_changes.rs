use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::{ByteLimit, ItemLimit};

use crate::AssetSourcePath;

const DEFAULT_MAX_SOURCE_CHANGES: usize = 4_096;
const DEFAULT_MAX_SOURCE_CHANGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetSourceChangeKind {
    MetaModified,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSourceChange {
    path: AssetSourcePath,
    observation: PendingSourceChange,
}

impl AssetSourceChange {
    #[must_use]
    pub const fn new(path: AssetSourcePath, kind: AssetSourceChangeKind) -> Self {
        Self {
            path,
            observation: PendingSourceChange::new(kind),
        }
    }

    /// Records a metadata sidecar removal with the principal's semantic removal behavior.
    #[must_use]
    pub const fn metadata_removed(path: AssetSourcePath) -> Self {
        Self {
            path,
            observation: PendingSourceChange {
                raw_source_changed: false,
                raw_metadata_changed: true,
                ..PendingSourceChange::new(AssetSourceChangeKind::Removed)
            },
        }
    }

    #[must_use]
    pub const fn path(&self) -> &AssetSourcePath {
        &self.path
    }

    #[must_use]
    pub const fn kind(&self) -> AssetSourceChangeKind {
        self.observation.kind()
    }

    /// Metadata dirtiness survives a coalesced removal without implying source existence.
    #[must_use]
    pub const fn metadata_dirty(&self) -> bool {
        self.observation.metadata_dirty
    }

    /// Whether any coalesced observation changed the raw principal input.
    #[must_use]
    pub const fn raw_source_changed(&self) -> bool {
        self.observation.raw_source_changed
    }

    /// Whether any coalesced observation changed the raw metadata sidecar input.
    #[must_use]
    pub const fn raw_metadata_changed(&self) -> bool {
        self.observation.raw_metadata_changed
    }

    #[cfg(any(feature = "runtime", test))]
    pub(crate) const fn metadata_after_removal(&self) -> bool {
        matches!(
            self.observation.last_invalidation,
            Some(SourceInvalidation::Metadata)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetSourceChangeLimits {
    items: ItemLimit,
    retained_bytes: ByteLimit,
}

impl AssetSourceChangeLimits {
    #[must_use]
    pub const fn new(items: ItemLimit, retained_bytes: ByteLimit) -> Self {
        Self {
            items,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn items(self) -> ItemLimit {
        self.items
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

impl Default for AssetSourceChangeLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(DEFAULT_MAX_SOURCE_CHANGES)
                .expect("default source-change item limit is non-zero"),
            ByteLimit::new(DEFAULT_MAX_SOURCE_CHANGE_BYTES)
                .expect("default source-change byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSourceChangeLimitKind {
    Items,
    RetainedBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetSourceChangeAdmissionError {
    kind: AssetSourceChangeLimitKind,
    limit: usize,
    attempted: Option<usize>,
}

impl AssetSourceChangeAdmissionError {
    const fn new(kind: AssetSourceChangeLimitKind, limit: usize, attempted: Option<usize>) -> Self {
        Self {
            kind,
            limit,
            attempted,
        }
    }

    #[must_use]
    pub const fn kind(self) -> AssetSourceChangeLimitKind {
        self.kind
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }

    #[must_use]
    pub const fn attempted(self) -> Option<usize> {
        self.attempted
    }
}

impl Display for AssetSourceChangeAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self.attempted {
            Some(attempted) => write!(
                formatter,
                "asset source-change {:?} limit {} rejected attempted value {attempted}",
                self.kind, self.limit
            ),
            None => write!(
                formatter,
                "asset source-change {:?} limit {} rejected an overflowing value",
                self.kind, self.limit
            ),
        }
    }
}

impl Error for AssetSourceChangeAdmissionError {}

fn checked_source_change_limit(
    current: usize,
    added: usize,
    limit: usize,
    kind: AssetSourceChangeLimitKind,
) -> Result<usize, AssetSourceChangeAdmissionError> {
    let attempted = current.checked_add(added);
    match attempted {
        Some(attempted) if attempted <= limit => Ok(attempted),
        attempted => Err(AssetSourceChangeAdmissionError::new(kind, limit, attempted)),
    }
}

/// Bounded semantic source-change intent consumed in `AssetTaskUpdateSet::ResolveSourceChanges`.
///
/// Watch adapters and explicit authoring tools are producers. Every staged input batch and the
/// retained set share the item and owned-path byte ceilings. Duplicate paths coalesce at admission
/// with the latest source existence, metadata dirtiness, and relative metadata/removal order
/// preserved; distinct paths retain
/// their owned logical-path capacity
/// until the next resolve stage drains the complete map. This authoring request intent is not
/// replay-recordable gameplay state.
#[derive(Debug)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct AssetSourceChanges {
    limits: AssetSourceChangeLimits,
    retained_payload_bytes: usize,
    changes: BTreeMap<AssetSourcePath, PendingSourceChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceInvalidation {
    Metadata,
    Removal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingSourceChange {
    removed: Option<bool>,
    metadata_dirty: bool,
    raw_source_changed: bool,
    raw_metadata_changed: bool,
    // The rightmost relevant observation survives source-only edits and regrouped batches.
    last_invalidation: Option<SourceInvalidation>,
}

impl PendingSourceChange {
    const fn new(kind: AssetSourceChangeKind) -> Self {
        Self {
            removed: match kind {
                AssetSourceChangeKind::Modified => Some(false),
                AssetSourceChangeKind::Removed => Some(true),
                AssetSourceChangeKind::MetaModified => None,
            },
            metadata_dirty: matches!(kind, AssetSourceChangeKind::MetaModified),
            raw_source_changed: !matches!(kind, AssetSourceChangeKind::MetaModified),
            raw_metadata_changed: matches!(kind, AssetSourceChangeKind::MetaModified),
            last_invalidation: match kind {
                AssetSourceChangeKind::MetaModified => Some(SourceInvalidation::Metadata),
                AssetSourceChangeKind::Removed => Some(SourceInvalidation::Removal),
                AssetSourceChangeKind::Modified => None,
            },
        }
    }

    fn merge(&mut self, incoming: Self) {
        if incoming.removed.is_some() {
            self.removed = incoming.removed;
        }
        self.metadata_dirty |= incoming.metadata_dirty;
        self.raw_source_changed |= incoming.raw_source_changed;
        self.raw_metadata_changed |= incoming.raw_metadata_changed;
        if incoming.last_invalidation.is_some() {
            self.last_invalidation = incoming.last_invalidation;
        }
    }

    const fn kind(self) -> AssetSourceChangeKind {
        if matches!(self.removed, Some(true)) {
            AssetSourceChangeKind::Removed
        } else if self.metadata_dirty {
            AssetSourceChangeKind::MetaModified
        } else {
            AssetSourceChangeKind::Modified
        }
    }
}

impl Default for AssetSourceChanges {
    fn default() -> Self {
        Self::with_limits(AssetSourceChangeLimits::default())
    }
}

impl AssetSourceChanges {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn with_limits(limits: AssetSourceChangeLimits) -> Self {
        Self {
            limits,
            retained_payload_bytes: 0,
            changes: BTreeMap::new(),
        }
    }

    pub fn push(
        &mut self,
        change: AssetSourceChange,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        let AssetSourceChange {
            path,
            observation: change,
        } = change;
        let retained_items = self.changes.len();
        match self.changes.entry(path) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().merge(change);
            }
            Entry::Vacant(entry) => {
                checked_source_change_limit(
                    retained_items,
                    1,
                    self.limits.items().get(),
                    AssetSourceChangeLimitKind::Items,
                )?;
                let retained_payload_bytes = checked_source_change_limit(
                    self.retained_payload_bytes,
                    entry.key().retained_bytes(),
                    self.limits.retained_bytes().get(),
                    AssetSourceChangeLimitKind::RetainedBytes,
                )?;
                entry.insert(change);
                self.retained_payload_bytes = retained_payload_bytes;
            }
        }
        Ok(())
    }

    pub fn modified(
        &mut self,
        path: AssetSourcePath,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::Modified,
        ))
    }

    pub fn meta_modified(
        &mut self,
        path: AssetSourcePath,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::MetaModified,
        ))
    }

    pub fn removed(
        &mut self,
        path: AssetSourcePath,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(path, AssetSourceChangeKind::Removed))
    }

    pub fn try_extend(
        &mut self,
        changes: impl IntoIterator<Item = AssetSourceChange>,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        let mut incoming: BTreeMap<AssetSourcePath, PendingSourceChange> = BTreeMap::new();
        let mut input_items = 0_usize;
        let mut input_payload_bytes = 0_usize;
        let mut retained_items = self.changes.len();
        let mut retained_payload_bytes = self.retained_payload_bytes;
        for AssetSourceChange {
            path,
            observation: pending,
        } in changes
        {
            input_items = checked_source_change_limit(
                input_items,
                1,
                self.limits.items().get(),
                AssetSourceChangeLimitKind::Items,
            )?;

            match incoming.entry(path) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().merge(pending);
                }
                Entry::Vacant(entry) => {
                    input_payload_bytes = checked_source_change_limit(
                        input_payload_bytes,
                        entry.key().retained_bytes(),
                        self.limits.retained_bytes().get(),
                        AssetSourceChangeLimitKind::RetainedBytes,
                    )?;
                    if !self.changes.contains_key(entry.key()) {
                        retained_items = checked_source_change_limit(
                            retained_items,
                            1,
                            self.limits.items().get(),
                            AssetSourceChangeLimitKind::Items,
                        )?;
                        retained_payload_bytes = checked_source_change_limit(
                            retained_payload_bytes,
                            entry.key().retained_bytes(),
                            self.limits.retained_bytes().get(),
                            AssetSourceChangeLimitKind::RetainedBytes,
                        )?;
                    }
                    entry.insert(pending);
                }
            }
        }

        self.retained_payload_bytes = retained_payload_bytes;
        for (path, change) in incoming {
            match self.changes.entry(path) {
                Entry::Occupied(mut entry) => entry.get_mut().merge(change),
                Entry::Vacant(entry) => {
                    entry.insert(change);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn limits(&self) -> AssetSourceChangeLimits {
        self.limits
    }

    #[must_use]
    pub const fn retained_payload_bytes(&self) -> usize {
        self.retained_payload_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn drain_coalesced(&mut self) -> Vec<AssetSourceChange> {
        self.retained_payload_bytes = 0;
        std::mem::take(&mut self.changes)
            .into_iter()
            .map(|(path, observation)| AssetSourceChange { path, observation })
            .collect()
    }
}

#[cfg(test)]
mod tests;
