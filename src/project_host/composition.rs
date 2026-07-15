use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_app::{
    EditedPluginGroup, Plugin, PluginDefinition, PluginGroup, PluginId, PluginPlan,
    PluginPlanError, PluginProductCapability, PluginSchemaProviderId,
};
use nara_project::{EffectiveProjectSettings, ProductCapability, ProductCapabilitySet};
use nara_reflect::{
    CatalogFingerprint, ComponentRegistry, ComponentRegistryError,
    ComponentSchemaProviderDefinition,
};

/// The product capabilities compiled into this root package instance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CompiledProductCapabilities(ProductCapabilitySet);

impl CompiledProductCapabilities {
    #[must_use]
    pub const fn capabilities(self) -> ProductCapabilitySet {
        self.0
    }
}

impl fmt::Debug for CompiledProductCapabilities {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CompiledProductCapabilities")
            .field(&self.0)
            .finish()
    }
}

/// Returns the non-forgeable Cargo capability ceiling for the current root package build.
#[must_use]
pub const fn compiled_product_capabilities() -> CompiledProductCapabilities {
    let capabilities = ProductCapabilitySet::new().with(ProductCapability::RuntimeCore);
    #[cfg(feature = "runtime-2d")]
    let capabilities = capabilities.with(ProductCapability::Runtime2d);
    #[cfg(feature = "runtime-ui")]
    let capabilities = capabilities.with(ProductCapability::RuntimeUi);
    #[cfg(feature = "tooling")]
    let capabilities = capabilities.with(ProductCapability::Tooling);
    #[cfg(feature = "asset-watch")]
    let capabilities = capabilities.with(ProductCapability::AssetWatch);
    #[cfg(feature = "desktop-winit")]
    let capabilities = capabilities.with(ProductCapability::DesktopWinit);
    #[cfg(feature = "render-wgpu")]
    let capabilities = capabilities.with(ProductCapability::RenderWgpu);
    #[cfg(feature = "tooling-egui")]
    let capabilities = capabilities.with(ProductCapability::ToolingEgui);
    CompiledProductCapabilities(capabilities)
}

/// Opaque identity of one authorized manifest byte sequence and selected profile.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectSettingsLineage(pub(super) [u8; 32]);

impl fmt::Debug for ProjectSettingsLineage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectSettingsLineage(..)")
    }
}

/// An immutable settings candidate whose product capability request fits this binary.
#[derive(Clone, PartialEq)]
pub struct ProjectSettingsCandidate {
    pub(super) lineage: ProjectSettingsLineage,
    pub(super) settings: EffectiveProjectSettings,
    pub(super) explicit: ProductCapabilitySet,
    pub(super) implied: ProductCapabilitySet,
    pub(super) normalized: ProductCapabilitySet,
    pub(super) required: ProductCapabilitySet,
    pub(super) compiled: CompiledProductCapabilities,
}

impl ProjectSettingsCandidate {
    #[must_use]
    pub const fn lineage(&self) -> ProjectSettingsLineage {
        self.lineage
    }

    #[must_use]
    pub const fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    #[must_use]
    pub const fn explicit_capabilities(&self) -> ProductCapabilitySet {
        self.explicit
    }

    #[must_use]
    pub const fn implied_capabilities(&self) -> ProductCapabilitySet {
        self.implied
    }

    #[must_use]
    pub const fn normalized_capabilities(&self) -> ProductCapabilitySet {
        self.normalized
    }

    #[must_use]
    pub const fn required_capabilities(&self) -> ProductCapabilitySet {
        self.required
    }

    #[must_use]
    pub const fn compiled_capabilities(&self) -> CompiledProductCapabilities {
        self.compiled
    }
}

impl fmt::Debug for ProjectSettingsCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectSettingsCandidate")
            .field("lineage", &self.lineage)
            .field("runtime_preset", &self.settings.runtime_preset)
            .field("profile_present", &self.settings.profile_name.is_some())
            .field("explicit", &self.explicit)
            .field("implied", &self.implied)
            .field("normalized", &self.normalized)
            .field("required", &self.required)
            .field("compiled", &self.compiled)
            .finish()
    }
}

