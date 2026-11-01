use std::sync::Mutex;

use nara_app::{PluginPlan, PluginPlanError, PluginSlotPresence};
use nara_render::RenderFramePacket;
use nara_render_wgpu::{
    WGPU_STOCK_CAPTURE_PLUGIN_ID, WGPU_STOCK_POLICY_PLUGIN_ID, WgpuPolicyBindingContext,
    WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuPolicyProgram,
    WgpuRenderPolicy, WgpuRenderProgramError, WgpuStockBindings, WgpuStockCapture,
    WgpuStockCaptureAdapter, WgpuStockPolicy, install_wgpu_policy,
};

use super::*;

pub(crate) struct TestCapture {
    pub value: u32,
    pub stock: WgpuStockCapture,
}

pub(crate) struct TestPolicy {
    value: u32,
    stock: WgpuStockPolicy,
}

impl WgpuRenderPolicy for TestPolicy {
    type Capture = TestCapture;
    type Bindings = WgpuStockBindings;

    fn device_requirements(&self) -> nara_render_wgpu::WgpuDeviceRequirements {
        self.stock.device_requirements()
    }

    fn bind(
        &self,
        capture: &TestCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        self.stock.bind(&capture.stock, context)
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &TestCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        if capture.value != self.value {
            return Err(WgpuRenderProgramError::feature(
                "foreign test policy configuration",
            ));
        }
        self.stock.validate_capture(packet, &capture.stock)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &TestCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        self.stock.build(bindings, &capture.stock, context)
    }
}

struct TestConfiguration {
    value: u32,
    // Test observations do not change policy behavior or its canonical configuration.
    factory_inputs: Arc<Mutex<Vec<(u32, usize)>>>,
}

impl ProductConfiguration for TestConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.value.to_le_bytes());
    }
}

impl TestConfiguration {
    fn observe(&self) {
        self.factory_inputs
            .lock()
            .unwrap()
            .push((self.value, std::ptr::from_ref(self) as usize));
    }

    fn policy(&self) -> TestPolicy {
        TestPolicy {
            value: self.value,
            stock: WgpuStockPolicy::new(),
        }
    }
}

struct TestPolicyPlugin {
    value: u32,
}

impl TestPolicyPlugin {
    fn from_configuration(configuration: &TestConfiguration) -> Self {
        configuration.observe();
        Self {
            value: configuration.value,
        }
    }
}

impl Plugin for TestPolicyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.recipe-policy"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[WGPU_STOCK_CAPTURE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let installation = WgpuPolicyDefinition::new(TestPolicy {
            value: self.value,
            stock: WgpuStockPolicy::new(),
        })
        .install()
        .map_err(|error| PluginError::SetupFailed {
            plugin: Self::declaration().id,
            message: error.to_string(),
        })?;
        install_wgpu_policy(app, installation)
    }
}

fn adapter(
    configuration: &TestConfiguration,
) -> Result<WgpuStockCaptureAdapter, WgpuRenderProgramError> {
    configuration.observe();
    let value = configuration.value;
    Ok(WgpuStockCaptureAdapter::new(
        WgpuPolicyDefinition::new(configuration.policy()).install()?,
        move |stock| Ok(Arc::new(TestCapture { value, stock })),
    ))
}

fn configured_recipe(configuration: TestConfiguration) -> ProductRecipe {
    ProductRecipe::new()
        .select_wgpu_policy(configuration, TestPolicyPlugin::from_configuration, adapter)
        .unwrap()
}

pub(crate) fn recipe(value: u32) -> ProductRecipe {
    configured_recipe(TestConfiguration {
        value,
        factory_inputs: Arc::default(),
    })
}

struct TestDefaults;

impl PluginGroup for TestDefaults {
    const ID: PluginGroupId = PluginGroupId::new("nara.test.recipe-policy-defaults");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_edited_group(
                WgpuBackendPlugins
                    .edit()
                    .disable_slot(SLOT_WGPU_WINDOW_PRESENTATION),
            )
    }
}

