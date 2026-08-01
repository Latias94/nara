use std::{
    fs::{self, File},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use nara::{
    MinimalPlugins, ProductConfiguration, ProductRecipe, ProductRecipeEntryKind,
    ProductRecipeError, SchemaContribution,
    app::{
        AddPluginsError, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError,
        PluginHook, PluginHookMutation, PluginId, PluginInstantiationError,
        PluginProductCapability, PluginSchemaProviderId, RuntimeCandidateRetirementState,
    },
    fs::{FileCapability, TrustMode},
    hierarchy::HIERARCHY_PLUGIN_ID,
    project::ProductCapability,
    project_host::{
        CompositionError, ProjectRuntimePlugins, RuntimePlanError, built_in_schema_providers,
        ingest_project_manifest, project_runtime_plugins, resolve_product_recipe,
        resolve_runtime_plan,
    },
    reflect::{
        ComponentRegistry, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId,
    },
    scene::{SCENE_COMPONENTS_PLUGIN_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER_ID},
    transform::{TRANSFORM_SCHEMA_PROVIDER_ID, TransformPlugin},
};

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
use nara::app::PluginPlanError;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const RUNTIME_2D_REQUIREMENT: PluginProductCapability = PluginProductCapability::new("runtime-2d");
const UNKNOWN_PRODUCT_REQUIREMENT: PluginProductCapability =
    PluginProductCapability::new("test.unknown-product-capability");
const TEST_PRODUCT_PLUGIN_ID: PluginId = PluginId::new("test.product-capability");
const TEST_UNKNOWN_PRODUCT_PLUGIN_ID: PluginId = PluginId::new("test.unknown-product-plugin");
#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
const TEST_ORDERING_PLUGIN_ID: PluginId = PluginId::new("test.ordering-plugin");
#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
const TEST_AFTER_ORDERING_PLUGIN_ID: PluginId = PluginId::new("test.after-ordering-plugin");
const TEST_PRODUCT_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_PRODUCT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_product_capabilities(&[RUNTIME_2D_REQUIREMENT]);
const TEST_UNKNOWN_PRODUCT_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_UNKNOWN_PRODUCT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_product_capabilities(&[UNKNOWN_PRODUCT_REQUIREMENT]);
#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
const TEST_ORDERING_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_ORDERING_PLUGIN_ID, PluginCategory::Runtime);
#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
const TEST_AFTER_ORDERING_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_AFTER_ORDERING_PLUGIN_ID, PluginCategory::Runtime);

const TEST_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.schema-provider");
const TEST_SECOND_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.second-schema-provider");
const TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_ID: PluginId =
    PluginId::new("test.reentrant-project-request");
const TEST_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("test.schema-provider.components");
const TEST_SCHEMA_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("test.schema-provider.components");
const TEST_SCHEMA_PROVIDER_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.schema-provider.components.native", 1);
const TEST_SCHEMA_PROVIDER_SECOND_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.schema-provider.components.alternate", 1);
const TEST_COUNTED_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.counted-schema-provider");
const TEST_CONFIGURED_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.configured-schema-provider");
const TEST_CONTRIBUTION_OWNED_SCHEMA_PLUGIN_ID: PluginId =
    PluginId::new("test.contribution-owned-schema-provider");
const TEST_DIVERGENT_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.divergent-schema-provider");
const TEST_COUNTED_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("test.counted-schema-provider.components");
const TEST_COUNTED_SCHEMA_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("test.counted-schema-provider.components");
const TEST_COUNTED_SCHEMA_PROVIDER_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.counted-schema-provider.components.native", 1);
const TEST_COUNTED_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_COUNTED_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[TEST_COUNTED_SCHEMA_PROVIDER_ID]);
const TEST_CONFIGURED_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_CONFIGURED_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[TEST_COUNTED_SCHEMA_PROVIDER_ID]);
const TEST_CONTRIBUTION_OWNED_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(
        TEST_CONTRIBUTION_OWNED_SCHEMA_PLUGIN_ID,
        PluginCategory::Runtime,
    )
    .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
    .provides_schema(&[TEST_COUNTED_SCHEMA_PROVIDER_ID]);
const TEST_DIVERGENT_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_DIVERGENT_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[TEST_COUNTED_SCHEMA_PROVIDER_ID]);

const TEST_RECIPE_PLUGIN_ID: PluginId = PluginId::new("test.product-recipe.runtime");
const TEST_RECIPE_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_RECIPE_PLUGIN_ID, PluginCategory::Runtime);
const TEST_DEFAULT_RECIPE_PLUGIN_ID: PluginId =
    PluginId::new("test.product-recipe.default-runtime");
const TEST_DEFAULT_RECIPE_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_DEFAULT_RECIPE_PLUGIN_ID, PluginCategory::Runtime);

static COUNTED_PROVIDER_VALIDATIONS: AtomicUsize = AtomicUsize::new(0);
static COUNTED_PROVIDER_REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);
static COUNTED_PROVIDER_TEST_LOCK: Mutex<()> = Mutex::new(());

struct RecipeProbeConfiguration(u32);

impl ProductConfiguration for RecipeProbeConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.0.to_le_bytes());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, nara::ecs::Resource)]
struct RecipeProbe {
    value: u32,
    instance: usize,
}

struct RecipeProbePlugin {
    value: u32,
    instance: usize,
}

impl Plugin for RecipeProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_RECIPE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        app.world_mut()?.insert_resource(RecipeProbe {
            value: self.value,
            instance: self.instance,
        });
        Ok(())
    }
}

#[derive(Default)]
struct DefaultRecipePlugin;

impl Plugin for DefaultRecipePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_DEFAULT_RECIPE_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn empty_schema_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog::default())
}

fn claimed_schema_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![ComponentSchema::new(
            ComponentTypeId::new("test.shared.Component"),
            "Shared component",
            ComponentSchemaVersion::ONE,
        )],
        ..ComponentSchemaCatalog::default()
    })
}

fn tombstoned_schema_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        type_tombstones: vec![ComponentTypeId::new("test.shared.Component")],
        ..ComponentSchemaCatalog::default()
    })
}

fn rejected_schema_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Err(ComponentSchemaProviderSourceError::new(
        "test-schema-source-rejected",
    ))
}

fn panicking_schema_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    panic!("schema source panic before provider callback")
}

const TEST_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[TEST_SCHEMA_PROVIDER_ID]);
const TEST_SECOND_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_SECOND_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .provides_schema(&[TEST_SCHEMA_PROVIDER_ID]);
const TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_ID,
    PluginCategory::Runtime,
);

