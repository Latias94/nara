//! Async image reload orchestration and plugin integration.

use crate::import::ImagePublicationSnapshot;
use crate::{
    ImageAsset, ImageFileImportRequest, ImageImportBudgetHost, ImageImportError, ImageImportLimits,
    ImageImportSettings, ImageImportedAsset, ImageImporter, ImageImporterCreateError,
    ImagePublicationFailureKind, ImageSourceDirectory,
};

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_app::{App, CoreStage, Plugin, PluginError, RealTime};
#[cfg(feature = "serde")]
use nara_asset::AssetMetaIdentityCandidate;
use nara_asset::{
    AssetEvents, AssetId, AssetLoadGeneration, AssetLoadGenerations, AssetPath,
    AssetPublicationRevision, AssetRecord, AssetReloadDiagnostics, AssetReloadRequest,
    AssetReloadRequestKind, AssetReloadRequests, AssetServer, AssetSlotRevision,
    AssetSourceChangeKind, AssetStateError, AssetStateRevision, AssetStates, AssetTaskUpdateSet,
    AssetVersion, Assets, Handle, ImageRecoveryContinuity, ImageReloadConsumer,
    ImageReloadDrainError, ImageReloadRegistrationError, register_image_reload_consumer,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};
use nara_ecs::{
    Res, ResMut, Resource, SystemSet, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs,
};
use nara_tasks::{
    OrderedTaskReadySnapshot, OrderedTaskResults, TaskCancellation, TaskCancellationReason,
    TaskCoalesceKey, TaskCompletionCutoff, TaskCompletionCutoffError, TaskDomainKey, TaskFailure,
    TaskHandle, TaskOrderKey, TaskOverloadPolicy, TaskPoolKind, TaskPools, TaskReadySnapshotError,
    TaskRejection, TaskSpawnOutcome, TaskSpawnRequest, TaskTerminal,
};

pub(super) const IMAGE_RELOAD_TASK_DOMAIN: TaskDomainKey =
    TaskDomainKey::new(0x6e61_7261_696d_6167);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReloadError {
    MissingSourceDirectory,
    MissingImportSettings,
    MetadataSupportUnavailable,
    MetadataInvalid,
    MetadataIdentityMismatch,
    InvalidSourceChange,
    Import(ImageImportError),
    TaskRejected(TaskRejection),
    TaskCancelled(TaskCancellation),
    TaskFailed(TaskFailure),
    TaskTracking,
    Publication(ImagePublicationFailureKind),
    Asset(nara_asset::AssetError),
}

impl Display for ImageReloadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSourceDirectory => formatter.write_str("image source authority is absent"),
            Self::MissingImportSettings => {
                formatter.write_str("image reload has no accepted import settings")
            }
            Self::MetadataSupportUnavailable => {
                formatter.write_str("image metadata reload support is unavailable")
            }
            Self::MetadataInvalid => formatter.write_str("image metadata is invalid"),
            Self::MetadataIdentityMismatch => {
                formatter.write_str("image metadata identity does not match the reload target")
            }
            Self::InvalidSourceChange => {
                formatter.write_str("image reload received an invalid source-change kind")
            }
            Self::Import(error) => Display::fmt(error, formatter),
            Self::TaskRejected(_) => formatter.write_str("image import task was rejected"),
            Self::TaskCancelled(_) => formatter.write_str("image import task was cancelled"),
            Self::TaskFailed(_) => formatter.write_str("image import task failed"),
            Self::TaskTracking => formatter.write_str("image import task tracking failed"),
            Self::Publication(error) => Display::fmt(error, formatter),
            Self::Asset(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ImageReloadError {}

impl ImageReloadError {
    const fn stable_code(&self) -> &'static str {
        match self {
            Self::MissingSourceDirectory => "image.reload-source-authority-missing",
            Self::MissingImportSettings => "image.reload-settings-missing",
            Self::MetadataSupportUnavailable => "image.reload-metadata-support-unavailable",
            Self::MetadataInvalid => "image.reload-metadata-invalid",
            Self::MetadataIdentityMismatch => "image.reload-metadata-identity-mismatch",
            Self::InvalidSourceChange => "image.reload-source-change-invalid",
            Self::Import(error) => error.stable_code(),
            Self::TaskRejected(_) => "image.reload-task-rejected",
            Self::TaskCancelled(_) => "image.reload-task-cancelled",
            Self::TaskFailed(_) => "image.reload-task-failed",
            Self::TaskTracking => "image.reload-task-tracking-failed",
            Self::Publication(_) => "image.reload-publication-failed",
            Self::Asset(_) => "image.reload-asset-identity-failed",
        }
    }

    const fn safe_summary(&self) -> &'static str {
        match self {
            Self::MissingSourceDirectory => "Image source authority is absent",
            Self::MissingImportSettings => "Image reload settings are absent",
            Self::MetadataSupportUnavailable => "Image metadata reload support is unavailable",
            Self::MetadataInvalid => "Image metadata is invalid",
            Self::MetadataIdentityMismatch => {
                "Image metadata identity does not match the reload target"
            }
            Self::InvalidSourceChange => "Image reload source change is invalid",
            Self::Import(error) => error.safe_summary(),
            Self::TaskRejected(_) => "Image import task was rejected",
            Self::TaskCancelled(_) => "Image import task was cancelled",
            Self::TaskFailed(_) => "Image import task failed",
            Self::TaskTracking => "Image import task tracking failed",
            Self::Publication(_) => "Image publication failed",
            Self::Asset(_) => "Image asset identity validation failed",
        }
    }
}

