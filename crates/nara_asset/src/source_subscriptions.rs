use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    mem::size_of,
    sync::Arc,
};

use nara_core::{ByteLimit, ItemLimit};

use crate::revision::{RevisionCounter, RevisionIdentity};
use crate::{
    AssetDependencyGraph, AssetLoadGenerations, AssetReloadRequest, AssetSourcePath,
    ImportRecipeDigest, SourceReloadConsumer, StableAssetId,
};

/// Selects the complete successful inputs or the latest executed failure's recovery inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSubscriptionKind {
    Published,
    Retry,
}

/// Ceilings for retained source owners, unique raw paths, set entries, and modeled storage.
///
/// A path present in both sets counts as two items. Byte charges include owned request payloads,
/// both path representations, and conservative tree-node overhead, even when paths are shared.
/// Fixed empty-graph fields and caller-owned preparation tickets are outside retained storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSubscriptionLimits {
    owners: ItemLimit,
    paths: ItemLimit,
    items: ItemLimit,
    retained_bytes: ByteLimit,
}

impl SourceSubscriptionLimits {
    #[must_use]
    pub const fn new(
        owners: ItemLimit,
        paths: ItemLimit,
        items: ItemLimit,
        retained_bytes: ByteLimit,
    ) -> Self {
        Self {
            owners,
            paths,
            items,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn owners(self) -> ItemLimit {
        self.owners
    }

    #[must_use]
    pub const fn paths(self) -> ItemLimit {
        self.paths
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

impl Default for SourceSubscriptionLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(4_096).expect("default subscription owner limit is non-zero"),
            ItemLimit::new(65_536).expect("default subscription path limit is non-zero"),
            ItemLimit::new(131_072).expect("default subscription item limit is non-zero"),
            ByteLimit::new(32 * 1024 * 1024).expect("default subscription byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SourceSubscriptionUsage {
    owners: usize,
    paths: usize,
    items: usize,
    retained_bytes: usize,
}

impl SourceSubscriptionUsage {
    #[must_use]
    pub const fn owners(self) -> usize {
        self.owners
    }

    #[must_use]
    pub const fn paths(self) -> usize {
        self.paths
    }

    #[must_use]
    pub const fn items(self) -> usize {
        self.items
    }

    #[must_use]
    pub const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSubscriptionLimitKind {
    Owners,
    Paths,
    Items,
    RetainedBytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSubscriptionError {
    AuthorityMismatch,
    Superseded,
    GraphChanged,
    GenerationStoreChanged,
    LimitExceeded {
        kind: SourceSubscriptionLimitKind,
        limit: usize,
        attempted: Option<usize>,
    },
}

impl Display for SourceSubscriptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorityMismatch => {
                formatter.write_str("source subscriptions belong to a different registration")
            }
            Self::Superseded => formatter.write_str("source subscription request was superseded"),
            Self::GraphChanged => formatter
                .write_str("source subscription dependency graph changed after preparation"),
            Self::GenerationStoreChanged => {
                formatter.write_str("source subscription generation store was replaced")
            }
            Self::LimitExceeded {
                kind,
                limit,
                attempted,
            } => write!(
                formatter,
                "source subscription {kind:?} limit {limit} rejected attempted value {attempted:?}"
            ),
        }
    }
}

impl Error for SourceSubscriptionError {}

#[derive(Debug)]
pub enum SourceSubscriptionCommitError<E> {
    Validation(SourceSubscriptionError),
    Mutation(E),
}

impl<E: Display> Display for SourceSubscriptionCommitError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(formatter),
            Self::Mutation(error) => {
                write!(formatter, "source subscription mutation failed: {error}")
            }
        }
    }
}

impl<E: Error + 'static> Error for SourceSubscriptionCommitError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Mutation(error) => Some(error),
        }
    }
}

