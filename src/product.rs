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
use nara_project::{EffectiveProjectSettings, ProductCapabilitySet, RuntimePreset};
use nara_reflect::ComponentSchemaProviderDefinition;
use thiserror::Error;

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
/// [`PluginGroup`], so the same value can be used by direct `App` composition and by the
/// file-backed Project Host. Schema contributions install their provider authority before their
/// plugin builds in a direct `App`, while a file-backed Host preloads the same provider set into
/// its admitted registry snapshot.
#[derive(Clone, Default)]
pub struct ProductRecipe {
    entries: BTreeMap<PluginId, ProductRecipeEntry>,
}

impl fmt::Debug for ProductRecipe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductRecipe")
            .field("entries", &self.entries.values().collect::<Vec<_>>())
            .finish()
    }
}

impl ProductRecipe {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
}

impl PluginGroup for ProductRecipe {
    const ID: PluginGroupId = PRODUCT_RECIPE_GROUP_ID;

    fn build(self) -> PluginGroupBuilder {
        let schema_authority = self.schema_authority_definition();
        let builder = schema_authority.map_or_else(PluginGroupBuilder::new, |definition| {
            PluginGroupBuilder::new().add_definition(definition)
        });
        self.entries.into_values().fold(builder, |builder, entry| {
            builder.add_definition(entry.definition)
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductRecipeError {
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
#[cfg(any(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render");
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
const SLOT_IMAGE_PREPARE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image-prepare");
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
const SLOT_IMAGE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image");
#[cfg(feature = "runtime-2d")]
const SLOT_SPRITE_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite-render");
#[cfg(feature = "runtime-ui")]
const SLOT_UI: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui");
#[cfg(feature = "runtime-ui")]
const SLOT_UI_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui-render");
#[cfg(feature = "desktop-winit")]
const SLOT_WINDOW: PluginSlotId = PluginSlotId::new("nara.plugins.slot.window");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render-wgpu");
#[cfg(feature = "tooling")]
const SLOT_TOOLING: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tooling");

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
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::HierarchyPlugin>(),
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
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::HierarchyPlugin>(),
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
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(SLOT_SPRITE, nara_sprite::SPRITE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_sprite::SpritePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(SLOT_TILEMAP, nara_tilemap::TILEMAP_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tilemap::TilemapPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
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
                PluginSlot::required(
                    SLOT_SPRITE_RENDER,
                    nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_sprite_render::SpriteRenderPlugin>(),
            )
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
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
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
                PluginSlot::required(SLOT_UI, nara_ui::UI_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui::UiPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_UI_RENDER, nara_ui_render::UI_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui_render::UiRenderPlugin>(),
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
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_WGPU_RENDER, nara_render_wgpu::WGPU_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render_wgpu::WgpuRenderPlugin>(),
            )
    }
}

#[cfg(feature = "tooling")]
#[derive(Debug, Default, Clone, Copy)]
pub struct ToolingPlugins;

#[cfg(feature = "tooling")]
impl PluginGroup for ToolingPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.tooling");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(SLOT_TOOLING, nara_tooling::TOOLING_PLUGIN_ID),
            PluginDefinition::for_default::<nara_tooling::ToolingPlugin>(),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectProfilePlugins {
    preset: RuntimePreset,
    capabilities: ProductCapabilitySet,
    task_config: nara_tasks::TaskPoolConfig,
    primary_window: Option<nara_window::Window>,
}

impl ProjectProfilePlugins {
    pub(crate) fn new(
        settings: &EffectiveProjectSettings,
        capabilities: ProductCapabilitySet,
    ) -> Self {
        Self {
            preset: settings.runtime_preset,
            capabilities,
            task_config: settings.tasks.pool_config,
            primary_window: settings.window.to_window(),
        }
    }
}

impl PluginGroup for ProjectProfilePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime");

    fn build(self) -> PluginGroupBuilder {
        let _ = &self.primary_window;
        let task_plugin = nara_tasks::plugin(self.task_config);
        let product_group_includes_minimal = self
            .capabilities
            .contains(nara_project::ProductCapability::Runtime2d)
            || self
                .capabilities
                .contains(nara_project::ProductCapability::RuntimeUi);
        let builder = match self.preset {
            RuntimePreset::Minimal if product_group_includes_minimal => PluginGroupBuilder::new(),
            RuntimePreset::Minimal => PluginGroupBuilder::new()
                .add_edited_group(MinimalPlugins.edit().configure(task_plugin.clone())),
            RuntimePreset::LocalHeadless => PluginGroupBuilder::new()
                .add_edited_group(HeadlessRuntimePlugins.edit().configure(task_plugin.clone())),
            RuntimePreset::Server => PluginGroupBuilder::new()
                .add_edited_group(ServerPlugins.edit().configure(task_plugin.clone())),
        };

        #[cfg(feature = "runtime-2d")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::Runtime2d)
        {
            builder.add_edited_group(Runtime2dPlugins.edit().configure(task_plugin.clone()))
        } else {
            builder
        };

        #[cfg(feature = "runtime-ui")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::RuntimeUi)
        {
            builder.add_edited_group(RuntimeUiPlugins.edit().configure(task_plugin.clone()))
        } else {
            builder
        };

        #[cfg(feature = "tooling")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::Tooling)
        {
            builder.add_group(ToolingPlugins)
        } else {
            builder
        };

        #[cfg(feature = "desktop-winit")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::DesktopWinit)
        {
            builder.add_edited_group(
                DesktopWinitPlugins
                    .edit()
                    .configure(nara_window::plugin(self.primary_window)),
            )
        } else {
            builder
        };

        #[cfg(feature = "render-wgpu")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::RenderWgpu)
        {
            builder.add_group(WgpuBackendPlugins)
        } else {
            builder
        };

        builder
    }
}