#[derive(Debug, Default, Resource)]
pub struct ImageReloadStats {
    pub spawned: u32,
    pub rejected: u32,
    pub applied: u32,
    pub failed: u32,
    pub stale: u32,
    pub cancelled: u32,
    pub removed: u32,
    pub pending: u32,
}

pub(super) type ImageImportTaskResult = Result<ImageImportedAsset, ImageReloadError>;

#[derive(Default)]
pub(super) struct PendingImageImportStream {
    generation: Option<AssetLoadGeneration>,
    ordered: OrderedTaskResults<ImageImportTaskResult>,
    attempts: BTreeMap<TaskOrderKey, ImageReloadAttempt>,
    retired_attempts: usize,
}

impl PendingImageImportStream {
    pub(super) fn push(
        &mut self,
        attempt: ImageReloadAttempt,
        handle: TaskHandle<ImageImportTaskResult>,
    ) -> Result<(), Box<(ImageReloadAttempt, TaskHandle<ImageImportTaskResult>)>> {
        let order_key = handle.order_key();
        let generation = attempt.request.generation();
        if self.generation.is_some_and(|current| current > generation) {
            return Err(Box::new((attempt, handle)));
        }
        self.retire_superseded(generation);
        match self.ordered.push(handle) {
            Ok(()) => {
                self.generation = Some(generation);
                self.attempts.insert(order_key, attempt);
                Ok(())
            }
            Err(handle) => Err(Box::new((attempt, handle))),
        }
    }

    fn retire_superseded(&mut self, desired: AssetLoadGeneration) {
        if self.generation.is_some_and(|tracked| tracked != desired) {
            self.retired_attempts = self.retired_attempts.saturating_add(self.attempts.len());
            self.ordered = OrderedTaskResults::default();
            self.attempts.clear();
            self.generation = None;
        }
    }

    fn take_retired_attempts(&mut self) -> usize {
        std::mem::take(&mut self.retired_attempts)
    }

