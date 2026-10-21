use std::{
    any::{TypeId, type_name},
    collections::{BTreeMap, btree_map::Entry},
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginId, PluginSchemaProviderId,
    PluginSlot, PluginSlotId,
};
use nara_reflect::ComponentSchemaProviderDefinition;
use thiserror::Error;

mod default_settings;
mod profile;
#[cfg(feature = "runtime-ui")]
mod ui_surface;

pub(crate) use profile::ProjectProfilePlugins;
#[cfg(feature = "presentation-ui")]
pub(crate) use ui_surface::AcceptedPrimaryUiSurfaceProfile;
#[cfg(feature = "runtime-ui")]
pub(crate) use ui_surface::PROJECT_PRIMARY_UI_SURFACE_ID;

const PRODUCT_RECIPE_GROUP_ID: PluginGroupId = PluginGroupId::new("nara.product-recipe");
const PRODUCT_RECIPE_CONFIGURATION_PREFIX: &[u8] = b"nara.product-recipe.config.v1\0";
const PRODUCT_RECIPE_SCHEMA_AUTHORITY_PLUGIN_ID: PluginId =
    PluginId::new("nara.product-recipe.schema-authority");
const PRODUCT_RECIPE_SCHEMA_AUTHORITY_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PRODUCT_RECIPE_SCHEMA_AUTHORITY_PLUGIN_ID,
    PluginCategory::Core,
)
.requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT);
const PRODUCT_RECIPE_SCHEMA_AUTHORITY_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.product-recipe.schema-authority", 1);
const PRODUCT_RECIPE_SCHEMA_AUTHORITY_CONFIGURATION_PREFIX: &[u8] =
    b"nara.product-recipe.schema-authority.v1\0";

/// Canonical encoding for one typed product-plugin configuration.
///
/// Implementations must write every behavior-bearing field in a deterministic,
/// architecture-independent form. The bytes are not persisted; they identify equivalent recipe
/// configurations across repeated resolution within one compiled product.
pub trait ProductConfiguration: Send + Sync + 'static {
    fn write_canonical(&self, output: &mut Vec<u8>);
}

impl ProductConfiguration for () {
    fn write_canonical(&self, _output: &mut Vec<u8>) {}
}

/// The product meaning of one recipe entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductRecipeEntryKind {
    RuntimePlugin,
    SchemaContribution,
}

/// An inspectable, replayable entry in a [`ProductRecipe`].
#[derive(Clone)]
pub struct ProductRecipeEntry {
    plugin_type: TypeId,
    plugin_type_name: &'static str,
    plugin_id: PluginId,
    kind: ProductRecipeEntryKind,
    configuration_fingerprint: [u8; 32],
    schema_provider_ids: Arc<[PluginSchemaProviderId]>,
    schema_providers: Arc<[ComponentSchemaProviderDefinition]>,
    definition: PluginDefinition,
}

#[derive(Clone, Copy)]
struct RecipeSchemaProviderBinding {
    plugin: PluginId,
    provider: ComponentSchemaProviderDefinition,
}

struct ProductRecipeSchemaAuthorityPlugin {
    providers: Arc<[RecipeSchemaProviderBinding]>,
}

impl Plugin for ProductRecipeSchemaAuthorityPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PRODUCT_RECIPE_SCHEMA_AUTHORITY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        for binding in self.providers.iter().copied() {
            nara_reflect::register_schema_provider_for_plugin(
                app,
                binding.plugin,
                binding.provider.id().as_str(),
                &binding.provider,
            )?;
        }
        Ok(())
    }
}

impl fmt::Debug for ProductRecipeEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductRecipeEntry")
            .field("plugin_type", &self.plugin_type_name)
            .field("plugin_id", &self.plugin_id)
            .field("kind", &self.kind)
            .field("configuration_fingerprint", &self.configuration_fingerprint)
            .field("schema_provider_ids", &self.schema_provider_ids)
            .finish_non_exhaustive()
    }
}

impl ProductRecipeEntry {
    #[must_use]
    pub const fn plugin_id(&self) -> PluginId {
        self.plugin_id
    }

    #[must_use]
    pub const fn kind(&self) -> ProductRecipeEntryKind {
        self.kind
    }