#[test]
fn selection_rejects_missing_capture_dependency_and_schema_contributors_before_factories() {
    struct WithoutCapture;
    impl Plugin for WithoutCapture {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.recipe-policy-without-capture"),
                PluginCategory::Runtime,
            );
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            unreachable!("selection cannot build an invalid policy")
        }
    }

    struct SchemaPolicy;
    impl Plugin for SchemaPolicy {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.recipe-schema-policy"),
                PluginCategory::Runtime,
            )
            .requires_plugins(&[WGPU_STOCK_CAPTURE_PLUGIN_ID])
            .provides_schema(&[PluginSchemaProviderId::new(
                "nara.test.recipe-schema-policy",
            )]);
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            unreachable!("selection cannot build a Schema policy")
        }
    }

    assert!(matches!(
        ProductRecipe::new().select_wgpu_policy::<WithoutCapture, _, _, _>(
            (),
            |_| panic!("an invalid selection must not construct a plugin"),
            |_| panic!("an invalid selection must not construct an adapter"),
        ),
        Err(ProductRecipeError::WgpuPolicyRequiresStockCapture { plugin })
            if plugin == WithoutCapture::declaration().id
    ));
    assert!(matches!(
        ProductRecipe::new().select_wgpu_policy::<SchemaPolicy, _, _, _>(
            (),
            |_| panic!("a Schema policy must not construct a plugin"),
            |_| panic!("a Schema policy must not construct an adapter"),
        ),
        Err(ProductRecipeError::RuntimePluginProvidesSchema { plugin })
            if plugin == SchemaPolicy::declaration().id
    ));
}

#[test]
fn selection_retains_the_exact_configured_entry_and_shared_factory_configuration() {
    let observations = Arc::default();
    let recipe = configured_recipe(TestConfiguration {
        value: 19,
        factory_inputs: Arc::clone(&observations),
    });
    let ordinary = ProductRecipe::new()
        .add_configured_plugin(
            TestConfiguration {
                value: 19,
                factory_inputs: Arc::default(),
            },
            TestPolicyPlugin::from_configuration,
        )
        .unwrap();
    let selected = recipe.selected_wgpu_policy().unwrap();
    assert_eq!(selected.plugin_type, TypeId::of::<TestPolicyPlugin>());
    assert_eq!(selected.kind(), ProductRecipeEntryKind::RuntimePlugin);
    assert_eq!(
        selected.definition.key(),
        ordinary.entries().next().unwrap().definition.key(),
    );
    assert_eq!(
        selected.configuration_fingerprint(),
        ordinary
            .entries()
            .next()
            .unwrap()
            .configuration_fingerprint(),
    );
    assert!(observations.lock().unwrap().is_empty());

    let plan = PluginPlan::resolve(recipe.clone().with_defaults(TestDefaults)).unwrap();
    let entry = plan
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == selected.plugin_id())
        .unwrap();
    assert_eq!(entry.definition_key(), selected.definition.key());
    assert_eq!(entry.slot(), Some(SLOT_WGPU_SELECTED_POLICY));
    assert_eq!(
        entry.slot_contract().unwrap().presence(),
        PluginSlotPresence::Required,
    );
    assert!(
        !plan
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == WGPU_STOCK_POLICY_PLUGIN_ID)
    );
    assert_eq!(
        plan.entries()
            .iter()
            .filter(|entry| entry.plugin_id() == WGPU_STOCK_CAPTURE_PLUGIN_ID)
            .count(),
        1,
    );

    let mut app = App::new();
    app.add_plugins(recipe.clone().with_defaults(TestDefaults))
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<WgpuPolicyInstallation<TestPolicy>>()
            .definition()
            .policy()
            .value,
        19,
    );
    assert!(
        !app.world()
            .contains_resource::<WgpuPolicyInstallation<WgpuStockPolicy>>()
    );
    let _scene = recipe.create_wgpu_capture_adapter().unwrap();
    let _canvas = recipe.create_wgpu_capture_adapter().unwrap();
    let observations = observations.lock().unwrap();
    assert_eq!(observations.len(), 3);
    assert!(observations.iter().all(|input| input == &observations[0]));
    assert_eq!(observations[0].0, 19);
}