#[derive(Default)]
struct ProductCapabilityPlugin;

impl Plugin for ProductCapabilityPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_PRODUCT_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct UnknownProductCapabilityPlugin;

impl Plugin for UnknownProductCapabilityPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_UNKNOWN_PRODUCT_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[derive(Default)]
struct OrderingPlugin;

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
impl Plugin for OrderingPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_ORDERING_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[derive(Default)]
struct AfterOrderingPlugin;

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
impl Plugin for AfterOrderingPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_AFTER_ORDERING_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct SchemaProviderPlugin;

impl Plugin for SchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_SCHEMA_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct SecondSchemaProviderPlugin;

impl Plugin for SecondSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_SECOND_SCHEMA_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct CountedSchemaProviderPlugin;

impl Plugin for CountedSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_COUNTED_SCHEMA_PLUGIN_DECLARATION
    }

    fn preflight(
        &self,
        context: &nara::app::PluginPreflightContext<'_>,
    ) -> Result<(), PluginError> {
        let registry = nara::reflect::registry_for_plugin_preflight(
            context,
            TEST_COUNTED_SCHEMA_PLUGIN_ID,
            TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        counted_schema_provider()
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    TEST_COUNTED_SCHEMA_PLUGIN_ID,
                    TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        nara::reflect::register_schema_provider_for_plugin(
            app,
            TEST_COUNTED_SCHEMA_PLUGIN_ID,
            TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
            &counted_schema_provider(),
        )
    }
}

#[derive(Default)]
struct ContributionOwnedSchemaProviderPlugin;

impl Plugin for ContributionOwnedSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_CONTRIBUTION_OWNED_SCHEMA_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct DivergentSchemaProviderPlugin;

impl Plugin for DivergentSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_DIVERGENT_SCHEMA_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        nara::reflect::register_schema_provider_for_plugin(
            app,
            TEST_DIVERGENT_SCHEMA_PLUGIN_ID,
            TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
            &alternate_counted_schema_provider(),
        )
    }
}

struct ConfiguredSchemaProviderPlugin {
    value: u32,
}

impl Plugin for ConfiguredSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_CONFIGURED_SCHEMA_PLUGIN_DECLARATION
    }

    fn preflight(
        &self,
        context: &nara::app::PluginPreflightContext<'_>,
    ) -> Result<(), PluginError> {
        let registry = nara::reflect::registry_for_plugin_preflight(
            context,
            TEST_CONFIGURED_SCHEMA_PLUGIN_ID,
            TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        counted_schema_provider()
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    TEST_CONFIGURED_SCHEMA_PLUGIN_ID,
                    TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        nara::reflect::register_schema_provider_for_plugin(
            app,
            TEST_CONFIGURED_SCHEMA_PLUGIN_ID,
            TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
            &counted_schema_provider(),
        )?;
        app.world_mut()?.insert_resource(RecipeProbe {
            value: self.value,
            instance: 0,
        });
        Ok(())
    }
}

struct ReceiptValidatingSchemaProviderPlugin {
    provider: ComponentSchemaProviderDefinition,
}

impl Plugin for ReceiptValidatingSchemaProviderPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_SCHEMA_PLUGIN_DECLARATION
    }

    fn preflight(
        &self,
        context: &nara::app::PluginPreflightContext<'_>,
    ) -> Result<(), PluginError> {
        let registry = nara::reflect::registry_for_plugin_preflight(
            context,
            TEST_SCHEMA_PLUGIN_ID,
            TEST_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        self.provider.preflight(registry).map_err(|error| {
            PluginError::component_registration(
                TEST_SCHEMA_PLUGIN_ID,
                TEST_SCHEMA_PROVIDER_ID.as_str(),
                error,
            )
        })
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        nara::reflect::register_schema_provider_for_plugin(
            app,
            TEST_SCHEMA_PLUGIN_ID,
            TEST_SCHEMA_PROVIDER_ID.as_str(),
            &self.provider,
        )
    }
}

struct ReentrantProjectRequestPlugin(Mutex<Option<ProjectRuntimePlugins>>);

impl Plugin for ReentrantProjectRequestPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        let request = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .expect("the project request is consumed once");
        let Err(error) = app.add_plugins(request.into_app_plugins()) else {
            panic!("hook-time project composition must reject");
        };
        assert!(matches!(
            error,
            AddPluginsError::Plugin(PluginError::HookMutationForbidden {
                plugin: TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_ID,
                hook: PluginHook::Build,
                mutation: PluginHookMutation::PluginMembership,
            })
        ));
        Ok(())
    }
}

#[test]
fn product_recipe_configuration_replaces_typed_entry_and_reconstructs_fresh_plugins() {
    let instances = Arc::new(AtomicUsize::new(0));
    let first_instances = Arc::clone(&instances);
    let recipe = ProductRecipe::new()
        .add_configured_plugin(
            RecipeProbeConfiguration(7),
            move |configuration: &RecipeProbeConfiguration| RecipeProbePlugin {
                value: configuration.0,
                instance: first_instances.fetch_add(1, Ordering::SeqCst),
            },
        )
        .unwrap();
    let first_fingerprint = recipe.entries().next().unwrap().configuration_fingerprint();
    let replacement_instances = Arc::clone(&instances);
    let recipe = recipe
        .configure_plugin(
            RecipeProbeConfiguration(11),
            move |configuration: &RecipeProbeConfiguration| RecipeProbePlugin {
                value: configuration.0,
                instance: replacement_instances.fetch_add(1, Ordering::SeqCst),
            },
        )
        .unwrap();
    let entry = recipe.entries().next().unwrap();
    assert_eq!(recipe.len(), 1);
    assert_eq!(entry.plugin_id(), TEST_RECIPE_PLUGIN_ID);
    assert_eq!(entry.kind(), ProductRecipeEntryKind::RuntimePlugin);
    assert_ne!(entry.configuration_fingerprint(), first_fingerprint);

    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let first = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    let second = resolve_product_recipe(&candidate, recipe).unwrap();
    let independently_reconstructed_instances = Arc::new(AtomicUsize::new(0));
    let independently_reconstructed_counter = Arc::clone(&independently_reconstructed_instances);
    let independently_reconstructed = ProductRecipe::new()
        .add_configured_plugin(
            RecipeProbeConfiguration(11),
            move |configuration: &RecipeProbeConfiguration| RecipeProbePlugin {
                value: configuration.0,
                instance: independently_reconstructed_counter.fetch_add(1, Ordering::SeqCst),
            },
        )
        .unwrap();
    let independently_reconstructed =
        resolve_product_recipe(&candidate, independently_reconstructed).unwrap();
    let first_key = first
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == TEST_RECIPE_PLUGIN_ID)
        .unwrap()
        .definition_key();
    let second_key = second
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == TEST_RECIPE_PLUGIN_ID)
        .unwrap()
        .definition_key();
    let independently_reconstructed_key = independently_reconstructed
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == TEST_RECIPE_PLUGIN_ID)
        .unwrap()
        .definition_key();
    assert_eq!(first_key, second_key);
    assert_eq!(first_key, independently_reconstructed_key);

    let first_app = first.plugin_plan().instantiate().unwrap();
    let second_app = second.plugin_plan().instantiate().unwrap();
    assert_eq!(
        first_app.world().resource::<RecipeProbe>(),
        &RecipeProbe {
            value: 11,
            instance: 0,
        }
    );
    assert_eq!(
        second_app.world().resource::<RecipeProbe>(),
        &RecipeProbe {
            value: 11,
            instance: 1,
        }
    );
}