    #[must_use]
    pub const fn configuration_fingerprint(&self) -> [u8; 32] {
        self.configuration_fingerprint
    }

    #[must_use]
    pub fn schema_provider_ids(&self) -> &[PluginSchemaProviderId] {
        &self.schema_provider_ids
    }
}

/// A replayable schema-owning plugin plus its trusted provider definitions.
///
/// Extension crates normally expose a small function returning this value. Product callers then
/// add that value once instead of maintaining parallel plugin and provider lists. When the
/// contribution enters a recipe, its provider definitions are the schema authority for both direct
/// `App` composition and file-backed Hosts. A plugin may still validate the same provider during
/// installation, but a different receipt is rejected before publication.
pub struct SchemaContribution<P: Plugin> {
    entry: ProductRecipeEntry,
    marker: PhantomData<fn() -> P>,
}

impl<P: Plugin> Clone for SchemaContribution<P> {
    fn clone(&self) -> Self {
        Self {
            entry: self.entry.clone(),
            marker: PhantomData,
        }
    }
}

impl<P: Plugin> fmt::Debug for SchemaContribution<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SchemaContribution")
            .field(&self.entry)
            .finish()
    }
}

impl<P: Plugin> SchemaContribution<P> {
    /// Creates a contribution whose plugin is reconstructed with `Default`.
    pub fn for_default(
        providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
    ) -> Result<Self, ProductRecipeError>
    where
        P: Default,
    {
        let declaration = recipe_declaration::<P>()?;
        let definition = PluginDefinition::infallible::<P, _>(
            recipe_definition_id(declaration),
            PRODUCT_RECIPE_CONFIGURATION_PREFIX,
            P::default,
        );
        schema_contribution(declaration, definition, providers)
    }

    /// Creates a contribution from typed configuration and a reconstructible factory.
    pub fn configured<C, F>(
        configuration: C,
        factory: F,
        providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
    ) -> Result<Self, ProductRecipeError>
    where
        C: ProductConfiguration,
        F: Fn(&C) -> P + Send + Sync + 'static,
    {
        let declaration = recipe_declaration::<P>()?;
        let definition = configured_definition(declaration, configuration, factory)?;
        schema_contribution(declaration, definition, providers)
    }
}

/// A pure, inspectable composition of replayable Rust product plugins.
///
/// Construction validates duplicate identity, declared provider bindings, and typed replacement
/// without creating an `App`, opening project data, or acquiring runtime authority. Full
/// cross-provider schema validation remains the responsibility of the selected composition path;
/// the file-backed Project Host performs it during admission. A recipe is also a normal
/// [`PluginGroup`]. Direct `App` composition supplies the owner of configured defaults through
/// [`Self::with_defaults`]; the file-backed Project Host supplies its selected profile. Schema
/// contributions install their provider authority before their
/// plugin builds in a direct `App`, while a file-backed Host preloads the same provider set into
/// its admitted registry snapshot. The contribution owns registration; its plugin builds runtime
/// behavior without registering the provider a second time.
#[derive(Clone, Default)]
pub struct ProductRecipe {
    entries: BTreeMap<PluginId, ProductRecipeEntry>,
    defaults: default_settings::ProductDefaultSettings,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) asset_domains: crate::project_content::ProjectAssetDomains,
}

impl fmt::Debug for ProductRecipe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("ProductRecipe");
        debug
            .field("entries", &self.entries.values().collect::<Vec<_>>())
            .field("defaults", &self.defaults);
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        debug.field("asset_domains", &self.asset_domains);
        debug.finish()
    }
}