/// A lineage-bound, editable request for one file-backed project runtime plan.
pub struct ProjectRuntimePlugins {
    lineage: ProjectSettingsLineage,
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
}

impl fmt::Debug for ProjectRuntimePlugins {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectRuntimePlugins")
            .field("lineage", &self.lineage)
            .finish_non_exhaustive()
    }
}

impl ProjectRuntimePlugins {
    #[must_use]
    pub fn disable<P: Plugin>(self) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.disable::<P>(),
        }
    }

    #[must_use]
    pub fn configure(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.configure(definition),
        }
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.insert_after::<P>(definition),
        }
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.insert_before::<P>(definition),
        }
    }
}

#[derive(Clone)]
pub struct SchemaValidationInput {
    provider_ids: Arc<[PluginSchemaProviderId]>,
    registry: Arc<ComponentRegistry>,
    fingerprint: CatalogFingerprint,
}

impl fmt::Debug for SchemaValidationInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchemaValidationInput")
            .field("provider_ids", &self.provider_ids)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl SchemaValidationInput {
    #[must_use]
    pub fn provider_ids(&self) -> &[PluginSchemaProviderId] {
        &self.provider_ids
    }

    #[must_use]
    pub const fn fingerprint(&self) -> CatalogFingerprint {
        self.fingerprint
    }

    #[must_use]
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }
}

#[derive(Clone)]
pub struct RuntimePlan {
    lineage: ProjectSettingsLineage,
    settings: EffectiveProjectSettings,
    compiled: CompiledProductCapabilities,
    explicit: ProductCapabilitySet,
    normalized: ProductCapabilitySet,
    required: ProductCapabilitySet,
    plugin_plan: PluginPlan,
    schema_validation: SchemaValidationInput,
}

impl fmt::Debug for RuntimePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePlan")
            .field("lineage", &self.lineage)
            .field("compiled", &self.compiled)
            .field("explicit", &self.explicit)
            .field("normalized", &self.normalized)
            .field("required", &self.required)
            .field("plugin_plan_fingerprint", &self.plugin_plan.fingerprint())
            .field("schema_validation", &self.schema_validation)
            .finish_non_exhaustive()
    }
}

impl RuntimePlan {
    #[must_use]
    pub const fn lineage(&self) -> ProjectSettingsLineage {
        self.lineage
    }

    #[must_use]
    pub const fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    #[must_use]
    pub const fn compiled_capabilities(&self) -> CompiledProductCapabilities {
        self.compiled
    }

    #[must_use]
    pub const fn explicit_capabilities(&self) -> ProductCapabilitySet {
        self.explicit
    }

    #[must_use]
    pub const fn normalized_capabilities(&self) -> ProductCapabilitySet {
        self.normalized
    }

    #[must_use]
    pub const fn required_capabilities(&self) -> ProductCapabilitySet {
        self.required
    }

    #[must_use]
    pub const fn plugin_plan(&self) -> &PluginPlan {
        &self.plugin_plan
    }

