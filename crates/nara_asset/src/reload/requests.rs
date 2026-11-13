use std::{any::TypeId, collections::BTreeMap, fmt, sync::Arc};

use nara_app::{App, PluginError};
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;

use crate::{
    AssetError, AssetId, AssetRecord, AssetServer, AssetSourceKind, AssetSourcePath,
    AssetStateRevision, AssetStates, AssetVersion, ImageRecoveryContinuity, ImportRecipeDigest,
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

/// Non-cloneable authority for one concrete source kind and Rust asset type in an App.
///
/// Registration routes source reload intent only; the consumer owns import execution and
/// publication. Publicly constructed requests carry no registration authority.
#[derive(Debug)]
pub struct SourceReloadConsumer {
    source_kind: AssetSourceKind,
    authority: Arc<()>,
}

impl SourceReloadConsumer {
    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        &self.source_kind
    }

    #[must_use]
    pub fn owns_request(&self, request: &AssetReloadRequest) -> bool {
        request.source_kind() == &self.source_kind
            && request
                .source_authority
                .as_ref()
                .is_some_and(|authority| Arc::ptr_eq(authority, &self.authority))
    }
}

#[derive(Debug)]
pub(super) struct SourceReloadRegistration {
    pub(super) source_kind: AssetSourceKind,
    asset_type: TypeId,
    asset_type_name: &'static str,
    pub(super) authority: Arc<()>,
    pub(super) validate_asset_type: fn(&AssetServer, AssetId) -> Result<(), AssetError>,
    pub(super) claim_asset_type: fn(&mut AssetServer, AssetId) -> Result<(), AssetError>,
    pub(super) image_recovery: bool,
}

#[derive(Debug, Default, Resource)]
pub(super) struct SourceReloadRegistrations {
    registrations: Vec<SourceReloadRegistration>,
}

impl SourceReloadRegistrations {
    pub(super) fn for_kind(
        &self,
        source_kind: &AssetSourceKind,
    ) -> Option<&SourceReloadRegistration> {
        self.registrations
            .iter()
            .find(|registration| &registration.source_kind == source_kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceReloadDrainError {
    AuthorityMismatch,
}

impl fmt::Display for SourceReloadDrainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorityMismatch => formatter
                .write_str("source reload requests were admitted for a different App registration"),
        }
    }
}

impl std::error::Error for SourceReloadDrainError {}

#[derive(Debug)]
pub enum SourceReloadRegistrationError {
    AssetSourcePluginMissing,
    AlreadyRegistered {
        source_kind: AssetSourceKind,
        asset_type: &'static str,
    },
    SourceKindConflict {
        source_kind: AssetSourceKind,
        existing_type: &'static str,
        requested_type: &'static str,
    },
    AppMutation(PluginError),
}

impl fmt::Display for SourceReloadRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetSourcePluginMissing => {
                formatter.write_str("source reload consumer requires AssetSourcePlugin")
            }
            Self::AlreadyRegistered {
                source_kind,
                asset_type,
            } => write!(
                formatter,
                "source reload consumer for {source_kind} and {asset_type} is already registered"
            ),
            Self::SourceKindConflict {
                source_kind,
                existing_type,
                requested_type,
            } => write!(
                formatter,
                "source reload kind {source_kind} is registered for {existing_type}, not {requested_type}"
            ),
            Self::AppMutation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SourceReloadRegistrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AppMutation(error) => Some(error),
            _ => None,
        }
    }
}

/// Registers routing authority without installing an importer or an execution system.
///
/// Each source kind has one consumer and concrete Rust asset type in an App. Distinct source
/// kinds may share the same Rust asset type.
pub fn register_source_reload_consumer<T: 'static>(
    app: &mut App,
    source_kind: AssetSourceKind,
) -> Result<SourceReloadConsumer, SourceReloadRegistrationError> {
    register_reload_consumer::<T>(app, source_kind, false)
}

