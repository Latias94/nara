use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, OnceLock},
};

#[cfg(feature = "project-source")]
use crate::fs::ContentDigest;
use nara_app::{
    EditedPluginGroup, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginId, PluginPlan,
    PluginPlanError, PluginProductCapability, PluginSchemaProviderId,
};
#[cfg(feature = "project-source")]
use nara_app::{Plugin, PluginDefinition};
#[cfg(feature = "runtime-input")]
use nara_input::INPUT_PLUGIN_ID;
#[cfg(feature = "project-source")]
pub use nara_project::manifest_source::ProjectSettingsLineage;
#[cfg(feature = "project-source")]
use nara_project::{EffectiveProjectSettings, ProjectPath, SourceRuntimeProjection};
use nara_reflect::provider::ResolvedComponentSchemaProvider;
#[cfg(feature = "serde")]
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, PersistentComponentSchemaOwnerId,
};
use nara_reflect::{
    ComponentRegistryBuilder, ComponentRegistryError, ComponentRegistrySnapshot,
    ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
    ComponentSchemaOwnerId, ComponentSchemaOwnerSnapshot, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderReceipt, ComponentTypeId, ExecutableRegistryFingerprint,
    SchemaCompositionFingerprint, preloaded_component_registry_plugin,
};
use nara_scene::runtime_configuration::{
    EffectiveRuntimeConfiguration, ProductCapability, ProductCapabilitySet,
    RuntimeCapabilityAdmission, admit_runtime_capabilities,
};
pub use nara_scene::runtime_configuration::{RuntimeCapabilityError, RuntimeCapabilityErrorKind};
use nara_scene::{SceneComponentBindingError, SceneComponentBindings};

mod runtime_configuration;

pub use runtime_configuration::{
    RuntimeConfigurationPlan, RuntimeConfigurationPlanError, resolve_runtime_configuration,
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

pub(super) fn validate_runtime_capabilities(
    settings: &EffectiveRuntimeConfiguration,
) -> Result<RuntimeCapabilityAdmission, RuntimeCapabilityError> {
    admit_runtime_capabilities(settings, compiled_product_capabilities().capabilities())
}

/// An immutable settings candidate whose product capability request fits this binary.
///
/// Source input selection remains an explicit obligation until content and receiving admission.
#[cfg(feature = "project-source")]
#[derive(Clone, PartialEq)]
pub struct ProjectSettingsCandidate {
    pub(super) lineage: ProjectSettingsLineage,
    pub(super) manifest_digest: ContentDigest,
    pub(super) settings: EffectiveProjectSettings,
    pub(super) source_runtime_projection: SourceRuntimeProjection,
    pub(super) capabilities: RuntimeCapabilityAdmission,
}

#[cfg(feature = "project-source")]
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

    /// Runtime-only values admitted from the fully resolved source profile.
    #[must_use]
    pub const fn runtime_configuration(&self) -> &EffectiveRuntimeConfiguration {
        self.source_runtime_projection.runtime()
    }

    /// Returns the source action-map path that still requires content and receiving admission.
    #[must_use]
    pub const fn input_requirement(&self) -> Option<&ProjectPath> {
        self.source_runtime_projection.input_requirement()
    }

    #[must_use]
    pub const fn explicit_capabilities(&self) -> ProductCapabilitySet {
        self.capabilities.explicit()
    }

    #[must_use]
    pub const fn implied_capabilities(&self) -> ProductCapabilitySet {
        self.capabilities
            .normalized()
            .difference(self.capabilities.explicit())
    }

    #[must_use]
    pub const fn normalized_capabilities(&self) -> ProductCapabilitySet {
        self.capabilities.normalized()
    }

    #[must_use]
    pub const fn required_capabilities(&self) -> ProductCapabilitySet {
        self.capabilities.required()
    }

    #[must_use]
    pub const fn compiled_capabilities(&self) -> CompiledProductCapabilities {
        CompiledProductCapabilities(self.capabilities.compiled())
    }
}

