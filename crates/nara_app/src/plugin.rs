use std::fmt::{self, Display, Formatter};

use nara_ecs::{Resource, World};
use thiserror::Error;

use crate::{App, RuntimeCloseParticipantId, ScheduleCompatibilityError};

mod definition;
mod fingerprint;
mod group;
mod resolve;

pub use definition::{
    PluginConfigurationFingerprint, PluginDefinition, PluginDefinitionId, PluginDefinitionKey,
    PluginPrepareError, PluginPrepareFailure,
};
pub use fingerprint::PluginPlanFingerprint;
pub use group::{
    EditedPluginGroup, EditedPluginGroupMarker, PluginGroup, PluginGroupBuilder, PluginSlot,
    PluginSlotPresence, Plugins, ReplayablePlugins,
};
pub use resolve::{
    AddPluginsError, PluginInstantiationError, PluginPlan, PluginPlanEntry, PluginPlanError,
    ResolvedPluginGroup, SealedApp,
};

pub(crate) use fingerprint::empty_plan_fingerprint;
pub(crate) use resolve::{
    CompositionPrefix, PluginCommitBatch, PluginDefinitionWitness, install_plugins,
    prefix_from_parts,
};

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(&'static str);

        impl $name {
            #[must_use]
            pub const fn new(id: &'static str) -> Self {
                Self(id)
            }

            #[must_use]
            pub const fn as_str(self) -> &'static str {
                self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.0)
            }
        }
    };
}

stable_id!(PluginId);
stable_id!(PluginCapability);
stable_id!(PluginServiceId);
stable_id!(PluginProductCapability);
stable_id!(PluginSchemaProviderId);
stable_id!(PluginShutdownObligationId);
stable_id!(PluginGroupId);
stable_id!(PluginSlotId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginCategory {
    Core,
    Asset,
    Runtime,
    Render,
    Input,
    Platform,
    Service,
    Tooling,
    Backend,
}

/// Static composition facts owned by a plugin type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginDeclaration {
    pub id: PluginId,
    pub category: PluginCategory,
    pub provides: &'static [PluginCapability],
    pub requires_plugins: &'static [PluginId],
    pub requires_capabilities: &'static [PluginCapability],
    pub conflicts: &'static [PluginId],
    pub provides_services: &'static [PluginServiceId],
    pub requires_services: &'static [PluginServiceId],
    pub requires_product_capabilities: &'static [PluginProductCapability],
    pub provides_schema: &'static [PluginSchemaProviderId],
    pub requires_schema: &'static [PluginSchemaProviderId],
    pub shutdown_obligations: &'static [PluginShutdownObligationId],
}

impl PluginDeclaration {
    #[must_use]
    pub const fn new(id: PluginId, category: PluginCategory) -> Self {
        Self {
            id,
            category,
            provides: &[],
            requires_plugins: &[],
            requires_capabilities: &[],
            conflicts: &[],
            provides_services: &[],
            requires_services: &[],
            requires_product_capabilities: &[],
            provides_schema: &[],
            requires_schema: &[],
            shutdown_obligations: &[],
        }
    }

    #[must_use]
    pub const fn provides(mut self, capabilities: &'static [PluginCapability]) -> Self {
        self.provides = capabilities;
        self
    }

    #[must_use]
    pub const fn requires_plugins(mut self, plugins: &'static [PluginId]) -> Self {
        self.requires_plugins = plugins;
        self
    }

    #[must_use]
    pub const fn requires_capabilities(
        mut self,
        capabilities: &'static [PluginCapability],
    ) -> Self {
        self.requires_capabilities = capabilities;
        self
    }

    #[must_use]
    pub const fn conflicts(mut self, plugins: &'static [PluginId]) -> Self {
        self.conflicts = plugins;
        self
    }

    #[must_use]
    pub const fn provides_services(mut self, services: &'static [PluginServiceId]) -> Self {
        self.provides_services = services;
        self
    }

    #[must_use]
    pub const fn requires_services(mut self, services: &'static [PluginServiceId]) -> Self {
        self.requires_services = services;
        self
    }

    #[must_use]
    pub const fn requires_product_capabilities(
        mut self,
        capabilities: &'static [PluginProductCapability],
    ) -> Self {
        self.requires_product_capabilities = capabilities;
        self
    }

    #[must_use]
    pub const fn provides_schema(mut self, providers: &'static [PluginSchemaProviderId]) -> Self {
        self.provides_schema = providers;
        self
    }

    #[must_use]
    pub const fn requires_schema(mut self, providers: &'static [PluginSchemaProviderId]) -> Self {
        self.requires_schema = providers;
        self
    }

