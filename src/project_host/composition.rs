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
use nara_fs::{ContentDigest, FileIdentity};
use nara_project::{EffectiveProjectSettings, ProductCapability, ProductCapabilitySet};
use nara_reflect::__private::{
    ResolvedComponentSchemaProvider, register_or_validate_resolved_schema_provider,
    resolve_schema_provider,
};
#[cfg(feature = "serde")]
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, PersistentComponentSchemaOwnerId,
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
    #[cfg(feature = "presentation-2d")]
    let capabilities = capabilities.with(ProductCapability::Presentation2d);
    #[cfg(feature = "presentation-ui")]
    let capabilities = capabilities.with(ProductCapability::PresentationUi);
    #[cfg(feature = "asset-watch")]
    let capabilities = capabilities.with(ProductCapability::AssetWatch);
    #[cfg(feature = "desktop-winit")]
    let capabilities = capabilities.with(ProductCapability::DesktopWinit);
    #[cfg(feature = "render-wgpu")]
    let capabilities = capabilities.with(ProductCapability::RenderWgpu);
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
    pub(super) manifest_digest: ContentDigest,
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
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
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
            .field("manifest_digest", &self.manifest_digest)
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

/// A lineage-bound request for explicit advanced runtime-plan inspection.
///
/// Ordinary file-backed product actions do not expose this type. They accept one
/// [`crate::ProductRecipe`] and lower it privately. The type remains available from
/// [`crate::project_host::advanced`] for engine integration tests and advanced embedders that need
/// to inspect profile-owned slot edits before any `App` exists.
pub struct ProjectRuntimePlugins {
    lineage: ProjectSettingsLineage,
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
}

pub(crate) struct RuntimePluginOverlay {
    definition: PluginDefinition,
}

pub(crate) fn runtime_plugin_overlay(definition: PluginDefinition) -> RuntimePluginOverlay {
    RuntimePluginOverlay { definition }
}

pub(crate) struct RuntimeHostPolicyContribution {
    definitions: Vec<PluginDefinition>,
}

pub(crate) fn runtime_host_policy_contribution(
    definition: PluginDefinition,
) -> RuntimeHostPolicyContribution {
    RuntimeHostPolicyContribution {
        definitions: vec![definition],
    }
}

impl RuntimeHostPolicyContribution {
    pub(crate) fn with_definition(mut self, definition: PluginDefinition) -> Self {
        self.definitions.push(definition);
        self
    }
}

const PROJECT_RUNTIME_OVERLAY_GROUP_ID: PluginGroupId =
    PluginGroupId::new("nara.plugins.project-runtime-overlay");

struct RuntimeOverlayPlugins {
    definition: PluginDefinition,
}

const PROJECT_RUNTIME_HOST_POLICY_GROUP_ID: PluginGroupId =
    PluginGroupId::new("nara.plugins.project-runtime-host-policy");

struct RuntimeHostPolicyPlugins {
    definitions: Vec<PluginDefinition>,
}

impl PluginGroup for RuntimeHostPolicyPlugins {
    const ID: PluginGroupId = PROJECT_RUNTIME_HOST_POLICY_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        self.definitions
            .into_iter()
            .fold(PluginGroupBuilder::new(), |builder, definition| {
                builder.add_definition(definition)
            })
    }
}

impl PluginGroup for RuntimeOverlayPlugins {
    const ID: PluginGroupId = PROJECT_RUNTIME_OVERLAY_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_definition(self.definition)
    }
}

struct ProjectRuntimeAppPlugins {
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
}

impl PluginGroup for ProjectRuntimeAppPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime-app");

    fn build(self) -> PluginGroupBuilder {
        let builder =
            PluginGroupBuilder::new().add_group(self.recipe.with_edited_defaults(self.plugins));
        let builder = match self.overlay {
            Some(overlay) => builder.add_group(RuntimeOverlayPlugins {
                definition: overlay.definition,
            }),
            None => builder,
        };
        match self.host_policy {
            Some(host_policy) => builder.add_group(RuntimeHostPolicyPlugins {
                definitions: host_policy.definitions,
            }),
            None => builder,
        }
    }
}

impl fmt::Debug for ProjectRuntimePlugins {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectRuntimePlugins")
            .field("lineage", &self.lineage)
            .field("recipe", &self.recipe)
            .field("has_advanced_overlay", &self.overlay.is_some())
            .field("has_host_policy", &self.host_policy.is_some())
            .finish_non_exhaustive()
    }
}