#[derive(Debug)]
struct SourceSubscriptionOwner {
    request: AssetReloadRequest,
    published: Box<[AssetSourcePath]>,
    retry: Box<[AssetSourcePath]>,
    retained_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct SourceSubscriptions {
    identity: Arc<()>,
    revision: RevisionCounter,
    limits: SourceSubscriptionLimits,
    usage: SourceSubscriptionUsage,
    owners: BTreeMap<StableAssetId, SourceSubscriptionOwner>,
    reverse: BTreeMap<AssetSourcePath, BTreeSet<StableAssetId>>,
}

impl Default for SourceSubscriptions {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
            revision: RevisionCounter::default(),
            limits: SourceSubscriptionLimits::default(),
            usage: SourceSubscriptionUsage::default(),
            owners: BTreeMap::new(),
            reverse: BTreeMap::new(),
        }
    }
}

impl SourceSubscriptions {
    pub(crate) fn advance_revision(&mut self) {
        self.revision.advance();
    }

    fn revision(&self) -> RevisionIdentity {
        RevisionIdentity::capture(&self.identity, Some(&self.revision))
    }

    fn projected_usage(
        &self,
        request: &AssetReloadRequest,
        published: &[AssetSourcePath],
        retry: &[AssetSourcePath],
    ) -> Result<(SourceSubscriptionUsage, usize), SourceSubscriptionError> {
        let source = request.stable_id();
        let old = self.owners.get(&source);
        let old_items = old.map_or(0, |owner| owner.published.len() + owner.retry.len());
        let new_items = published.len().checked_add(retry.len());
        let nonempty = !published.is_empty() || !retry.is_empty();
        let owners = check_limit(
            self.usage
                .owners
                .checked_sub(usize::from(old.is_some()))
                .and_then(|count| count.checked_add(usize::from(nonempty))),
            self.limits.owners.get(),
            SourceSubscriptionLimitKind::Owners,
        )?;
        let items = check_limit(
            self.usage
                .items
                .checked_sub(old_items)
                .and_then(|count| count.checked_add(new_items?)),
            self.limits.items.get(),
            SourceSubscriptionLimitKind::Items,
        )?;
        let removed_paths = old.map_or(0, |owner| {
            unique_paths(&owner.published, &owner.retry)
                .filter(|path| !contains_path(published, retry, path))
                .filter(|path| {
                    self.reverse
                        .get(*path)
                        .is_some_and(|owners| owners.len() == 1)
                })
                .count()
        });
        let added_paths = unique_paths(published, retry)
            .filter(|path| !self.reverse.contains_key(*path))
            .count();
        let paths = check_limit(
            self.usage
                .paths
                .checked_sub(removed_paths)
                .and_then(|count| count.checked_add(added_paths)),
            self.limits.paths.get(),
            SourceSubscriptionLimitKind::Paths,
        )?;
        let owner_bytes = check_limit(
            if nonempty {
                owner_retained_bytes(request, published, retry)
            } else {
                Some(0)
            },
            self.limits.retained_bytes.get(),
            SourceSubscriptionLimitKind::RetainedBytes,
        )?;
        let retained_bytes = check_limit(
            self.usage
                .retained_bytes
                .checked_sub(old.map_or(0, |owner| owner.retained_bytes))
                .and_then(|bytes| bytes.checked_add(owner_bytes)),
            self.limits.retained_bytes.get(),
            SourceSubscriptionLimitKind::RetainedBytes,
        )?;
        Ok((
            SourceSubscriptionUsage {
                owners,
                paths,
                items,
                retained_bytes,
            },
            owner_bytes,
        ))
    }

