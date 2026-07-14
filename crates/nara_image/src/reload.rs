//! Async image reload orchestration and plugin integration.

use crate::import::ImagePublicationSnapshot;
use crate::{
    ImageAsset, ImageColorSpace, ImageFileImportRequest, ImageImportBudgetHost, ImageImportError,
    ImageImportLimits, ImageImportedAsset, ImageImporter, ImageImporterCreateError,
    ImagePreparePlugin, ImagePublicationFailureKind, ImageSourceDirectory,
};

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_app::{App, CoreStage, Plugin, PluginError, RealTime, TaskUpdateSet};
use nara_asset::{
    AssetEvents, AssetId, AssetLoadGenerations, AssetPath, AssetPlugin, AssetReloadDiagnostics,
    AssetReloadRequest, AssetReloadRequestKind, AssetReloadRequests, AssetServer, AssetSourceKind,
    AssetStateError, AssetStates, Assets, Handle, ImporterRegistry, ImporterRegistryError,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_tasks::{
    OrderedTaskResults, TaskCancellation, TaskCancellationReason, TaskCoalesceKey, TaskDomainKey,
    TaskFailure, TaskHandle, TaskOrderKey, TaskOverloadPolicy, TaskPoolKind, TaskPools,
    TaskRejection, TaskSpawnOutcome, TaskSpawnRequest, TaskTerminal,
};

pub(super) const IMAGE_RELOAD_TASK_DOMAIN: TaskDomainKey =
    TaskDomainKey::new(0x6e61_7261_696d_6167);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReloadError {
    MissingSourceDirectory,
    Import(ImageImportError),
    TaskRejected(TaskRejection),
    TaskCancelled(TaskCancellation),
    TaskFailed(TaskFailure),
    TaskTracking,
    Publication(ImagePublicationFailureKind),
}

impl Display for ImageReloadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSourceDirectory => formatter.write_str("image source authority is absent"),
            Self::Import(error) => Display::fmt(error, formatter),
            Self::TaskRejected(_) => formatter.write_str("image import task was rejected"),
            Self::TaskCancelled(_) => formatter.write_str("image import task was cancelled"),
            Self::TaskFailed(_) => formatter.write_str("image import task failed"),
            Self::TaskTracking => formatter.write_str("image import task tracking failed"),
            Self::Publication(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ImageReloadError {}

impl ImageReloadError {
    const fn stable_code(&self) -> &'static str {
        match self {
            Self::MissingSourceDirectory => "image.reload-source-authority-missing",
            Self::Import(error) => error.stable_code(),
            Self::TaskRejected(_) => "image.reload-task-rejected",
            Self::TaskCancelled(_) => "image.reload-task-cancelled",
            Self::TaskFailed(_) => "image.reload-task-failed",
            Self::TaskTracking => "image.reload-task-tracking-failed",
            Self::Publication(_) => "image.reload-publication-failed",
        }
    }

    const fn safe_summary(&self) -> &'static str {
        match self {
            Self::MissingSourceDirectory => "Image source authority is absent",
            Self::Import(error) => error.safe_summary(),
            Self::TaskRejected(_) => "Image import task was rejected",
            Self::TaskCancelled(_) => "Image import task was cancelled",
            Self::TaskFailed(_) => "Image import task failed",
            Self::TaskTracking => "Image import task tracking failed",
            Self::Publication(_) => "Image publication failed",
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
    ordered: OrderedTaskResults<ImageImportTaskResult>,
    attempts: BTreeMap<TaskOrderKey, ImageReloadAttempt>,
}

impl PendingImageImportStream {
    pub(super) fn push(
        &mut self,
        attempt: ImageReloadAttempt,
        handle: TaskHandle<ImageImportTaskResult>,
    ) -> Result<(), Box<(ImageReloadAttempt, TaskHandle<ImageImportTaskResult>)>> {
        let order_key = handle.order_key();
        match self.ordered.push(handle) {
            Ok(()) => {
                self.attempts.insert(order_key, attempt);
                Ok(())
            }
            Err(handle) => Err(Box::new((attempt, handle))),
        }
    }

    pub(super) fn drain_ready_prefix(&mut self) -> Vec<ReadyImageImportJob> {
        self.ordered
            .drain_ready_prefix()
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
    publication: ImagePublicationSnapshot,
}

impl ImageReloadAttempt {
    pub(super) fn capture(
        request: AssetReloadRequest,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Self {
        let publication = ImagePublicationSnapshot::capture(
            &request.record(),
            Handle::<ImageAsset>::new(request.asset_id()),
            request.expected_version(),
            images,
            states,
        );
        Self {
            request,
            publication,
        }
    }

    fn is_current(&self, images: &Assets<ImageAsset>, states: &AssetStates) -> bool {
        self.publication.is_current(images, states)
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
    removals: Vec<AssetReloadRequest>,
}

#[derive(Debug, Clone, Default)]
pub struct ImagePlugin {
    importer: ImageImporter,
    source_directory: Option<Arc<ImageSourceDirectory>>,
}

impl ImagePlugin {
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
    pub fn with_color_space(mut self, color_space: ImageColorSpace) -> Self {
        self.importer = self.importer.with_color_space(color_space);
        self
    }

    #[must_use]
    pub fn with_source_directory(mut self, source_directory: ImageSourceDirectory) -> Self {
        self.source_directory = Some(Arc::new(source_directory));
        self
    }
}

impl Plugin for ImagePlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.image"),
            nara_app::PluginCategory::Asset,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(AssetPlugin)?;
        app.add_plugin_if_missing(ImagePreparePlugin)?;
        app.init_resource::<ImageReloadStats>()?;
        app.init_resource::<PendingImageJobs>()?;
        app.init_resource::<ReadyImageJobs>()?;
        app.init_resource::<ImporterRegistry>()?;
        app.insert_resource(self.importer.clone())?;
        register_image_importer(app, self.importer.clone())?;
        app.add_systems(
            CoreStage::TaskUpdate,
            poll_image_reload_results.in_set(TaskUpdateSet::Poll),
        )?;
        let source_directory = self.source_directory.clone();
        app.add_systems(
            CoreStage::TaskUpdate,
            (move |requests: ResMut<AssetReloadRequests>,
                   importer: Res<ImageImporter>,
                   asset_server: Res<AssetServer>,
                   images: Res<Assets<ImageAsset>>,
                   states: Res<AssetStates>,
                   task_pools: Res<TaskPools>,
                   real_time: Res<RealTime>,
                   pending: ResMut<PendingImageJobs>,
                   ready: ResMut<ReadyImageJobs>,
                   stats: ResMut<ImageReloadStats>| {
                spawn_image_reload_jobs(
                    source_directory.as_deref(),
                    requests,
                    importer,
                    asset_server,
                    images,
                    states,
                    task_pools,
                    real_time,
                    pending,
                    ready,
                    stats,
                );
            })
            .in_set(TaskUpdateSet::SpawnAssetJobs),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            apply_image_reload_results.in_set(TaskUpdateSet::ApplyAssetResults),
        )?;
        Ok(())
    }
}

fn register_image_importer(app: &mut App, importer: ImageImporter) -> Result<(), PluginError> {
    let mut registry = app.world_mut()?.resource_mut::<ImporterRegistry>();
    registry
        .register(importer)
        .map_err(|error| image_plugin_setup_error("register image importer", error))
}

fn image_plugin_setup_error(context: &'static str, error: ImporterRegistryError) -> PluginError {
    PluginError::SetupFailed {
        plugin: nara_app::PluginId::new("nara.image"),
        message: format!("{context}: {error}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_image_reload_jobs(
    source_directory: Option<&ImageSourceDirectory>,
    mut requests: ResMut<AssetReloadRequests>,
    importer: Res<ImageImporter>,
    asset_server: Res<AssetServer>,
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    task_pools: Res<TaskPools>,
    real_time: Res<RealTime>,
    mut pending: ResMut<PendingImageJobs>,
    mut ready: ResMut<ReadyImageJobs>,
    mut stats: ResMut<ImageReloadStats>,
) {
    for request in requests.drain_for_source_kind(&AssetSourceKind::Image) {
        match request.request_kind() {
            AssetReloadRequestKind::Remove => ready.removals.push(request),
            AssetReloadRequestKind::LoadOrReload => {
                let attempt = ImageReloadAttempt::capture(request, &images, &states);
                let record = attempt.request.record();
                let asset_id = attempt.request.asset_id();
                let admitted = source_directory
                    .ok_or(ImageReloadError::MissingSourceDirectory)
                    .and_then(|source_directory| {
                        source_directory
                            .open(&record)
                            .map_err(ImageReloadError::Import)
                    })
                    .and_then(|file| {
                        importer
                            .admit_file_with_snapshot(
                                ImageFileImportRequest::new(
                                    record.clone(),
                                    file,
                                    nara_asset::ImportDependencyDigest::empty(),
                                    nara_asset::ImportSettingsHash::default(),
                                    nara_asset::ImportProfile::default(),
                                ),
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
    stats.pending = pending.len().min(u32::MAX as usize) as u32;
}

fn poll_image_reload_results(
    mut pending: ResMut<PendingImageJobs>,
    mut ready: ResMut<ReadyImageJobs>,
    mut stats: ResMut<ImageReloadStats>,
) {
    pending.imports.retain(|_, stream| {
        ready.imports.extend(stream.drain_ready_prefix());
        !stream.is_empty()
    });
    stats.pending = pending.len().min(u32::MAX as usize) as u32;
}

#[allow(clippy::too_many_arguments)]
fn apply_image_reload_results(
    mut ready: ResMut<ReadyImageJobs>,
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
    removals.sort_by_key(AssetReloadRequest::id);
    for request in removals {
        if !generations.is_current(request.asset_id(), request.generation()) {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let handle = Handle::<ImageAsset>::new(request.asset_id());
        match images.remove_with_state(handle, &mut states, &mut events) {
            Ok(_) => stats.removed = stats.removed.saturating_add(1),
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
                &mut images,
                &mut states,
                &mut events,
                &mut diagnostics,
                &mut stats,
            );
        }
    }

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
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    diagnostics: &mut AssetReloadDiagnostics,
    stats: &mut ImageReloadStats,
) {
    let request = &attempt.request;
    if !attempt.is_current(images, states) {
        stats.stale = stats.stale.saturating_add(1);
        return;
    }
    let handle = Handle::<ImageAsset>::new(request.asset_id());
    if let Err(state_error) = states.ensure_version(handle.id(), request.expected_version()) {
        match state_error {
            AssetStateError::StaleReload { .. } => stats.stale = stats.stale.saturating_add(1),
            _ => {
                diagnostics.push(image_reload_diagnostic(request.path(), &error));
                stats.failed = stats.failed.saturating_add(1);
            }
        }
        return;
    }
    diagnostics.push(image_reload_diagnostic(request.path(), &error));
    let result = if images.get(handle).is_some() {
        images.record_reload_failure(handle, states, events, error.stable_code())
    } else {
        images.record_load_failure(handle, states, events, error.stable_code())
    };

    match result {
        Ok(_) => stats.failed = stats.failed.saturating_add(1),
        Err(_) => stats.stale = stats.stale.saturating_add(1),
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
        ImageReloadError::MissingSourceDirectory => {}
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
                ImageImportError::Selection(_)
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
        nara_tasks::TaskRejectReason::TaskIdExhausted => "task-id-exhausted",
    }
}

fn task_cancellation_reason(reason: TaskCancellationReason) -> &'static str {
    match reason {
        TaskCancellationReason::Requested => "requested",
        TaskCancellationReason::Coalesced { .. } => "coalesced",
        TaskCancellationReason::PoolShutdown => "pool-shutdown",
    }
}
