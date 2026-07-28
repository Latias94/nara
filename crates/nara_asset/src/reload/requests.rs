use std::{collections::BTreeMap, fmt, sync::Arc};

use nara_app::{App, PluginError};
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;

use crate::{
    AssetId, AssetPath, AssetRecord, AssetSourceKind, AssetVersion, ImportArtifactDigest,
    StableAssetId,
};

use super::AssetSourceChangeKind;

const DEFAULT_MAX_RELOAD_REQUESTS: usize = 1_024;
const DEFAULT_MAX_RELOAD_REQUEST_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetReloadRequestLimits {
    items: ItemLimit,
    retained_bytes: ByteLimit,
}

impl AssetReloadRequestLimits {
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

impl Default for AssetReloadRequestLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(DEFAULT_MAX_RELOAD_REQUESTS)
                .expect("default reload request item limit is non-zero"),
            ByteLimit::new(DEFAULT_MAX_RELOAD_REQUEST_BYTES)
                .expect("default reload request byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetReloadRequestLimitKind {
    Items,
    RetainedBytes,
}

impl AssetReloadRequestLimitKind {
    pub(super) const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Items => "asset.reload-request-item-limit-exceeded",
            Self::RetainedBytes => "asset.reload-request-byte-limit-exceeded",
        }
    }

    pub(super) const fn safe_summary(self) -> &'static str {
        match self {
            Self::Items => "Asset reload request item limit was exceeded",
            Self::RetainedBytes => "Asset reload request byte limit was exceeded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetReloadRequestAdmissionError {
    kind: AssetReloadRequestLimitKind,
    limit: usize,
    attempted: Option<usize>,
}

impl AssetReloadRequestAdmissionError {
    const fn new(
        kind: AssetReloadRequestLimitKind,
        limit: usize,
        attempted: Option<usize>,
    ) -> Self {
        Self {
            kind,
            limit,
            attempted,
        }
    }

    #[must_use]
    pub const fn kind(self) -> AssetReloadRequestLimitKind {
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

impl fmt::Display for AssetReloadRequestAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.attempted {
            Some(attempted) => write!(
                formatter,
                "asset reload {:?} limit {} rejected attempted retained value {attempted}",
                self.kind, self.limit
            ),
            None => write!(
                formatter,
                "asset reload {:?} limit {} rejected an overflowing retained value",
                self.kind, self.limit
            ),
        }
    }
}

impl std::error::Error for AssetReloadRequestAdmissionError {}

/// Non-cloneable authority for the concrete first-party image reload integration.
///
/// The token is returned directly to `ImagePlugin` and never stored as a public ECS resource.
/// Requests admitted for another App generation cannot be drained with this token.
#[derive(Debug)]
pub struct ImageReloadConsumer {
    authority: Arc<()>,
}

#[derive(Debug, Resource)]
pub(super) struct ImageReloadRegistration {
    pub(super) authority: Arc<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageReloadDrainError {
    AuthorityMismatch,
}

impl fmt::Display for ImageReloadDrainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorityMismatch => formatter
                .write_str("image reload requests were admitted for a different App registration"),
        }
    }
}

impl std::error::Error for ImageReloadDrainError {}

#[derive(Debug)]
pub enum ImageReloadRegistrationError {
    AssetPluginMissing,
    AlreadyRegistered,
    AppMutation(PluginError),
}

impl fmt::Display for ImageReloadRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetPluginMissing => {
                formatter.write_str("image reload consumer requires AssetPlugin")
            }
            Self::AlreadyRegistered => {
                formatter.write_str("image reload requests already have a registered consumer")
            }
            Self::AppMutation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ImageReloadRegistrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AppMutation(error) => Some(error),
            Self::AssetPluginMissing | Self::AlreadyRegistered => None,
        }
    }
}