    fn replace(&mut self, prepared: PreparedSourceSubscriptions) {
        let source = prepared.request.stable_id();
        let old = self.owners.remove(&source);
        if let Some(owner) = &old {
            for path in unique_paths(&owner.published, &owner.retry) {
                if let Some(owners) = self.reverse.get_mut(path) {
                    owners.remove(&source);
                    if owners.is_empty() {
                        self.reverse.remove(path);
                    }
                }
            }
        }
        let (published, retry) = match prepared.kind {
            SourceSubscriptionKind::Published => (prepared.paths, Box::default()),
            SourceSubscriptionKind::Retry => (
                old.map_or_else(Box::default, |owner| owner.published),
                prepared.paths,
            ),
        };
        if !published.is_empty() || !retry.is_empty() {
            for path in unique_paths(&published, &retry) {
                self.reverse.entry(path.clone()).or_default().insert(source);
            }
            self.owners.insert(
                source,
                SourceSubscriptionOwner {
                    request: prepared.request,
                    published,
                    retry,
                    retained_bytes: prepared.owner_bytes,
                },
            );
        }
        self.usage = prepared.usage;
        self.advance_revision();
    }
}

/// A non-cloneable, pre-admitted replacement against an exact graph and request generation.
///
/// Preparation does not mutate the graph or reserve persistent capacity. A successful mutation
/// closure must atomically commit its value/state baseline, such as `AssetMutationBaseline`.
/// Graph relationships change only after that closure succeeds. Root commits run sequentially;
/// another graph mutation invalidates this ticket and requires fresh preparation.
#[must_use]
pub struct PreparedSourceSubscriptions {
    revision: RevisionIdentity,
    generation_store: Arc<()>,
    request: AssetReloadRequest,
    kind: SourceSubscriptionKind,
    paths: Box<[AssetSourcePath]>,
    usage: SourceSubscriptionUsage,
    owner_bytes: usize,
}

impl fmt::Debug for PreparedSourceSubscriptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSourceSubscriptions")
            .field("source", &self.request.stable_id())
            .field("generation", &self.request.generation())
            .field("kind", &self.kind)
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl PreparedSourceSubscriptions {
    pub fn validate(
        &self,
        graph: &AssetDependencyGraph,
        generations: &AssetLoadGenerations,
    ) -> Result<(), SourceSubscriptionError> {
        if self.revision != graph.source_subscriptions.revision() {
            return Err(SourceSubscriptionError::GraphChanged);
        }
        if !Arc::ptr_eq(&self.generation_store, generations.store_identity()) {
            return Err(SourceSubscriptionError::GenerationStoreChanged);
        }
        if !generations.is_current_request(&self.request) {
            return Err(SourceSubscriptionError::Superseded);
        }
        Ok(())
    }

    pub fn commit_with<T, E>(
        self,
        graph: &mut AssetDependencyGraph,
        generations: &AssetLoadGenerations,
        mutation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, SourceSubscriptionCommitError<E>> {
        self.validate(graph, generations)
            .map_err(SourceSubscriptionCommitError::Validation)?;
        let result = mutation().map_err(SourceSubscriptionCommitError::Mutation)?;
        graph.source_subscriptions.replace(self);
        Ok(result)
    }
}

impl AssetDependencyGraph {
    #[must_use]
    pub fn with_source_subscription_limits(limits: SourceSubscriptionLimits) -> Self {
        let mut graph = Self::default();
        graph.source_subscriptions.limits = limits;
        graph
    }

    #[must_use]
    pub fn source_subscription_limits(&self) -> SourceSubscriptionLimits {
        self.source_subscriptions.limits
    }

    #[must_use]
    pub fn source_subscription_usage(&self) -> SourceSubscriptionUsage {
        self.source_subscriptions.usage
    }

    /// Borrows the complete successful set or the latest executed failure's retry set.
    #[must_use]
    pub fn source_subscription_paths(
        &self,
        source: StableAssetId,
        kind: SourceSubscriptionKind,
    ) -> &[AssetSourcePath] {
        self.source_subscriptions
            .owners
            .get(&source)
            .map_or(&[][..], |owner| match kind {
                SourceSubscriptionKind::Published => owner.published.as_ref(),
                SourceSubscriptionKind::Retry => owner.retry.as_ref(),
            })
    }

    /// Routes raw input changes without requiring a catalogue record or source UUID for the input.
    pub fn raw_dependents_for_path(
        &self,
        path: &AssetSourcePath,
    ) -> impl Iterator<Item = StableAssetId> + '_ {
        self.source_subscriptions
            .reverse
            .get(path)
            .into_iter()
            .flat_map(|owners| owners.iter().copied())
    }

    /// Admits a replacement without publishing a value or changing graph relationships.
    pub fn prepare_source_subscriptions(
        &self,
        consumer: &SourceReloadConsumer,
        request: &AssetReloadRequest,
        generations: &AssetLoadGenerations,
        mut paths: Vec<AssetSourcePath>,
        kind: SourceSubscriptionKind,
    ) -> Result<PreparedSourceSubscriptions, SourceSubscriptionError> {
        let subscriptions = &self.source_subscriptions;
        let old = subscriptions.owners.get(&request.stable_id());
        if !consumer.owns_request(request)
            || old.is_some_and(|owner| !consumer.owns_request(&owner.request))
        {
            return Err(SourceSubscriptionError::AuthorityMismatch);
        }
        if !generations.is_current_request(request) {
            return Err(SourceSubscriptionError::Superseded);
        }
        paths.sort_unstable();
        paths.dedup();
        let paths = paths.into_boxed_slice();
        let request = request.clone();
        let (published, retry): (&[AssetSourcePath], &[AssetSourcePath]) = match kind {
            SourceSubscriptionKind::Published => (&paths, &[]),
            SourceSubscriptionKind::Retry => (
                old.map_or(&[][..], |owner| owner.published.as_ref()),
                &paths,
            ),
        };
        let (usage, owner_bytes) = subscriptions.projected_usage(&request, published, retry)?;
        Ok(PreparedSourceSubscriptions {
            revision: subscriptions.revision(),
            generation_store: Arc::clone(generations.store_identity()),
            request,
            kind,
            paths,
            usage,
            owner_bytes,
        })
    }
}

