use std::{
    fs::{self, File},
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara::{
    app::{
        AddPluginsError, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginError, PluginHook, PluginHookMutation, PluginId, PluginInstantiationError,
        PluginProductCapability, PluginSchemaProviderId, RuntimeAdmissionReservation,
        RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeFaultKind,
        RuntimeObligationLedger,
    },
    fs::{FileCapability, TrustMode},
    project::ProductCapability,
    project_host::{
        CompositionError, ProjectRuntimePlugins, RuntimePlanError, built_in_schema_providers,
        ingest_project_manifest, project_runtime_plugins, resolve_runtime_plan,
    },
    reflect::{
        ComponentRegistry, ComponentRegistryError, ComponentSchema,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaVersion, ComponentTypeId,
    },
    scene::HIERARCHY_SCHEMA_PROVIDER_ID,
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
const TEST_SCHEMA_PROVIDER_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.schema-provider.components.native", 1);
const TEST_SCHEMA_PROVIDER_SECOND_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.schema-provider.components.alternate", 1);
const TEST_COUNTED_SCHEMA_PLUGIN_ID: PluginId = PluginId::new("test.counted-schema-provider");
const TEST_COUNTED_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("test.counted-schema-provider.components");
const TEST_COUNTED_SCHEMA_PROVIDER_BINDING_ID: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("test.counted-schema-provider.components.native", 1);
const TEST_COUNTED_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_COUNTED_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[TEST_COUNTED_SCHEMA_PROVIDER_ID]);

static COUNTED_PROVIDER_VALIDATIONS: AtomicUsize = AtomicUsize::new(0);
static COUNTED_PROVIDER_REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);
const TEST_SCHEMA_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TEST_SCHEMA_PLUGIN_ID, PluginCategory::Runtime)
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
        let registry = context
            .get_structural_resource::<ComponentRegistry>()
            .ok_or_else(|| {
                PluginError::component_registration(
                    TEST_COUNTED_SCHEMA_PLUGIN_ID,
                    TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
                    "component registry unavailable",
                )
            })?;
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
        let mut registry = app.world_mut()?.resource_mut::<ComponentRegistry>();
        counted_schema_provider()
            .register_or_validate_into(&mut registry)
            .map_err(|error| {
                PluginError::component_registration(
                    TEST_COUNTED_SCHEMA_PLUGIN_ID,
                    TEST_COUNTED_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
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
        let registry = context
            .get_structural_resource::<ComponentRegistry>()
            .ok_or_else(|| {
                PluginError::component_registration(
                    TEST_SCHEMA_PLUGIN_ID,
                    TEST_SCHEMA_PROVIDER_ID.as_str(),
                    "component registry unavailable",
                )
            })?;
        self.provider.preflight(registry).map_err(|error| {
            PluginError::component_registration(
                TEST_SCHEMA_PLUGIN_ID,
                TEST_SCHEMA_PROVIDER_ID.as_str(),
                error,
            )
        })
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        self.provider
            .register_or_validate_into(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(
                    TEST_SCHEMA_PLUGIN_ID,
                    TEST_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
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
fn project_runtime_plan_is_pure_repeatable_and_schema_bound() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    let first = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
    let repeated = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();

    assert_eq!(first.lineage(), candidate.lineage());
    assert_eq!(first.lineage(), repeated.lineage());
    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert_eq!(
        first.schema_validation().fingerprint(),
        repeated.schema_validation().fingerprint()
    );
    assert_eq!(
        first.schema_validation().provider_ids(),
        repeated.schema_validation().provider_ids()
    );
    assert!(
        first
            .required_capabilities()
            .contains(ProductCapability::RuntimeCore)
    );
    assert_eq!(
        first.schema_validation().provider_ids(),
        [HIERARCHY_SCHEMA_PROVIDER_ID, TRANSFORM_SCHEMA_PROVIDER_ID]
    );

    let app = first.plugin_plan().instantiate().unwrap();
    let runtime_registry = app.world().resource::<ComponentRegistry>();
    let runtime_fingerprint = runtime_registry.catalog().unwrap().fingerprint();
    assert_eq!(runtime_fingerprint, first.schema_validation().fingerprint());
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
}

#[test]
fn file_backed_schema_provider_registration_is_not_replayed_in_the_candidate() {
    COUNTED_PROVIDER_VALIDATIONS.store(0, Ordering::SeqCst);
    COUNTED_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate).insert_after::<TransformPlugin>(
        PluginDefinition::for_default::<CountedSchemaProviderPlugin>(),
    );
    let mut providers = built_in_schema_providers();
    providers.push(counted_schema_provider());
    let plan = resolve_runtime_plan(&candidate, request, providers).unwrap();
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(
        plan.schema_validation()
            .provider_receipts()
            .iter()
            .map(|receipt| receipt.provider())
            .collect::<Vec<_>>(),
        plan.schema_validation().provider_ids()
    );

    let app = plan.plugin_plan().instantiate().unwrap();
    let registry = app.world().resource::<ComponentRegistry>();
    assert!(registry.shares_snapshot(plan.schema_validation().snapshot()));
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(COUNTED_PROVIDER_VALIDATIONS.load(Ordering::SeqCst), 0);
}

#[test]
fn code_first_schema_provider_builds_and_freezes_once_without_a_host() {
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
    let registry = app.world().resource::<ComponentRegistry>();
    assert!(registry.is_frozen());
    assert_eq!(COUNTED_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst), 1);
    nara::reflect::validate_component_registry_authority(app.world()).unwrap();
}

#[test]
fn code_first_runtime_faults_when_the_registry_is_rewrapped_with_its_same_snapshot() {
    let mut app = nara::app::App::new();
    app.add_plugins(nara::reflect::ComponentRegistryPlugin)
        .unwrap();
    app.add_systems(CoreStage::Last, rewrap_code_first_registry)
        .unwrap();
    let candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let mut runtime = candidate.complete_startup().unwrap().promote();

    let error = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(error.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
    assert_eq!(runtime.state(), nara::app::RuntimeState::Faulted);
}

#[test]
fn file_backed_candidate_rejects_divergent_binding_codec_and_migration_receipts() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let admitted = ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
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
            TEST_SCHEMA_PROVIDER_ID,
            divergent,
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
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
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
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let mut providers = built_in_schema_providers();
    providers.extend([
        ComponentSchemaProviderDefinition::new(
            PROVIDER_ID,
            TEST_SCHEMA_PROVIDER_BINDING_ID,
            register_empty_schema_provider,
        ),
        ComponentSchemaProviderDefinition::new(
            PROVIDER_ID,
            TEST_SCHEMA_PROVIDER_SECOND_BINDING_ID,
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
fn multiple_plugins_cannot_own_the_same_schema_provider() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>())
        .insert_after::<SchemaProviderPlugin>(PluginDefinition::for_default::<
            SecondSchemaProviderPlugin,
        >());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        register_empty_schema_provider,
    ));

    assert_eq!(
        resolve_runtime_plan(&candidate, request, providers).unwrap_err(),
        RuntimePlanError::Composition(CompositionError::AmbiguousSchemaProviderOwner {
            provider: TEST_SCHEMA_PROVIDER_ID,
        })
    );
}

#[test]
fn schema_provider_callback_rejection_preserves_the_composition_phase() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
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
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
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
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        register_empty_schema_provider,
    ));
    resolve_runtime_plan(&candidate, request(), corrected).unwrap();
}