#[test]
fn schema_contribution_binds_provider_once_with_direct_and_file_backed_parity() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let contribution = SchemaContribution::<ContributionOwnedSchemaProviderPlugin>::for_default([
        counted_schema_provider(),
    ])
    .unwrap();
    let recipe = ProductRecipe::new().add_contribution(contribution).unwrap();
    let recipe_entry = recipe.entries().next().unwrap();
    assert_eq!(
        recipe_entry.kind(),
        ProductRecipeEntryKind::SchemaContribution
    );
    assert_eq!(
        recipe_entry.schema_provider_ids(),
        &[TEST_COUNTED_SCHEMA_PROVIDER_ID]
    );

    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let file_plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    let file_app = file_plan.plugin_plan().instantiate().unwrap();
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    let file_registry = nara::reflect::component_registry(file_app.world()).unwrap();
    let file_fingerprint = file_registry
        .snapshot()
        .unwrap()
        .schema_composition_fingerprint()
        .unwrap();

    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let mut direct_app = nara::app::App::new();
    direct_app
        .add_plugins((MinimalPlugins, recipe.clone()))
        .unwrap();
    let direct_app = direct_app.seal().unwrap();
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    let direct_registry = nara::reflect::component_registry(direct_app.world()).unwrap();
    assert_eq!(
        direct_registry
            .snapshot()
            .unwrap()
            .schema_composition_fingerprint()
            .unwrap(),
        file_fingerprint
    );
    assert_eq!(
        file_plan.schema_validation().composition_fingerprint(),
        file_fingerprint
    );
    let file_entry = file_plan
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == TEST_CONTRIBUTION_OWNED_SCHEMA_PLUGIN_ID)
        .unwrap();
    assert_eq!(
        file_entry
            .definition_key()
            .unwrap()
            .configuration()
            .as_bytes(),
        recipe_entry.configuration_fingerprint()
    );
}

#[test]
fn schema_contribution_rejects_divergent_plugin_receipts_in_direct_and_file_backed_apps() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let recipe = ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<DivergentSchemaProviderPlugin>::for_default([
                counted_schema_provider(),
            ])
            .unwrap(),
        )
        .unwrap();

    let mut direct_app = nara::app::App::new();
    let Err(direct_error) = direct_app.add_plugins((MinimalPlugins, recipe.clone())) else {
        panic!("direct recipe composition must reject a divergent provider receipt");
    };
    assert!(matches!(
        direct_error,
        AddPluginsError::Plugin(PluginError::ComponentRegistrationFailed {
            plugin: TEST_DIVERGENT_SCHEMA_PLUGIN_ID,
            ..
        })
    ));
    assert!(
        direct_error
            .to_string()
            .contains("different executable behavior receipt")
    );

    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let file_plan = resolve_product_recipe(&candidate, recipe).unwrap();
    let mut file_failure = file_plan.plugin_plan().instantiate_retained().unwrap_err();
    let file_error = file_failure.error().clone();
    while file_failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
        file_failure.drive_retirement();
    }
    assert!(matches!(
        file_error,
        PluginInstantiationError::Plugin(PluginError::ComponentRegistrationFailed {
            plugin: TEST_DIVERGENT_SCHEMA_PLUGIN_ID,
            ..
        })
    ));
    assert!(
        file_error
            .to_string()
            .contains("different executable behavior receipt")
    );
}

#[test]
fn schema_contribution_configuration_replaces_the_replayable_plugin_entry() {
    let contribution = SchemaContribution::<ConfiguredSchemaProviderPlugin>::configured(
        RecipeProbeConfiguration(3),
        |configuration: &RecipeProbeConfiguration| ConfiguredSchemaProviderPlugin {
            value: configuration.0,
        },
        [counted_schema_provider()],
    )
    .unwrap();
    let recipe = ProductRecipe::new().add_contribution(contribution).unwrap();
    let first_fingerprint = recipe.entries().next().unwrap().configuration_fingerprint();
    let replacement = SchemaContribution::<ConfiguredSchemaProviderPlugin>::configured(
        RecipeProbeConfiguration(9),
        |configuration: &RecipeProbeConfiguration| ConfiguredSchemaProviderPlugin {
            value: configuration.0,
        },
        [counted_schema_provider()],
    )
    .unwrap();
    let recipe = recipe.configure_contribution(replacement).unwrap();
    let entry = recipe.entries().next().unwrap();
    assert_eq!(entry.kind(), ProductRecipeEntryKind::SchemaContribution);
    assert_ne!(entry.configuration_fingerprint(), first_fingerprint);

    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe).unwrap();
    let app = plan.plugin_plan().instantiate().unwrap();
    assert_eq!(
        app.world().resource::<RecipeProbe>(),
        &RecipeProbe {
            value: 9,
            instance: 0,
        }
    );
}

#[test]
fn product_recipe_rejects_duplicate_plugins_and_mismatched_provider_declarations() {
    let error = ProductRecipe::new()
        .add_plugin::<DefaultRecipePlugin>()
        .unwrap()
        .add_plugin::<DefaultRecipePlugin>()
        .unwrap_err();
    assert_eq!(
        error,
        ProductRecipeError::DuplicatePlugin {
            plugin: TEST_DEFAULT_RECIPE_PLUGIN_ID,
        }
    );

    let error = SchemaContribution::<CountedSchemaProviderPlugin>::for_default([
        nara::transform::TRANSFORM_SCHEMA_PROVIDER,
    ])
    .unwrap_err();
    assert!(matches!(
        error,
        ProductRecipeError::SchemaProviderMismatch {
            plugin: TEST_COUNTED_SCHEMA_PLUGIN_ID,
            ..
        }
    ));
}