#[cfg(feature = "project-source")]
impl fmt::Debug for ProjectSettingsCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectSettingsCandidate")
            .field("lineage", &self.lineage)
            .field("manifest_digest", &self.manifest_digest)
            .field("runtime_preset", &self.settings.runtime_preset)
            .field("profile_present", &self.settings.profile_name.is_some())
            .field("explicit", &self.capabilities.explicit())
            .field("implied", &self.implied_capabilities())
            .field("normalized", &self.capabilities.normalized())
            .field("required", &self.capabilities.required())
            .field("compiled", &self.compiled_capabilities())
            .finish()
    }
}

/// A lineage-bound request for explicit advanced runtime-plan inspection.
///
/// Ordinary file-backed product actions do not expose this type. They accept one
/// [`crate::ProductRecipe`] and lower it privately. The type remains available from
/// [`crate::project_host::advanced`] for engine integration tests and advanced embedders that need
/// to inspect profile-owned slot edits before any `App` exists.
#[cfg(feature = "project-source")]
pub struct ProjectRuntimePlugins {
    lineage: ProjectSettingsLineage,
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
}

#[cfg(feature = "project-source")]
pub(crate) struct RuntimePluginOverlay {
    definition: PluginDefinition,
}

#[cfg(feature = "project-source")]
pub(crate) fn runtime_plugin_overlay(definition: PluginDefinition) -> RuntimePluginOverlay {
    RuntimePluginOverlay { definition }
}

#[cfg(feature = "project-source")]
const PROJECT_RUNTIME_OVERLAY_GROUP_ID: PluginGroupId =
    PluginGroupId::new("nara.plugins.project-runtime-overlay");

#[cfg(feature = "project-source")]
struct RuntimeOverlayPlugins {
    definition: PluginDefinition,
}

#[cfg(feature = "project-source")]
impl PluginGroup for RuntimeOverlayPlugins {
    const ID: PluginGroupId = PROJECT_RUNTIME_OVERLAY_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_definition(self.definition)
    }
}

struct ProjectRuntimeAppPlugins {
    plugins: EditedPluginGroup<crate::product::ProjectProfilePlugins>,
    recipe: crate::ProductRecipe,
    #[cfg(feature = "project-source")]
    overlay: Option<RuntimePluginOverlay>,
}

impl PluginGroup for ProjectRuntimeAppPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime-app");

    fn build(self) -> PluginGroupBuilder {
        let builder =
            PluginGroupBuilder::new().add_group(self.recipe.with_edited_defaults(self.plugins));
        #[cfg(feature = "project-source")]
        if let Some(overlay) = self.overlay {
            return builder.add_group(RuntimeOverlayPlugins {
                definition: overlay.definition,
            });
        }
        builder
    }
}

#[cfg(feature = "project-source")]
impl fmt::Debug for ProjectRuntimePlugins {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectRuntimePlugins")
            .field("lineage", &self.lineage)
            .field("recipe", &self.recipe)
            .field("has_advanced_overlay", &self.overlay.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "project-source")]
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
}

/// Frozen schema authority selected for one admitted Runtime plan.
///
/// Native receipts project the selected registry; data owners retain their admitted snapshots.
/// Managed admission requires the same complete selection, independently of equal fingerprints.
#[derive(Clone)]
pub struct SchemaValidationInput {
    provider_ids: Arc<[PluginSchemaProviderId]>,
    provider_receipts: Arc<[ComponentSchemaProviderReceipt]>,
    #[cfg(feature = "serde")]
    authoring_schema_bundle: AuthoringSchemaBundle,
    bindings: SceneComponentBindings,
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
                &self.snapshot().contribution_receipts().count(),
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

    /// Projects every selected native and data owner into metadata for authoring.
    #[cfg(feature = "serde")]
    #[must_use]
    pub const fn authoring_schema_bundle(&self) -> &AuthoringSchemaBundle {
        &self.authoring_schema_bundle
    }

    pub fn contribution_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaContributionReceipt> + '_ {
        self.snapshot().contribution_receipts()
    }

    #[must_use]
    pub fn provider_receipts(&self) -> &[ComponentSchemaProviderReceipt] {
        &self.provider_receipts
    }

    pub fn owner_receipts(
        &self,
    ) -> impl ExactSizeIterator<Item = ComponentSchemaOwnerContributionReceipt> + '_ {
        self.snapshot().owner_receipts()
    }

    #[must_use]
    pub fn snapshot(&self) -> &ComponentRegistrySnapshot {
        self.bindings.native_snapshot()
    }

    #[must_use]
    pub fn bindings(&self) -> &SceneComponentBindings {
        &self.bindings
    }

    #[must_use]
    pub const fn composition_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.composition_fingerprint
    }

    #[must_use]
    pub const fn executable_fingerprint(&self) -> ExecutableRegistryFingerprint {
        self.executable_fingerprint
    }
}

