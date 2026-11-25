use std::sync::{Arc, atomic::Ordering};

use nara_ecs::Resource;

use crate::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginId, PluginShutdownObligationId, RuntimeCloseContext,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeCloseProgress,
};

use super::{
    PhysicalOwner, TaskInlineRunReport, TaskPoolConfig, TaskPoolError, TaskPoolKind, TaskPools,
    TaskShutdownError, TaskShutdownReport, TaskStats, lock, result::Activity,
};

/// Host-owned facilities that may serve several independently retired runtime activities.
#[derive(Debug)]
pub struct TaskFacilities {
    pools: TaskPools,
}

impl TaskFacilities {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::try_new(config).map(|pools| Self { pools })
    }

    pub fn try_new_lazy(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::try_new_lazy(config).map(|pools| Self { pools })
    }

    pub fn inline_for_tests(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        TaskPools::inline_for_tests(config).map(|pools| Self { pools })
    }

    pub fn config(&self) -> &TaskPoolConfig {
        self.pools.config()
    }

    pub fn stats(&self) -> TaskStats {
        self.pools.stats()
    }

    pub fn scope(&self) -> TaskScope {
        TaskScope {
            pools: self.pools.facade(Arc::new(Activity::default()), true),
        }
    }

    pub fn run_pending_for_tests(&self) -> TaskInlineRunReport {
        self.pools.run_pending_for_tests()
    }

    pub fn shutdown_blocking(&self) -> Result<TaskShutdownReport, TaskShutdownError> {
        self.pools.shutdown_blocking()
    }
}

/// Owns admission and physical work for one activity on shared facilities.
#[derive(Debug)]
pub struct TaskScope {
    pools: TaskPools,
}

impl TaskScope {
    pub fn spawner(&self) -> TaskPools {
        self.pools.facade(self.pools.activity.clone(), true)
    }

    pub fn begin_close(&mut self) {
        self.pools.facility.close_scope(&self.pools.activity);
    }

    pub fn poll_close(&mut self) -> TaskScopeCloseReport {
        self.begin_close();
        let state = lock(&self.pools.facility.state);
        TaskScopeCloseReport {
            pending: self.pools.activity.pending.load(Ordering::Relaxed),
            cleanup_failed: self.pools.cleanup_failed(&state),
        }
    }
}

impl Drop for TaskScope {
    fn drop(&mut self) {
        self.begin_close();
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskScopeCloseReport {
    pub pending: usize,
    pub cleanup_failed: bool,
}

impl TaskScopeCloseReport {
    pub const fn is_complete(self) -> bool {
        self.pending == 0
    }
}

#[derive(Debug, Default, Clone)]
pub struct TaskPlugin {
    config: TaskPoolConfig,
    facilities: Placement,
}

#[derive(Debug, Default, Clone)]
enum Placement {
    #[default]
    Owned,
    Lazy,
    Shared(Arc<TaskFacilities>),
}

pub const TASK_PLUGIN_ID: PluginId = PluginId::new("nara.tasks");
const TASK_PLUGIN_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.tasks.configured", 1);
pub const SHARED_TASK_PLUGIN_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.tasks.shared", 1);
pub const TASK_POOLS_SHUTDOWN_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.tasks.pools");
pub const TASK_POOLS_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.tasks.pools");
pub const TASK_SCOPE_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.tasks.scope");
pub const TASK_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TASK_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[TASK_POOLS_SHUTDOWN_OBLIGATION]);

impl TaskPlugin {
    /// Installs facilities on first use, or respects a preinstalled host-owned pool.
    pub const fn new(config: TaskPoolConfig) -> Self {
        Self {
            config,
            facilities: Placement::Owned,
        }
    }

    /// Installs private first-use facilities and requires an unoccupied task resource.
    pub const fn lazy(config: TaskPoolConfig) -> Self {
        Self {
            config,
            facilities: Placement::Lazy,
        }
    }