    fn validate_completion_cutoff(
        &self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<(), TaskCompletionCutoffError> {
        self.ordered.validate_completion_cutoff(cutoff)
    }

    pub(super) fn capture_ready_prefix(
        &mut self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<OrderedTaskReadySnapshot<ImageImportTaskResult>, TaskCompletionCutoffError> {
        self.ordered.capture_ready_prefix(cutoff)
    }

    fn consume_ready_terminals(
        &mut self,
        terminals: Vec<nara_tasks::OrderedTaskTerminal<ImageImportTaskResult>>,
    ) -> Vec<ReadyImageImportJob> {
        terminals
            .into_iter()
            .filter_map(|ordered| {
                let attempt = self.attempts.remove(&ordered.order_key)?;
                Some(ReadyImageImportJob {
                    attempt,
                    order_key: Some(ordered.order_key),
                    outcome: ReadyImageImportOutcome::Terminal(Box::new(ordered.terminal)),
                })
            })
            .collect()
    }

    pub(super) fn len(&self) -> usize {
        self.ordered.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }
}

#[derive(Default, Resource)]
pub(super) struct PendingImageJobs {
    pub(super) imports: BTreeMap<AssetId, PendingImageImportStream>,
}

impl PendingImageJobs {
    fn len(&self) -> usize {
        self.imports
            .values()
            .map(PendingImageImportStream::len)
            .sum()
    }

    fn retire_superseded(&mut self, generations: &AssetLoadGenerations) -> u32 {
        let mut retired = 0_u32;
        for (&asset_id, stream) in &mut self.imports {
            stream.retire_superseded(generations.current(asset_id));
            retired = retired
                .saturating_add(stream.take_retired_attempts().min(u32::MAX as usize) as u32);
        }
        self.imports.retain(|_, stream| !stream.is_empty());
        retired
    }

    pub(super) fn capture_ready_prefixes(
        &mut self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<
        Vec<(AssetId, OrderedTaskReadySnapshot<ImageImportTaskResult>)>,
        TaskCompletionCutoffError,
    > {
        for stream in self.imports.values() {
            stream.validate_completion_cutoff(cutoff)?;
        }
        self.imports
            .iter_mut()
            .map(|(&asset_id, stream)| {
                stream
                    .capture_ready_prefix(cutoff)
                    .map(|snapshot| (asset_id, snapshot))
            })
            .collect()
    }

    pub(super) fn consume_ready_prefixes(
        &mut self,
        snapshots: Vec<(AssetId, OrderedTaskReadySnapshot<ImageImportTaskResult>)>,
    ) -> Result<Vec<ReadyImageImportJob>, TaskReadySnapshotError> {
        let terminals = snapshots
            .into_iter()
            .map(|(asset_id, snapshot)| {
                snapshot
                    .into_terminals()
                    .map(|terminals| (asset_id, terminals))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut ready = Vec::new();
        for (asset_id, terminals) in terminals {
            if let Some(stream) = self.imports.get_mut(&asset_id) {
                ready.extend(stream.consume_ready_terminals(terminals));
            }
        }
        self.imports.retain(|_, stream| !stream.is_empty());
        Ok(ready)
    }
}

enum ReadyImageImportOutcome {
    Terminal(Box<TaskTerminal<ImageImportTaskResult>>),
    Rejected(TaskRejection),
    ImmediateFailure(ImageReloadError),
    TrackingFailure,
}

#[derive(Debug)]
pub(super) struct ImageReloadAttempt {
    request: AssetReloadRequest,
    handle: Handle<ImageAsset>,
    publication: ImagePublicationSnapshot,
    settings: Option<ImageImportSettings>,
    recovery: Option<RemovedImageRecovery>,
}

impl ImageReloadAttempt {
    pub(super) fn capture(
        request: AssetReloadRequest,
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, ImagePublicationFailureKind> {
        let (publication, settings) = ImagePublicationSnapshot::capture_with_settings(
            &request.record(),
            handle,
            request.expected_version(),
            images,
            states,
        )?;
        Ok(Self {
            request,
            handle,
            publication,
            settings,
            recovery: None,
        })
    }

    fn is_current(
        &self,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        self.publication.is_current(asset_server, images, states)
    }
}

pub(super) struct ReadyImageImportJob {
    attempt: ImageReloadAttempt,
    pub(super) order_key: Option<TaskOrderKey>,
    outcome: ReadyImageImportOutcome,
}

impl ReadyImageImportJob {
    pub(super) fn sort_key(&self) -> (u64, u64, u64, u64) {
        let (admission_tick, domain_key, task_id) = self.order_key.map_or_else(
            || match &self.outcome {
                ReadyImageImportOutcome::Rejected(rejection) => (
                    rejection.admission_tick,
                    rejection.domain_key.raw(),
                    u64::MAX,
                ),
                ReadyImageImportOutcome::Terminal(_)
                | ReadyImageImportOutcome::ImmediateFailure(_)
                | ReadyImageImportOutcome::TrackingFailure => (u64::MAX, u64::MAX, u64::MAX),
            },
            |key| {
                (
                    key.admission_tick(),
                    key.domain_key().raw(),
                    key.task_id().raw(),
                )
            },
        );
        (
            admission_tick,
            domain_key,
            task_id,
            self.attempt.request.id().raw(),
        )
    }
}

#[derive(Default, Resource)]
struct ReadyImageJobs {
    imports: Vec<ReadyImageImportJob>,
    removals: Vec<ImageReloadAttempt>,
}

#[derive(Default, Resource)]
struct RemovedImageRecoveries {
    images: BTreeMap<AssetId, RemovedImageRecovery>,
}

#[derive(Debug, Clone)]
struct RemovedImageRecovery {
    handle: Handle<ImageAsset>,
    record: AssetRecord,
    removed_version: AssetVersion,
    empty_slot: AssetSlotRevision,
    continuity: ImageRecoveryContinuity,
}

impl RemovedImageRecovery {
    fn capture(
        consumer: &ImageReloadConsumer,
        record: AssetRecord,
        handle: Handle<ImageAsset>,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Option<Self> {
        let recovery = Self {
            handle,
            record,
            removed_version: states.version(handle.id())?,
            empty_slot: images.slot_revision(handle),
            continuity: consumer.recovery_continuity(states, handle.id()),
        };
        recovery
            .is_current(server, images, states)
            .then_some(recovery)
    }

    fn is_current(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        self.continuity.is_current(states, self.handle.id())
            && states.version(self.handle.id()) == Some(self.removed_version)
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_none())
            && images.get(self.handle).is_none()
            && images.slot_revision(self.handle) == self.empty_slot
            && images.mutation_baseline(self.handle, states).is_ok()
            && self.record.source_kind() == &nara_asset::AssetSourceKind::Image
            && server.get_handle::<ImageAsset>(self.handle.id()) == Some(self.handle)
            && server.stable_id(self.handle.id()) == Some(self.record.stable_id())
            && server.path(self.handle.id()) == Some(self.record.path().as_str())
    }
}

#[derive(Resource)]
struct ImageReloadAuthority(ImageReloadConsumer);

#[cfg(all(test, feature = "serde"))]
pub(super) fn recovery_facts_for_tests(
    world: &nara_ecs::World,
    handle: Handle<ImageAsset>,
) -> (bool, bool) {
    let eligible = world
        .resource::<RemovedImageRecoveries>()
        .images
        .get(&handle.id())
        .is_some_and(|recovery| {
            recovery.is_current(
                world.resource::<AssetServer>(),
                world.resource::<Assets<ImageAsset>>(),
                world.resource::<AssetStates>(),
            )
        });
    let retained_recipe = world
        .resource::<DesiredImageSettings>()
        .images
        .contains_key(&handle.id());
    (eligible, retained_recipe)
}

#[derive(Default, Resource)]
struct DesiredImageSettings {
    images: BTreeMap<AssetId, DesiredImageRecipe>,
    revoked: BTreeMap<AssetId, (AssetSlotRevision, Option<AssetStateRevision>)>,
}

struct DesiredImageRecipe {
    settings: ImageImportSettings,
    publication: AssetPublicationRevision,
    // A successful publication satisfies this intent; later requests seed from its Image instead.
    slot: AssetSlotRevision,
    metadata_observation: Option<AssetStateRevision>,
}

impl DesiredImageRecipe {
    fn capture(
        settings: ImageImportSettings,
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Self {
        Self {
            settings,
            publication: states.publication_revision(handle.id()),
            slot: images.slot_revision(handle),
            metadata_observation: states.metadata_change_revision(handle.id()).cloned(),
        }
    }

    fn is_current(
        &self,
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        self.publication == states.publication_revision(handle.id())
            && self.slot == images.slot_revision(handle)
            && self.metadata_observation.as_ref() == states.metadata_change_revision(handle.id())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub(crate) enum ImageReloadInternalSet {
    ApplyResults,
}

/// Installs native Image storage without importing, watching, or reading source files.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImagePlugin;

pub const IMAGE_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.image");
const IMAGE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[nara_asset::ASSET_PLUGIN_ID];
const IMAGE_PLUGIN_PRODUCT_ALTERNATIVES: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("runtime-2d"),
    nara_app::PluginProductCapability::new("presentation-ui"),
];
pub const IMAGE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(IMAGE_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(IMAGE_PLUGIN_REQUIREMENTS)
        .requires_any_product_capabilities(IMAGE_PLUGIN_PRODUCT_ALTERNATIVES);

impl Plugin for ImagePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &IMAGE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<ImageAsset>>()?;
        Ok(())
    }
}

/// Installs explicit source Image reloads using Tasks-owned import execution.
///
/// File access requires a Host-supplied [`ImageSourceDirectory`]. A queued import without that
/// authority fails with [`ImageReloadError::MissingSourceDirectory`]; it is never silently skipped.
#[derive(Debug, Clone, Default)]
pub struct ImageSourcePlugin {
    importer: ImageImporter,
    source_directory: Option<Arc<ImageSourceDirectory>>,
}

pub const IMAGE_SOURCE_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.image.source");
const IMAGE_SOURCE_PLUGIN_DEFINITION_ID: nara_app::PluginDefinitionId =
    nara_app::PluginDefinitionId::new("nara.image.source.configured", 1);
pub const IMAGE_SOURCE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(IMAGE_SOURCE_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(&[
            IMAGE_PLUGIN_ID,
            nara_asset::ASSET_SOURCE_PLUGIN_ID,
            nara_tasks::TASK_PLUGIN_ID,
        ]);

/// Creates a repeatable source Image plugin definition from pure import limits.
///
/// Select [`ImagePlugin`], [`nara_asset::AssetSourcePlugin`], and [`nara_tasks::TaskPlugin`]
/// separately. This definition grants no source-directory authority.
#[must_use]
pub fn source_plugin(limits: ImageImportLimits) -> nara_app::PluginDefinition {
    let canonical_configuration = image_source_plugin_configuration(limits);
    nara_app::PluginDefinition::fallible::<ImageSourcePlugin, _>(
        IMAGE_SOURCE_PLUGIN_DEFINITION_ID,
        canonical_configuration,
        move || {
            ImageSourcePlugin::with_limits(limits).map_err(|_| {
                nara_app::PluginDomainError::new(
                    "nara.image.invalid-import-limits",
                    "Image import limits are invalid",
                )
                .expect("image preparation uses safe static failure facts")
            })
        },
    )
}

fn image_source_plugin_configuration(limits: ImageImportLimits) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(b"nara.image.source.plugin-config.v1\0");
    bytes.extend_from_slice(&(limits.max_encoded_bytes().get() as u64).to_le_bytes());
    bytes.extend_from_slice(&limits.max_width().get().to_le_bytes());
    bytes.extend_from_slice(&limits.max_height().get().to_le_bytes());
    bytes.extend_from_slice(&limits.max_pixels().get().to_le_bytes());
    bytes.extend_from_slice(&(limits.max_rgba_bytes().get() as u64).to_le_bytes());
    bytes.extend_from_slice(&(limits.max_decoder_work_bytes().get() as u64).to_le_bytes());
    bytes.extend_from_slice(&(limits.max_in_flight_bytes().get() as u64).to_le_bytes());
    bytes
}

impl ImageSourcePlugin {
    pub fn with_limits(limits: ImageImportLimits) -> Result<Self, ImageImporterCreateError> {
        Ok(Self {
            importer: ImageImporter::with_limits(limits)?,
            source_directory: None,
        })
    }

    pub fn with_budget_host(
        limits: ImageImportLimits,
        budget_host: ImageImportBudgetHost,
    ) -> Result<Self, ImageImporterCreateError> {
        Ok(Self {
            importer: ImageImporter::with_budget_host(limits, budget_host)?,
            source_directory: None,
        })
    }

    #[must_use]
    pub fn with_source_directory(mut self, source_directory: ImageSourceDirectory) -> Self {
        self.source_directory = Some(Arc::new(source_directory));
        self
    }
}

impl Plugin for ImageSourcePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &IMAGE_SOURCE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ImageReloadStats>()?;
        app.init_resource::<PendingImageJobs>()?;
        app.init_resource::<ReadyImageJobs>()?;
        app.init_resource::<RemovedImageRecoveries>()?;
        app.init_resource::<DesiredImageSettings>()?;
        app.insert_resource(self.importer.clone())?;
        let reload_consumer = register_image_reload_consumer::<ImageAsset>(app)
            .map_err(image_reload_registration_plugin_error)?;
        app.insert_resource(ImageReloadAuthority(reload_consumer))?;
        app.add_systems(
            CoreStage::TaskUpdate,
            poll_image_reload_results.in_set(AssetTaskUpdateSet::Poll),
        )?;
        let source_directory = self.source_directory.clone();
        app.add_systems(
            CoreStage::TaskUpdate,
            (move |requests: ResMut<AssetReloadRequests>,
                   authority: Res<ImageReloadAuthority>,
                   importer: Res<ImageImporter>,
                   asset_server: ResMut<AssetServer>,
                   images: Res<Assets<ImageAsset>>,
                   states: Res<AssetStates>,
                   generations: Res<AssetLoadGenerations>,
                   task_pools: Res<TaskPools>,
                   real_time: Res<RealTime>,
                   pending: ResMut<PendingImageJobs>,
                   ready: ResMut<ReadyImageJobs>,
                   recoveries: ResMut<RemovedImageRecoveries>,
                   desired: ResMut<DesiredImageSettings>,
                   diagnostics: ResMut<AssetReloadDiagnostics>,
                   stats: ResMut<ImageReloadStats>| {
                spawn_image_reload_jobs(
                    source_directory.as_deref(),
                    requests,
                    &authority.0,
                    importer,
                    asset_server,
                    images,
                    states,
                    generations,
                    task_pools,
                    real_time,
                    pending,
                    ready,
                    recoveries,
                    desired,
                    diagnostics,
                    stats,
                )
                .map_err(BevyError::error)
            })
            .in_set(AssetTaskUpdateSet::SpawnJobs),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            apply_image_reload_results
                .in_set(AssetTaskUpdateSet::ApplyResults)
                .in_set(ImageReloadInternalSet::ApplyResults),
        )?;
        Ok(())
    }
}

fn image_reload_registration_plugin_error(error: ImageReloadRegistrationError) -> PluginError {
    match error {
        ImageReloadRegistrationError::AppMutation(error) => error,
        error => PluginError::SetupFailed {
            plugin: IMAGE_SOURCE_PLUGIN_ID,
            message: error.to_string(),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_image_reload_jobs(
    source_directory: Option<&ImageSourceDirectory>,
    mut requests: ResMut<AssetReloadRequests>,
    consumer: &ImageReloadConsumer,
    importer: Res<ImageImporter>,
    mut asset_server: ResMut<AssetServer>,
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    generations: Res<AssetLoadGenerations>,
    task_pools: Res<TaskPools>,
    real_time: Res<RealTime>,
    mut pending: ResMut<PendingImageJobs>,
    mut ready: ResMut<ReadyImageJobs>,
    mut recoveries: ResMut<RemovedImageRecoveries>,
    mut desired: ResMut<DesiredImageSettings>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
    mut stats: ResMut<ImageReloadStats>,
) -> Result<(), ImageReloadDrainError> {
    stats.stale = stats
        .stale
        .saturating_add(pending.retire_superseded(&generations));
    for request in requests.drain_images(consumer)? {
        let previous_recovery = recoveries.images.remove(&request.asset_id());
        if request
            .resolved_state_revision()
            .is_none_or(|expected| expected != &states.state_revision(request.asset_id()))
        {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let handle = match asset_server.claim::<ImageAsset>(request.asset_id()) {
            Ok(handle) => handle,
            Err(error) => {
                diagnostics.push(image_reload_diagnostic(
                    request.path(),
                    &ImageReloadError::Asset(error),
                ));
                stats.failed = stats.failed.saturating_add(1);
                continue;
            }
        };
        let previous_recovery = previous_recovery.filter(|recovery| {
            recovery.is_current(&asset_server, &images, &states)
                && recovery.removed_version == request.expected_version()
                && recovery.record == request.record()
        });
        let recovering_removed = previous_recovery.is_some();
        let mut attempt = match ImageReloadAttempt::capture(request, handle, &images, &states) {
            Ok(attempt) => attempt,
            Err(_) => {
                stats.stale = stats.stale.saturating_add(1);
                continue;
            }
        };
        if attempt.request.request_kind() == AssetReloadRequestKind::Remove
            || desired
                .images
                .get(&handle.id())
                .is_some_and(|recipe| !recipe.is_current(handle, &images, &states))
        {
            desired.images.remove(&handle.id());
        }
        if desired
            .revoked
            .get(&handle.id())
            .is_some_and(|(slot, observation)| {
                slot != &images.slot_revision(handle)
                    || observation.as_ref() != states.metadata_change_revision(handle.id())
            })
        {
            desired.revoked.remove(&handle.id());
        }
        if let Some(recipe) = desired.images.get(&handle.id()) {
            attempt.settings = Some(recipe.settings);
        }
        attempt.recovery = previous_recovery.clone();
        if let Some(recovery) = previous_recovery {
            recoveries.images.insert(handle.id(), recovery);
        }
        match attempt.request.request_kind() {
            AssetReloadRequestKind::Remove => ready.removals.push(attempt),
            AssetReloadRequestKind::LoadOrReload => {
                let record = attempt.request.record();
                let asset_id = attempt.request.asset_id();
                let source_only =
                    attempt.request.source_change_kind() == AssetSourceChangeKind::Modified;
                let settings = if source_only
                    && (desired.revoked.contains_key(&asset_id)
                        || (states.source_removal_pending(asset_id)
                            && !recovering_removed
                            && !desired.images.contains_key(&asset_id)
                            && !states.metadata_change_after_source_removal(asset_id)))
                {
                    Err(ImageReloadError::MissingImportSettings)
                } else if source_only
                    && states.metadata_change_pending(asset_id)
                    && !desired.images.contains_key(&asset_id)
                {
                    admit_metadata_reload_settings(source_directory, &record)
                } else {
                    accepted_reload_settings(&attempt, source_directory, &record)
                };
                if let Ok(settings) = settings.as_ref() {
                    desired.images.insert(
                        asset_id,
                        DesiredImageRecipe::capture(*settings, handle, &images, &states),
                    );
                    desired.revoked.remove(&asset_id);
                } else if matches!(&settings, Err(ImageReloadError::MetadataIdentityMismatch)) {
                    desired.revoked.insert(
                        asset_id,
                        (
                            images.slot_revision(handle),
                            states.metadata_change_revision(asset_id).cloned(),
                        ),
                    );
                }
                let admitted = source_directory
                    .ok_or(ImageReloadError::MissingSourceDirectory)
                    .and_then(|source_directory| {
                        settings.map(|settings| (source_directory, settings))
                    })
                    .and_then(|(source_directory, settings)| {
                        source_directory
                            .open(&record)
                            .map(|file| (file, settings))
                            .map_err(ImageReloadError::Import)
                    })
                    .and_then(|(file, settings)| {
                        importer
                            .admit_file_with_snapshot(
                                ImageFileImportRequest::new(record.clone(), file, settings),
                                attempt.publication.clone(),
                                &asset_server,
                                &images,
                                &states,
                            )
                            .map_err(ImageReloadError::Import)
                    });
                let admitted = match admitted {
                    Ok(admitted) => admitted,
                    Err(error) => {
                        ready.imports.push(ReadyImageImportJob {
                            attempt,
                            order_key: None,
                            outcome: ReadyImageImportOutcome::ImmediateFailure(error),
                        });
                        continue;
                    }
                };
                let spawn_request =
                    TaskSpawnRequest::new(real_time.frame, IMAGE_RELOAD_TASK_DOMAIN).with_overload(
                        TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(asset_id.raw())),
                    );
                let outcome =
                    task_pools.spawn(TaskPoolKind::Io, spawn_request, move |cancellation| {
                        admitted
                            .import(cancellation)
                            .map_err(ImageReloadError::Import)
                    });
                match outcome {
                    TaskSpawnOutcome::Accepted(handle)
                    | TaskSpawnOutcome::Coalesced { handle, .. } => {
                        let stream = pending.imports.entry(asset_id).or_default();
                        if let Err(untracked) = stream.push(attempt, handle) {
                            let (attempt, mut handle) = *untracked;
                            handle.cancel();
                            let order_key = handle.order_key();
                            let _ = handle.try_take();
                            ready.imports.push(ReadyImageImportJob {
                                attempt,
                                order_key: Some(order_key),
                                outcome: ReadyImageImportOutcome::TrackingFailure,
                            });
                        } else {
                            stats.spawned = stats.spawned.saturating_add(1);
                        }
                    }
                    TaskSpawnOutcome::Rejected(rejection) => {
                        stats.rejected = stats.rejected.saturating_add(1);
                        ready.imports.push(ReadyImageImportJob {
                            attempt,
                            order_key: rejection.task.map(|task| task.order_key()),
                            outcome: ReadyImageImportOutcome::Rejected(rejection),
                        });
                    }
                }
            }
        }
    }
    recoveries
        .images
        .retain(|_, snapshot| snapshot.is_current(&asset_server, &images, &states));
    stats.pending = pending.len().min(u32::MAX as usize) as u32;
    Ok(())
}

fn accepted_reload_settings(
    attempt: &ImageReloadAttempt,
    source_directory: Option<&ImageSourceDirectory>,
    record: &nara_asset::AssetRecord,
) -> Result<ImageImportSettings, ImageReloadError> {
    match attempt.request.source_change_kind() {
        AssetSourceChangeKind::Modified => match attempt.settings {
            Some(settings) => Ok(settings),
            None if attempt.recovery.is_some() => {
                admit_metadata_reload_settings(source_directory, record)
            }
            None => Err(ImageReloadError::MissingImportSettings),
        },
        AssetSourceChangeKind::MetaModified => {
            admit_metadata_reload_settings(source_directory, record)
        }
        AssetSourceChangeKind::Removed => Err(ImageReloadError::InvalidSourceChange),
    }
}

#[cfg(feature = "serde")]
fn admit_metadata_reload_settings(
    source_directory: Option<&ImageSourceDirectory>,
    record: &nara_asset::AssetRecord,
) -> Result<ImageImportSettings, ImageReloadError> {
    let source_directory = source_directory.ok_or(ImageReloadError::MissingSourceDirectory)?;
    let bytes = source_directory
        .read_metadata_bytes(record)
        .map_err(ImageReloadError::Import)?;
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
        .map_err(|_| ImageReloadError::MetadataInvalid)?;
    let identity = candidate.identity();
    if identity.stable_id() != record.stable_id()
        || identity.path() != record.path()
        || identity.source_kind() != record.source_kind()
    {
        return Err(ImageReloadError::MetadataIdentityMismatch);
    }
    candidate
        .admit::<ImageImportSettings>()
        .map(|candidate| candidate.into_meta().into_parts().1)
        .map_err(|_| ImageReloadError::MetadataInvalid)
}

#[cfg(not(feature = "serde"))]
fn admit_metadata_reload_settings(
    source_directory: Option<&ImageSourceDirectory>,
    _record: &nara_asset::AssetRecord,
) -> Result<ImageImportSettings, ImageReloadError> {
    source_directory.ok_or(ImageReloadError::MissingSourceDirectory)?;
    Err(ImageReloadError::MetadataSupportUnavailable)
}

fn poll_image_reload_results(
    task_pools: Res<TaskPools>,
    generations: Res<AssetLoadGenerations>,
    mut pending: ResMut<PendingImageJobs>,
    mut ready: ResMut<ReadyImageJobs>,
    mut stats: ResMut<ImageReloadStats>,
) -> Result<(), BevyError> {
    let cutoff = task_pools.capture_completion_cutoff();
    stats.stale = stats
        .stale
        .saturating_add(pending.retire_superseded(&generations));
    let snapshots = pending
        .capture_ready_prefixes(&cutoff)
        .map_err(BevyError::error)?;
    ready.imports.extend(
        pending
            .consume_ready_prefixes(snapshots)
            .map_err(BevyError::error)?,
    );
    stats.pending = pending.len().min(u32::MAX as usize) as u32;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_image_reload_results(
    authority: Res<ImageReloadAuthority>,
    mut ready: ResMut<ReadyImageJobs>,
    mut recoveries: ResMut<RemovedImageRecoveries>,
    pending: Res<PendingImageJobs>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
    generations: Res<AssetLoadGenerations>,
    mut stats: ResMut<ImageReloadStats>,
) {
    let mut removals = std::mem::take(&mut ready.removals);
    removals.sort_by_key(|attempt| attempt.request.id());
    for attempt in removals {
        let request = &attempt.request;
        if !generations.is_current(request.asset_id(), request.generation())
            || !attempt.is_current(&asset_server, &images, &states)
        {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let restore_eligible = attempt.recovery.is_some()
            || (request.previous_usable_version() == Some(request.expected_version())
                && images.get(attempt.handle).is_some_and(|image| {
                    image.source().is_some_and(|source| {
                        source.stable_id() == request.stable_id() && source.path() == request.path()
                    })
                }));
        match attempt
            .publication
            .mutation(&images, &states)
            .and_then(|baseline| {
                baseline
                    .remove(&mut images, &mut states, &mut events)
                    .map_err(|_| ImagePublicationFailureKind::StateChanged)
            }) {
            Ok(_) => {
                stats.removed = stats.removed.saturating_add(1);
                if restore_eligible
                    && let Some(recovery) = RemovedImageRecovery::capture(
                        &authority.0,
                        request.record(),
                        attempt.handle,
                        &asset_server,
                        &images,
                        &states,
                    )
                {
                    recoveries.images.insert(request.asset_id(), recovery);
                }
            }
            Err(_) => stats.failed = stats.failed.saturating_add(1),
        }
    }

    let mut finished = std::mem::take(&mut ready.imports);
    finished.sort_by_key(ReadyImageImportJob::sort_key);

    for job in finished {
        let ReadyImageImportJob {
            attempt,
            order_key: _,
            outcome,
        } = job;
        let request = &attempt.request;
        let coalesced = matches!(
            &outcome,
            ReadyImageImportOutcome::Terminal(terminal)
                if matches!(
                    terminal.as_ref(),
                    TaskTerminal::Cancelled(TaskCancellation {
                        reason: TaskCancellationReason::Coalesced { .. },
                        ..
                    })
                )
        );
        if matches!(
            &outcome,
            ReadyImageImportOutcome::Terminal(terminal)
                if matches!(terminal.as_ref(), TaskTerminal::Cancelled(_))
        ) {
            stats.cancelled = stats.cancelled.saturating_add(1);
        }
        if coalesced {
            if !generations.is_current(request.asset_id(), request.generation()) {
                stats.stale = stats.stale.saturating_add(1);
            }
            continue;
        }
        if !generations.is_current(request.asset_id(), request.generation()) {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }

        let failure = match outcome {
            ReadyImageImportOutcome::Terminal(terminal) => match *terminal {
                TaskTerminal::Completed(Ok(imported)) => apply_imported_image(
                    imported,
                    &asset_server,
                    &mut images,
                    &mut states,
                    &mut events,
                    &mut stats,
                ),
                TaskTerminal::Completed(Err(error)) => Some(error),
                TaskTerminal::Cancelled(cancellation) => {
                    Some(ImageReloadError::TaskCancelled(cancellation))
                }
                TaskTerminal::Failed(failure) => Some(ImageReloadError::TaskFailed(failure)),
            },
            ReadyImageImportOutcome::Rejected(rejection) => {
                Some(ImageReloadError::TaskRejected(rejection))
            }
            ReadyImageImportOutcome::ImmediateFailure(error) => Some(error),
            ReadyImageImportOutcome::TrackingFailure => Some(ImageReloadError::TaskTracking),
        };
        if let Some(error) = failure {
            record_image_reload_failure(
                attempt,
                error,
                &asset_server,
                &mut images,
                &mut states,
                &mut events,
                &mut diagnostics,
                &mut stats,
            );
        }
    }

    recoveries
        .images
        .retain(|_, snapshot| snapshot.is_current(&asset_server, &images, &states));

    stats.pending = pending.len().min(u32::MAX as usize) as u32;
}

pub(super) fn apply_imported_image(
    imported: ImageImportedAsset,
    asset_server: &AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    stats: &mut ImageReloadStats,
) -> Option<ImageReloadError> {
    match imported.commit(asset_server, images, states, events) {
        Ok(_) => {
            stats.applied = stats.applied.saturating_add(1);
            None
        }
        Err(error) if error.is_stale_conflict() => {
            stats.stale = stats.stale.saturating_add(1);
            None
        }
        Err(error) => Some(ImageReloadError::Publication(error)),
    }
}

pub(super) fn record_image_reload_failure(
    attempt: ImageReloadAttempt,
    error: ImageReloadError,
    asset_server: &AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    diagnostics: &mut AssetReloadDiagnostics,
    stats: &mut ImageReloadStats,
) -> bool {
    let request = &attempt.request;
    if !attempt.is_current(asset_server, images, states) {
        stats.stale = stats.stale.saturating_add(1);
        return false;
    }
    let handle = attempt.handle;
    if let Err(state_error) = states.ensure_version(handle.id(), request.expected_version()) {
        match state_error {
            AssetStateError::StaleReload { .. } => stats.stale = stats.stale.saturating_add(1),
            _ => {
                diagnostics.push(image_reload_diagnostic(request.path(), &error));
                stats.failed = stats.failed.saturating_add(1);
            }
        }
        return false;
    }
    diagnostics.push(image_reload_diagnostic(request.path(), &error));
    let result = match attempt.publication.mutation(images, states) {
        Ok(baseline) => match &attempt.recovery {
            Some(recovery) if !matches!(error, ImageReloadError::MetadataIdentityMismatch) => {
                recovery.continuity.record_failure(
                    baseline,
                    images,
                    states,
                    events,
                    error.stable_code(),
                )
            }
            _ => baseline.record_failure(images, states, events, error.stable_code()),
        },
        Err(_) => {
            stats.stale = stats.stale.saturating_add(1);
            return false;
        }
    };
    if result.is_ok() && matches!(error, ImageReloadError::MetadataIdentityMismatch) {
        states.invalidate_publication(handle.id());
    }

    match result {
        Ok(_) => {
            stats.failed = stats.failed.saturating_add(1);
            true
        }
        Err(_) => {
            stats.stale = stats.stale.saturating_add(1);
            false
        }
    }
}

pub(super) fn image_reload_diagnostic(path: &AssetPath, error: &ImageReloadError) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(
        DiagnosticCode::new(error.stable_code())
            .expect("image diagnostic code literals must be valid"),
        SafeSummary::new(error.safe_summary())
            .expect("image diagnostic summaries must be safe literals"),
    );
    let path_key = image_diagnostic_field_key("asset-path");
    let path_field = DiagnosticField::project_relative(path_key, path.as_str())
        .unwrap_or_else(|_| DiagnosticField::sensitive(path_key));
    diagnostic = with_image_diagnostic_field(diagnostic, path_field);

    match error {
        ImageReloadError::MissingSourceDirectory
        | ImageReloadError::MissingImportSettings
        | ImageReloadError::MetadataSupportUnavailable
        | ImageReloadError::MetadataInvalid
        | ImageReloadError::MetadataIdentityMismatch
        | ImageReloadError::InvalidSourceChange => {}
        ImageReloadError::Import(error) => {
            diagnostic = with_image_public_identifier(diagnostic, "stage", error.stage().as_str());
            match error {
                ImageImportError::Budget { error, .. } => {
                    diagnostic = with_image_public_identifier(
                        diagnostic,
                        "limit-kind",
                        error.kind().as_str(),
                    );
                    diagnostic = with_image_diagnostic_field(
                        diagnostic,
                        DiagnosticField::public_u64(
                            image_diagnostic_field_key("limit"),
                            error.limit(),
                        ),
                    );
                    if let Some(observed) = error.observed() {
                        diagnostic = with_image_diagnostic_field(
                            diagnostic,
                            DiagnosticField::public_u64(
                                image_diagnostic_field_key("observed"),
                                observed,
                            ),
                        );
                    }
                    if let Some(in_use) = error.in_use() {
                        diagnostic = with_image_diagnostic_field(
                            diagnostic,
                            DiagnosticField::public_u64(
                                image_diagnostic_field_key("in-use"),
                                in_use,
                            ),
                        );
                    }
                }
                ImageImportError::Unsupported { feature, .. } => {
                    diagnostic =
                        with_image_public_identifier(diagnostic, "feature", feature.as_str());
                }
                ImageImportError::Png { kind, .. } => {
                    diagnostic = with_image_public_identifier(diagnostic, "reason", kind.as_str());
                    diagnostic = with_image_diagnostic_field(
                        diagnostic,
                        DiagnosticField::sensitive(image_diagnostic_field_key("decoder-detail")),
                    );
                }
                ImageImportError::Source { kind, .. } => {
                    diagnostic = with_image_public_identifier(diagnostic, "reason", kind.as_str());
                    diagnostic = with_image_diagnostic_field(
                        diagnostic,
                        DiagnosticField::sensitive(image_diagnostic_field_key("source-detail")),
                    );
                }
                ImageImportError::Publication(kind) => {
                    diagnostic = with_image_public_identifier(diagnostic, "reason", kind.as_str());
                }
                ImageImportError::MissingSourceExtension { .. }
                | ImageImportError::UnsupportedFormat { .. }
                | ImageImportError::ArtifactPath(_)
                | ImageImportError::Cancelled { .. } => {}
            }
        }
        ImageReloadError::TaskRejected(rejection) => {
            diagnostic = with_image_public_identifier(
                diagnostic,
                "reason",
                task_rejection_reason(rejection.reason),
            );
        }
        ImageReloadError::TaskCancelled(cancellation) => {
            diagnostic = with_image_public_identifier(
                diagnostic,
                "reason",
                task_cancellation_reason(cancellation.reason),
            );
            diagnostic = with_image_diagnostic_field(
                diagnostic,
                DiagnosticField::public_bool(
                    image_diagnostic_field_key("before-start"),
                    cancellation.before_start,
                ),
            );
        }
        ImageReloadError::TaskFailed(_) => {
            diagnostic = with_image_public_identifier(diagnostic, "reason", "panicked");
            diagnostic = with_image_diagnostic_field(
                diagnostic,
                DiagnosticField::sensitive(image_diagnostic_field_key("panic-payload")),
            );
        }
        ImageReloadError::TaskTracking => {}
        ImageReloadError::Asset(_) => {}
        ImageReloadError::Publication(kind) => {
            diagnostic = with_image_public_identifier(diagnostic, "reason", kind.as_str());
        }
    }
    diagnostic
}

fn with_image_public_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    let value = PublicDiagnosticIdentifier::new(value)
        .expect("image diagnostic public identifiers must be valid");
    with_image_diagnostic_field(
        diagnostic,
        DiagnosticField::public_identifier(image_diagnostic_field_key(key), value),
    )
}

fn with_image_diagnostic_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("image diagnostic fields use unique engine-owned keys")
}

fn image_diagnostic_field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("image diagnostic field key literals must be valid")
}

fn task_rejection_reason(reason: nara_tasks::TaskRejectReason) -> &'static str {
    match reason {
        nara_tasks::TaskRejectReason::QueueFull { .. } => "queue-full",
        nara_tasks::TaskRejectReason::PoolClosed => "pool-closed",
        nara_tasks::TaskRejectReason::ScopeClosed => "scope-closed",
        nara_tasks::TaskRejectReason::WorkerStartupFailed => "worker-startup-failed",
        nara_tasks::TaskRejectReason::TaskIdExhausted => "task-id-exhausted",
    }
}

fn task_cancellation_reason(reason: TaskCancellationReason) -> &'static str {
    match reason {
        TaskCancellationReason::Requested => "requested",
        TaskCancellationReason::Coalesced { .. } => "coalesced",
        TaskCancellationReason::PoolShutdown => "pool-shutdown",
        TaskCancellationReason::ScopeClosed => "scope-closed",
    }
}