#[test]
fn project_runtime_plan_is_pure_repeatable_and_schema_bound() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    let first = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
    let mut reversed_providers = built_in_schema_providers();
    reversed_providers.reverse();
    let repeated = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        reversed_providers,
    )
    .unwrap();

    assert_eq!(first.lineage(), candidate.lineage());
    assert_eq!(first.lineage(), repeated.lineage());
    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert_eq!(
        first.schema_validation().composition_fingerprint(),
        repeated.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        first.schema_validation().provider_ids(),
        repeated.schema_validation().provider_ids()
    );
    assert_eq!(
        first
            .schema_validation()
            .contribution_receipts()
            .collect::<Vec<_>>(),
        repeated
            .schema_validation()
            .contribution_receipts()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        first.schema_validation().executable_fingerprint(),
        repeated.schema_validation().executable_fingerprint()
    );
    assert!(
        !first
            .schema_validation()
            .snapshot()
            .ptr_eq(repeated.schema_validation().snapshot()),
        "independently resolved plans retain distinct snapshot authority"
    );
    assert!(
        first
            .required_capabilities()
            .contains(ProductCapability::RuntimeCore)
    );
    assert_eq!(
        first.schema_validation().provider_ids(),
        [
            SCENE_COMPONENTS_SCHEMA_PROVIDER_ID,
            TRANSFORM_SCHEMA_PROVIDER_ID
        ]
    );
    let plugin_entries = first.plugin_plan().entries();
    assert!(
        plugin_entries
            .iter()
            .any(|entry| entry.plugin_id() == HIERARCHY_PLUGIN_ID)
    );
    assert!(
        plugin_entries
            .iter()
            .any(|entry| entry.plugin_id() == SCENE_COMPONENTS_PLUGIN_ID)
    );

    let app = first.plugin_plan().instantiate().unwrap();
    let runtime_registry = nara::reflect::component_registry(app.world()).unwrap();
    let runtime_fingerprint = runtime_registry
        .snapshot()
        .unwrap()
        .schema_composition_fingerprint()
        .unwrap();
    assert_eq!(
        runtime_fingerprint,
        first.schema_validation().composition_fingerprint()
    );
    assert!(
        runtime_registry.shares_snapshot(first.schema_validation().snapshot()),
        "the plan and instantiated World must share one executable behavior snapshot"
    );

    let mut raw_app = nara::app::App::new();
    raw_app
        .add_plugins(project_runtime_plugins(&candidate).into_app_plugins())
        .unwrap();
    assert_ne!(
        raw_app.configuration_fingerprint(),
        first.plugin_plan().fingerprint(),
        "the code-first registry recipe must not impersonate a snapshot-bound runtime plan"
    );
    let raw_app = raw_app.seal().unwrap();
    let raw_snapshot = nara::reflect::component_registry(raw_app.world())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        raw_snapshot.schema_composition_fingerprint().unwrap(),
        first.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        raw_snapshot.executable_registry_fingerprint().unwrap(),
        first.schema_validation().executable_fingerprint()
    );
    assert_eq!(
        raw_snapshot.contribution_receipts().collect::<Vec<_>>(),
        first
            .schema_validation()
            .contribution_receipts()
            .collect::<Vec<_>>()
    );
    assert!(
        !raw_snapshot.ptr_eq(first.schema_validation().snapshot()),
        "equivalent direct and file-backed registries retain distinct authority"
    );
}

#[cfg(feature = "runtime-2d")]
#[test]
fn omitted_known_owner_component_rejects_before_world_mutation() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
    assert!(
        !plan
            .schema_validation()
            .provider_ids()
            .contains(&nara::tilemap::TILEMAP_SCHEMA_PROVIDER_ID)
    );

    let document = nara::scene::SceneDocument::new([nara::scene::SceneEntityRecord::new(
        nara::identity::SceneEntityId::new("omitted-owner").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.tilemap.Tilemap"),
        nara::scene::SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            nara::reflect::ComponentValue::Null,
        ),
    )]);
    let mut world = nara::ecs::World::new();
    world.spawn_empty();
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();

    let report =
        nara::scene::spawn_scene(&mut world, plan.schema_validation().registry(), &document);

    assert!(report.instance.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.unknown-component"
            || diagnostic.code().as_str() == "scene.unsupported-component-version"
    }));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        baseline,
    );
}

#[test]
fn file_backed_schema_provider_callbacks_run_once_in_the_private_candidate() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
        PluginDefinition::for_default::<CountedSchemaProviderPlugin>(),
    );
    let mut providers = built_in_schema_providers();
    providers.push(counted_schema_provider());
    providers.push(counted_schema_provider());
    let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(
        plan.schema_validation()
            .provider_receipts()
            .iter()
            .copied()
            .map(|receipt| receipt.provider())
            .collect::<Vec<_>>(),
        plan.schema_validation().provider_ids()
    );

    let app = plan.plugin_plan().instantiate().unwrap();
    let registry = nara::reflect::component_registry(app.world()).unwrap();
    assert!(registry.shares_snapshot(plan.schema_validation().snapshot()));
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
}

#[test]
fn distinct_definitions_for_one_owner_reject_before_selected_callbacks() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    const ALTERNATE_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.counted-schema-provider.alternate");
    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
        PluginDefinition::for_default::<CountedSchemaProviderPlugin>(),
    );
    let mut providers = built_in_schema_providers();
    providers.extend([
        counted_schema_provider(),
        ComponentSchemaProviderDefinition::new(
            TEST_COUNTED_SCHEMA_OWNER_ID,
            ALTERNATE_PROVIDER,
            ComponentSchemaProviderBindingId::new(
                "test.counted-schema-provider.alternate.native",
                1,
            ),
            empty_schema_catalog_source,
            panic_schema_provider,
        ),
    ]);

    assert_eq!(
        resolve_runtime_plan(&candidate, request, providers).unwrap_err(),
        RuntimePlanError::Composition(CompositionError::DivergentSchemaOwner {
            owner: TEST_COUNTED_SCHEMA_OWNER_ID,
        }),
    );
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 0);
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 0);
}