#[test]
fn schema_catalog_freeze_failure_does_not_publish_a_runtime_plan() {
    let manifest = TestManifest::new(MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let request = project_runtime_plugins(&candidate)
        .insert_after::<TransformPlugin>(PluginDefinition::for_default::<SchemaProviderPlugin>());
    let mut providers = built_in_schema_providers();
    providers.push(ComponentSchemaProviderDefinition::new(
        TEST_SCHEMA_PROVIDER_ID,
        TEST_SCHEMA_PROVIDER_BINDING_ID,
        register_unbound_component_schema,
    ));

    let error = resolve_runtime_plan(&candidate, request, providers).unwrap_err();
    let RuntimePlanError::Composition(CompositionError::SchemaFreezeRejected { source }) = error
    else {
        panic!("unbound schema must fail during catalog freeze");
    };
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

fn rewrap_code_first_registry(world: &mut nara::ecs::World) {
    let snapshot = world
        .resource::<ComponentRegistry>()
        .snapshot()
        .expect("the code-first registry is frozen before runtime execution");
    world.insert_resource(ComponentRegistry::from_snapshot(snapshot));
}

fn counted_schema_provider() -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::with_validation(
        TEST_COUNTED_SCHEMA_PROVIDER_ID,
        TEST_COUNTED_SCHEMA_PROVIDER_BINDING_ID,
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