    #[must_use]
    pub const fn shutdown_obligations(
        mut self,
        obligations: &'static [PluginShutdownObligationId],
    ) -> Self {
        self.shutdown_obligations = obligations;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleState {
    Configuring,
    Finishing,
    Ready,
    Poisoned,
    ShuttingDown,
    ShutdownComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginHook {
    Preflight,
    Build,
    Finish,
    Shutdown,
}

impl Display for PluginHook {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Preflight => "preflight",
            Self::Build => "build",
            Self::Finish => "finish",
            Self::Shutdown => "shutdown",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginHookMutation {
    PluginMembership,
    RunnerSelection,
}

impl Display for PluginHookMutation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PluginMembership => "plugin membership",
            Self::RunnerSelection => "runner selection",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailure {
    plugin: PluginId,
    hook: PluginHook,
    error: PluginError,
}

impl PluginFailure {
    #[must_use]
    pub const fn plugin(&self) -> PluginId {
        self.plugin
    }

    #[must_use]
    pub const fn hook(&self) -> PluginHook {
        self.hook
    }

    #[must_use]
    pub const fn error(&self) -> &PluginError {
        &self.error
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailureReport {
    pub(crate) primary: Option<PluginFailure>,
    pub(crate) shutdown_failures: Vec<PluginFailure>,
    pub(crate) shutdown_complete: bool,
}

impl PluginFailureReport {
    #[must_use]
    pub const fn primary(&self) -> Option<&PluginFailure> {
        self.primary.as_ref()
    }

    #[must_use]
    pub fn shutdown_failures(&self) -> &[PluginFailure] {
        &self.shutdown_failures
    }

    #[must_use]
    pub const fn shutdown_complete(&self) -> bool {
        self.shutdown_complete
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginShutdownError {
    #[error("plugin shutdown cannot run while a committed plugin hook is active")]
    HookActive,
    #[error("plugin lifecycle or shutdown failed")]
    Failure(Box<PluginFailureReport>),
}

/// Immutable structural view available to a plugin immediately before commit.
pub struct PluginPreflightContext<'plan> {
    entries: &'plan [PluginPlanEntry],
    world: &'plan World,
}

/// Marker for immutable structural state explicitly admitted to plugin preflight.
pub trait PluginPreflightResource: Resource {}

impl PluginPreflightContext<'_> {
    #[must_use]
    pub fn entries(&self) -> &[PluginPlanEntry] {
        self.entries
    }

    #[must_use]
    pub fn has_plugin(&self, plugin: PluginId) -> bool {
        self.entries.iter().any(|entry| entry.plugin_id() == plugin)
    }

    #[must_use]
    pub fn get_structural_resource<R: PluginPreflightResource>(&self) -> Option<&R> {
        self.world.get_resource::<R>()
    }
}

pub struct PluginShutdownContext<'world> {
    world: &'world mut World,
}

impl PluginShutdownContext<'_> {
    #[must_use]
    pub fn world(&self) -> &World {
        self.world
    }

    #[must_use]
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }
}

pub trait Plugin: Send + Sync + 'static {
    fn declaration() -> &'static PluginDeclaration
    where
        Self: Sized;

    fn preflight(&self, _context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError>;

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error("plugin hook {plugin} attempted forbidden {mutation} mutation during {hook}")]
    HookMutationForbidden {
        plugin: PluginId,
        hook: PluginHook,
        mutation: PluginHookMutation,
    },
    #[error("App configuration is sealed")]
    AppSealed,
    #[error("raw mutable access to engine-owned schedules is forbidden")]
    RawBuiltInScheduleMutationForbidden,
    #[error("schedule compatibility validation failed: {0}")]
    ScheduleCompatibility(#[from] ScheduleCompatibilityError),
    #[error("plugin {plugin} failed to initialize: {message}")]
    SetupFailed { plugin: PluginId, message: String },
    #[error("plugin {plugin} failed to register component {component}: {message}")]
    ComponentRegistrationFailed {
        plugin: PluginId,
        component: String,
        message: String,
    },
    #[error("plugin {plugin} panicked during {hook}")]
    HookPanicked { plugin: PluginId, hook: PluginHook },
    #[error("plugin {plugin} preflight rejected after another plugin committed: {source}")]
    CommittedPreflightRejected {
        plugin: PluginId,
        source: Box<PluginError>,
    },
    #[error("plugin shutdown obligation {obligation} was not registered by {plugin}")]
    MissingShutdownObligation {
        plugin: PluginId,
        obligation: PluginShutdownObligationId,
    },
    #[error("plugin {plugin} registered undeclared shutdown obligation {obligation}")]
    UndeclaredShutdownObligation {
        plugin: PluginId,
        obligation: PluginShutdownObligationId,
    },
    #[error("plugin shutdown obligation {obligation} was registered more than once by {plugin}")]
    DuplicateShutdownObligation {
        plugin: PluginId,
        obligation: PluginShutdownObligationId,
    },
    #[error(
        "runtime close participant {participant_id} for plugin {plugin} obligation {obligation} was registered more than once"
    )]
    DuplicateRuntimeCloseParticipant {
        plugin: PluginId,
        obligation: PluginShutdownObligationId,
        participant_id: RuntimeCloseParticipantId,
    },
    #[error("shutdown obligations may only be registered by the active plugin build hook")]
    ShutdownObligationOutsideBuild,
    #[error("app lifecycle is already shut down")]
    LifecycleShutdown,
    #[error("app plugin lifecycle is poisoned")]
    LifecyclePoisoned,
    #[error("plugin finishing cannot be re-entered")]
    FinishReentered,
}

impl PluginError {
    #[must_use]
    pub fn component_registration(
        plugin: PluginId,
        component: impl Into<String>,
        error: impl Display,
    ) -> Self {
        Self::ComponentRegistrationFailed {
            plugin,
            component: component.into(),
            message: error.to_string(),
        }
    }
}

pub(crate) fn preflight_context<'a>(
    entries: &'a [PluginPlanEntry],
    world: &'a World,
) -> PluginPreflightContext<'a> {
    PluginPreflightContext { entries, world }
}

pub(crate) fn shutdown_context(world: &mut World) -> PluginShutdownContext<'_> {
    PluginShutdownContext { world }
}

pub(crate) fn failure(plugin: PluginId, hook: PluginHook, error: PluginError) -> PluginFailure {
    PluginFailure {
        plugin,
        hook,
        error,
    }
}