/// A source-lineage-bound Runtime composition and its remaining input selection obligation.
#[cfg(feature = "project-source")]
#[derive(Clone)]
pub struct RuntimePlan {
    lineage: ProjectSettingsLineage,
    configuration: RuntimeConfigurationPlan,
    input_requirement: Option<ProjectPath>,
    pub(crate) asset_domains: crate::project_content::ProjectAssetDomains,
}

#[cfg(feature = "project-source")]
impl fmt::Debug for RuntimePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePlan")
            .field("lineage", &self.lineage)
            .field("configuration", &self.configuration)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "project-source")]
impl RuntimePlan {
    #[must_use]
    pub const fn configuration_plan(&self) -> &RuntimeConfigurationPlan {
        &self.configuration
    }

    #[must_use]
    pub const fn lineage(&self) -> ProjectSettingsLineage {
        self.lineage
    }

    #[must_use]
    /// Returns only the admitted values consumed by the Runtime composition.
    ///
    /// The source input obligation is retained separately through [`Self::input_requirement`].
    /// Other project paths, authoring inputs and profile selection remain with the source candidate.
    pub const fn settings(&self) -> &EffectiveRuntimeConfiguration {
        self.configuration.settings()
    }

    /// Returns the exact source action-map path that the Host must fulfill before startup.
    #[must_use]
    pub const fn input_requirement(&self) -> Option<&ProjectPath> {
        self.input_requirement.as_ref()
    }

    #[must_use]
    pub const fn compiled_capabilities(&self) -> CompiledProductCapabilities {
        self.configuration.compiled_capabilities()
    }

    #[must_use]
    pub const fn explicit_capabilities(&self) -> ProductCapabilitySet {
        self.configuration.explicit_capabilities()
    }

    #[must_use]
    pub const fn normalized_capabilities(&self) -> ProductCapabilitySet {
        self.configuration.normalized_capabilities()
    }

    /// Returns the conservative product-capability witness used to admit this plan.
    ///
    /// For an any-of plugin clause, every requested matching alternative is retained. This set is
    /// therefore deterministic and monotonic, but is not a minimal dependency solution.
    #[must_use]
    pub const fn required_capabilities(&self) -> ProductCapabilitySet {
        self.configuration.required_capabilities()
    }

    #[must_use]
    pub const fn plugin_plan(&self) -> &PluginPlan {
        self.configuration.plugin_plan()
    }