#[doc(hidden)]
pub fn register_image_reload_consumer(
    app: &mut App,
) -> Result<ImageReloadConsumer, ImageReloadRegistrationError> {
    if !app.world().contains_resource::<AssetReloadRequests>() {
        return Err(ImageReloadRegistrationError::AssetPluginMissing);
    }
    if app.world().contains_resource::<ImageReloadRegistration>() {
        return Err(ImageReloadRegistrationError::AlreadyRegistered);
    }

    let authority = Arc::new(());
    app.insert_resource(ImageReloadRegistration {
        authority: Arc::clone(&authority),
    })
    .map_err(ImageReloadRegistrationError::AppMutation)?;
    Ok(ImageReloadConsumer { authority })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetLoadGeneration(u64);

impl AssetLoadGeneration {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Default, Resource)]
pub struct AssetLoadGenerations {
    generations: BTreeMap<AssetId, AssetLoadGeneration>,
}

impl AssetLoadGenerations {
    #[must_use]
    pub fn current(&self, id: AssetId) -> AssetLoadGeneration {
        self.generations
            .get(&id)
            .copied()
            .unwrap_or(AssetLoadGeneration::ZERO)
    }

    pub fn begin_request(&mut self, id: AssetId) -> AssetLoadGeneration {
        let next = AssetLoadGeneration(self.current(id).raw().saturating_add(1));
        self.generations.insert(id, next);
        next
    }