impl ProductRecipe {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            defaults: default_settings::ProductDefaultSettings::new(),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            asset_domains: crate::project_content::ProjectAssetDomains::new(),
        }
    }

    #[must_use]
    /// Returns the number of explicit plugin entries, excluding default selections.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        let empty = self.entries.is_empty() && self.defaults.is_empty();
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        let empty = empty && self.asset_domains.is_empty();
        empty
    }

    /// Selects one compiled source domain for file-backed Check and Play. Settings live in metadata.
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub fn add_asset_domain<D: crate::project_content::ProjectAssetDomain>(
        mut self,
    ) -> Result<Self, ProductRecipeError> {
        self.asset_domains.add::<D>()?;
        Ok(self)
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &ProductRecipeEntry> {
        self.entries.values()
    }

    /// Adds a replayable runtime-only plugin reconstructed with `Default`.
    pub fn add_plugin<P>(self) -> Result<Self, ProductRecipeError>
    where
        P: Plugin + Default,
    {
        let declaration = recipe_declaration::<P>()?;
        let definition = PluginDefinition::infallible::<P, _>(
            recipe_definition_id(declaration),
            PRODUCT_RECIPE_CONFIGURATION_PREFIX,
            P::default,
        );
        self.add_runtime_entry(recipe_entry::<P>(
            declaration,
            ProductRecipeEntryKind::RuntimePlugin,
            definition,
            Vec::new(),
        )?)
    }

    /// Adds a replayable runtime-only plugin with typed configuration.
    pub fn add_configured_plugin<P, C, F>(
        self,
        configuration: C,
        factory: F,
    ) -> Result<Self, ProductRecipeError>
    where
        P: Plugin,
        C: ProductConfiguration,
        F: Fn(&C) -> P + Send + Sync + 'static,
    {
        let declaration = recipe_declaration::<P>()?;
        let definition = configured_definition(declaration, configuration, factory)?;
        self.add_runtime_entry(recipe_entry::<P>(
            declaration,
            ProductRecipeEntryKind::RuntimePlugin,
            definition,
            Vec::new(),
        )?)
    }

    /// Replaces the typed configuration and factory for an existing runtime-only plugin.
    pub fn configure_plugin<P, C, F>(
        self,
        configuration: C,
        factory: F,
    ) -> Result<Self, ProductRecipeError>
    where
        P: Plugin,
        C: ProductConfiguration,
        F: Fn(&C) -> P + Send + Sync + 'static,
    {
        let declaration = recipe_declaration::<P>()?;
        let definition = configured_definition(declaration, configuration, factory)?;
        self.replace_entry(recipe_entry::<P>(
            declaration,
            ProductRecipeEntryKind::RuntimePlugin,
            definition,
            Vec::new(),
        )?)
    }

    /// Adds one schema-owning plugin and its provider definitions as a single contribution.
    pub fn add_contribution<P: Plugin>(
        self,
        contribution: SchemaContribution<P>,
    ) -> Result<Self, ProductRecipeError> {
        self.insert_entry(contribution.entry)
    }

    /// Replaces an existing schema-owning contribution of the same plugin type.
    pub fn configure_contribution<P: Plugin>(
        self,
        contribution: SchemaContribution<P>,
    ) -> Result<Self, ProductRecipeError> {
        self.replace_entry(contribution.entry)
    }

    fn add_runtime_entry(self, entry: ProductRecipeEntry) -> Result<Self, ProductRecipeError> {
        if !entry.schema_provider_ids.is_empty() {
            return Err(ProductRecipeError::RuntimePluginProvidesSchema {
                plugin: entry.plugin_id,
            });
        }
        self.insert_entry(entry)
    }

    fn insert_entry(mut self, entry: ProductRecipeEntry) -> Result<Self, ProductRecipeError> {
        let plugin = entry.plugin_id;
        match self.entries.entry(plugin) {
            Entry::Vacant(slot) => {
                slot.insert(entry);
                Ok(self)
            }
            Entry::Occupied(_) => Err(ProductRecipeError::DuplicatePlugin { plugin }),
        }
    }

    fn replace_entry(mut self, entry: ProductRecipeEntry) -> Result<Self, ProductRecipeError> {
        let plugin = entry.plugin_id;
        match self.entries.entry(plugin) {
            Entry::Vacant(_) => Err(ProductRecipeError::PluginNotPresent { plugin }),
            Entry::Occupied(mut slot) => {
                let (existing_type, existing_kind) = {
                    let existing = slot.get();
                    (existing.plugin_type, existing.kind)
                };
                if existing_type != entry.plugin_type {
                    return Err(ProductRecipeError::PluginTypeMismatch { plugin });
                }
                if existing_kind != entry.kind {
                    return Err(ProductRecipeError::EntryKindMismatch {
                        plugin,
                        expected: existing_kind,
                        actual: entry.kind,
                    });
                }
                slot.insert(entry);
                Ok(self)
            }
        }
    }

    pub(crate) fn schema_providers(
        &self,
    ) -> impl Iterator<Item = ComponentSchemaProviderDefinition> + '_ {
        self.schema_provider_bindings()
            .map(|binding| binding.provider)
    }

    fn schema_provider_bindings(&self) -> impl Iterator<Item = RecipeSchemaProviderBinding> + '_ {
        self.entries.values().flat_map(|entry| {
            entry.schema_providers.iter().copied().map(move |provider| {
                RecipeSchemaProviderBinding {
                    plugin: entry.plugin_id,
                    provider,
                }
            })
        })
    }

    fn schema_authority_definition(&self) -> Option<PluginDefinition> {
        let mut providers = self.schema_provider_bindings().collect::<Vec<_>>();
        if providers.is_empty() {
            return None;
        }
        providers.sort_by_key(|binding| (binding.plugin, binding.provider.id()));
        let canonical_configuration = schema_authority_configuration(&providers);
        let providers: Arc<[RecipeSchemaProviderBinding]> = providers.into();
        Some(PluginDefinition::infallible::<
            ProductRecipeSchemaAuthorityPlugin,
            _,
        >(
            PRODUCT_RECIPE_SCHEMA_AUTHORITY_DEFINITION_ID,
            canonical_configuration,
            move || ProductRecipeSchemaAuthorityPlugin {
                providers: Arc::clone(&providers),
            },
        ))
    }

    fn build_entries(self, builder: PluginGroupBuilder) -> PluginGroupBuilder {
        let builder = match self.schema_authority_definition() {
            Some(definition) => builder.add_definition(definition),
            None => builder,
        };
        self.entries.into_values().fold(builder, |builder, entry| {
            builder.add_definition(entry.definition)
        })
    }
}

