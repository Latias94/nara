//! Explicit typed source-build consumers in the existing Asset and Tasks execution phases.

use std::{collections::BTreeMap, marker::PhantomData, mem::size_of, sync::Arc};

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, RealTime,
};
use nara_asset::{
    AssetDependencyGraph, AssetDomain, AssetEvents, AssetId, AssetLoadGenerations,
    AssetMutationBaseline, AssetPathScope, AssetProductRef, AssetReloadDiagnostics,
    AssetReloadRequest, AssetReloadRequestKind, AssetReloadRequests, AssetServer,
    AssetSourceChange, AssetSourceChangeKind, AssetSourceChanges, AssetSourceKind, AssetStates,
    AssetTaskUpdateSet, Assets, ImportedProductId, SourceAssetBuild, SourceReloadConsumer,
    SourceSubscriptionKind, register_source_reload_consumer,
};
use nara_core::ItemLimit;
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_ecs::{Res, ResMut, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs};
use nara_fs::DirectoryCapability;
use nara_tasks::{
    OrderedTaskResults, TaskDomainKey, TaskOrderKey, TaskPoolKind, TaskPools, TaskSpawnOutcome,
    TaskSpawnRequest, TaskTerminal,
};
use serde::Serialize;

use super::{
    ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildStatus, ArtifactBuildStore,
    ArtifactSourceRequest, BuiltSourceAsset,
    budget::ArtifactBuildLease,
    execute::{SourceBuildObservations, build_source_observed},
    live_value::decode_source_build,
};

mod apply;
#[cfg(test)]
mod tests;

const SOURCE_BUILD_TASK_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x6e61_7261_6275_696c);

struct SourceBuildConfiguration {
    store: Arc<ArtifactBuildStore>,
    directory: DirectoryCapability,
    scope: AssetPathScope,
    definition: ArtifactBuildDefinition,
    pending_limit: ItemLimit,
    _lease: Option<ArtifactBuildLease>,
}

/// Executes one compiled build family when the existing source resolver requests it.
///
/// Select AssetPlugin, AssetSourcePlugin and TaskPlugin separately. Insert the actual source
/// records in ProjectAssetDatabase and submit initial source changes through AssetSourceChanges.
/// Later raw dependency changes use that same queue and require no explicit rebuild or graph edit.
/// The source directory resolves paths relative to `scope`; no ambient filesystem path is used.
pub struct SourceAssetBuildPlugin<D: SourceAssetBuild>
where
    D::Settings: Serialize,
{
    configuration: SourceBuildConfiguration,
    domain: PhantomData<fn() -> D>,
}

impl<D: SourceAssetBuild> SourceAssetBuildPlugin<D>
where
    D::Settings: Serialize,
{
    const DECLARATION: PluginDeclaration =
        PluginDeclaration::new(PluginId::new(D::BUILD_ID), PluginCategory::Asset).requires_plugins(
            &[
                nara_asset::ASSET_SOURCE_PLUGIN_ID,
                nara_tasks::TASK_PLUGIN_ID,
            ],
        );

    #[must_use]
    pub fn new(
        store: Arc<ArtifactBuildStore>,
        directory: DirectoryCapability,
        scope: AssetPathScope,
        definition: ArtifactBuildDefinition,
    ) -> Self {
        Self {
            configuration: SourceBuildConfiguration {
                store,
                directory,
                scope,
                definition,
                pending_limit: ItemLimit::new(1024).expect("fixed nonzero source build limit"),
                _lease: None,
            },
            domain: PhantomData,
        }
    }

    /// Bounds the adapter's current attempts; original Tasks limits still govern execution.
    #[must_use]
    pub fn with_pending_limit(mut self, limit: ItemLimit) -> Self {
        self.configuration.pending_limit = limit;
        self
    }
}