    #[must_use]
    pub const fn schema_validation(&self) -> &SchemaValidationInput {
        &self.schema_validation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    ProjectLineageMismatch,
    UnknownProductCapability {
        plugin: PluginId,
        capability: PluginProductCapability,
    },
    UncompiledProductCapability {
        plugin: PluginId,
        capability: ProductCapability,
    },
    UnrequestedProductCapability {
        plugin: PluginId,
        capability: ProductCapability,
    },
    MissingSchemaProvider {
        provider: PluginSchemaProviderId,
    },
    DivergentSchemaProvider {
        provider: PluginSchemaProviderId,
    },
    AmbiguousSchemaProviderOwner {
        provider: PluginSchemaProviderId,
    },
    SchemaProviderRejected {
        provider: PluginSchemaProviderId,
        source: Box<ComponentRegistryError>,
    },
    SchemaProviderPanicked {
        provider: PluginSchemaProviderId,
    },
    SchemaFreezeRejected {
        source: Box<ComponentRegistryError>,
    },
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProjectLineageMismatch => {
                "project runtime request lineage does not match candidate"
            }
            Self::UnknownProductCapability { .. } => {
                "plugin requires an unknown product capability"
            }
            Self::UncompiledProductCapability { .. } => {
                "plugin requires a product capability that is not compiled"
            }
            Self::UnrequestedProductCapability { .. } => {
                "plugin requires a product capability that was not requested"
            }
            Self::MissingSchemaProvider { .. } => {
                "plugin plan declares a schema provider without a typed binding"
            }
            Self::DivergentSchemaProvider { .. } => {
                "schema provider ID has divergent typed bindings"
            }
            Self::AmbiguousSchemaProviderOwner { .. } => {
                "schema provider ID is declared by more than one plugin"
            }
            Self::SchemaProviderRejected { .. } => "component schema provider was rejected",
            Self::SchemaProviderPanicked { .. } => "component schema provider panicked",
            Self::SchemaFreezeRejected { .. } => "component schema catalog freeze was rejected",
        })
    }
}

impl std::error::Error for CompositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SchemaProviderRejected { source, .. } | Self::SchemaFreezeRejected { source } => {
                Some(source.as_ref())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimePlanError {
    Composition(CompositionError),
    PluginPlan(PluginPlanError),
}

impl fmt::Display for RuntimePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(error) => write!(formatter, "runtime composition failed: {error}"),
            Self::PluginPlan(error) => write!(formatter, "plugin plan resolution failed: {error}"),
        }
    }
}

impl std::error::Error for RuntimePlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Composition(error) => Some(error),
            Self::PluginPlan(error) => Some(error),
        }
    }
}

impl From<CompositionError> for RuntimePlanError {
    fn from(error: CompositionError) -> Self {
        Self::Composition(error)
    }
}

impl From<PluginPlanError> for RuntimePlanError {
    fn from(error: PluginPlanError) -> Self {
        Self::PluginPlan(error)
    }
}

/// Returns every trusted schema-provider binding compiled into this root package.
#[must_use]
pub fn built_in_schema_providers() -> Vec<ComponentSchemaProviderDefinition> {
    #[allow(unused_mut)]
    let mut providers = vec![
        nara_scene::HIERARCHY_SCHEMA_PROVIDER,
        nara_transform::TRANSFORM_SCHEMA_PROVIDER,
    ];
    #[cfg(feature = "runtime-2d")]
    {
        providers.push(nara_sprite::SPRITE_SCHEMA_PROVIDER);
        providers.push(nara_tilemap::TILEMAP_SCHEMA_PROVIDER);
    }
    #[cfg(any(
        feature = "runtime-2d",
        feature = "runtime-ui",
        feature = "render-wgpu"
    ))]
    providers.push(nara_render::RENDER_SCHEMA_PROVIDER);
    #[cfg(feature = "runtime-ui")]
    providers.push(nara_ui::UI_SCHEMA_PROVIDER);
    providers
}

/// Builds the editable, lineage-bound plugin request for one project settings candidate.
#[must_use]
pub fn project_runtime_plugins(candidate: &ProjectSettingsCandidate) -> ProjectRuntimePlugins {
    ProjectRuntimePlugins {
        lineage: candidate.lineage,
        plugins: crate::product::ProjectProfilePlugins::new(
            candidate.settings(),
            candidate.normalized_capabilities(),
        )
        .edit(),
    }
}