    #[must_use]
    pub const fn schema_validation(&self) -> &SchemaValidationInput {
        self.configuration.schema_validation()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    ProjectLineageMismatch,
    /// A selected global action map has no actual Input plugin in the resolved plan.
    InputOwnerRequired,
    /// The first-party Input plugin was selected without its Host integration feature.
    InputFeatureRequired,
    /// Processed startup Input requires a selected decoder for the actual typed family and format.
    InputCodecRequired,
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
    DataContributionNotSelected {
        plugin: PluginId,
    },
    SceneBindingsRejected {
        source: Box<SceneComponentBindingError>,
    },
    SchemaAuthorityPublicationFailed,
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProjectLineageMismatch => {
                "project runtime request lineage does not match candidate"
            }
            Self::InputOwnerRequired => "selected input action map requires the Input plugin",
            Self::InputFeatureRequired => "the Input plugin requires the Host runtime-input feature",
            Self::InputCodecRequired => "processed input action map requires its selected typed Runtime codec",
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
            Self::DataContributionNotSelected { .. } => {
                "data Schema contribution requires its exact recipe plugin definition"
            }
            Self::SceneBindingsRejected { .. } => "Scene component binding selection was rejected",
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
            Self::SceneBindingsRejected { source } => Some(source.as_ref()),
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
/// Ordinary file-backed products bind schema authority through [`crate::SchemaContribution`].
#[must_use]
pub fn built_in_schema_providers() -> Vec<ComponentSchemaProviderDefinition> {
    #[allow(unused_mut)]
    let mut providers = vec![
        nara_scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
        nara_scene::TRANSFORM_SCHEMA_PROVIDER,
    ];
    #[cfg(feature = "runtime-2d")]
    {
        providers.push(nara_render::sprite::SPRITE_SCHEMA_PROVIDER);
        providers.push(nara_render::tilemap::TILEMAP_SCHEMA_PROVIDER);
    }
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    providers.push(nara_render::material::MATERIAL_SCHEMA_PROVIDER);
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
#[cfg(feature = "project-source")]
#[must_use]
pub fn project_runtime_plugins(candidate: &ProjectSettingsCandidate) -> ProjectRuntimePlugins {
    project_runtime_plugins_from_profile(
        candidate,
        crate::product::ProjectProfilePlugins::new(
            candidate.runtime_configuration(),
            candidate.normalized_capabilities(),
        ),
    )
}

#[cfg(feature = "project-source")]
fn project_runtime_plugins_from_profile(
    candidate: &ProjectSettingsCandidate,
    plugins: crate::product::ProjectProfilePlugins,
) -> ProjectRuntimePlugins {
    ProjectRuntimePlugins {
        lineage: candidate.lineage,
        plugins: configured_profile_plugins(candidate.runtime_configuration(), plugins),
        recipe: crate::ProductRecipe::new(),
        overlay: None,
    }
}

fn configured_profile_plugins(
    settings: &EffectiveRuntimeConfiguration,
    plugins: crate::product::ProjectProfilePlugins,
) -> EditedPluginGroup<crate::product::ProjectProfilePlugins> {
    plugins.edit().configure(nara_app::diagnostics::plugin(
        settings.diagnostics().runtime,
        nara_app::diagnostics::RuntimePressureSettings::default(),
    ))
}

/// Builds the file-backed project request for one ordinary product recipe.
#[cfg(feature = "project-source")]
#[must_use]
pub(crate) fn project_runtime_plugins_with_recipe(
    candidate: &ProjectSettingsCandidate,
    recipe: crate::ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
) -> ProjectRuntimePlugins {
    let request = project_runtime_plugins(candidate).with_recipe(recipe);
    match overlay {
        Some(overlay) => request.with_overlay(overlay),
        None => request,
    }
}

#[cfg(feature = "project-source")]
pub(crate) fn resolve_project_runtime_request(
    candidate: &ProjectSettingsCandidate,
    request: ProjectRuntimePlugins,
) -> Result<RuntimePlan, RuntimePlanError> {
    resolve_runtime_plan(candidate, request, built_in_schema_providers())
}

#[cfg(feature = "project-source")]
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

/// Resolves one ordinary product recipe before creating an `App` or acquiring runtime authority.
#[cfg(feature = "project-source")]
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
#[cfg(feature = "project-source")]
pub fn resolve_runtime_plan(
    candidate: &ProjectSettingsCandidate,
    request: ProjectRuntimePlugins,
    known_providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<RuntimePlan, RuntimePlanError> {
    if request.lineage != candidate.lineage {
        return Err(CompositionError::ProjectLineageMismatch.into());
    }

    let resolved = resolve_runtime_composition(
        candidate.runtime_configuration(),
        candidate.capabilities,
        ProjectRuntimeAppPlugins {
            plugins: request.plugins,
            recipe: request.recipe,
            overlay: request.overlay,
        },
        known_providers,
    )?;
    if candidate.input_requirement().is_some()
        && !has_input_owner(resolved.configuration.plugin_plan())
    {
        return Err(CompositionError::InputOwnerRequired.into());
    }
    Ok(RuntimePlan {
        lineage: candidate.lineage,
        configuration: resolved.configuration,
        input_requirement: candidate.input_requirement().cloned(),
        asset_domains: resolved.asset_domains,
    })
}

struct ResolvedRuntimeComposition {
    configuration: RuntimeConfigurationPlan,
    #[cfg(feature = "project-source")]
    asset_domains: crate::project_content::ProjectAssetDomains,
}

#[cfg(not(feature = "runtime-input"))]
const INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.input");

fn has_input_owner(plan: &PluginPlan) -> bool {
    plan.entries()
        .iter()
        .any(|entry| entry.plugin_id() == INPUT_PLUGIN_ID)
}

fn resolve_runtime_composition(
    settings: &EffectiveRuntimeConfiguration,
    capabilities: RuntimeCapabilityAdmission,
    mut request: ProjectRuntimeAppPlugins,
    known_providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<ResolvedRuntimeComposition, RuntimePlanError> {
    #[cfg(feature = "render-wgpu")]
    {
        request.recipe = request
            .recipe
            .with_profile_capabilities(capabilities.normalized());
    }
    if let Some(plugin) = request
        .recipe
        .unsupported_default_plugin(capabilities.normalized())
    {
        return Err(CompositionError::UnsupportedDefaultConfiguration { plugin }.into());
    }

    let recipe_providers = request.recipe.schema_providers().collect::<Vec<_>>();
    let data_owners = request
        .recipe
        .data_schema_owners()
        .map(|(_, owner)| owner)
        .collect::<Vec<_>>();
    let data_contributors = request.recipe.data_contributors();
    #[cfg(feature = "project-source")]
    let asset_domains = request.recipe.asset_domains.clone();
    #[cfg(feature = "serde")]
    let runtime_codecs = request.recipe.runtime_codecs.clone();
    let snapshot_cell = Arc::new(OnceLock::new());
    request.plugins = request
        .plugins
        .configure(preloaded_component_registry_plugin(Arc::clone(
            &snapshot_cell,
        )));
    let plugin_plan = PluginPlan::resolve(request)?;
    let input_owner_selected = has_input_owner(&plugin_plan);
    #[cfg(not(feature = "runtime-input"))]
    if input_owner_selected {
        return Err(CompositionError::InputFeatureRequired.into());
    }
    if settings.input().action_map.is_some() {
        if !input_owner_selected {
            return Err(CompositionError::InputOwnerRequired.into());
        }
        #[cfg(all(feature = "serde", feature = "runtime-input"))]
        let input_codec_selected = runtime_codecs.has_family_format::<nara_input::ActionMapDomain>(
            <nara_input::ActionMapRuntimeCodec as nara_asset::RuntimeAssetCodec<
                nara_input::ActionMapAsset,
            >>::RUNTIME_KIND,
            <nara_input::ActionMapRuntimeCodec as nara_asset::RuntimeAssetCodec<
                nara_input::ActionMapAsset,
            >>::RUNTIME_VERSION,
        );
        #[cfg(not(all(feature = "serde", feature = "runtime-input")))]
        let input_codec_selected = false;
        if !input_codec_selected {
            return Err(CompositionError::InputCodecRequired.into());
        }
    }
    if let Some(plugin) =
        crate::product::data_contributor_mismatch(&data_contributors, plugin_plan.entries())
    {
        return Err(CompositionError::DataContributionNotSelected { plugin }.into());
    }
    #[cfg(feature = "project-source")]
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
    let required = resolve_product_requirements(capabilities, &plugin_plan)?;
    let schema_validation = resolve_schema_validation(
        &plugin_plan,
        known_providers.into_iter().chain(recipe_providers),
        data_owners,
    )?;
    snapshot_cell
        .set(schema_validation.snapshot().clone())
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;

    Ok(ResolvedRuntimeComposition {
        configuration: RuntimeConfigurationPlan {
            settings: settings.clone(),
            compiled: CompiledProductCapabilities(capabilities.compiled()),
            explicit: capabilities.explicit(),
            normalized: capabilities.normalized(),
            required,
            plugin_plan,
            schema_validation,
            #[cfg(feature = "serde")]
            runtime_codecs,
        },
        #[cfg(feature = "project-source")]
        asset_domains,
    })
}

fn resolve_product_requirements(
    capabilities: RuntimeCapabilityAdmission,
    plugin_plan: &PluginPlan,
) -> Result<ProductCapabilitySet, CompositionError> {
    let mut required = capabilities.required();
    for entry in plugin_plan.entries() {
        for declared in entry.declaration().requires_product_capabilities {
            let Some(capability) = product_capability(*declared) else {
                return Err(CompositionError::UnknownProductCapability {
                    plugin: entry.plugin_id(),
                    capability: *declared,
                });
            };
            if !capabilities.normalized().contains(capability) {
                return Err(CompositionError::UnrequestedProductCapability {
                    plugin: entry.plugin_id(),
                    capability,
                });
            }
            if !capabilities.compiled().contains(capability) {
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

        let requested_alternatives = alternatives.intersection(capabilities.normalized());
        if requested_alternatives.is_empty() {
            return Err(CompositionError::UnrequestedProductCapabilityAlternatives {
                plugin: entry.plugin_id(),
                alternatives,
            });
        }
        for capability in requested_alternatives.iter() {
            if !capabilities.compiled().contains(capability) {
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
    data_owners: Vec<ComponentSchemaOwnerSnapshot>,
) -> Result<SchemaValidationInput, CompositionError> {
    let mut provider_catalog =
        BTreeMap::<PluginSchemaProviderId, ResolvedComponentSchemaProvider>::new();
    for provider in known_providers {
        let provider_id = provider.id();
        let resolved =
            provider
                .resolve()
                .map_err(|source| CompositionError::SchemaProviderRejected {
                    provider: provider_id,
                    source: Box::new(source),
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
        for source in std::iter::once(resolved.current()).chain(resolved.predecessor()) {
            for component_id in source
                .catalog()
                .components()
                .iter()
                .map(|schema| schema.id())
                .chain(source.catalog().type_tombstones())
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
    }

    for owner in &data_owners {
        if known_owners
            .keys()
            .any(|native| native.as_str() == owner.owner_id().as_str())
        {
            return Err(CompositionError::SceneBindingsRejected {
                source: Box::new(SceneComponentBindingError::DuplicateOwner {
                    owner: owner.owner_id().clone(),
                }),
            });
        }
        for catalog in std::iter::once(owner.current()).chain(owner.predecessor()) {
            for component in catalog
                .components()
                .iter()
                .map(|schema| schema.id())
                .chain(catalog.type_tombstones())
            {
                if known_claims.contains_key(component) {
                    return Err(CompositionError::SceneBindingsRejected {
                        source: Box::new(SceneComponentBindingError::DivergentComponentOwnership {
                            component: component.clone(),
                        }),
                    });
                }
            }
        }
    }

    let mut selected_providers = BTreeSet::<PluginSchemaProviderId>::new();
    for entry in plugin_plan.entries() {
        for provider in entry.declaration().provides_schema {
            selected_providers.insert(*provider);
        }
    }

    let mut registry = ComponentRegistryBuilder::new();
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
            registry.register_resolved_schema_provider(provider)
        }))
        .map_err(|_| CompositionError::SchemaProviderPanicked {
            provider: *provider_id,
        })?
        .map_err(|source| CompositionError::SchemaProviderRejected {
            provider: *provider_id,
            source: Box::new(source),
        })?;
    }
    let snapshot = registry
        .freeze()
        .map_err(|source| CompositionError::SchemaFreezeRejected {
            source: Box::new(source.into_error()),
        })?;
    let bindings =
        SceneComponentBindings::new(snapshot.clone(), data_owners).map_err(|source| {
            CompositionError::SceneBindingsRejected {
                source: Box::new(source),
            }
        })?;
    let composition_fingerprint = bindings.composition_fingerprint();
    let executable_fingerprint = snapshot
        .executable_registry_fingerprint()
        .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?;
    let provider_receipts = snapshot.provider_receipts().collect::<Vec<_>>();
    let provider_ids = provider_receipts
        .iter()
        .map(|receipt| receipt.provider())
        .collect::<Vec<_>>();
    #[cfg(feature = "serde")]
    let authoring_schema_bundle = {
        authoring_schema_owners.extend(bindings.data_owners().iter().map(|owner| {
            AuthoringSchemaBundleOwner::new(
                owner.owner_id().clone(),
                owner.current().clone(),
                owner.predecessor().cloned(),
            )
        }));
        AuthoringSchemaBundle::new(authoring_schema_owners)
            .map_err(|_| CompositionError::SchemaAuthorityPublicationFailed)?
    };

    Ok(SchemaValidationInput {
        provider_ids: provider_ids.into(),
        provider_receipts: provider_receipts.into(),
        #[cfg(feature = "serde")]
        authoring_schema_bundle,
        bindings,
        composition_fingerprint,
        executable_fingerprint,
    })
}