#[test]
fn schema_source_failure_rejects_before_any_selected_callback() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    const REJECTED_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.rejected-source-provider");
    const PANICKED_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.panicked-source-provider");
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    for (provider_id, source, expected_panic) in [
        (
            REJECTED_PROVIDER,
            rejected_schema_catalog_source as nara::reflect::ComponentSchemaProviderSource,
            false,
        ),
        (
            PANICKED_PROVIDER,
            panicking_schema_catalog_source as nara::reflect::ComponentSchemaProviderSource,
            true,
        ),
    ] {
        COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
        COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
        let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::for_default::<CountedSchemaProviderPlugin>(),
        );
        let mut providers = built_in_schema_providers();
        providers.extend([
            counted_schema_provider(),
            ComponentSchemaProviderDefinition::new(
                ComponentSchemaOwnerId::new(provider_id.as_str()),
                provider_id,
                ComponentSchemaProviderBindingId::new(provider_id.as_str(), 1),
                source,
                panic_schema_provider,
            ),
        ]);

        let error = resolve_runtime_plan(&candidate, request, providers).unwrap_err();
        assert!(matches!(
            error,
            RuntimePlanError::Composition(CompositionError::SchemaProviderRejected {
                provider,
                source,
            }) if provider == provider_id
                && matches!(
                    (source.as_ref(), expected_panic),
                    (ComponentRegistryError::SchemaProviderSourcePanicked { .. }, true)
                        | (ComponentRegistryError::SchemaProviderSourceRejected { .. }, false)
                )
        ));
        assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 0);
        assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn code_first_schema_provider_builds_and_freezes_once_without_a_host() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let mut app = nara::app::App::new();
    app.add_plugins((
        nara::reflect::ComponentRegistryPlugin,
        CountedSchemaProviderPlugin,
    ))
    .unwrap();
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);

    let app = app.seal().unwrap();
    let registry = nara::reflect::component_registry(app.world()).unwrap();
    assert!(registry.is_frozen());
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    nara::reflect::validate_component_registry_authority(app.world()).unwrap();
}

#[test]
fn validation_rejection_and_panic_have_direct_and_file_backed_parity() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    for (validation, panics, configuration) in [
        (
            reject_counted_schema_provider_validation
                as fn(&ComponentRegistry) -> Result<(), ComponentRegistryError>,
            false,
            b"reject".as_slice(),
        ),
        (
            panic_counted_schema_provider_validation
                as fn(&ComponentRegistry) -> Result<(), ComponentRegistryError>,
            true,
            b"panic".as_slice(),
        ),
    ] {
        COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
        COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
        let provider = ComponentSchemaProviderDefinition::with_validation(
            TEST_SCHEMA_OWNER_ID,
            TEST_SCHEMA_PROVIDER_ID,
            TEST_SCHEMA_PROVIDER_BINDING_ID,
            empty_schema_catalog_source,
            validation,
            counted_schema_provider_registration,
        );
        let direct_plugin = ReceiptValidatingSchemaProviderPlugin { provider };
        let mut app = nara::app::App::new();
        let Err(direct_error) =
            app.add_plugins((nara::reflect::ComponentRegistryPlugin, direct_plugin))
        else {
            panic!("rejecting schema validation unexpectedly admitted the direct App");
        };
        assert!(matches!(
            (direct_error, panics),
            (
                AddPluginsError::Plugin(PluginError::ComponentRegistrationFailed {
                    plugin: TEST_SCHEMA_PLUGIN_ID,
                    ..
                }),
                false,
            ) | (
                AddPluginsError::Plugin(PluginError::HookPanicked {
                    plugin: TEST_SCHEMA_PLUGIN_ID,
                    hook: PluginHook::Build,
                }),
                true,
            )
        ));
        assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
        assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 0);

        COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
        let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::infallible::<ReceiptValidatingSchemaProviderPlugin, _>(
                nara::app::PluginDefinitionId::new("test.schema-provider.validation-parity", 1),
                configuration,
                move || ReceiptValidatingSchemaProviderPlugin { provider },
            ),
        );
        let mut providers = built_in_schema_providers();
        providers.push(provider);
        let file_error = resolve_runtime_plan(&candidate, request, providers).unwrap_err();
        assert!(matches!(
            (file_error, panics),
            (
                RuntimePlanError::Composition(CompositionError::SchemaProviderRejected {
                    provider: TEST_SCHEMA_PROVIDER_ID,
                    ..
                }),
                false,
            ) | (
                RuntimePlanError::Composition(CompositionError::SchemaProviderPanicked {
                    provider: TEST_SCHEMA_PROVIDER_ID,
                }),
                true,
            )
        ));
        assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 1);
        assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn file_backed_candidate_rejects_divergent_binding_codec_and_migration_receipts() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let admitted = ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        register_empty_schema_provider,
    );

    for (configuration, divergent) in [
        (
            b"binding".as_slice(),
            TEST_SCHEMA_PROVIDER_SECOND_BINDING_ID,
        ),
        (
            b"codec".as_slice(),
            TEST_SCHEMA_PROVIDER_BINDING_ID.with_codec_version(2),
        ),
        (
            b"migration".as_slice(),
            TEST_SCHEMA_PROVIDER_BINDING_ID.with_migration_version(2),
        ),
    ] {
        let candidate_provider = ComponentSchemaProviderDefinition::new(
            TEST_SCHEMA_OWNER_ID,
            TEST_SCHEMA_PROVIDER_ID,
            divergent,
            empty_schema_catalog_source,
            panic_schema_provider,
        );
        let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::infallible::<ReceiptValidatingSchemaProviderPlugin, _>(
                nara::app::PluginDefinitionId::new("test.schema-provider.receipt-check", 1),
                configuration,
                move || ReceiptValidatingSchemaProviderPlugin {
                    provider: candidate_provider,
                },
            ),
        );
        let mut providers = built_in_schema_providers();
        providers.push(admitted);
        let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();
        assert_eq!(
            plan.schema_validation()
                .provider_receipts()
                .iter()
                .copied()
                .find(|receipt| receipt.provider() == TEST_SCHEMA_PROVIDER_ID)
                .unwrap()
                .binding(),
            TEST_SCHEMA_PROVIDER_BINDING_ID
        );

        let mut failure = plan.plugin_plan().instantiate_retained().unwrap_err();
        let error = failure.error().clone();
        while failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
            failure.drive_retirement();
        }
        let PluginInstantiationError::Plugin(error) = error else {
            panic!("receipt mismatch failed in the wrong phase: {error:?}");
        };
        assert!(plugin_error_contains_receipt_rejection(&error));
    }
}

#[test]
fn project_request_app_input_preserves_hook_time_sticky_poison() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let mut app = nara::app::App::new();

    let Err(error) = app.add_plugins(ReentrantProjectRequestPlugin(Mutex::new(Some(
        project_runtime_plugins(&candidate),
    )))) else {
        panic!("the hook-time request must poison the App");
    };

    assert!(matches!(
        error,
        AddPluginsError::Plugin(PluginError::HookMutationForbidden {
            plugin: TEST_REENTRANT_PROJECT_REQUEST_PLUGIN_ID,
            hook: PluginHook::Build,
            mutation: PluginHookMutation::PluginMembership,
        })
    ));
}