fn register_reload_consumer<T: 'static>(
    app: &mut App,
    source_kind: AssetSourceKind,
    image_recovery: bool,
) -> Result<SourceReloadConsumer, SourceReloadRegistrationError> {
    if !app.world().contains_resource::<AssetReloadRequests>() {
        return Err(SourceReloadRegistrationError::AssetSourcePluginMissing);
    }
    let asset_type = TypeId::of::<T>();
    let asset_type_name = std::any::type_name::<T>();
    if let Some(registrations) = app.world().get_resource::<SourceReloadRegistrations>() {
        if let Some(existing) = registrations.for_kind(&source_kind) {
            return Err(if existing.asset_type == asset_type {
                SourceReloadRegistrationError::AlreadyRegistered {
                    source_kind,
                    asset_type: asset_type_name,
                }
            } else {
                SourceReloadRegistrationError::SourceKindConflict {
                    source_kind,
                    existing_type: existing.asset_type_name,
                    requested_type: asset_type_name,
                }
            });
        }
    }

    let authority = Arc::new(());
    let world = app
        .world_mut()
        .map_err(SourceReloadRegistrationError::AppMutation)?;
    world
        .get_resource_or_insert_with(SourceReloadRegistrations::default)
        .registrations
        .push(SourceReloadRegistration {
            source_kind: source_kind.clone(),
            asset_type,
            asset_type_name,
            authority: Arc::clone(&authority),
            validate_asset_type: AssetServer::validate_type_claim::<T>,
            claim_asset_type: |server, id| server.claim::<T>(id).map(|_| ()),
            image_recovery,
        });
    Ok(SourceReloadConsumer {
        source_kind,
        authority,
    })
}

/// Non-cloneable authority for the concrete first-party image reload integration.
///
/// The token is returned directly to `ImageSourcePlugin` and never stored as a public ECS resource.
/// Requests admitted for another App generation cannot be drained with this token.
#[derive(Debug)]
pub struct ImageReloadConsumer {
    consumer: SourceReloadConsumer,
}

impl ImageReloadConsumer {
    /// Captures state-owner continuity; Image separately admits successful-removal recovery.
    #[must_use]
    pub fn recovery_continuity(
        &self,
        states: &AssetStates,
        id: AssetId,
    ) -> ImageRecoveryContinuity {
        states.image_recovery_continuity(id, Arc::clone(&self.consumer.authority))
    }
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
    AssetSourcePluginMissing,
    AlreadyRegistered,
    AppMutation(PluginError),
}

impl fmt::Display for ImageReloadRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetSourcePluginMissing => {
                formatter.write_str("image reload consumer requires AssetSourcePlugin")
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
            Self::AssetSourcePluginMissing | Self::AlreadyRegistered => None,
        }
    }
}

#[doc(hidden)]
pub fn register_image_reload_consumer<T: 'static>(
    app: &mut App,
) -> Result<ImageReloadConsumer, ImageReloadRegistrationError> {
    register_reload_consumer::<T>(app, AssetSourceKind::Image, true)
        .map(|consumer| ImageReloadConsumer { consumer })
        .map_err(|error| match error {
            SourceReloadRegistrationError::AssetSourcePluginMissing => {
                ImageReloadRegistrationError::AssetSourcePluginMissing
            }
            SourceReloadRegistrationError::AppMutation(error) => {
                ImageReloadRegistrationError::AppMutation(error)
            }
            _ => ImageReloadRegistrationError::AlreadyRegistered,
        })
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
    store_identity: Arc<()>,
}

impl AssetLoadGenerations {
    pub(crate) fn store_identity(&self) -> &Arc<()> {
        &self.store_identity
    }

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

