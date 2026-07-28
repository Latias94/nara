use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    fmt::{self, Display, Formatter},
    path::{Path, PathBuf},
};

use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;

use crate::{AssetDatabaseError, AssetPath, ProjectAssetDatabase};

const DEFAULT_MAX_SOURCE_CHANGES: usize = 4_096;
const DEFAULT_MAX_SOURCE_CHANGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct AssetSourceRoot {
    root: PathBuf,
}

impl AssetSourceRoot {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn source_path(&self, path: &AssetPath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |path, segment| path.join(segment))
    }

    pub fn logical_path_from_source_path(
        &self,
        source_path: impl AsRef<Path>,
    ) -> Result<AssetPath, AssetDatabaseError> {
        ProjectAssetDatabase::logical_path_from_source_path(&self.root, source_path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetSourceChangeKind {
    MetaModified,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSourceChange {
    path: AssetPath,
    kind: AssetSourceChangeKind,
}

impl AssetSourceChange {
    #[must_use]
    pub const fn new(path: AssetPath, kind: AssetSourceChangeKind) -> Self {
        Self { path, kind }
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn kind(&self) -> AssetSourceChangeKind {
        self.kind
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
/// with the latest semantic event winning; distinct paths retain their owned logical-path capacity
/// until the next resolve stage drains the complete map. This authoring request intent is not
/// replay-recordable gameplay state.
#[derive(Debug, Resource)]
pub struct AssetSourceChanges {
    limits: AssetSourceChangeLimits,
    retained_payload_bytes: usize,
    changes: BTreeMap<AssetPath, AssetSourceChangeKind>,
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
        let AssetSourceChange { path, kind } = change;
        let retained_items = self.changes.len();
        match self.changes.entry(path) {
            Entry::Occupied(mut entry) => {
                entry.insert(kind);
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
                entry.insert(kind);
                self.retained_payload_bytes = retained_payload_bytes;
            }
        }
        Ok(())
    }

    pub fn modified(&mut self, path: AssetPath) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::Modified,
        ))
    }

    pub fn meta_modified(
        &mut self,
        path: AssetPath,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::MetaModified,
        ))
    }

    pub fn removed(&mut self, path: AssetPath) -> Result<(), AssetSourceChangeAdmissionError> {
        self.push(AssetSourceChange::new(path, AssetSourceChangeKind::Removed))
    }

    pub fn try_extend(
        &mut self,
        changes: impl IntoIterator<Item = AssetSourceChange>,
    ) -> Result<(), AssetSourceChangeAdmissionError> {
        let mut incoming = BTreeMap::new();
        let mut input_items = 0_usize;
        let mut input_payload_bytes = 0_usize;
        let mut retained_items = self.changes.len();
        let mut retained_payload_bytes = self.retained_payload_bytes;
        for AssetSourceChange { path, kind } in changes {
            input_items = checked_source_change_limit(
                input_items,
                1,
                self.limits.items().get(),
                AssetSourceChangeLimitKind::Items,
            )?;

            match incoming.entry(path) {
                Entry::Occupied(mut entry) => {
                    entry.insert(kind);
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
                    entry.insert(kind);
                }
            }
        }

        self.retained_payload_bytes = retained_payload_bytes;
        self.changes.extend(incoming);
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
            .map(|(path, kind)| AssetSourceChange::new(path, kind))
            .collect()
    }
}