impl<D: SourceAssetBuild> Plugin for SourceAssetBuildPlugin<D>
where
    D::Settings: Serialize,
{
    fn declaration() -> &'static PluginDeclaration {
        &Self::DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if self.configuration.definition.id != D::BUILD_ID
            || self.configuration.definition.revision.get() != D::BUILD_REVISION
        {
            return Err(setup_error::<D>(
                "source build definition does not match its compiled domain",
            ));
        }
        let configuration_bytes = size_of::<SourceBuildConfiguration>()
            .checked_add(64)
            .and_then(|bytes| bytes.checked_add(self.configuration.definition.id.len()))
            .and_then(|bytes| {
                bytes.checked_add(
                    self.configuration
                        .scope
                        .prefix()
                        .map_or(0, |prefix| prefix.as_str().len()),
                )
            })
            .ok_or_else(|| setup_error::<D>("source build configuration storage overflows"))?;
        let configuration_lease = self
            .configuration
            .store
            .reserve_retained(configuration_bytes)
            .map_err(|_| {
                setup_error::<D>("source build configuration exceeds its original budget")
            })?;
        let configuration = Arc::new(SourceBuildConfiguration {
            store: Arc::clone(&self.configuration.store),
            directory: self.configuration.directory.share(),
            scope: self.configuration.scope.clone(),
            definition: self.configuration.definition.clone(),
            pending_limit: self.configuration.pending_limit,
            _lease: Some(configuration_lease),
        });
        let storage = configuration
            .pending_limit
            .get()
            .checked_mul(
                size_of::<ReadyBuild<D::Product>>() + size_of::<PendingBuild<D::Product>>() + 128,
            )
            .ok_or_else(|| setup_error::<D>("source build attempt storage overflows"))?;
        let lease = configuration.store.reserve_retained(storage).map_err(|_| {
            setup_error::<D>("source build attempt storage exceeds its original budget")
        })?;
        let consumer = register_source_reload_consumer::<BuiltSourceAsset<D::Product>>(
            app,
            AssetSourceKind::Other(D::SOURCE_KIND.to_owned()),
        )
        .map_err(|error| setup_error::<D>(&error.to_string()))?;
        app.init_resource::<Assets<BuiltSourceAsset<D::Product>>>()?;
        app.init_resource::<SourceAssetBuildStats<D>>()?;
        app.insert_resource(SourceBuildExecutor::<D> {
            ready: Vec::with_capacity(configuration.pending_limit.get()),
            configuration,
            consumer,
            pending: BTreeMap::new(),
            domain: PhantomData,
            _lease: lease,
        })?;
        app.add_systems(
            CoreStage::TaskUpdate,
            poll_source_builds::<D>.in_set(AssetTaskUpdateSet::Poll),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            spawn_source_builds::<D>.in_set(AssetTaskUpdateSet::SpawnJobs),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            apply::apply_source_builds::<D>.in_set(AssetTaskUpdateSet::ApplyResults),
        )?;
        Ok(())
    }
}

fn setup_error<D: SourceAssetBuild>(message: &str) -> PluginError
where
    D::Settings: Serialize,
{
    PluginError::SetupFailed {
        plugin: PluginId::new(D::BUILD_ID),
        message: message.to_owned(),
    }
}

/// Counters for this compiled adapter. Pending counts logical observed attempts, not physical
/// Tasks retirement; built/reused count successfully adopted values, not discarded store results.
#[derive(Resource)]
pub struct SourceAssetBuildStats<D: AssetDomain> {
    pub spawned: u64,
    pub built: u64,
    pub reused: u64,
    pub adopted: u64,
    pub failed: u64,
    pub rejected: u64,
    pub stale: u64,
    pub pending: usize,
    domain: PhantomData<fn() -> D>,
}

impl<D: AssetDomain> Default for SourceAssetBuildStats<D> {
    fn default() -> Self {
        Self {
            spawned: 0,
            built: 0,
            reused: 0,
            adopted: 0,
            failed: 0,
            rejected: 0,
            stale: 0,
            pending: 0,
            domain: PhantomData,
        }
    }
}