#[test]
fn compiled_but_unrequested_plugin_capability_rejects_in_composition_phase() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    let error = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::for_default::<ProductCapabilityPlugin>(),
        ),
        built_in_schema_providers(),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::Composition(CompositionError::UnrequestedProductCapability {
            plugin: TEST_PRODUCT_PLUGIN_ID,
            capability: ProductCapability::Runtime2d,
        })
    ));

    resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
}

#[test]
fn unknown_plugin_product_capability_rejects_in_composition_phase() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    let error = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::for_default::<UnknownProductCapabilityPlugin>(),
        ),
        built_in_schema_providers(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        RuntimePlanError::Composition(CompositionError::UnknownProductCapability {
            plugin: TEST_UNKNOWN_PRODUCT_PLUGIN_ID,
            capability: UNKNOWN_PRODUCT_REQUIREMENT,
        })
    );
}

#[test]
fn missing_schema_provider_rejects_before_a_plan_is_published_and_can_be_corrected() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plugins = || {
        project_runtime_plugins(&candidate)
            .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>())
    };

    let error =
        resolve_runtime_plan(&candidate, plugins(), built_in_schema_providers()).unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::Composition(CompositionError::MissingSchemaProvider {
            provider: TEST_SCHEMA_PROVIDER_ID,
        })
    ));

    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        register_empty_schema_provider,
    ));
    let corrected = resolve_runtime_plan(&candidate, plugins(), providers).unwrap();
    assert!(
        corrected
            .schema_validation()
            .provider_ids()
            .contains(&TEST_SCHEMA_PROVIDER_ID)
    );
}

#[test]
fn runtime_request_rejects_a_different_project_lineage() {
    let first = TestManifest::new(MINIMAL_MANIFEST);
    let changed = TestManifest::new(CHANGED_MANIFEST);
    let first = ingest_project_manifest(&first.capability(), None).unwrap();
    let changed = ingest_project_manifest(&changed.capability(), None).unwrap();

    let error = resolve_runtime_plan(
        &changed,
        project_runtime_plugins(&first),
        built_in_schema_providers(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        RuntimePlanError::Composition(CompositionError::ProjectLineageMismatch)
    );
}

#[test]
fn manifest_and_profile_identity_participate_in_opaque_lineage() {
    let first = TestManifest::new(MINIMAL_MANIFEST);
    let repeated = TestManifest::new(MINIMAL_MANIFEST);
    let changed = TestManifest::new(CHANGED_MANIFEST);

    let first = ingest_project_manifest(&first.capability(), None).unwrap();
    let repeated = ingest_project_manifest(&repeated.capability(), None).unwrap();
    let changed = ingest_project_manifest(&changed.capability(), None).unwrap();

    assert_eq!(first.lineage(), repeated.lineage());
    assert_ne!(first.lineage(), changed.lineage());

    let profiled = TestManifest::new(PROFILE_MANIFEST);
    let base = ingest_project_manifest(&profiled.capability(), None).unwrap();
    let dev = ingest_project_manifest(&profiled.capability(), Some("dev")).unwrap();
    let repeated_dev = ingest_project_manifest(&profiled.capability(), Some("dev")).unwrap();
    let release = ingest_project_manifest(&profiled.capability(), Some("release")).unwrap();
    assert_ne!(base.lineage(), dev.lineage());
    assert_eq!(dev.lineage(), repeated_dev.lineage());
    assert_ne!(dev.lineage(), release.lineage());
}

#[test]
fn divergent_schema_provider_bindings_reject_even_when_not_selected() {
    const PROVIDER_ID: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.divergent-schema-provider");
    const OWNER_ID: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("test.divergent-schema-owner");
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let mut providers = built_in_schema_providers();
    providers.extend([
        ComponentSchemaProviderDefinition::new(
            OWNER_ID,
            PROVIDER_ID,
            TEST_SCHEMA_PROVIDER_BINDING_ID,
            empty_schema_catalog_source,
            register_empty_schema_provider,
        ),
        ComponentSchemaProviderDefinition::new(
            OWNER_ID,
            PROVIDER_ID,
            TEST_SCHEMA_PROVIDER_SECOND_BINDING_ID,
            empty_schema_catalog_source,
            register_second_empty_schema_provider,
        ),
    ]);

    let error = resolve_runtime_plan(&candidate, project_runtime_plugins(&candidate), providers)
        .unwrap_err();

    assert_eq!(
        error,
        RuntimePlanError::Composition(CompositionError::DivergentSchemaProvider {
            provider: PROVIDER_ID,
        })
    );
}

#[test]
fn known_inactive_schema_owners_reserve_their_type_claims_without_running_callbacks() {
    const FIRST_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.inactive-owner-a.provider");
    const SECOND_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.inactive-owner-b.provider");
    const FIRST_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("test.inactive-owner-a");
    const SECOND_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("test.inactive-owner-b");
    const FIRST_BINDING: ComponentSchemaProviderBindingId =
        ComponentSchemaProviderBindingId::new("test.inactive-owner-a.native", 1);
    const SECOND_BINDING: ComponentSchemaProviderBindingId =
        ComponentSchemaProviderBindingId::new("test.inactive-owner-b.native", 1);

    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let mut providers = built_in_schema_providers();
    providers.extend([
        ComponentSchemaProviderDefinition::new(
            FIRST_OWNER,
            FIRST_PROVIDER,
            FIRST_BINDING,
            claimed_schema_catalog_source,
            panic_schema_provider,
        ),
        ComponentSchemaProviderDefinition::new(
            SECOND_OWNER,
            SECOND_PROVIDER,
            SECOND_BINDING,
            claimed_schema_catalog_source,
            panic_schema_provider,
        ),
    ]);

    assert!(matches!(
        resolve_runtime_plan(&candidate, project_runtime_plugins(&candidate), providers),
        Err(RuntimePlanError::Composition(
            CompositionError::ConflictingSchemaOwnerClaim {
                component_id,
                first_owner,
                second_owner,
            }
        )) if component_id == ComponentTypeId::new("test.shared.Component")
            && [first_owner, second_owner].contains(&FIRST_OWNER)
            && [first_owner, second_owner].contains(&SECOND_OWNER)
    ));
}

#[test]
fn known_inactive_owner_tombstones_reserve_type_claims() {
    const ACTIVE_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.inactive-active-owner.provider");
    const TOMBSTONE_PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("test.inactive-tombstone-owner.provider");
    const ACTIVE_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("test.inactive-active-owner");
    const TOMBSTONE_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("test.inactive-tombstone-owner");

    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let mut providers = built_in_schema_providers();
    providers.extend([
        ComponentSchemaProviderDefinition::new(
            ACTIVE_OWNER,
            ACTIVE_PROVIDER,
            ComponentSchemaProviderBindingId::new("test.inactive-active-owner.native", 1),
            claimed_schema_catalog_source,
            panic_schema_provider,
        ),
        ComponentSchemaProviderDefinition::new(
            TOMBSTONE_OWNER,
            TOMBSTONE_PROVIDER,
            ComponentSchemaProviderBindingId::new("test.inactive-tombstone-owner.native", 1),
            tombstoned_schema_catalog_source,
            panic_schema_provider,
        ),
    ]);

    assert!(matches!(
        resolve_runtime_plan(&candidate, project_runtime_plugins(&candidate), providers),
        Err(RuntimePlanError::Composition(
            CompositionError::ConflictingSchemaOwnerClaim { component_id, .. }
        )) if component_id == ComponentTypeId::new("test.shared.Component")
    ));
}

#[test]
fn multiple_plugins_may_select_one_identical_schema_provider() {
    let _guard = COUNTED_PROVIDER_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>())
        .insert_after::<SchemaProviderPlugin>(PluginDefinition::for_default::<
            SecondSchemaProviderPlugin,
        >());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        counted_schema_provider_registration,
    ));

    let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();

    assert!(
        plan.schema_validation()
            .provider_ids()
            .contains(&TEST_SCHEMA_PROVIDER_ID)
    );
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
}