    #[must_use]
    pub fn is_current(&self, id: AssetId, generation: AssetLoadGeneration) -> bool {
        self.current(id) == generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetReloadRequestId(u64);

impl AssetReloadRequestId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetReloadRequestKind {
    LoadOrReload,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetReloadRequest {
    id: AssetReloadRequestId,
    asset_id: AssetId,
    record: AssetRecord,
    request_kind: AssetReloadRequestKind,
    source_change_kind: AssetSourceChangeKind,
    expected_version: AssetVersion,
    generation: AssetLoadGeneration,
    affected_artifacts: Vec<ImportArtifactDigest>,
    image_authority: Option<Arc<()>>,
}

impl AssetReloadRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: AssetReloadRequestId,
        asset_id: AssetId,
        record: AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        affected_artifacts: Vec<ImportArtifactDigest>,
    ) -> Self {
        Self {
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_artifacts,
            image_authority: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new_admitted_image(
        id: AssetReloadRequestId,
        asset_id: AssetId,
        record: AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        affected_artifacts: Vec<ImportArtifactDigest>,
        image_authority: Arc<()>,
    ) -> Self {
        let mut request = Self::new(
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_artifacts,
        );
        request.image_authority = Some(image_authority);
        request
    }

    #[must_use]
    pub const fn id(&self) -> AssetReloadRequestId {
        self.id
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.record.stable_id()
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        self.record.path()
    }

    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        self.record.source_kind()
    }

    #[must_use]
    pub const fn request_kind(&self) -> AssetReloadRequestKind {
        self.request_kind
    }

    #[must_use]
    pub const fn source_change_kind(&self) -> AssetSourceChangeKind {
        self.source_change_kind
    }

    #[must_use]
    pub const fn expected_version(&self) -> AssetVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn generation(&self) -> AssetLoadGeneration {
        self.generation
    }

    #[must_use]
    pub fn affected_artifacts(&self) -> &[ImportArtifactDigest] {
        &self.affected_artifacts
    }

    #[must_use]
    pub fn record(&self) -> AssetRecord {
        self.record.clone()
    }

    pub(super) fn retained_payload_bytes(&self) -> Option<usize> {
        self.record.retained_bytes()?.checked_add(
            self.affected_artifacts
                .capacity()
                .checked_mul(std::mem::size_of::<ImportArtifactDigest>())?,
        )
    }
}

/// Bounded task-update queue from source resolution to typed asset consumers.
///
/// `ResolveSourceChanges` is the sole producer. The concrete image domain drains through its
/// `ImageReloadConsumer` no later than `ApplyResults`; the engine then rejects every unclaimed
/// current-generation request, publishes diagnostics/events, and releases all queue charges in
/// the same task update. The queue is runtime request intent and is not replay-recordable.
#[derive(Debug, Resource)]
pub struct AssetReloadRequests {
    next_id: u64,
    limits: AssetReloadRequestLimits,
    retained_payload_bytes: usize,
    requests: Vec<AssetReloadRequest>,
}

impl Default for AssetReloadRequests {
    fn default() -> Self {
        Self::with_limits(AssetReloadRequestLimits::default())
    }
}

impl AssetReloadRequests {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn with_limits(limits: AssetReloadRequestLimits) -> Self {
        Self {
            next_id: 0,
            limits,
            retained_payload_bytes: 0,
            requests: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_push_resolved(
        &mut self,
        asset_id: AssetId,
        record: AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        affected_artifacts: Vec<ImportArtifactDigest>,
        image_authority: Arc<()>,
    ) -> Result<AssetReloadRequestId, AssetReloadRequestAdmissionError> {
        self.check_item_capacity()?;
        let id = AssetReloadRequestId(self.next_id);
        let request = AssetReloadRequest::new_admitted_image(
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_artifacts,
            image_authority,
        );
        let request_bytes = request.retained_payload_bytes();
        let attempted_bytes =
            request_bytes.and_then(|bytes| self.retained_payload_bytes.checked_add(bytes));
        let Some(attempted_bytes) = attempted_bytes else {
            return Err(AssetReloadRequestAdmissionError::new(
                AssetReloadRequestLimitKind::RetainedBytes,
                self.limits.retained_bytes().get(),
                None,
            ));
        };
        if attempted_bytes > self.limits.retained_bytes().get() {
            return Err(AssetReloadRequestAdmissionError::new(
                AssetReloadRequestLimitKind::RetainedBytes,
                self.limits.retained_bytes().get(),
                Some(attempted_bytes),
            ));
        }

        self.next_id = self.next_id.saturating_add(1);
        self.retained_payload_bytes = attempted_bytes;
        self.requests.push(request);
        Ok(id)
    }

    pub(super) fn check_item_capacity(&self) -> Result<(), AssetReloadRequestAdmissionError> {
        let attempted = self.requests.len().checked_add(1);
        if attempted.is_none_or(|items| items > self.limits.items().get()) {
            return Err(AssetReloadRequestAdmissionError::new(
                AssetReloadRequestLimitKind::Items,
                self.limits.items().get(),
                attempted,
            ));
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &AssetReloadRequest> {
        self.requests.iter()
    }

    #[must_use]
    pub const fn limits(&self) -> AssetReloadRequestLimits {
        self.limits
    }

    #[must_use]
    pub const fn retained_payload_bytes(&self) -> usize {
        self.retained_payload_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn drain_images(
        &mut self,
        consumer: &ImageReloadConsumer,
    ) -> Result<Vec<AssetReloadRequest>, ImageReloadDrainError> {
        if self.requests.iter().any(|request| {
            request.source_kind() == &AssetSourceKind::Image
                && request
                    .image_authority
                    .as_ref()
                    .is_none_or(|authority| !Arc::ptr_eq(authority, &consumer.authority))
        }) {
            return Err(ImageReloadDrainError::AuthorityMismatch);
        }
        let retained_payload_bytes = &mut self.retained_payload_bytes;
        Ok(self
            .requests
            .extract_if(.., |request| {
                if request.source_kind() != &AssetSourceKind::Image {
                    return false;
                }
                *retained_payload_bytes = retained_payload_bytes
                    .checked_sub(
                        request
                            .retained_payload_bytes()
                            .expect("admitted reload request retained bytes remain representable"),
                    )
                    .expect("reload request retained-byte accounting cannot underflow");
                true
            })
            .collect())
    }

    pub(super) fn drain_unclaimed(&mut self) -> Vec<AssetReloadRequest> {
        self.retained_payload_bytes = 0;
        std::mem::take(&mut self.requests)
    }
}
