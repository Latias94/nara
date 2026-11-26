use std::{
    any::{TypeId, type_name},
    collections::HashMap,
    fmt::{self, Display, Formatter},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_ecs::{Resource, World};
use thiserror::Error;

use crate::{App, RuntimeCloseParticipantId, ScheduleGraphError};

mod domain_error;
pub use domain_error::{PluginDomainError, PluginDomainErrorError};

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(&'static str);

        impl $name {
            pub const fn new(id: &'static str) -> Self {
                Self(id)
            }

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

identifier!(PluginId);
identifier!(PluginSchemaProviderId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginDeclaration {
    pub id: PluginId,
    pub category: PluginCategory,
    pub requires_plugins: &'static [PluginId],
    pub conflicts: &'static [PluginId],
    pub provides_schema: &'static [PluginSchemaProviderId],
    pub requires_schema: &'static [PluginSchemaProviderId],
}

impl PluginDeclaration {
    pub const fn new(id: PluginId, category: PluginCategory) -> Self {
        Self {
            id,
            category,
            requires_plugins: &[],
            conflicts: &[],
            provides_schema: &[],
            requires_schema: &[],
        }
    }

    pub const fn requires_plugins(mut self, plugins: &'static [PluginId]) -> Self {
        self.requires_plugins = plugins;
        self
    }

    pub const fn conflicts(mut self, plugins: &'static [PluginId]) -> Self {
        self.conflicts = plugins;
        self
    }

    pub const fn provides_schema(mut self, providers: &'static [PluginSchemaProviderId]) -> Self {
        self.provides_schema = providers;
        self
    }

    pub const fn requires_schema(mut self, providers: &'static [PluginSchemaProviderId]) -> Self {
        self.requires_schema = providers;
        self
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginHook {
    Preflight,
    Build,
    Finish,
    Shutdown,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error(transparent)]
    Domain(#[from] PluginDomainError),
    #[error(transparent)]
    ExecutionProfile(#[from] crate::ExecutionProfileError),
    #[error("App configuration is sealed")]
    AppSealed,
    #[error("App configuration is already in progress")]
    ConfigurationInProgress,
    #[error("the App is closing")]
    LifecycleShutdown,
    #[error(transparent)]
    ScheduleGraph(#[from] ScheduleGraphError),
    #[error("schedule {schedule:?} requires automatic deferred insertion")]
    ScheduleCompatibility { schedule: crate::CoreStage },
    #[error("plugin {plugin} failed to initialize: {message}")]
    SetupFailed { plugin: PluginId, message: String },
    #[error("plugin {plugin} failed to register component {component}: {message}")]
    ComponentRegistrationFailed {
        plugin: PluginId,
        component: String,
        message: String,
    },
    #[error("plugin {plugin} panicked during {hook:?}")]
    HookPanicked { plugin: PluginId, hook: PluginHook },
    #[error("plugin {plugin} has a failed preflight after another plugin was built: {source}")]
    CommittedPreflightRejected {
        plugin: PluginId,
        source: Box<PluginError>,
    },
    #[error("the runtime already has close participant {id}")]
    DuplicateCloseParticipant { id: RuntimeCloseParticipantId },
}

impl PluginError {
    pub fn domain_error(&self) -> Option<&PluginDomainError> {
        match self {
            Self::Domain(error) => Some(error),
            Self::CommittedPreflightRejected { source, .. } => source.domain_error(),
            _ => None,
        }
    }

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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error(transparent)]
pub struct PluginCommitError(#[from] PluginError);

impl PluginCommitError {
    pub const fn error(&self) -> &PluginError {
        &self.0
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AddPluginsError {
    #[error(transparent)]
    Plan(#[from] PluginPlanError),
    #[error(transparent)]
    Plugin(#[from] PluginCommitError),
}

impl From<PluginError> for AddPluginsError {
    fn from(error: PluginError) -> Self {
        Self::Plugin(error.into())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginPlanError {
    #[error("plugin {plugin} is included more than once with different installations")]
    Duplicate { plugin: PluginId },
    #[error("plugin {plugin} requires {required}")]
    MissingPlugin {
        plugin: PluginId,
        required: PluginId,
    },
    #[error("plugin {plugin} requires schema provider {required}")]
    MissingSchema {
        plugin: PluginId,
        required: PluginSchemaProviderId,
    },
    #[error("plugins {plugin} and {other} both provide schema {provider}")]
    DuplicateSchema {
        plugin: PluginId,
        other: PluginId,
        provider: PluginSchemaProviderId,
    },
    #[error("plugins {plugin} and {other} conflict")]
    Conflict { plugin: PluginId, other: PluginId },
    #[error("plugin {plugin} participates in an unresolved dependency cycle")]
    DependencyCycle { plugin: PluginId },
    #[error("a plugin declaration or group builder panicked")]
    DeclarationPanicked,
    #[error("plugin group {group} contains itself")]
    GroupCycle { group: &'static str },
    #[error("plugin {plugin} factory panicked")]
    FactoryPanicked { plugin: PluginId },
}

pub trait PluginPreflightResource: Resource {}

pub struct PluginPreflightContext<'a> {
    pub(crate) entries: &'a [PluginPlanEntry],
    pub(crate) world: &'a World,
}

impl PluginPreflightContext<'_> {
    pub fn entries(&self) -> &[PluginPlanEntry] {
        self.entries
    }

    pub fn has_plugin(&self, plugin: PluginId) -> bool {
        self.entries.iter().any(|entry| entry.plugin_id() == plugin)
    }

    pub fn get_structural_resource<R: PluginPreflightResource>(&self) -> Option<&R> {
        self.world.get_resource()
    }
}

pub struct PluginShutdownContext<'a> {
    pub(crate) world: &'a mut World,
}

impl PluginShutdownContext<'_> {
    pub fn world(&self) -> &World {
        self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginPlanEntry(&'static PluginDeclaration);

impl PluginPlanEntry {
    pub const fn plugin_id(&self) -> PluginId {
        self.0.id
    }

    pub const fn declaration(&self) -> &'static PluginDeclaration {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginDefinitionId(&'static str, u32);

impl PluginDefinitionId {
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self(id, version)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct DefinitionKey {
    plugin_type: TypeId,
    factory_type: TypeId,
    id: Option<PluginDefinitionId>,
    configuration: Arc<[u8]>,
}

type Factory = dyn Fn() -> Result<Arc<dyn Plugin>, PluginDomainError> + Send + Sync;

#[derive(Clone)]
pub struct PluginDefinition {
    declaration: fn() -> &'static PluginDeclaration,
    key: DefinitionKey,
    factory: Arc<Factory>,
}

impl PluginDefinition {
    pub fn for_default<P: Plugin + Default>() -> Self {
        Self::factory::<P, _>(None, &[], || Ok(P::default()))
    }

    pub fn infallible<P, F>(
        id: PluginDefinitionId,
        configuration: impl AsRef<[u8]>,
        factory: F,
    ) -> Self
    where
        P: Plugin,
        F: Fn() -> P + Send + Sync + 'static,
    {
        Self::fallible(id, configuration, move || Ok(factory()))
    }

    pub fn fallible<P, F>(
        id: PluginDefinitionId,
        configuration: impl AsRef<[u8]>,
        factory: F,
    ) -> Self
    where
        P: Plugin,
        F: Fn() -> Result<P, PluginDomainError> + Send + Sync + 'static,
    {
        Self::factory(Some(id), configuration.as_ref(), factory)
    }

    fn factory<P, F>(id: Option<PluginDefinitionId>, configuration: &[u8], factory: F) -> Self
    where
        P: Plugin,
        F: Fn() -> Result<P, PluginDomainError> + Send + Sync + 'static,
    {
        Self {
            declaration: P::declaration,
            key: DefinitionKey {
                plugin_type: TypeId::of::<P>(),
                factory_type: TypeId::of::<F>(),
                id,
                configuration: configuration.into(),
            },
            factory: Arc::new(move || Ok(Arc::new(factory()?))),
        }
    }
}

enum PluginInput {
    Value(fn() -> &'static PluginDeclaration, Arc<dyn Plugin>),
    Definition(PluginDefinition),
    Group(
        TypeId,
        &'static str,
        Box<dyn FnOnce() -> PluginGroupBuilder>,
    ),
}

pub trait PluginGroup: Sized + 'static {
    fn build(self) -> PluginGroupBuilder;
}

#[derive(Default)]
pub struct PluginGroupBuilder(Vec<PluginInput>);

impl PluginGroupBuilder {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add<P: Plugin>(mut self, plugin: P) -> Self {
        self.0
            .push(PluginInput::Value(P::declaration, Arc::new(plugin)));
        self
    }

    pub fn add_default<P: Plugin + Default>(self) -> Self {
        self.add_definition(PluginDefinition::for_default::<P>())
    }

    pub fn add_definition(mut self, definition: PluginDefinition) -> Self {
        self.0.push(PluginInput::Definition(definition));
        self
    }

    pub fn add_group<G: PluginGroup>(mut self, group: G) -> Self {
        self.0.push(PluginInput::Group(
            TypeId::of::<G>(),
            type_name::<G>(),
            Box::new(move || group.build()),
        ));
        self
    }
}

pub trait Plugins<M> {
    #[doc(hidden)]
    fn collect(self, plugins: PluginGroupBuilder) -> PluginGroupBuilder;
}

#[doc(hidden)]
pub struct PluginMarker;
#[doc(hidden)]
pub struct GroupMarker;

impl<P: Plugin> Plugins<PluginMarker> for P {
    fn collect(self, plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins.add(self)
    }
}

impl<G: PluginGroup> Plugins<GroupMarker> for G {
    fn collect(self, plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins.add_group(self)
    }
}

impl Plugins<PluginDefinition> for PluginDefinition {
    fn collect(self, plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins.add_definition(self)
    }
}

impl Plugins<PluginGroupBuilder> for PluginGroupBuilder {
    fn collect(self, mut plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins.0.extend(self.0);
        plugins
    }
}

impl Plugins<()> for () {
    fn collect(self, plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins
    }
}

macro_rules! tuple_plugins {
    ([$(($plugin:ident, $marker:ident)),+] $next:tt $($rest:tt)*) => {
        tuple_plugins!([$(($plugin, $marker)),+]);
        tuple_plugins!([$(($plugin, $marker)),+, $next] $($rest)*);
    };
    ([$(($plugin:ident, $marker:ident)),+]) => {
        impl<$($plugin, $marker),+> Plugins<($($marker,)+)> for ($($plugin,)+)
        where $($plugin: Plugins<$marker>,)+
        {
            #[allow(non_snake_case)]
            fn collect(self, mut plugins: PluginGroupBuilder) -> PluginGroupBuilder {
                let ($($plugin,)+) = self;
                $(plugins = $plugin.collect(plugins);)+
                plugins
            }
        }
    };
}

tuple_plugins!([(P1, M1)](P2, M2)(P3, M3)(P4, M4)(P5, M5)(P6, M6)(P7, M7)(
    P8, M8
)(P9, M9)(P10, M10)(P11, M11));

pub(crate) struct InstalledPlugin {
    pub(crate) entry: PluginPlanEntry,
    pub(crate) plugin: Arc<dyn Plugin>,
    key: Option<DefinitionKey>,
}

enum Materializer {
    Value(Arc<dyn Plugin>),
    Definition(PluginDefinition),
}

struct PendingPlugin {
    entry: PluginPlanEntry,
    materializer: Materializer,
}

impl PendingPlugin {
    fn key(&self) -> Option<&DefinitionKey> {
        match &self.materializer {
            Materializer::Definition(definition) => Some(&definition.key),
            Materializer::Value(_) => None,
        }
    }

    fn prepare(self) -> Result<InstalledPlugin, AddPluginsError> {
        let (plugin, key) = match self.materializer {
            Materializer::Value(plugin) => (plugin, None),
            Materializer::Definition(definition) => {
                let plugin = catch_unwind(AssertUnwindSafe(|| (definition.factory)()))
                    .map_err(|_| PluginPlanError::FactoryPanicked {
                        plugin: self.entry.plugin_id(),
                    })?
                    .map_err(PluginError::from)?;
                (plugin, Some(definition.key))
            }
        };
        Ok(InstalledPlugin {
            entry: self.entry,
            plugin,
            key,
        })
    }
}

fn expand(
    inputs: PluginGroupBuilder,
    pending: &mut Vec<PendingPlugin>,
    groups: &mut Vec<TypeId>,
) -> Result<(), PluginPlanError> {
    for input in inputs.0 {
        let (declaration, materializer) = match input {
            PluginInput::Value(declare, plugin) => (declare(), Materializer::Value(plugin)),
            PluginInput::Definition(definition) => (
                (definition.declaration)(),
                Materializer::Definition(definition),
            ),
            PluginInput::Group(group, name, build) => {
                if groups.contains(&group) {
                    return Err(PluginPlanError::GroupCycle { group: name });
                }
                groups.push(group);
                expand(build(), pending, groups)?;
                groups.pop();
                continue;
            }
        };
        pending.push(PendingPlugin {
            entry: PluginPlanEntry(declaration),
            materializer,
        });
    }
    Ok(())
}

pub(crate) fn prepare<M>(
    inputs: impl Plugins<M>,
    installed: &[InstalledPlugin],
) -> Result<Vec<InstalledPlugin>, AddPluginsError> {
    let pending = catch_unwind(AssertUnwindSafe(|| {
        let mut pending = Vec::new();
        expand(
            inputs.collect(PluginGroupBuilder::new()),
            &mut pending,
            &mut Vec::new(),
        )?;
        Ok::<_, PluginPlanError>(pending)
    }))
    .map_err(|_| PluginPlanError::DeclarationPanicked)??;

    let mut keys: HashMap<_, _> = installed
        .iter()
        .map(|plugin| {
            (
                plugin.entry.plugin_id(),
                (plugin.entry.declaration(), plugin.key.clone()),
            )
        })
        .collect();
    let mut unique = Vec::new();
    for candidate in pending {
        let plugin = candidate.entry.plugin_id();
        if let Some((declaration, existing)) = keys.get(&plugin) {
            if candidate.key().is_some()
                && existing.as_ref() == candidate.key()
                && *declaration == candidate.entry.declaration()
            {
                continue;
            }
            return Err(PluginPlanError::Duplicate { plugin }.into());
        }
        keys.insert(
            plugin,
            (candidate.entry.declaration(), candidate.key().cloned()),
        );
        unique.push(candidate);
    }

    let declarations = installed
        .iter()
        .map(|plugin| plugin.entry.0)
        .chain(unique.iter().map(|plugin| plugin.entry.0));
    let mut providers = HashMap::new();
    for declaration in declarations {
        let plugin = declaration.id;
        for &provider in declaration.provides_schema {
            if let Some(other) = providers.insert(provider, plugin) {
                return Err(PluginPlanError::DuplicateSchema {
                    plugin,
                    other,
                    provider,
                }
                .into());
            }
        }
        for &other in declaration.conflicts {
            if keys.contains_key(&other) {
                return Err(PluginPlanError::Conflict { plugin, other }.into());
            }
        }
    }
    let mut deps = HashMap::new();
    for candidate in &unique {
        let declaration = candidate.entry.0;
        let plugin = declaration.id;
        let mut required_plugins = declaration.requires_plugins.to_vec();
        for &required in &required_plugins {
            if !keys.contains_key(&required) {
                return Err(PluginPlanError::MissingPlugin { plugin, required }.into());
            }
        }
        for &required in declaration.requires_schema {
            let provider = *providers
                .get(&required)
                .ok_or(PluginPlanError::MissingSchema { plugin, required })?;
            if provider != plugin {
                required_plugins.push(provider);
            }
        }
        deps.insert(plugin, required_plugins);
    }

    let mut ordered = Vec::with_capacity(unique.len());
    while !unique.is_empty() {
        let Some(index) = unique.iter().position(|candidate| {
            deps[&candidate.entry.plugin_id()]
                .iter()
                .all(|plugin| !deps.contains_key(plugin))
        }) else {
            return Err(PluginPlanError::DependencyCycle {
                plugin: unique[0].entry.plugin_id(),
            }
            .into());
        };
        let candidate = unique.remove(index);
        deps.remove(&candidate.entry.plugin_id());
        ordered.push(candidate);
    }
    ordered.into_iter().map(PendingPlugin::prepare).collect()
}

pub(crate) fn run_hook(
    plugin: PluginId,
    hook: PluginHook,
    run: impl FnOnce() -> Result<(), PluginError>,
) -> Result<(), PluginError> {
    catch_unwind(AssertUnwindSafe(run)).map_err(|_| PluginError::HookPanicked { plugin, hook })?
}