    pub fn shared(facilities: Arc<TaskFacilities>) -> Self {
        Self {
            config: *facilities.config(),
            facilities: Placement::Shared(facilities),
        }
    }
}

pub fn plugin(config: TaskPoolConfig) -> PluginDefinition {
    definition(TaskPlugin::new(config))
}

pub fn lazy_plugin(config: TaskPoolConfig) -> PluginDefinition {
    definition(TaskPlugin::lazy(config))
}

pub fn shared_plugin(facilities: Arc<TaskFacilities>) -> PluginDefinition {
    definition(TaskPlugin::shared(facilities))
}

fn definition(plugin: TaskPlugin) -> PluginDefinition {
    let (definition, placement) = match &plugin.facilities {
        Placement::Owned => (TASK_PLUGIN_DEFINITION_ID, 0),
        Placement::Lazy => (TASK_PLUGIN_DEFINITION_ID, 1),
        Placement::Shared(_) => (SHARED_TASK_PLUGIN_DEFINITION_ID, 2),
    };
    let mut bytes = b"nara.tasks.plugin-config.v1\0".to_vec();
    bytes.push(placement);
    for kind in TaskPoolKind::ALL {
        let kind = plugin.config.kind(kind);
        bytes.extend_from_slice(&(kind.workers().get() as u64).to_le_bytes());
        bytes.extend_from_slice(&(kind.pending().get() as u64).to_le_bytes());
    }
    let shutdown = plugin.config.shutdown_policy();
    for timeout in [
        shutdown.drain_timeout(),
        shutdown.cancel_timeout(),
        shutdown.join_timeout(),
    ] {
        bytes.extend_from_slice(&timeout.get().as_secs().to_le_bytes());
        bytes.extend_from_slice(&timeout.get().subsec_nanos().to_le_bytes());
    }
    PluginDefinition::infallible::<TaskPlugin, _>(definition, bytes, move || plugin.clone())
}

impl Plugin for TaskPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TASK_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if app.world().contains_resource::<TaskPools>() {
            return match self.facilities {
                Placement::Owned => app
                    .register_plugin_shutdown_obligation(TASK_POOLS_SHUTDOWN_OBLIGATION)
                    .map(|_| ()),
                _ => Err(PluginError::SetupFailed {
                    plugin: TASK_PLUGIN_ID,
                    message: "task installation requires an unoccupied task facade".to_owned(),
                }),
            };
        }
        if let Placement::Shared(facilities) = &self.facilities {
            let scope = facilities.scope();
            let pools = scope.spawner();
            app.register_plugin_runtime_close_participant(
                TASK_POOLS_SHUTDOWN_OBLIGATION,
                TASK_SCOPE_CLOSE_PARTICIPANT,
                scope,
            )?;
            app.insert_resource(pools)?;
            return Ok(());
        }
        let mut pools =
            TaskPools::try_new_lazy(self.config).map_err(|error| PluginError::SetupFailed {
                plugin: TASK_PLUGIN_ID,
                message: error.to_string(),
            })?;
        let participant = PoolCloseParticipant {
            activity: pools.activity.clone(),
            owner: pools
                .owner
                .take()
                .expect("new facilities provide their physical owner"),
        };
        app.register_plugin_runtime_close_participant(
            TASK_POOLS_SHUTDOWN_OBLIGATION,
            TASK_POOLS_CLOSE_PARTICIPANT,
            participant,
        )?;
        app.insert_resource(pools)?;
        Ok(())
    }
}

struct PoolCloseParticipant {
    activity: Arc<Activity>,
    owner: PhysicalOwner,
}

impl RuntimeCloseParticipant for PoolCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.owner.begin_close();
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let (report, timed_out) = self.owner.poll();
        let complete = report.is_complete();
        let failed = report.has_cleanup_failure();
        context
            .insert_resource(report)
            .map_err(|_| resource_access_error())?;
        if complete {
            retire_facade(context, &self.activity)?;
            Ok(close_complete(failed, "nara.tasks.cleanup-failed"))
        } else if timed_out {
            Err(RuntimeCloseParticipantError::new(
                "nara.tasks.close-deadline",
            ))
        } else {
            Ok(RuntimeCloseProgress::Pending)
        }
    }
}

impl RuntimeCloseParticipant for TaskScope {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        TaskScope::begin_close(self);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let report = TaskScope::poll_close(self);
        context
            .insert_resource(report)
            .map_err(|_| resource_access_error())?;
        if report.is_complete() {
            retire_facade(context, &self.pools.activity)?;
            Ok(close_complete(
                report.cleanup_failed,
                "nara.tasks.scope-cleanup-failed",
            ))
        } else {
            Ok(RuntimeCloseProgress::Pending)
        }
    }
}

fn retire_facade(
    context: &mut RuntimeCloseContext<'_>,
    activity: &Arc<Activity>,
) -> Result<(), RuntimeCloseParticipantError> {
    if context
        .world()
        .get_resource::<TaskPools>()
        .is_some_and(|pools| Arc::ptr_eq(&pools.activity, activity))
    {
        context
            .remove_resource::<TaskPools>()
            .map_err(|_| resource_access_error())?;
    }
    Ok(())
}

fn resource_access_error() -> RuntimeCloseParticipantError {
    RuntimeCloseParticipantError::terminal("nara.tasks.runtime-resource-access")
}

fn close_complete(failed: bool, code: &'static str) -> RuntimeCloseProgress {
    if failed {
        RuntimeCloseProgress::CompleteWithError { code }
    } else {
        RuntimeCloseProgress::Complete
    }
}