fn unique_paths<'a>(
    published: &'a [AssetSourcePath],
    retry: &'a [AssetSourcePath],
) -> impl Iterator<Item = &'a AssetSourcePath> {
    published.iter().chain(
        retry
            .iter()
            .filter(move |path| published.binary_search(path).is_err()),
    )
}

fn contains_path(
    published: &[AssetSourcePath],
    retry: &[AssetSourcePath],
    path: &AssetSourcePath,
) -> bool {
    published.binary_search(path).is_ok() || retry.binary_search(path).is_ok()
}

fn check_limit(
    attempted: Option<usize>,
    limit: usize,
    kind: SourceSubscriptionLimitKind,
) -> Result<usize, SourceSubscriptionError> {
    match attempted {
        Some(attempted) if attempted <= limit => Ok(attempted),
        attempted => Err(SourceSubscriptionError::LimitExceeded {
            kind,
            limit,
            attempted,
        }),
    }
}

/// Charges a full eleven-slot node and child links per entry, including sparsely occupied trees.
fn tree_entry_bytes<K, V>() -> Option<usize> {
    size_of::<(K, V)>()
        .checked_mul(11)?
        .checked_add(12 * size_of::<usize>())?
        .checked_add(64)
}

fn owner_retained_bytes(
    request: &AssetReloadRequest,
    published: &[AssetSourcePath],
    retry: &[AssetSourcePath],
) -> Option<usize> {
    let mut bytes = tree_entry_bytes::<StableAssetId, SourceSubscriptionOwner>()?
        .checked_add(request.path().retained_bytes())?
        .checked_add(request.source_kind().retained_bytes())?
        .checked_add(
            request
                .affected_recipes()
                .len()
                .checked_mul(size_of::<ImportRecipeDigest>())?,
        )?
        .checked_add(8 * 64)?;
    let reverse_entry = tree_entry_bytes::<AssetSourcePath, BTreeSet<StableAssetId>>()?
        .checked_add(tree_entry_bytes::<StableAssetId, ()>()?)?;
    for path in published.iter().chain(retry) {
        bytes = bytes
            .checked_add(size_of::<AssetSourcePath>())?
            .checked_add(path.retained_bytes().checked_mul(2)?)?
            .checked_add(reverse_entry)?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests;