impl PluginGroup for ProductRecipe {
    const ID: PluginGroupId = PRODUCT_RECIPE_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        if self.defaults.is_empty() {
            self.build_entries(PluginGroupBuilder::new())
        } else {
            self.with_defaults(default_settings::MissingProductDefaults)
                .build()
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductRecipeError {
    #[error("asset domain identity {kind} is empty or reserved by the product")]
    ReservedAssetDomain { kind: &'static str },
    #[error("asset domain {kind} appears more than once in the product recipe")]
    DuplicateAssetDomain { kind: &'static str },
    #[error("default plugin {plugin} is configured more than once in the product recipe")]
    DuplicateDefaultConfiguration { plugin: PluginId },
    #[error("plugin declaration for {plugin_type} panicked")]
    DeclarationPanicked { plugin_type: &'static str },
    #[error("plugin {plugin} configuration encoding panicked")]
    ConfigurationPanicked { plugin: PluginId },
    #[error("plugin {plugin} did not produce a stable recipe configuration identity")]
    MissingConfigurationIdentity { plugin: PluginId },
    #[error("plugin {plugin} appears more than once in the product recipe")]
    DuplicatePlugin { plugin: PluginId },
    #[error("plugin {plugin} is not present in the product recipe")]
    PluginNotPresent { plugin: PluginId },
    #[error("plugin {plugin} is already bound to another Rust plugin type")]
    PluginTypeMismatch { plugin: PluginId },
    #[error("plugin {plugin} cannot change recipe entry kind from {expected:?} to {actual:?}")]
    EntryKindMismatch {
        plugin: PluginId,
        expected: ProductRecipeEntryKind,
        actual: ProductRecipeEntryKind,
    },
    #[error("runtime-only plugin {plugin} declares schema providers and requires a contribution")]
    RuntimePluginProvidesSchema { plugin: PluginId },
    #[error("schema contribution plugin {plugin} declares no schema providers")]
    SchemaContributionWithoutSchema { plugin: PluginId },
    #[error(
        "schema contribution for {plugin} does not match its declared providers: declared {declared:?}, supplied {supplied:?}"
    )]
    SchemaProviderMismatch {
        plugin: PluginId,
        declared: Vec<PluginSchemaProviderId>,
        supplied: Vec<PluginSchemaProviderId>,
    },
    #[error("schema contribution for {plugin} supplied provider {provider} more than once")]
    DuplicateSchemaProvider {
        plugin: PluginId,
        provider: PluginSchemaProviderId,
    },
}

fn recipe_declaration<P: Plugin>() -> Result<&'static PluginDeclaration, ProductRecipeError> {
    catch_unwind(AssertUnwindSafe(P::declaration)).map_err(|_| {
        ProductRecipeError::DeclarationPanicked {
            plugin_type: type_name::<P>(),
        }
    })
}

const fn recipe_definition_id(declaration: &PluginDeclaration) -> PluginDefinitionId {
    PluginDefinitionId::new(declaration.id.as_str(), 1)
}

fn configured_definition<P, C, F>(
    declaration: &'static PluginDeclaration,
    configuration: C,
    factory: F,
) -> Result<PluginDefinition, ProductRecipeError>
where
    P: Plugin,
    C: ProductConfiguration,
    F: Fn(&C) -> P + Send + Sync + 'static,
{
    let mut canonical = Vec::from(PRODUCT_RECIPE_CONFIGURATION_PREFIX);
    catch_unwind(AssertUnwindSafe(|| {
        configuration.write_canonical(&mut canonical);
    }))
    .map_err(|_| ProductRecipeError::ConfigurationPanicked {
        plugin: declaration.id,
    })?;
    Ok(PluginDefinition::infallible::<P, _>(
        recipe_definition_id(declaration),
        canonical,
        move || factory(&configuration),
    ))
}

fn schema_contribution<P: Plugin>(
    declaration: &'static PluginDeclaration,
    definition: PluginDefinition,
    providers: impl IntoIterator<Item = ComponentSchemaProviderDefinition>,
) -> Result<SchemaContribution<P>, ProductRecipeError> {
    if declaration.provides_schema.is_empty() {
        return Err(ProductRecipeError::SchemaContributionWithoutSchema {
            plugin: declaration.id,
        });
    }

    let mut providers = providers.into_iter().collect::<Vec<_>>();
    providers.sort_by_key(|provider| provider.id());
    for pair in providers.windows(2) {
        if pair[0].id() == pair[1].id() {
            return Err(ProductRecipeError::DuplicateSchemaProvider {
                plugin: declaration.id,
                provider: pair[0].id(),
            });
        }
    }
    let supplied = providers
        .iter()
        .map(|provider| provider.id())
        .collect::<Vec<_>>();
    let mut declared = declaration.provides_schema.to_vec();
    declared.sort_unstable();
    if supplied != declared {
        return Err(ProductRecipeError::SchemaProviderMismatch {
            plugin: declaration.id,
            declared,
            supplied,
        });
    }

    Ok(SchemaContribution {
        entry: recipe_entry::<P>(
            declaration,
            ProductRecipeEntryKind::SchemaContribution,
            definition,
            providers,
        )?,
        marker: PhantomData,
    })
}

fn recipe_entry<P: Plugin>(
    declaration: &'static PluginDeclaration,
    kind: ProductRecipeEntryKind,
    definition: PluginDefinition,
    providers: Vec<ComponentSchemaProviderDefinition>,
) -> Result<ProductRecipeEntry, ProductRecipeError> {
    let configuration_fingerprint = definition
        .key()
        .ok_or(ProductRecipeError::MissingConfigurationIdentity {
            plugin: declaration.id,
        })?
        .configuration()
        .as_bytes();
    let schema_provider_ids = declaration.provides_schema.into();
    Ok(ProductRecipeEntry {
        plugin_type: TypeId::of::<P>(),
        plugin_type_name: type_name::<P>(),
        plugin_id: declaration.id,
        kind,
        configuration_fingerprint,
        schema_provider_ids,
        schema_providers: providers.into(),
        definition,
    })
}

fn schema_authority_configuration(providers: &[RecipeSchemaProviderBinding]) -> Vec<u8> {
    let mut output = Vec::from(PRODUCT_RECIPE_SCHEMA_AUTHORITY_CONFIGURATION_PREFIX);
    output.extend_from_slice(&(providers.len() as u64).to_le_bytes());
    for binding in providers {
        write_canonical_string(&mut output, binding.plugin.as_str());
        write_canonical_string(&mut output, binding.provider.owner().as_str());
        write_canonical_string(&mut output, binding.provider.id().as_str());
        let receipt = binding.provider.receipt();
        let provider_binding = receipt.binding();
        write_canonical_string(&mut output, provider_binding.as_str());
        output.extend_from_slice(&provider_binding.version().to_le_bytes());
        output.extend_from_slice(&provider_binding.codec_version().to_le_bytes());
        output.extend_from_slice(&provider_binding.migration_version().to_le_bytes());
    }
    output
}

fn write_canonical_string(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u64).to_le_bytes());
    output.extend_from_slice(value.as_bytes());
}