/// Resolves one project request without creating an `App` or acquiring runtime authority.
pub fn resolve_runtime_plan(
    candidate: &ProjectSettingsCandidate,
    request: ProjectRuntimePlugins,
    providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<RuntimePlan, RuntimePlanError> {
    if request.lineage != candidate.lineage {
        return Err(CompositionError::ProjectLineageMismatch.into());
    }

    let plugin_plan = PluginPlan::resolve(request.plugins)?;
    let required = resolve_product_requirements(candidate, &plugin_plan)?;
    let schema_validation = resolve_schema_validation(&plugin_plan, providers)?;

    Ok(RuntimePlan {
        lineage: candidate.lineage,
        settings: candidate.settings.clone(),
        compiled: candidate.compiled,
        explicit: candidate.explicit,
        normalized: candidate.normalized,
        required,
        plugin_plan,
        schema_validation,
    })
}

fn resolve_product_requirements(
    candidate: &ProjectSettingsCandidate,
    plugin_plan: &PluginPlan,
) -> Result<ProductCapabilitySet, CompositionError> {
    let mut required = candidate.required;
    for entry in plugin_plan.entries() {
        for declared in entry.declaration().requires_product_capabilities {
            let Some(capability) = product_capability(*declared) else {
                return Err(CompositionError::UnknownProductCapability {
                    plugin: entry.plugin_id(),
                    capability: *declared,
                });
            };
            if !candidate.normalized.contains(capability) {
                return Err(CompositionError::UnrequestedProductCapability {
                    plugin: entry.plugin_id(),
                    capability,
                });
            }
            if !candidate.compiled.capabilities().contains(capability) {
                return Err(CompositionError::UncompiledProductCapability {
                    plugin: entry.plugin_id(),
                    capability,
                });
            }
            required.insert(capability);
        }
    }
    Ok(required)
}

fn product_capability(declared: PluginProductCapability) -> Option<ProductCapability> {
    ProductCapability::ALL
        .into_iter()
        .find(|capability| capability.as_str() == declared.as_str())
}

fn resolve_schema_validation(
    plugin_plan: &PluginPlan,
    providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<SchemaValidationInput, CompositionError> {
    let mut provider_catalog = BTreeMap::new();
    for provider in providers {
        match provider_catalog.entry(provider.id()) {
            Entry::Vacant(entry) => {
                entry.insert(provider);
            }
            Entry::Occupied(entry) if entry.get().has_same_binding(provider) => {}
            Entry::Occupied(entry) => {
                return Err(CompositionError::DivergentSchemaProvider {
                    provider: *entry.key(),
                });
            }
        }
    }

    let mut owners = BTreeMap::<PluginSchemaProviderId, PluginId>::new();
    for entry in plugin_plan.entries() {
        for provider in entry.declaration().provides_schema {
            if let Some(owner) = owners.insert(*provider, entry.plugin_id())
                && owner != entry.plugin_id()
            {
                return Err(CompositionError::AmbiguousSchemaProviderOwner {
                    provider: *provider,
                });
            }
        }
    }

    let mut registry = ComponentRegistry::new();
    for provider_id in owners.keys() {
        let provider = provider_catalog.get(provider_id).copied().ok_or(
            CompositionError::MissingSchemaProvider {
                provider: *provider_id,
            },
        )?;
        catch_unwind(AssertUnwindSafe(|| provider.register_into(&mut registry)))
            .map_err(|_| CompositionError::SchemaProviderPanicked {
                provider: *provider_id,
            })?
            .map_err(|source| CompositionError::SchemaProviderRejected {
                provider: *provider_id,
                source: Box::new(source),
            })?;
    }
    registry
        .freeze()
        .map_err(|source| CompositionError::SchemaFreezeRejected {
            source: Box::new(source),
        })?;
    let fingerprint = registry
        .catalog()
        .map_err(|source| CompositionError::SchemaFreezeRejected {
            source: Box::new(source),
        })?
        .fingerprint();

    Ok(SchemaValidationInput {
        provider_ids: owners.into_keys().collect::<Vec<_>>().into(),
        registry: Arc::new(registry),
        fingerprint,
    })
}