#[test]
fn schema_provider_callback_rejection_preserves_the_composition_phase() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        reject_schema_provider,
    ));

    assert_eq!(
        resolve_runtime_plan(&candidate, request, providers).unwrap_err(),
        RuntimePlanError::Composition(CompositionError::SchemaProviderRejected {
            provider: TEST_SCHEMA_PROVIDER_ID,
            source: Box::new(ComponentRegistryError::Frozen),
        })
    );
}

#[test]
fn schema_provider_panic_is_typed_and_a_corrected_binding_can_retry() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = || {
        project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
            PluginDefinition::for_default::<SchemaProviderPlugin>(),
        )
    };
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        panic_schema_provider,
    ));

    assert_eq!(
        resolve_runtime_plan(&candidate, request(), providers).unwrap_err(),
        RuntimePlanError::Composition(CompositionError::SchemaProviderPanicked {
            provider: TEST_SCHEMA_PROVIDER_ID,
        })
    );

    let mut corrected = built_in_schema_providers();
    corrected.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        register_empty_schema_provider,
    ));
    resolve_runtime_plan(&candidate, request(), corrected).unwrap();
}

#[test]
fn owner_candidate_freeze_failure_does_not_publish_a_runtime_plan() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_OWNER_ID,
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        register_unbound_component_schema,
    ));

    let error = resolve_runtime_plan(&candidate, request, providers).unwrap_err();
    let RuntimePlanError::Composition(CompositionError::SchemaProviderRejected {
        provider,
        source,
    }) = error
    else {
        panic!("unbound schema must fail during owner-candidate freeze");
    };
    assert_eq!(provider, TEST_SCHEMA_PROVIDER_ID);
    assert!(matches!(
        *source,
        ComponentRegistryError::MissingNativeBinding { .. }
    ));
}

#[test]
fn lineage_and_runtime_plan_debug_output_do_not_disclose_manifest_values() {
    const CANARY: &str = "credential-canary-value";
    let source = format!(
        r#"
schema_version = 1

[project]
name = "{CANARY}"

[runtime]
preset = "minimal"

[capabilities]
requested = []
"#
    );
    let manifest = TestManifest::new(&source);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();

    for debug in [
        format!("{candidate:?}"),
        format!("{:?}", candidate.lineage()),
        format!("{plan:?}"),
    ] {
        assert!(!debug.contains(CANARY));
    }
}

#[cfg(feature = "desktop-winit")]
#[test]
fn plugin_debug_output_does_not_disclose_manifest_window_title() {
    const CANARY: &str = "window-title-credential-canary";
    let source = format!(
        r#"
schema_version = 1

[project]
name = "Window Debug Privacy"

[runtime]
preset = "minimal"

[capabilities]
requested = ["desktop-winit"]

[window]
enabled = true
title = "{CANARY}"
width = 1024
height = 576
"#
    );
    let manifest = TestManifest::new(&source);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let definition = nara::window::plugin(candidate.settings().window.to_window());
    let request = project_runtime_plugins(&candidate).configure(definition.clone());

    let definition_debug = format!("{definition:?}");
    let request_debug = format!("{request:?}");
    let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();

    for debug in [definition_debug, request_debug, format!("{plan:?}")] {
        assert!(!debug.contains(CANARY));
    }
}

#[cfg(feature = "desktop-winit")]
#[test]
fn desktop_project_configures_the_existing_window_plugin_slot() {
    use nara::window::{WINDOW_PLUGIN_ID, Window};

    let manifest = TestManifest::new(DESKTOP_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();

    assert!(
        plan.required_capabilities()
            .contains(ProductCapability::DesktopWinit)
    );
    let window_entry = plan
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == WINDOW_PLUGIN_ID)
        .unwrap();
    assert_eq!(
        window_entry.definition_key(),
        nara::window::plugin(candidate.settings().window.to_window()).key()
    );

    let app = plan.plugin_plan().instantiate().unwrap();
    let windows = app
        .world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Window>())
        .collect::<Vec<_>>();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].title, "Configured Nara Window");
    assert_eq!(windows[0].resolution.physical_width, 1024);
    assert_eq!(windows[0].resolution.physical_height, 576);
}

