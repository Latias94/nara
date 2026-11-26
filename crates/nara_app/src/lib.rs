//! Application startup, frame schedules, and runtime ownership.

use std::{collections::HashMap, sync::Arc, time::Duration};

use bevy_platform::time::Instant;
use nara_ecs::{
    Resource, World,
    bevy_ecs::{
        error::FallbackErrorHandler,
        observer::{IntoObserver, Observer},
        schedule::{
            InternedScheduleLabel, InternedSystemSet, ScheduleBuildSettings, Schedules,
            SingleThreadedExecutor,
        },
        system::ScheduleSystem,
        world::FromWorld,
    },
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, SystemSet},
};
use thiserror::Error;

#[cfg(feature = "diagnostics")]
pub mod diagnostics;
mod execution;
mod failure_metadata;
mod plugin;
mod runtime;
mod scaled_time;
#[cfg(feature = "tasks")]
pub mod tasks;
mod time;

pub use execution::{ExecutionProfile, ExecutionProfileError};
pub use plugin::{
    AddPluginsError, Plugin, PluginCategory, PluginCommitError, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginDomainError, PluginDomainErrorError, PluginError,
    PluginGroup, PluginGroupBuilder, PluginHook, PluginId, PluginPlanEntry, PluginPlanError,
    PluginPreflightContext, PluginPreflightResource, PluginSchemaProviderId, PluginShutdownContext,
    Plugins,
};
pub use runtime::{
    RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseContext,
    RuntimeCloseErrorDisposition, RuntimeCloseEvidence, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeClosePendingParticipant, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlFailure, RuntimeControlRejection, RuntimeControlRequestResult,
    RuntimeControlStatus, RuntimeControlTicket, RuntimeDriveError, RuntimeDriveOutcome,
    RuntimeDriverScope, RuntimeFault, RuntimeFaultDetail, RuntimeFaultDetailError,
    RuntimeFaultKind, RuntimeFaultReporter, RuntimeFrameKind, RuntimeGeneration, RuntimeInstance,
    RuntimeRetirement, RuntimeScheduleError, RuntimeScopeError, RuntimeScopeOutcome, RuntimeState,
    RuntimeValidator, RuntimeWorldAccessError,
};
pub use scaled_time::ScaledTimeRemainder;
pub use time::{
    FixedCatchUpPolicy, FixedTime, FixedTimeError, FrameExecutionStart, RealTime, RenderTime,
    RuntimeFrameStatus, RuntimeTimeContinuation, RuntimeTimeContinuationError,
    RuntimeTimeContinuationRejection, RuntimeTimeSettings, TimeFrameError, TimeSettingsError,
    VirtualTime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct FrameIntent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum FrameIntentSet {
    Prepare,
    Collect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct FrameCleanup;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum StartupStage {
    Core,
    Platform,
    Runtime,
    Scene,
    Tooling,
}

impl StartupStage {
    pub const ALL: [Self; 5] = [
        Self::Core,
        Self::Platform,
        Self::Runtime,
        Self::Scene,
        Self::Tooling,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum CoreStage {
    First,
    TaskUpdate,
    PreUpdate,
    FixedUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
    Queue,
    Sort,
    Render,
    Cleanup,
    Last,
}

impl CoreStage {
    pub const ALL: [Self; 13] = [
        Self::First,
        Self::TaskUpdate,
        Self::PreUpdate,
        Self::FixedUpdate,
        Self::Update,
        Self::PostUpdate,
        Self::Extract,
        Self::Prepare,
        Self::Queue,
        Self::Sort,
        Self::Render,
        Self::Cleanup,
        Self::Last,
    ];

    const fn runs_while_paused(self) -> bool {
        !matches!(
            self,
            Self::PreUpdate | Self::FixedUpdate | Self::Update | Self::PostUpdate
        )
    }
}

fn built_in_schedules() -> impl Iterator<Item = InternedScheduleLabel> {
    StartupStage::ALL
        .into_iter()
        .map(|stage| stage.intern())
        .chain(CoreStage::ALL.into_iter().map(|stage| stage.intern()))
        .chain([FrameIntent.intern(), FrameCleanup.intern()])
}

fn is_built_in_schedule(label: InternedScheduleLabel) -> bool {
    built_in_schedules().any(|built_in| built_in == label)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum FixedUpdateSet {
    Prepare,
    Simulate,
    Finalize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScheduleGraphError {
    #[error("schedule {schedule:?} failed to build: {message}")]
    BuildFailed {
        schedule: InternedScheduleLabel,
        message: String,
    },
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum AppRunError {
    #[error("plugin lifecycle failed: {error}")]
    Plugin {
        #[source]
        error: PluginError,
    },
    #[error("app runner failed: {message}")]
    Runner { message: String },
    #[error("app runner failed ({prior}) and teardown failed ({teardown})")]
    RunnerTeardown {
        prior: Box<AppRunError>,
        teardown: Box<AppRunError>,
    },
    #[error("time frame planning failed: {error}")]
    Time {
        #[source]
        error: TimeFrameError,
    },
    #[error("managed runtime faulted: kind={kind:?}, source={fault_source}")]
    ManagedRuntime {
        kind: RuntimeFaultKind,
        fault_source: &'static str,
        detail: Option<RuntimeFaultDetail>,
    },
}

impl AppRunError {
    pub fn runner(message: impl Into<String>) -> Self {
        Self::Runner {
            message: message.into(),
        }
    }

    pub fn runner_teardown(prior: Self, teardown: Self) -> Self {
        Self::RunnerTeardown {
            prior: Box::new(prior),
            teardown: Box::new(teardown),
        }
    }

    pub const fn managed_runtime(kind: RuntimeFaultKind, fault_source: &'static str) -> Self {
        Self::ManagedRuntime {
            kind,
            fault_source,
            detail: None,
        }
    }

    pub const fn managed_runtime_fault(fault: &RuntimeFault) -> Self {
        Self::ManagedRuntime {
            kind: fault.kind(),
            fault_source: fault.source(),
            detail: fault.detail(),
        }
    }

    pub fn runtime_fault_detail(&self) -> Option<RuntimeFaultDetail> {
        self.runtime_fault_parts().and_then(|(_, _, detail)| detail)
    }

    pub(crate) fn runtime_fault_parts(
        &self,
    ) -> Option<(RuntimeFaultKind, &'static str, Option<RuntimeFaultDetail>)> {
        match self {
            Self::ManagedRuntime {
                kind,
                fault_source,
                detail,
            } => Some((*kind, *fault_source, *detail)),
            Self::RunnerTeardown { prior, teardown } => prior
                .runtime_fault_parts()
                .or_else(|| teardown.runtime_fault_parts()),
            _ => None,
        }
    }

    pub fn plugin_error(&self) -> Option<&PluginError> {
        match self {
            Self::Plugin { error } => Some(error),
            Self::RunnerTeardown { prior, teardown } => {
                prior.plugin_error().or_else(|| teardown.plugin_error())
            }
            _ => None,
        }
    }
}

impl From<PluginError> for AppRunError {
    fn from(error: PluginError) -> Self {
        Self::Plugin { error }
    }
}

impl From<TimeFrameError> for AppRunError {
    fn from(error: TimeFrameError) -> Self {
        Self::Time { error }
    }
}

impl From<FixedTimeError> for AppRunError {
    fn from(error: FixedTimeError) -> Self {
        TimeFrameError::from(error).into()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppExit {
    #[default]
    Success,
    Requested,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct AppExitRequests(Option<AppExit>);

impl AppExitRequests {
    pub fn request(&mut self, exit: AppExit) {
        self.0 = Some(exit);
    }

    pub fn request_success(&mut self) {
        self.request(AppExit::Success);
    }

    pub fn request_exit(&mut self) {
        self.request(AppExit::Requested);
    }

    pub const fn requested(&self) -> Option<AppExit> {
        self.0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AppFrameOutcome {
    pub exit: Option<AppExit>,
    pub status: RuntimeFrameStatus,
}

type Runner = Box<dyn FnOnce(App) -> Result<AppExit, AppRunError>>;

/// Owns an ECS world and its startup, fixed-step, and frame schedules.
pub struct App {
    close: runtime::Close,
    plugins: Vec<plugin::InstalledPlugin>,
    schedules: Schedules,
    validators: Vec<(PluginId, Arc<RuntimeValidator>)>,
    observers: Vec<Observer>,
    final_deferred: HashMap<InternedScheduleLabel, bool>,
    reporter: RuntimeFaultReporter,
    generation: Option<RuntimeGeneration>,
    configuration_error: Option<PluginError>,
    in_hook: bool,
    execution_profile: ExecutionProfile,
    runner: Option<Runner>,
    previous_update: Option<Instant>,
    world: World,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut world = World::new();
        world.init_resource::<RealTime>();
        world.init_resource::<VirtualTime>();
        world.init_resource::<FixedTime>();
        world.init_resource::<RenderTime>();
        world.init_resource::<RuntimeTimeSettings>();
        world.init_resource::<RuntimeFrameStatus>();
        world.init_resource::<AppExitRequests>();
        let reporter = RuntimeFaultReporter::new();
        world.insert_resource(reporter.clone());

        let mut schedules = Schedules::new();
        for label in built_in_schedules() {
            let mut schedule = Schedule::new(label);
            schedule.set_executor(SingleThreadedExecutor::new());
            schedules.insert(schedule);
        }
        schedules.configure_sets(
            CoreStage::FixedUpdate,
            (
                FixedUpdateSet::Prepare,
                FixedUpdateSet::Simulate,
                FixedUpdateSet::Finalize,
            )
                .chain(),
        );
        schedules.configure_sets(
            FrameIntent,
            (FrameIntentSet::Prepare, FrameIntentSet::Collect).chain(),
        );
        Self {
            close: runtime::Close::default(),
            plugins: Vec::new(),
            schedules,
            validators: Vec::new(),
            observers: Vec::new(),
            final_deferred: HashMap::new(),
            reporter,
            generation: None,
            configuration_error: None,
            in_hook: false,
            execution_profile: ExecutionProfile::Serial,
            runner: None,
            previous_update: None,
            world,
        }
    }

    pub fn set_runner(
        &mut self,
        runner: impl FnOnce(App) -> Result<AppExit, AppRunError> + 'static,
    ) -> &mut Self {
        self.runner = Some(Box::new(runner));
        self
    }

    /// Runs the selected platform runner or one frame when no runner is installed.
    pub fn run(&mut self) -> Result<AppExit, AppRunError> {
        if self.in_hook {
            return Err(PluginError::ConfigurationInProgress.into());
        }
        if let Some(runner) = self.runner.take() {
            return runner(std::mem::take(self));
        }
        Ok(self.update()?.exit.unwrap_or_default())
    }

    pub fn update(&mut self) -> Result<AppFrameOutcome, AppRunError> {
        let now = Instant::now();
        let delta = self
            .previous_update
            .replace(now)
            .map_or(Duration::ZERO, |previous| {
                now.saturating_duration_since(previous)
            });
        self.update_with_delta(delta)
    }

    pub fn update_with_delta(&mut self, delta: Duration) -> Result<AppFrameOutcome, AppRunError> {
        self.drive_headless(delta, None)
    }

    /// Runs one fixed tick and the intent and cleanup schedules. Any elapsed-time debt is preserved.
    pub fn step_fixed(&mut self, delta: Duration) -> Result<AppFrameOutcome, AppRunError> {
        if delta.is_zero() {
            return Err(FixedTimeError::ZeroStep.into());
        }
        self.drive_headless(delta, Some(delta))
    }

    fn drive_headless(
        &mut self,
        delta: Duration,
        step: Option<Duration>,
    ) -> Result<AppFrameOutcome, AppRunError> {
        self.start()?;
        self.run_frame(delta, false, step).map_err(|error| {
            AppRunError::managed_runtime_fault(&self.reporter.record(RuntimeFault::app(error)))
        })
    }

    pub const fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> Result<&mut World, PluginError> {
        self.check_open()?;
        Ok(&mut self.world)
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> Result<&mut Self, PluginError> {
        self.world_mut()?.insert_resource(resource);
        Ok(self)
    }

    pub fn init_resource<R: Resource + FromWorld>(&mut self) -> Result<&mut Self, PluginError> {
        self.world_mut()?.init_resource::<R>();
        Ok(self)
    }

    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.schedules.add_systems(schedule, systems);
        Ok(self)
    }

    pub fn configure_sets<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        sets: impl IntoScheduleConfigs<InternedSystemSet, M>,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.schedules.configure_sets(schedule, sets);
        Ok(self)
    }

    pub fn add_observer<M>(
        &mut self,
        observer: impl IntoObserver<M>,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.observers.push(observer.into_observer());
        Ok(self)
    }

    pub fn init_schedule(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.schedules.entry(schedule);
        Ok(self)
    }

    pub fn set_schedule_build_settings(
        &mut self,
        schedule: impl ScheduleLabel,
        settings: ScheduleBuildSettings,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.schedules.entry(schedule).set_build_settings(settings);
        Ok(self)
    }

    pub fn set_schedule_apply_final_deferred(
        &mut self,
        schedule: impl ScheduleLabel,
        apply: bool,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        let schedule = schedule.intern();
        if is_built_in_schedule(schedule) {
            self.final_deferred.insert(schedule, apply);
        }
        self.schedules
            .entry(schedule)
            .set_apply_final_deferred(apply);
        Ok(self)
    }

    pub fn get_schedule(&self, schedule: impl ScheduleLabel) -> Option<&Schedule> {
        self.schedules.get(schedule)
    }

    pub fn get_schedule_mut(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<Option<&mut Schedule>, PluginError> {
        self.check_configuration()?;
        Ok(self.schedules.get_mut(schedule))
    }

    pub fn add_plugins<M>(
        &mut self,
        inputs: impl Plugins<M>,
    ) -> Result<&mut Self, AddPluginsError> {
        self.check_configuration()?;
        if self.in_hook {
            return Err(PluginError::ConfigurationInProgress.into());
        }
        let plugins = plugin::prepare(inputs, &self.plugins)?;
        let entries: Vec<_> = self
            .plugins
            .iter()
            .chain(&plugins)
            .map(|installed| installed.entry)
            .collect();
        for (index, installed) in plugins.into_iter().enumerate() {
            let id = installed.entry.plugin_id();
            let context = PluginPreflightContext {
                entries: &entries,
                world: &self.world,
            };
            if let Err(error) = plugin::run_hook(id, PluginHook::Preflight, || {
                installed.plugin.preflight(&context)
            }) {
                if index == 0 && !matches!(error, PluginError::HookPanicked { .. }) {
                    return Err(error.into());
                }
                let error = PluginError::CommittedPreflightRejected {
                    plugin: id,
                    source: Box::new(error),
                };
                return Err(self.fail_configuration(error).into());
            }
            let plugin = installed.plugin.clone();
            self.plugins.push(installed);
            self.in_hook = true;
            let result = plugin::run_hook(id, PluginHook::Build, || plugin.build(self));
            self.in_hook = false;
            if let Err(error) = result {
                return Err(self.fail_configuration(error).into());
            }
        }
        Ok(self)
    }

    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> Result<&mut Self, AddPluginsError> {
        self.add_plugins(plugin)
    }

    pub fn has_plugin(&self, id: PluginId) -> bool {
        self.plugins
            .iter()
            .any(|plugin| plugin.entry.plugin_id() == id)
    }

    pub fn installed_plugins(&self) -> impl Iterator<Item = PluginDeclaration> + '_ {
        self.plugins
            .iter()
            .map(|plugin| *plugin.entry.declaration())
    }

    pub fn register_runtime_validator(
        &mut self,
        plugin: PluginId,
        validator: Arc<RuntimeValidator>,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        if self.validators.iter().any(|(owner, _)| *owner == plugin) {
            return Err(PluginError::SetupFailed {
                plugin,
                message: "a runtime validator is already registered for this plugin".to_owned(),
            });
        }
        self.validators.push((plugin, validator));
        Ok(self)
    }

    pub fn register_close_participant(
        &mut self,
        id: RuntimeCloseParticipantId,
        participant: impl RuntimeCloseParticipant,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.close.register(id, participant)?;
        Ok(self)
    }

    pub const fn execution_profile(&self) -> ExecutionProfile {
        self.execution_profile
    }

    pub fn set_execution_profile(
        &mut self,
        profile: ExecutionProfile,
    ) -> Result<&mut Self, PluginError> {
        self.check_configuration()?;
        self.execution_profile = profile;
        Ok(self)
    }

    pub fn finish(self) -> Result<RuntimeCloseEvidence, RuntimeRetirement> {
        RuntimeRetirement::new(self, true).finish()
    }

    fn check_open(&self) -> Result<(), PluginError> {
        if let Some(error) = &self.configuration_error {
            return Err(error.clone());
        }
        if !self.close.is_open() {
            return Err(PluginError::LifecycleShutdown);
        }
        Ok(())
    }

    fn check_configuration(&self) -> Result<(), PluginError> {
        self.check_open()?;
        if self.generation.is_some() {
            return Err(PluginError::AppSealed);
        }
        Ok(())
    }

    fn fail_configuration(&mut self, error: PluginError) -> PluginError {
        self.configuration_error = Some(error.clone());
        self.close.begin();
        self.drive_close();
        error
    }

    fn finish_configuration(&mut self) -> Result<(), PluginError> {
        for index in 0..self.plugins.len() {
            let installed = &self.plugins[index];
            let plugin = installed.plugin.clone();
            let id = installed.entry.plugin_id();
            self.in_hook = true;
            let result = plugin::run_hook(id, PluginHook::Finish, || plugin.finish(self));
            self.in_hook = false;
            result?;
        }
        self.execution_profile.install(&mut self.schedules)?;
        for (label, apply) in &self.final_deferred {
            self.schedules
                .entry(*label)
                .set_apply_final_deferred(*apply);
        }
        for stage in [CoreStage::FixedUpdate, CoreStage::Cleanup] {
            let schedule = self.schedules.entry(stage);
            if !schedule.get_build_settings().auto_insert_apply_deferred {
                return Err(PluginError::ScheduleCompatibility { schedule: stage });
            }
            schedule.set_apply_final_deferred(true);
        }
        for (_, schedule) in self.schedules.iter_mut() {
            if let Err(error) = schedule.initialize(&mut self.world) {
                return Err(ScheduleGraphError::BuildFailed {
                    schedule: schedule.label(),
                    message: error.to_string(schedule.graph(), &self.world),
                }
                .into());
            }
        }
        Ok(())
    }

    fn start(&mut self) -> Result<RuntimeGeneration, AppRunError> {
        if self.in_hook {
            return Err(PluginError::ConfigurationInProgress.into());
        }
        self.check_open()?;
        if let Some(fault) = self.reporter.fault_ref() {
            return Err(AppRunError::managed_runtime_fault(fault));
        }
        if let Some(generation) = self.generation {
            self.check_health()
                .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
            return Ok(generation);
        }
        if let Err(error) = self.finish_configuration() {
            return Err(self.fail_configuration(error).into());
        }
        self.check_health()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        let generation = RuntimeGeneration::allocate()?;
        self.generation = Some(generation);
        self.world.insert_resource(generation);
        self.world
            .insert_resource(FallbackErrorHandler(runtime::handle_execution_error));
        runtime::execute(|| {
            for observer in std::mem::take(&mut self.observers) {
                self.world
                    .spawn(observer.with_error_handler(runtime::handle_execution_error));
            }
            self.world.flush();
        })
        .map_err(|fault| AppRunError::managed_runtime_fault(&self.reporter.record(fault)))?;
        for stage in StartupStage::ALL {
            self.run_schedule(stage)
                .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        }
        time::FrameTime::plan(&self.world, Duration::ZERO, false, None)
            .map_err(|error| self.reporter.record(RuntimeFault::app(error.into())))
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        Ok(generation)
    }

    fn check_health(&self) -> Result<(), RuntimeFault> {
        if let Some(fault) = self.reporter.fault() {
            return Err(fault);
        }
        for (_, validate) in &self.validators {
            validate(&self.world, self.generation).map_err(|fault| self.reporter.record(fault))?;
        }
        match self.reporter.fault() {
            Some(fault) => Err(fault),
            None => Ok(()),
        }
    }

    fn run_schedule(&mut self, schedule: impl ScheduleLabel) -> Result<(), RuntimeFault> {
        self.check_health()?;
        runtime::execute(|| {
            if let Some(schedule) = self.schedules.get_mut(schedule) {
                schedule.run(&mut self.world);
            }
        })
        .map_err(|fault| self.reporter.record(fault))?;
        self.check_health()
    }

    fn run_frame(
        &mut self,
        delta: Duration,
        paused: bool,
        exact_step: Option<Duration>,
    ) -> Result<AppFrameOutcome, AppRunError> {
        self.check_health()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        let started_at = Instant::now();
        let mut time = time::FrameTime::plan(&self.world, delta, paused, exact_step)?;
        time.start(&mut self.world, started_at);
        let status = if exact_step.is_some() {
            self.frame_schedule(FrameIntent)?;
            self.frame_schedule(CoreStage::FixedUpdate)?;
            time.settle(&mut self.world)?
        } else {
            let mut status = RuntimeFrameStatus::default();
            for stage in CoreStage::ALL {
                if stage == CoreStage::FixedUpdate {
                    for _ in 0..time.steps {
                        time.fixed.consume_tick();
                        *time::get_mut::<FixedTime>(&mut self.world)? = time.fixed;
                        self.frame_schedule(stage)?;
                    }
                    status = time.settle(&mut self.world)?;
                } else if !paused || stage.runs_while_paused() {
                    self.frame_schedule(stage)?;
                    if stage == CoreStage::PreUpdate {
                        self.frame_schedule(FrameIntent)?;
                    }
                }
            }
            status
        };
        *time::get_mut::<RuntimeFrameStatus>(&mut self.world)? = status;
        self.frame_schedule(FrameCleanup)?;
        let exit = time::get_mut::<AppExitRequests>(&mut self.world)?.0.take();
        runtime::execute(|| self.world.clear_trackers())
            .map_err(|fault| AppRunError::managed_runtime_fault(&self.reporter.record(fault)))?;
        self.check_health()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        Ok(AppFrameOutcome { exit, status })
    }

    fn frame_schedule(&mut self, stage: impl ScheduleLabel) -> Result<(), AppRunError> {
        self.run_schedule(stage)
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))
    }

    fn drive_close(&mut self) {
        self.close.drive(&mut self.world, &self.plugins);
    }

    fn finish_close(&mut self) {
        self.close.finish(&mut self.world, &self.plugins);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.finish_close()));
    }
}