#[test]
fn selection_has_stable_behavioral_fingerprints_and_preserves_default_selection() {
    let resolve =
        |recipe: ProductRecipe| PluginPlan::resolve(recipe.with_defaults(TestDefaults)).unwrap();
    let first = resolve(recipe(19));
    assert_eq!(first.fingerprint(), resolve(recipe(19)).fingerprint());
    assert_ne!(first.fingerprint(), resolve(recipe(23)).fingerprint());

    let default = ProductRecipe::new();
    assert!(default.is_empty());
    assert!(default.selected_wgpu_policy().is_none());
    let default_plan = resolve(default);
    assert_eq!(
        default_plan.fingerprint(),
        resolve(ProductRecipe::default()).fingerprint(),
    );
    assert_eq!(
        default_plan
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == WGPU_STOCK_POLICY_PLUGIN_ID)
            .count(),
        1,
    );
    let mut app = App::new();
    app.add_plugins(ProductRecipe::new().with_defaults(TestDefaults))
        .unwrap();
    assert!(
        app.world()
            .contains_resource::<WgpuPolicyInstallation<WgpuStockPolicy>>()
    );
    assert!(
        !app.world()
            .contains_resource::<WgpuPolicyInstallation<TestPolicy>>()
    );
}

#[test]
fn selection_rejects_unpaired_reconfiguration_and_missing_defaults() {
    assert!(matches!(
        recipe(19).configure_plugin(
            TestConfiguration {
                value: 23,
                factory_inputs: Arc::default(),
            },
            TestPolicyPlugin::from_configuration,
        ),
        Err(ProductRecipeError::SelectedWgpuPolicyConfiguration { plugin })
            if plugin == TestPolicyPlugin::declaration().id
    ));
    assert!(matches!(
        recipe(19).select_wgpu_policy(
            TestConfiguration {
                value: 23,
                factory_inputs: Arc::default(),
            },
            TestPolicyPlugin::from_configuration,
            adapter,
        ),
        Err(ProductRecipeError::DuplicateDefaultConfiguration { .. })
    ));
    assert_eq!(
        PluginPlan::resolve(recipe(19).with_defaults(MinimalPlugins)).unwrap_err(),
        PluginPlanError::MissingEditTarget,
    );
}

#[test]
fn outer_policy_configuration_must_preserve_the_exact_recipe_definition() {
    let selected = recipe(19);
    let replacement = recipe(23)
        .selected_wgpu_policy()
        .unwrap()
        .definition
        .clone();
    let mut app = App::new();
    let error = app
        .add_plugins(
            selected
                .clone()
                .with_defaults(TestDefaults)
                .edit()
                .configure(replacement),
        )
        .err()
        .expect("an outer policy configuration cannot detach its paired Editor factory");
    let failure = error
        .plugin_failure()
        .expect("the recipe authority must reject in preflight");
    assert!(
        matches!(
            failure.error(),
            PluginError::CommittedPreflightRejected { plugin, source }
                if *plugin == PRODUCT_RECIPE_SCHEMA_AUTHORITY_PLUGIN_ID
                    && matches!(source.as_ref(), PluginError::SetupFailed { plugin, .. }
                        if *plugin == TestPolicyPlugin::declaration().id)
        ),
        "unexpected policy selection failure: {failure:?}",
    );
    assert!(
        !app.world()
            .contains_resource::<WgpuPolicyInstallation<TestPolicy>>()
    );

    // Exact selected definitions and unrelated default configuration remain legal edits.
    let exact = selected.selected_wgpu_policy().unwrap().definition.clone();
    let diagnostics = PluginDefinition::infallible::<nara_diagnostic::DiagnosticsPlugin, _>(
        PluginDefinitionId::new("nara.test.recipe-policy.configured-diagnostics", 1),
        b"configured diagnostics",
        nara_diagnostic::DiagnosticsPlugin::default,
    );
    let mut app = App::new();
    app.add_plugins(
        selected
            .with_defaults(TestDefaults)
            .edit()
            .configure(exact)
            .configure(diagnostics),
    )
    .unwrap();
    assert_eq!(
        app.world()
            .resource::<WgpuPolicyInstallation<TestPolicy>>()
            .definition()
            .policy()
            .value,
        19,
    );
}