const SERVER_TIME_POLICY_PLUGIN_ID: PluginId = PluginId::new("nara.server-time-policy");
const SERVER_TIME_POLICY_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SERVER_TIME_POLICY_PLUGIN_ID, nara_app::PluginCategory::Core);

const SLOT_COMPONENT_REGISTRY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.component-registry");
const SLOT_HIERARCHY: PluginSlotId = PluginSlotId::new("nara.plugins.slot.hierarchy");
const SLOT_SCENE_COMPONENTS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.scene-components");
const SLOT_DIAGNOSTICS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.diagnostics");
const SLOT_TASKS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tasks");
const SLOT_ASSET: PluginSlotId = PluginSlotId::new("nara.plugins.slot.asset");
const SLOT_TRANSFORM: PluginSlotId = PluginSlotId::new("nara.plugins.slot.transform");
const SLOT_INPUT: PluginSlotId = PluginSlotId::new("nara.plugins.slot.input");
const SLOT_GAMEPLAY_COMMANDS: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.gameplay-commands");
const SLOT_SERVER_TIME_POLICY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.server-time-policy");
#[cfg(feature = "runtime-2d")]
const SLOT_SPRITE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite");
#[cfg(feature = "runtime-2d")]
const SLOT_TILEMAP: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tilemap");
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
const SLOT_TILESET_ASSETS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tileset-assets");
#[cfg(any(
    feature = "runtime-2d",
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER_COMPONENTS: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-components");
#[cfg(any(
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render");
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
const SLOT_IMAGE_PREPARE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image-prepare");
#[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
const SLOT_IMAGE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image");
#[cfg(feature = "presentation-2d")]
const SLOT_SPRITE_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite-render");
#[cfg(feature = "runtime-ui")]
const SLOT_UI: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui");
#[cfg(feature = "presentation-ui")]
const SLOT_UI_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui-render");
#[cfg(feature = "presentation-ui")]
const SLOT_UI_PRESENTATION_BRIDGE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.ui-presentation-bridge");
#[cfg(feature = "desktop-winit")]
const SLOT_WINDOW: PluginSlotId = PluginSlotId::new("nara.plugins.slot.window");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render-wgpu");

