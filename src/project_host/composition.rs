use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, OnceLock},
};

use nara_app::{
    EditedPluginGroup, Plugin, PluginDefinition, PluginGroup, PluginGroupBuilder, PluginGroupId,
    PluginId, PluginPlan, PluginPlanError, PluginProductCapability, PluginSchemaProviderId,
};
use nara_fs::FileIdentity;
use nara_project::{EffectiveProjectSettings, ProductCapability, ProductCapabilitySet};
use nara_reflect::__private::{
    ResolvedComponentSchemaProvider, register_or_validate_resolved_schema_provider,
    resolve_schema_provider,
};
use nara_reflect::{
    ComponentRegistry, ComponentRegistryError, ComponentRegistrySnapshot,
    ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
    ComponentSchemaOwnerId, ComponentSchemaProviderDefinition, ComponentSchemaProviderReceipt,
    ComponentTypeId, ExecutableRegistryFingerprint, SchemaCompositionFingerprint,
    preloaded_component_registry_plugin,
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

/// Opaque identity of one authorized project root, manifest byte sequence, and selected profile.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectSettingsLineage {
    pub(super) settings_digest: [u8; 32],
    pub(super) project_root_identity: Option<FileIdentity>,
}

impl fmt::Debug for ProjectSettingsLineage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectSettingsLineage(..)")
    }
}

impl ProjectSettingsLineage {
    #[cfg(all(feature = "serde", feature = "runtime-2d"))]
    pub(crate) const fn settings_digest(self) -> [u8; 32] {
        self.settings_digest
    }

    #[cfg(all(feature = "serde", feature = "runtime-2d"))]
    pub(crate) const fn project_root_identity(self) -> Option<FileIdentity> {
        self.project_root_identity
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
    recipe: crate::ProductRecipe,
}

struct ProjectRuntimeAppPlugins {
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
    recipe: crate::ProductRecipe,
}

impl PluginGroup for ProjectRuntimeAppPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime-app");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_edited_group(self.plugins)
            .add_group(self.recipe)
    }
}

impl fmt::Debug for ProjectRuntimePlugins {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectRuntimePlugins")
            .field("lineage", &self.lineage)
            .field("recipe", &self.recipe)
            .finish_non_exhaustive()
    }
}

impl ProjectRuntimePlugins {
    #[must_use]
    pub fn disable<P: Plugin>(self) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.disable::<P>(),
            recipe: self.recipe,
        }
    }

    #[must_use]
    pub fn configure(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.configure(definition),
            recipe: self.recipe,
        }
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.insert_after::<P>(definition),
            recipe: self.recipe,
        }
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(self, definition: PluginDefinition) -> Self {
        Self {
            lineage: self.lineage,
            plugins: self.plugins.insert_before::<P>(definition),
            recipe: self.recipe,
        }
    }

    /// Attaches the ordinary-author product recipe to this advanced project request.
    #[must_use]
    pub fn with_recipe(mut self, recipe: crate::ProductRecipe) -> Self {
        self.recipe = recipe;
        self
    }

    /// Returns this project recipe as ordinary `App::add_plugins` input.
    ///
    /// This lower-level embedding path does not perform project capability, schema-provider,
    /// snapshot, or lineage admission. A file-backed [`RuntimePlan`] deliberately configures a
    /// plan-local frozen registry definition, so callers must not compare this raw recipe's
    /// configuration fingerprint with an admitted plan and treat them as interchangeable.
    #[must_use]
    pub fn into_app_plugins(self) -> impl PluginGroup {
        ProjectRuntimeAppPlugins {
            plugins: self.plugins,
            recipe: self.recipe,
        }
    }
}