#[cfg(all(
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[test]
fn requested_product_profile_selects_ui_tooling_window_and_wgpu_plugins() {
    use nara::{
        render::RENDER_SCHEMA_PROVIDER_ID,
        render_wgpu::WGPU_RENDER_PLUGIN_ID,
        tooling::TOOLING_PLUGIN_ID,
        ui::{UI_PLUGIN_ID, UI_SCHEMA_PROVIDER_ID},
        window::WINDOW_PLUGIN_ID,
    };

    let manifest = TestManifest::new(FULL_PRODUCT_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();

    for capability in [
        ProductCapability::RuntimeUi,
        ProductCapability::Tooling,
        ProductCapability::DesktopWinit,
        ProductCapability::RenderWgpu,
    ] {
        assert!(plan.required_capabilities().contains(capability));
    }
    let entries = plan.plugin_plan().entries();
    for plugin in [
        UI_PLUGIN_ID,
        TOOLING_PLUGIN_ID,
        WINDOW_PLUGIN_ID,
        WGPU_RENDER_PLUGIN_ID,
    ] {
        assert!(entries.iter().any(|entry| entry.plugin_id() == plugin));
    }
    assert!(
        plan.schema_validation()
            .provider_ids()
            .contains(&UI_SCHEMA_PROVIDER_ID)
    );
    assert!(
        plan.schema_validation()
            .provider_ids()
            .contains(&RENDER_SCHEMA_PROVIDER_ID)
    );
}

#[cfg(all(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[test]
fn overlapping_product_groups_accept_atomic_type_directed_edits() {
    use nara::{
        image::{IMAGE_PLUGIN_ID, ImageImportLimits},
        render::{RENDER_PLUGIN_ID, RenderPlugin},
    };

    let manifest = TestManifest::new(FULL_PRODUCT_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .insert_before::<RenderPlugin>(PluginDefinition::for_default::<OrderingPlugin>())
        .insert_after::<RenderPlugin>(PluginDefinition::for_default::<AfterOrderingPlugin>());
    let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
    let ids = plan
        .plugin_plan()
        .entries()
        .iter()
        .map(|entry| entry.plugin_id())
        .collect::<Vec<_>>();

    assert_eq!(ids.iter().filter(|id| **id == RENDER_PLUGIN_ID).count(), 1);
    assert_eq!(ids.iter().filter(|id| **id == IMAGE_PLUGIN_ID).count(), 1);
    let before = ids
        .iter()
        .position(|id| *id == TEST_ORDERING_PLUGIN_ID)
        .unwrap();
    let render = ids.iter().position(|id| *id == RENDER_PLUGIN_ID).unwrap();
    let after = ids
        .iter()
        .position(|id| *id == TEST_AFTER_ORDERING_PLUGIN_ID)
        .unwrap();
    assert!(before < render && render < after);

    assert!(matches!(
        resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate).disable::<RenderPlugin>(),
            built_in_schema_providers(),
        )
        .unwrap_err(),
        RuntimePlanError::PluginPlan(PluginPlanError::RequiredSlotDisabled { slot })
            if slot.as_str() == "nara.plugins.slot.render"
    ));
}

fn register_empty_schema_provider(
    _registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    Ok(())
}

fn counted_schema_provider() -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::with_validation(
        TEST_COUNTED_SCHEMA_OWNER_ID,
        TEST_COUNTED_SCHEMA_PROVIDER_ID,
        TEST_COUNTED_SCHEMA_PROVIDER_BINDING_ID,
        empty_schema_catalog_source,
        counted_schema_provider_validation,
        counted_schema_provider_registration,
    )
}

fn alternate_counted_schema_provider() -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::with_validation(
        TEST_COUNTED_SCHEMA_OWNER_ID,
        TEST_COUNTED_SCHEMA_PROVIDER_ID,
        ComponentSchemaProviderBindingId::new(
            "test.counted-schema-provider.components.alternate",
            1,
        ),
        empty_schema_catalog_source,
        counted_schema_provider_validation,
        counted_schema_provider_registration,
    )
}

fn counted_schema_provider_validation(
    _registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    COUNTED_PROVIDER_VALIDATIONS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn reject_counted_schema_provider_validation(
    _registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    COUNTED_PROVIDER_VALIDATIONS.fetch_add(1, Ordering::SeqCst);
    Err(ComponentRegistryError::Frozen)
}

fn panic_counted_schema_provider_validation(
    _registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    COUNTED_PROVIDER_VALIDATIONS.fetch_add(1, Ordering::SeqCst);
    panic!("schema provider validation panic")
}

fn counted_schema_provider_registration(
    _registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    COUNTED_PROVIDER_REGISTRATIONS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn plugin_error_contains_receipt_rejection(error: &PluginError) -> bool {
    match error {
        PluginError::ComponentRegistrationFailed {
            plugin, message, ..
        } => {
            *plugin == TEST_SCHEMA_PLUGIN_ID
                && message.contains("different executable behavior receipt")
        }
        PluginError::CommittedPreflightRejected { source, .. } => {
            plugin_error_contains_receipt_rejection(source)
        }
        _ => false,
    }
}

fn register_second_empty_schema_provider(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    if registry.is_frozen() {
        return Err(ComponentRegistryError::Frozen);
    }
    Ok(())
}

fn reject_schema_provider(_registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    Err(ComponentRegistryError::Frozen)
}

fn panic_schema_provider(_registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    panic!("schema provider probe")
}

fn register_unbound_component_schema(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry
        .register_component_schema(ComponentSchema::new(
            ComponentTypeId::new("test.UnboundComponent"),
            "Unbound Component",
            ComponentSchemaVersion::ONE,
        ))
        .map(|_| ())
}

struct TestManifest {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl TestManifest {
    fn new(bytes: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_plugin_composition_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("nara.toml");
        fs::write(&path, bytes).unwrap();
        Self { root, path }
    }

    fn capability(&self) -> FileCapability {
        FileCapability::from_host_handle(
            File::open(&self.path).unwrap(),
            TrustMode::TrustedLocal,
            1,
        )
        .unwrap()
    }
}

impl Drop for TestManifest {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

const MINIMAL_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Plugin Composition"

[runtime]
preset = "minimal"

[capabilities]
requested = []
"#;

const CHANGED_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Changed Plugin Composition"

[runtime]
preset = "minimal"

[capabilities]
requested = []
"#;

const PROFILE_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Profile Lineage"

[runtime]
preset = "minimal"

[capabilities]
requested = []

[profiles.dev]

[profiles.release]
"#;

#[cfg(feature = "desktop-winit")]
const DESKTOP_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Desktop Plugin Composition"

[runtime]
preset = "minimal"

[capabilities]
requested = ["desktop-winit"]

[window]
enabled = true
title = "Configured Nara Window"
width = 1024
height = 576
"#;

#[cfg(all(
    feature = "runtime-ui",
    feature = "tooling",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
const FULL_PRODUCT_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Full Product Plugin Composition"

[runtime]
preset = "minimal"

[capabilities]
requested = ["runtime-ui", "tooling", "desktop-winit", "render-wgpu"]

[window]
enabled = true
title = "Full Product Window"
width = 1280
height = 720
"#;