#[derive(Resource)]
struct SourceBuildExecutor<D: AssetDomain> {
    configuration: Arc<SourceBuildConfiguration>,
    consumer: SourceReloadConsumer,
    pending: BTreeMap<AssetId, PendingBuild<D::Product>>,
    ready: Vec<ReadyBuild<D::Product>>,
    domain: PhantomData<fn() -> D>,
    _lease: ArtifactBuildLease,
}

struct BuildContext<P: Send + Sync + 'static> {
    request: AssetReloadRequest,
    baseline: AssetMutationBaseline<BuiltSourceAsset<P>>,
    source: ArtifactSourceRequest,
    _lease: ArtifactBuildLease,
}

struct PendingBuild<P: Send + Sync + 'static> {
    context: Arc<BuildContext<P>>,
    ordered: OrderedTaskResults<SourceBuildOutput<P>>,
}

struct ReadyBuild<P: Send + Sync + 'static> {
    context: Arc<BuildContext<P>>,
    order: TaskOrderKey,
    terminal: TaskTerminal<SourceBuildOutput<P>>,
}

struct PreparedBuiltSource<P> {
    value: BuiltSourceAsset<P>,
    status: ArtifactBuildStatus,
}

struct SourceBuildOutput<P> {
    result: Result<PreparedBuiltSource<P>, ArtifactBuildError>,
    observations: SourceBuildObservations,
}