/// Frozen schema authority selected for one admitted Runtime plan.
///
/// Provider and contribution receipts are immutable projections of `snapshot`; exact authority
/// inside the managed Host still requires the same snapshot identity rather than equal hashes.
#[derive(Clone)]
pub struct SchemaValidationInput {
    provider_ids: Arc<[PluginSchemaProviderId]>,
    provider_receipts: Arc<[ComponentSchemaProviderReceipt]>,
    snapshot: ComponentRegistrySnapshot,
    registry: Arc<ComponentRegistry>,
    composition_fingerprint: SchemaCompositionFingerprint,
    executable_fingerprint: ExecutableRegistryFingerprint,
}

impl fmt::Debug for SchemaValidationInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchemaValidationInput")
            .field("provider_ids", &self.provider_ids)
            .field("provider_receipts", &self.provider_receipts)
            .field(
                "contribution_count",
                &self.snapshot.contribution_receipts().count(),
            )
            .field("composition_fingerprint", &self.composition_fingerprint)
            .field("executable_fingerprint", &self.executable_fingerprint)
            .finish_non_exhaustive()
    }
}

impl SchemaValidationInput {
    #[must_use]
    pub fn provider_ids(&self) -> &[PluginSchemaProviderId] {
        &self.provider_ids
    }

    pub fn contribution_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaContributionReceipt> + '_ {
        self.snapshot.contribution_receipts()
    }

    #[must_use]
    pub fn provider_receipts(&self) -> &[ComponentSchemaProviderReceipt] {
        &self.provider_receipts
    }

    pub fn owner_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaOwnerContributionReceipt> + '_ {
        self.snapshot.owner_receipts()
    }

    #[must_use]
    pub fn snapshot(&self) -> &ComponentRegistrySnapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn composition_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.composition_fingerprint
    }

    #[must_use]
    pub const fn executable_fingerprint(&self) -> ExecutableRegistryFingerprint {
        self.executable_fingerprint
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
    DivergentSchemaOwner {
        owner: ComponentSchemaOwnerId,
    },
    ConflictingSchemaOwnerClaim {
        component_id: ComponentTypeId,
        first_owner: ComponentSchemaOwnerId,
        second_owner: ComponentSchemaOwnerId,
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
    SchemaAuthorityPublicationFailed,
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
            Self::DivergentSchemaOwner { .. } => {
                "schema owner has more than one trusted provider definition"
            }
            Self::ConflictingSchemaOwnerClaim { .. } => {
                "different schema owners claim the same component type ID"
            }
            Self::SchemaProviderRejected { .. } => "component schema provider was rejected",
            Self::SchemaProviderPanicked { .. } => "component schema provider panicked",
            Self::SchemaFreezeRejected { .. } => "component schema catalog freeze was rejected",
            Self::SchemaAuthorityPublicationFailed => {
                "component registry snapshot authority was published more than once"
            }
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
        nara_scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
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
        recipe: crate::ProductRecipe::new(),
    }
}

/// Builds the file-backed project request for one ordinary product recipe.
#[must_use]
pub fn project_runtime_plugins_with_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
) -> ProjectRuntimePlugins {
    project_runtime_plugins(candidate).with_recipe(recipe)
}

/// Resolves one ordinary product recipe before creating an `App` or acquiring runtime authority.
pub fn resolve_product_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_runtime_plan(
        candidate,
        project_runtime_plugins_with_recipe(candidate, recipe),
        built_in_schema_providers(),
    )
}