/// Minimal runtime defaults for headless examples and code-first games.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.minimal");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_COMPONENT_REGISTRY,
                    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_reflect::ComponentRegistryPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_HIERARCHY, nara_hierarchy::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_hierarchy::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SCENE_COMPONENTS,
                    nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_scene::SceneComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_DIAGNOSTICS, nara_diagnostic::DIAGNOSTICS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_diagnostic::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_transform::TRANSFORM_PLUGIN_ID),
                PluginDefinition::for_default::<nara_transform::TransformPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_INPUT, nara_input::INPUT_PLUGIN_ID),
                PluginDefinition::for_default::<nara_input::InputPlugin>(),
            )
    }
}

/// Local headless defaults with input observations and semantic gameplay commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeadlessRuntimePlugins;

impl PluginGroup for HeadlessRuntimePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.headless-runtime");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(
                    SLOT_GAMEPLAY_COMMANDS,
                    nara_gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_gameplay::GameplayCommandPlugin>(),
            )
    }
}

/// Dedicated-server defaults without raw input, window, render, or tooling installation.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServerPlugins;

#[derive(Debug, Default, Clone, Copy)]
struct ServerTimePolicyPlugin;

impl Plugin for ServerTimePolicyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SERVER_TIME_POLICY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let Some(mut fixed_time) = app.world_mut()?.get_resource_mut::<nara_app::FixedTime>()
        else {
            return Err(PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: "server time policy requires FixedTime".to_owned(),
            });
        };
        fixed_time
            .set_catch_up_policy(nara_app::FixedCatchUpPolicy::PreserveDebt)
            .map_err(|error| PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: error.to_string(),
            })?;
        Ok(())
    }
}