fn execute_source_build<D: SourceAssetBuild>(
    configuration: &SourceBuildConfiguration,
    context: &BuildContext<D::Product>,
    cancellation: nara_tasks::TaskCancellationToken,
) -> SourceBuildOutput<D::Product>
where
    D::Settings: Serialize,
{
    let built = build_source_observed::<D>(&configuration.store, &context.source, |_| {
        if cancellation.is_cancelled() {
            Err(ArtifactBuildError::Aborted)
        } else {
            Ok(())
        }
    });
    let result = built.result.and_then(|built| {
        let status = built.status();
        let group = built.into_group();
        let expected =
            AssetProductRef::new(context.request.stable_id(), ImportedProductId::PRIMARY).map_err(
                |_| ArtifactBuildError::InvalidCandidate("source Product identity is invalid"),
            )?;
        if group.seed().source != expected {
            return Err(ArtifactBuildError::InvalidCandidate(
                "built source identity changed",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(ArtifactBuildError::Aborted);
        }
        let value = decode_source_build::<D>(&configuration.store, group)?;
        Ok(PreparedBuiltSource { value, status })
    });
    SourceBuildOutput {
        result,
        observations: built.observations,
    }
}

fn poll_source_builds<D: AssetDomain>(
    task_pools: Res<TaskPools>,
    mut executor: ResMut<SourceBuildExecutor<D>>,
    mut stats: ResMut<SourceAssetBuildStats<D>>,
) -> Result<(), BevyError> {
    let cutoff = task_pools.capture_completion_cutoff();
    let SourceBuildExecutor { pending, ready, .. } = &mut *executor;
    let mut failure = None;
    pending.retain(|_, pending| {
        if failure.is_some() {
            return true;
        }
        let terminals = pending
            .ordered
            .capture_ready_prefix(&cutoff)
            .map_err(BevyError::error)
            .and_then(|snapshot| snapshot.into_terminals().map_err(BevyError::error));
        match terminals {
            Ok(terminals) => {
                let finished = !terminals.is_empty();
                for terminal in terminals {
                    ready.push(ReadyBuild {
                        context: Arc::clone(&pending.context),
                        order: terminal.order_key,
                        terminal: terminal.terminal,
                    });
                }
                !finished
            }
            Err(error) => {
                failure = Some(error);
                true
            }
        }
    });
    stats.pending = executor.pending.len();
    failure.map_or(Ok(()), Err)
}

#[allow(clippy::too_many_arguments)]
fn spawn_source_builds<D: SourceAssetBuild>(
    mut executor: ResMut<SourceBuildExecutor<D>>,
    mut requests: ResMut<AssetReloadRequests>,
    mut server: ResMut<AssetServer>,
    mut assets: ResMut<Assets<BuiltSourceAsset<D::Product>>>,
    mut states: ResMut<AssetStates>,
    generations: Res<AssetLoadGenerations>,
    mut graph: ResMut<AssetDependencyGraph>,
    mut events: ResMut<AssetEvents>,
    task_pools: Res<TaskPools>,
    real_time: Res<RealTime>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
    mut stats: ResMut<SourceAssetBuildStats<D>>,
) -> Result<(), BevyError>
where
    D::Settings: Serialize,
{
    executor.pending.retain(|_, attempt| {
        let current = generations.is_current_request(&attempt.context.request);
        if !current {
            stats.stale = stats.stale.saturating_add(1);
        }
        current
    });
    for request in requests
        .drain_source(&executor.consumer)
        .map_err(BevyError::error)?
    {
        if !generations.is_current_request(&request)
            || request.resolved_state_revision() != Some(&states.state_revision(request.asset_id()))
        {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let handle = server
            .claim::<BuiltSourceAsset<D::Product>>(request.asset_id())
            .map_err(BevyError::error)?;
        let baseline = match assets.prepare_mutation(handle, &states) {
            Ok(baseline) => baseline,
            Err(_) => {
                stats.stale = stats.stale.saturating_add(1);
                continue;
            }
        };
        if request.request_kind() == AssetReloadRequestKind::Remove {
            match graph.prepare_source_subscriptions(
                &executor.consumer,
                &request,
                &generations,
                Vec::new(),
                SourceSubscriptionKind::Published,
            ) {
                Ok(prepared) => {
                    if prepared
                        .commit_with(&mut graph, &generations, || {
                            baseline.remove(&mut assets, &mut states, &mut events)
                        })
                        .is_err()
                    {
                        stats.stale = stats.stale.saturating_add(1);
                    }
                }
                Err(_) => record_failure(
                    baseline,
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    diagnostic(
                        "asset.build.subscription-refused",
                        "Source dependency subscriptions were refused",
                    ),
                ),
            }
            continue;
        }
        if executor.pending.len() >= executor.configuration.pending_limit.get() {
            stats.rejected = stats.rejected.saturating_add(1);
            record_failure(
                baseline,
                &mut assets,
                &mut states,
                &mut events,
                &mut diagnostics,
                &mut stats,
                diagnostic(
                    "asset.build.pending-limit",
                    "Source build attempt capacity was exceeded",
                ),
            );
            continue;
        }
        let context = prepare_context(&executor.configuration, request, baseline.clone());
        let context = match context {
            Ok(context) => Arc::new(context),
            Err(error) => {
                stats.rejected = stats.rejected.saturating_add(1);
                record_failure(
                    baseline,
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    build_diagnostic(error),
                );
                continue;
            }
        };
        let configuration = Arc::clone(&executor.configuration);
        let task_context = Arc::clone(&context);
        let spawned = task_pools.spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(real_time.frame, SOURCE_BUILD_TASK_DOMAIN),
            move |cancellation| {
                execute_source_build::<D>(&configuration, &task_context, cancellation)
            },
        );
        match spawned {
            TaskSpawnOutcome::Accepted(handle) | TaskSpawnOutcome::Coalesced { handle, .. } => {
                let mut ordered = OrderedTaskResults::default();
                if let Err(handle) = ordered.push(handle) {
                    handle.cancel();
                    record_failure(
                        baseline,
                        &mut assets,
                        &mut states,
                        &mut events,
                        &mut diagnostics,
                        &mut stats,
                        diagnostic(
                            "asset.build.task-tracking",
                            "Source build task could not be observed",
                        ),
                    );
                    continue;
                }
                executor.pending.insert(
                    context.request.asset_id(),
                    PendingBuild { context, ordered },
                );
                stats.spawned = stats.spawned.saturating_add(1);
            }
            TaskSpawnOutcome::Rejected(_) => {
                stats.rejected = stats.rejected.saturating_add(1);
                record_failure(
                    baseline,
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    diagnostic(
                        "asset.build.task-rejected",
                        "Source build task admission was refused",
                    ),
                );
            }
        }
    }
    stats.pending = executor.pending.len();
    Ok(())
}

fn prepare_context<P: Send + Sync + 'static>(
    configuration: &SourceBuildConfiguration,
    request: AssetReloadRequest,
    baseline: AssetMutationBaseline<BuiltSourceAsset<P>>,
) -> Result<BuildContext<P>, ArtifactBuildError> {
    let bytes = request
        .retained_payload_bytes()
        .and_then(|bytes| bytes.checked_add(size_of::<BuildContext<P>>() + 1024))
        .and_then(|bytes| bytes.checked_add(request.path().as_str().len().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(configuration.definition.id.len()))
        .and_then(|bytes| {
            bytes.checked_add(
                configuration
                    .scope
                    .prefix()
                    .map_or(0, |prefix| prefix.as_str().len()),
            )
        })
        .ok_or(ArtifactBuildError::Overflow)?;
    let lease = configuration.store.reserve_retained(bytes)?;
    let path = configuration.scope.relativize(request.path()).ok_or(
        ArtifactBuildError::InvalidCandidate("source request escapes its configured scope"),
    )?;
    let source = ArtifactSourceRequest::new(
        configuration.directory.share(),
        configuration.scope.clone(),
        path,
        configuration.definition.clone(),
    );
    Ok(BuildContext {
        request,
        baseline,
        source,
        _lease: lease,
    })
}

fn diagnostic(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("fixed source build diagnostic code"),
        SafeSummary::new(summary).expect("fixed safe source build summary"),
    )
}

fn build_diagnostic(error: ArtifactBuildError) -> Diagnostic {
    match error {
        ArtifactBuildError::Domain(diagnostic) => *diagnostic,
        ArtifactBuildError::Codec(error) => error.into_diagnostic(),
        ArtifactBuildError::Budget { .. } | ArtifactBuildError::Overflow => diagnostic(
            "asset.build.budget",
            "Source build exceeded its original allocation or input bounds",
        ),
        ArtifactBuildError::Stale => diagnostic(
            "asset.build.inputs-changed",
            "Source build inputs changed before adoption",
        ),
        ArtifactBuildError::Panicked => {
            diagnostic("asset.build.panicked", "Trusted source build code panicked")
        }
        ArtifactBuildError::Aborted => diagnostic(
            "asset.build.aborted",
            "Source build was cancelled before adoption",
        ),
        ArtifactBuildError::Host(_) => diagnostic(
            "asset.build.source-unavailable",
            "An authorized source build input was unavailable",
        ),
        _ => diagnostic(
            "asset.build.rejected",
            "The source build candidate was rejected",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn record_failure<D: AssetDomain>(
    baseline: AssetMutationBaseline<BuiltSourceAsset<D::Product>>,
    assets: &mut Assets<BuiltSourceAsset<D::Product>>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    diagnostics: &mut AssetReloadDiagnostics,
    stats: &mut SourceAssetBuildStats<D>,
    diagnostic: Diagnostic,
) {
    match baseline.record_failure(assets, states, events, diagnostic.code().as_str()) {
        Ok(_) => {
            stats.failed = stats.failed.saturating_add(1);
            diagnostics.push(diagnostic);
        }
        Err(_) => {
            stats.stale = stats.stale.saturating_add(1);
        }
    }
}