/// Resolves one project request without creating an `App` or acquiring runtime authority.
///
/// `known_providers` must contain the complete trusted provider set compiled for this product,
/// including definitions that the selected plugin recipe does not activate. Resolution executes
/// every current-head source to reserve inactive owner claims, but it executes native registration
/// callbacks only for providers selected by the resolved plugin plan.
pub fn resolve_runtime_plan(
    candidate: &ProjectSettingsCandidate,
    request: ProjectRuntimePlugins,
    known_providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<RuntimePlan, RuntimePlanError> {
    if request.lineage != candidate.lineage {
        return Err(CompositionError::ProjectLineageMismatch.into());
    }

    let recipe_providers = request.recipe.schema_providers().collect::<Vec<_>>();
    let snapshot_cell = Arc::new(OnceLock::new());
    let request = request.configure(preloaded_component_registry_plugin(Arc::clone(
        &snapshot_cell,
    )));
    let plugin_plan = PluginPlan::resolve((request.plugins, request.recipe))?;
    let required = resolve_product_requirements(candidate, &plugin_plan)?;
    let schema_validation = resolve_schema_validation(
        &plugin_plan,
        known_providers.into_iter().chain(recipe_providers),
    )?;
    snapshot_cell
        .set(schema_validation.snapshot().clone())
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;

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
    known_providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<SchemaValidationInput, CompositionError> {
    let mut provider_catalog =
        BTreeMap::<PluginSchemaProviderId, ResolvedComponentSchemaProvider>::new();
    for provider in known_providers {
        let provider_id = provider.id();
        let resolved = resolve_schema_provider(provider).map_err(|source| {
            CompositionError::SchemaProviderRejected {
                provider: provider_id,
                source: Box::new(source),
            }
        })?;
        match provider_catalog.entry(provider_id) {
            Entry::Vacant(entry) => {
                entry.insert(resolved);
            }
            Entry::Occupied(entry)
                if entry.get().provider_receipt() == resolved.provider_receipt()
                    && entry.get().owner_receipt() == resolved.owner_receipt() => {}
            Entry::Occupied(entry) => {
                return Err(CompositionError::DivergentSchemaProvider {
                    provider: *entry.key(),
                });
            }
        }
    }

    let mut known_owners = BTreeMap::<ComponentSchemaOwnerId, PluginSchemaProviderId>::new();
    let mut known_claims = BTreeMap::<ComponentTypeId, ComponentSchemaOwnerId>::new();
    for resolved in provider_catalog.values() {
        let owner = resolved.definition().owner();
        if let Some(existing) = known_owners.insert(owner, resolved.definition().id())
            && existing != resolved.definition().id()
        {
            return Err(CompositionError::DivergentSchemaOwner { owner });
        }
        for component_id in resolved
            .current()
            .catalog()
            .components()
            .iter()
            .map(|schema| schema.id())
            .chain(resolved.current().catalog().type_tombstones())
        {
            if let Some(existing) = known_claims.insert(component_id.clone(), owner)
                && existing != owner
            {
                return Err(CompositionError::ConflictingSchemaOwnerClaim {
                    component_id: component_id.clone(),
                    first_owner: existing,
                    second_owner: owner,
                });
            }
        }
    }

    let mut selected_providers = BTreeSet::<PluginSchemaProviderId>::new();
    for entry in plugin_plan.entries() {
        for provider in entry.declaration().provides_schema {
            selected_providers.insert(*provider);
        }
    }

    let mut registry = ComponentRegistry::new();
    for provider_id in &selected_providers {
        let provider = provider_catalog.remove(provider_id).ok_or(
            CompositionError::MissingSchemaProvider {
                provider: *provider_id,
            },
        )?;
        catch_unwind(AssertUnwindSafe(|| {
            register_or_validate_resolved_schema_provider(&mut registry, provider)
        }))
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
    let snapshot =
        registry
            .snapshot()
            .map_err(|source| CompositionError::SchemaFreezeRejected {
                source: Box::new(source),
            })?;
    let composition_fingerprint = snapshot
        .schema_composition_fingerprint()
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;
    let executable_fingerprint = snapshot
        .executable_registry_fingerprint()
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;
    let provider_receipts = snapshot.provider_receipts().collect::<Vec<_>>();
    let provider_ids = provider_receipts
        .iter()
        .map(|receipt| receipt.provider())
        .collect::<Vec<_>>();
    let registry = ComponentRegistry::from_snapshot(snapshot.clone());

    Ok(SchemaValidationInput {
        provider_ids: provider_ids.into(),
        provider_receipts: provider_receipts.into(),
        snapshot,
        registry: Arc::new(registry),
        composition_fingerprint,
        executable_fingerprint,
    })
}