    /// Checks the original issuing store and the current generation of an admitted request.
    /// Publicly constructed requests have no issuer and cannot be current.
    #[must_use]
    pub fn is_current_request(&self, request: &AssetReloadRequest) -> bool {
        request
            .generation_issuer
            .as_ref()
            .is_some_and(|issuer| Arc::ptr_eq(issuer, &self.store_identity))
            && self.is_current(request.asset_id(), request.generation())
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

#[derive(Debug, Clone)]
pub struct AssetReloadRequest {
    id: AssetReloadRequestId,
    asset_id: AssetId,
    record: AssetRecord,
    request_kind: AssetReloadRequestKind,
    source_change_kind: AssetSourceChangeKind,
    expected_version: AssetVersion,
    previous_state: Option<(AssetStateRevision, Option<AssetVersion>)>,
    resolved_state_revision: Option<AssetStateRevision>,
    generation: AssetLoadGeneration,
    generation_issuer: Option<Arc<()>>,
    affected_recipes: Vec<ImportRecipeDigest>,
    source_authority: Option<Arc<()>>,
}

impl PartialEq for AssetReloadRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.asset_id == other.asset_id
            && self.record == other.record
            && self.request_kind == other.request_kind
            && self.source_change_kind == other.source_change_kind
            && self.expected_version == other.expected_version
            && self.previous_state == other.previous_state
            && self.resolved_state_revision == other.resolved_state_revision
            && self.generation == other.generation
            && self.affected_recipes == other.affected_recipes
            && match (&self.generation_issuer, &other.generation_issuer) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
            && match (&self.source_authority, &other.source_authority) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for AssetReloadRequest {}

impl AssetReloadRequest {
    /// Constructs request data without source registration or generation-store authority.
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
        affected_recipes: Vec<ImportRecipeDigest>,
    ) -> Self {
        Self {
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            previous_state: None,
            resolved_state_revision: None,
            generation,
            generation_issuer: None,
            affected_recipes,
            source_authority: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new_admitted_source(
        id: AssetReloadRequestId,
        asset_id: AssetId,
        record: AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        generation_issuer: Arc<()>,
        affected_recipes: Vec<ImportRecipeDigest>,
        source_authority: Arc<()>,
        previous_state: Option<(AssetStateRevision, Option<AssetVersion>)>,
    ) -> Self {
        let mut request = Self::new(
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_recipes,
        );
        request.generation_issuer = Some(generation_issuer);
        request.source_authority = Some(source_authority);
        request.previous_state = previous_state;
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
    pub const fn path(&self) -> &AssetSourcePath {
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

    #[doc(hidden)]
    #[must_use]
    pub fn previous_state_revision(&self) -> Option<&AssetStateRevision> {
        self.previous_state.as_ref().map(|(revision, _)| revision)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn previous_usable_version(&self) -> Option<AssetVersion> {
        self.previous_state
            .as_ref()
            .and_then(|(_, version)| *version)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn resolved_state_revision(&self) -> Option<&AssetStateRevision> {
        self.resolved_state_revision.as_ref()
    }

    pub(super) fn set_resolved_state_revision(&mut self, revision: AssetStateRevision) {
        self.resolved_state_revision = Some(revision);
    }

    #[must_use]
    pub const fn generation(&self) -> AssetLoadGeneration {
        self.generation
    }

    #[must_use]
    pub fn affected_recipes(&self) -> &[ImportRecipeDigest] {
        &self.affected_recipes
    }

    #[must_use]
    pub fn record(&self) -> AssetRecord {
        self.record.clone()
    }

    /// Returns the dynamic payload bytes retained by this request without allocating.
    ///
    /// Counts the owned record's string capacities and the affected-recipe vector's full
    /// capacity, including unused storage. The inline request value and shared registration
    /// authority are excluded. `None` indicates that the byte total cannot fit in `usize`.
    #[must_use]
    pub fn retained_payload_bytes(&self) -> Option<usize> {
        self.record.retained_bytes()?.checked_add(
            self.affected_recipes
                .capacity()
                .checked_mul(std::mem::size_of::<ImportRecipeDigest>())?,
        )
    }
}

/// Bounded task-update queue from source resolution to typed asset consumers.
///
/// `ResolveSourceChanges` is the sole producer. Concrete domains drain through their registered
/// consumer no later than `ApplyResults`; the engine then rejects every unclaimed
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
    pub(super) fn try_push_resolved(
        &mut self,
        asset_id: AssetId,
        record: AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        generation_issuer: Arc<()>,
        affected_recipes: Vec<ImportRecipeDigest>,
        source_authority: Arc<()>,
        previous_state: Option<(AssetStateRevision, Option<AssetVersion>)>,
    ) -> Result<&mut AssetReloadRequest, AssetReloadRequestAdmissionError> {
        let attempted_bytes = self.check_capacity_for(&record, affected_recipes.capacity())?;
        let id = AssetReloadRequestId(self.next_id);
        let request = AssetReloadRequest::new_admitted_source(
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            generation_issuer,
            affected_recipes,
            source_authority,
            previous_state,
        );

        self.next_id = self.next_id.saturating_add(1);
        self.retained_payload_bytes = attempted_bytes;
        let request_index = self.requests.len();
        self.requests.push(request);
        Ok(&mut self.requests[request_index])
    }

    pub(super) fn check_capacity_for(
        &self,
        record: &AssetRecord,
        affected_recipe_capacity: usize,
    ) -> Result<usize, AssetReloadRequestAdmissionError> {
        self.check_item_capacity()?;
        let attempted_bytes = record
            .retained_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    affected_recipe_capacity
                        .checked_mul(std::mem::size_of::<ImportRecipeDigest>())?,
                )
            })
            .and_then(|bytes| self.retained_payload_bytes.checked_add(bytes));
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

        Ok(attempted_bytes)
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
        self.drain_source(&consumer.consumer)
            .map_err(|_| ImageReloadDrainError::AuthorityMismatch)
    }

    /// Drains only this consumer's source kind, rejecting foreign authority without mutation.
    pub fn drain_source(
        &mut self,
        consumer: &SourceReloadConsumer,
    ) -> Result<Vec<AssetReloadRequest>, SourceReloadDrainError> {
        if self.requests.iter().any(|request| {
            request.source_kind() == consumer.source_kind() && !consumer.owns_request(request)
        }) {
            return Err(SourceReloadDrainError::AuthorityMismatch);
        }
        let retained_payload_bytes = &mut self.retained_payload_bytes;
        Ok(self
            .requests
            .extract_if(.., |request| {
                if request.source_kind() != consumer.source_kind() {
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