impl PluginGroup for ServerPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.server");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(SLOT_SERVER_TIME_POLICY, SERVER_TIME_POLICY_PLUGIN_ID),
                PluginDefinition::for_default::<ServerTimePolicyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_COMPONENT_REGISTRY,
                    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_reflect::ComponentRegistryPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_HIERARCHY, nara_hierarchy::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_hierarchy::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SCENE_COMPONENTS,
                    nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_scene::SceneComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_DIAGNOSTICS, nara_diagnostic::DIAGNOSTICS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_diagnostic::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_transform::TRANSFORM_PLUGIN_ID),
                PluginDefinition::for_default::<nara_transform::TransformPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_GAMEPLAY_COMMANDS,
                    nara_gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_gameplay::GameplayCommandPlugin>(),
            )
    }
}

#[cfg(feature = "runtime-2d")]
#[derive(Debug, Default, Clone, Copy)]
pub struct Runtime2dPlugins;

#[cfg(feature = "runtime-2d")]
impl PluginGroup for Runtime2dPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.runtime-2d");

    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_SPRITE, nara_sprite::SPRITE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_sprite::SpritePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(SLOT_TILEMAP, nara_tilemap::TILEMAP_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tilemap::TilemapPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePlugin>(),
            );
        #[cfg(feature = "serde")]
        let builder = builder.add_slot(
            PluginSlot::optional(SLOT_TILESET_ASSETS, nara_tilemap::TILESET_ASSET_PLUGIN_ID),
            PluginDefinition::for_default::<nara_tilemap::TileSetAssetPlugin>(),
        );
        builder
    }
}

#[cfg(feature = "runtime-ui")]
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeUiPlugins;

#[cfg(feature = "runtime-ui")]
impl PluginGroup for RuntimeUiPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.runtime-ui");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(SLOT_UI, nara_ui::UI_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui::UiPlugin>(),
            )
    }
}

/// Backend-neutral UI packet preparation for products that present runtime UI.
#[cfg(feature = "presentation-ui")]
#[derive(Debug, Default, Clone, Copy)]
pub struct PresentationUiPlugins;

#[cfg(feature = "presentation-ui")]
impl PluginGroup for PresentationUiPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.presentation-ui");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE_PREPARE, nara_image::IMAGE_PREPARE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_UI_RENDER, nara_ui_render::UI_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui_render::UiRenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_UI_PRESENTATION_BRIDGE,
                    crate::ui_presentation::UI_PRESENTATION_BRIDGE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<crate::ui_presentation::UiPresentationBridgePlugin>(
                ),
            )
    }
}

/// Backend-neutral 2D packet production for products that present sprite and tilemap content.
#[cfg(feature = "presentation-2d")]
#[derive(Debug, Default, Clone, Copy)]
pub struct Presentation2dPlugins;

#[cfg(feature = "presentation-2d")]
impl PluginGroup for Presentation2dPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.presentation-2d");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE_PREPARE, nara_image::IMAGE_PREPARE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SPRITE_RENDER,
                    nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_sprite_render::SpriteRenderPlugin>(),
            )
    }
}

#[cfg(feature = "desktop-winit")]
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopWinitPlugins;

#[cfg(feature = "desktop-winit")]
impl PluginGroup for DesktopWinitPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.desktop-winit");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(SLOT_WINDOW, nara_window::WINDOW_PLUGIN_ID),
            PluginDefinition::for_default::<nara_window::WindowPlugin>(),
        )
    }
}

#[cfg(feature = "render-wgpu")]
#[derive(Debug, Default, Clone, Copy)]
pub struct WgpuBackendPlugins;

#[cfg(feature = "render-wgpu")]
impl PluginGroup for WgpuBackendPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.render-wgpu");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(SLOT_WGPU_RENDER, nara_render_wgpu::WGPU_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render_wgpu::WgpuRenderPlugin>(),
            )
    }
}