impl ProjectRuntimePlugins {
    #[must_use]
    pub fn disable<P: Plugin>(mut self) -> Self {
        self.plugins = self.plugins.disable::<P>();
        self
    }

    #[must_use]
    pub fn configure(mut self, definition: PluginDefinition) -> Self {
        self.plugins = self.plugins.configure(definition);
        self
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugins = self.plugins.insert_after::<P>(definition);
        self
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugins = self.plugins.insert_before::<P>(definition);
        self
    }

    /// Attaches the ordinary-author product recipe to this advanced project request.
    #[must_use]
    pub fn with_recipe(mut self, recipe: crate::ProductRecipe) -> Self {
        self.recipe = recipe;
        self
    }

    pub(crate) fn with_overlay(mut self, overlay: RuntimePluginOverlay) -> Self {
        self.overlay = Some(overlay);
        self
    }

    pub(crate) fn with_host_policy(mut self, host_policy: RuntimeHostPolicyContribution) -> Self {
        self.host_policy = Some(host_policy);
        self
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
    #[cfg(feature = "serde")]
    authoring_schema_bundle: AuthoringSchemaBundle,
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

    /// Projects the selected native schema owners into a data-only authoring bundle.
    #[cfg(feature = "serde")]
    #[must_use]
    pub const fn authoring_schema_bundle(&self) -> &AuthoringSchemaBundle {
        &self.authoring_schema_bundle
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

    /// Returns the conservative product-capability witness used to admit this plan.
    ///
    /// For an any-of plugin clause, every requested matching alternative is retained. This set is
    /// therefore deterministic and monotonic, but is not a minimal dependency solution.
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
    UnsupportedDefaultConfiguration {
        plugin: PluginId,
    },
    AdvancedProbeProvidesSchema {
        plugin: PluginId,
    },
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
    UnrequestedProductCapabilityAlternatives {
        plugin: PluginId,
        alternatives: ProductCapabilitySet,
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
            Self::UnsupportedDefaultConfiguration { .. } => {
                "product recipe configures a default absent from the selected profile"
            }
            Self::AdvancedProbeProvidesSchema { .. } => {
                "advanced Host probes cannot declare schema authority"
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
            Self::UnrequestedProductCapabilityAlternatives { .. } => {
                "plugin requires at least one product capability alternative, but none were requested"
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
///
/// This is an advanced plan-inspection input, not a schema-provider channel on
/// [`crate::project_host::advanced::RuntimeProbe`]. Ordinary file-backed products bind schema
/// authority through [`crate::SchemaContribution`].
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
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    providers.push(nara_render::RENDER_SCHEMA_PROVIDER);
    #[cfg(feature = "runtime-ui")]
    providers.push(nara_ui::UI_SCHEMA_PROVIDER);
    providers
}

/// Builds an explicit advanced, lineage-bound plugin request for one project settings candidate.
#[must_use]
pub fn project_runtime_plugins(candidate: &ProjectSettingsCandidate) -> ProjectRuntimePlugins {
    project_runtime_plugins_from_profile(
        candidate,
        crate::product::ProjectProfilePlugins::new(
            candidate.settings(),
            candidate.normalized_capabilities(),
        ),
    )
}

fn project_runtime_plugins_from_profile(
    candidate: &ProjectSettingsCandidate,
    plugins: crate::product::ProjectProfilePlugins,
) -> ProjectRuntimePlugins {
    ProjectRuntimePlugins {
        lineage: candidate.lineage,
        plugins: plugins.edit(),
        recipe: crate::ProductRecipe::new(),
        overlay: None,
        host_policy: None,
    }
}

/// Builds the file-backed project request for one ordinary product recipe.
#[must_use]
pub(crate) fn project_runtime_plugins_with_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
) -> ProjectRuntimePlugins {
    finalize_project_runtime_plugins(project_runtime_plugins(candidate), recipe, overlay, None)
}

pub(crate) fn editor_play_runtime_plugins_with_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
) -> ProjectRuntimePlugins {
    finalize_project_runtime_plugins(
        project_runtime_plugins_from_profile(
            candidate,
            crate::product::ProjectProfilePlugins::for_editor_play(
                candidate.settings(),
                candidate.normalized_capabilities(),
            ),
        ),
        recipe,
        overlay,
        host_policy,
    )
}

fn finalize_project_runtime_plugins(
    request: ProjectRuntimePlugins,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
) -> ProjectRuntimePlugins {
    let request = request.with_recipe(recipe);
    let request = match host_policy {
        Some(host_policy) => request.with_host_policy(host_policy),
        None => request,
    };
    match overlay {
        Some(overlay) => request.with_overlay(overlay),
        None => request,
    }
}

pub(crate) fn resolve_project_runtime_request(
    candidate: &ProjectSettingsCandidate,
    request: ProjectRuntimePlugins,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_runtime_plan(candidate, request, built_in_schema_providers())
}

pub(crate) fn resolve_product_recipe_with_overlay(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_project_runtime_request(
        candidate,
        project_runtime_plugins_with_recipe(candidate, recipe, overlay),
    )
}

pub(crate) fn resolve_product_recipe_with_host_policy(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: RuntimeHostPolicyContribution,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_project_runtime_request(
        candidate,
        finalize_project_runtime_plugins(
            project_runtime_plugins(candidate),
            recipe,
            overlay,
            Some(host_policy),
        ),
    )
}

/// Resolves one ordinary product recipe before creating an `App` or acquiring runtime authority.
pub fn resolve_product_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_product_recipe_with_overlay(candidate, recipe, None)
}

/// Resolves one explicit advanced project request without creating an `App` or acquiring runtime
/// authority.
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

    if let Some(plugin) = request
        .recipe
        .unsupported_default_plugin(candidate.normalized_capabilities())
    {
        return Err(CompositionError::UnsupportedDefaultConfiguration { plugin }.into());
    }

    let recipe_providers = request.recipe.schema_providers().collect::<Vec<_>>();
    let snapshot_cell = Arc::new(OnceLock::new());
    let request = request.configure(preloaded_component_registry_plugin(Arc::clone(
        &snapshot_cell,
    )));
    let plugin_plan = PluginPlan::resolve(ProjectRuntimeAppPlugins {
        plugins: request.plugins,
        recipe: request.recipe,
        overlay: request.overlay,
        host_policy: request.host_policy,
    })?;
    if let Some(overlay) = plugin_plan.entries().iter().find(|entry| {
        entry
            .group_provenance()
            .contains(&PROJECT_RUNTIME_OVERLAY_GROUP_ID)
    }) && !overlay.declaration().provides_schema.is_empty()
    {
        return Err(CompositionError::AdvancedProbeProvidesSchema {
            plugin: overlay.plugin_id(),
        }
        .into());
    }
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

        let mut alternatives = ProductCapabilitySet::new();
        for declared in entry.declaration().requires_any_product_capabilities {
            let Some(capability) = product_capability(*declared) else {
                return Err(CompositionError::UnknownProductCapability {
                    plugin: entry.plugin_id(),
                    capability: *declared,
                });
            };
            alternatives.insert(capability);
        }
        if alternatives.is_empty() {
            continue;
        }

        let requested_alternatives = alternatives.intersection(candidate.normalized);
        if requested_alternatives.is_empty() {
            return Err(CompositionError::UnrequestedProductCapabilityAlternatives {
                plugin: entry.plugin_id(),
                alternatives,
            });
        }
        for capability in requested_alternatives.iter() {
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
    #[cfg(feature = "serde")]
    let mut authoring_schema_owners = Vec::with_capacity(selected_providers.len());
    for provider_id in &selected_providers {
        let provider = provider_catalog.remove(provider_id).ok_or(
            CompositionError::MissingSchemaProvider {
                provider: *provider_id,
            },
        )?;
        #[cfg(feature = "serde")]
        authoring_schema_owners.push(AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(provider.current().owner().as_str())
                .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?,
            provider.current().catalog().clone(),
            provider
                .predecessor()
                .map(|predecessor| predecessor.catalog().clone()),
        ));
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
    #[cfg(feature = "serde")]
    let authoring_schema_bundle = AuthoringSchemaBundle::new(authoring_schema_owners)
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;

    Ok(SchemaValidationInput {
        provider_ids: provider_ids.into(),
        provider_receipts: provider_receipts.into(),
        #[cfg(feature = "serde")]
        authoring_schema_bundle,
        snapshot,
        registry: Arc::new(registry),
        composition_fingerprint,
        executable_fingerprint,
    })
}
