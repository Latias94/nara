#![cfg(feature = "runtime-core")]

use std::{
    fs::{self, File},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginProductCapability,
    },
    fs::{FileCapability, TrustMode},
    input::{INPUT_PLUGIN_ID, InputPlugin},
    project::{ProductCapability, ProjectPath, RuntimeConfiguration, RuntimePreset},
    project_host::{
        CompositionError, ProjectCandidateErrorKind, RuntimeConfigurationPlanError,
        RuntimePlanError, ingest_project_manifest, resolve_product_recipe,
        resolve_runtime_configuration,
    },
    reflect::{
        ComponentCapability, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
        ComponentSchemaVersion, ComponentTypeId, PersistentComponentSchemaOwnerId,
    },
    transform::TRANSFORM_SCHEMA_PROVIDER_ID,
};

#[cfg(not(feature = "runtime-ui"))]
use nara::project_host::compiled_product_capabilities;

const DATA_COMPONENT: &str = "test.runtime-configuration.Payload";

#[derive(Default)]
struct DataPlugin;

impl Plugin for DataPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.runtime-configuration.data"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn mixed_recipe() -> ProductRecipe {
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("test.runtime-configuration.data").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(DATA_COMPONENT),
                    "Payload",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<DataPlugin>::data_for_default([owner]).unwrap())
        .unwrap()
}

#[test]
fn source_and_runtime_configuration_resolve_the_same_executable_closure() {
    let manifest = TestManifest::new(
        r#"
schema_version = 1
[project]
source_universe = 'ab6dcb23-33ca-59eb-85d5-31234eb0e2ea'
native_document_identity = 'project_location'
name = 'Configuration composition'
[profiles.delivery.runtime]
preset = 'local-headless'
paused = true
time_scale = 0.5
fixed_timestep_seconds = 0.0125
[profiles.delivery.diagnostics]
runtime_capacity = 7
[profiles.delivery.tasks.io]
workers = 1
pending_capacity = 8
"#,
    );
    let candidate = ingest_project_manifest(&manifest.capability(), Some("delivery")).unwrap();
    let configuration = candidate
        .runtime_configuration()
        .to_configuration()
        .resolve()
        .unwrap();
    let recipe = mixed_recipe();
    let source = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    let runtime = resolve_runtime_configuration(&configuration, recipe).unwrap();

    assert_eq!(source.lineage(), candidate.lineage());
    assert!(candidate.input_requirement().is_none());
    assert!(source.input_requirement().is_none());
    assert_eq!(source.settings(), runtime.settings());
    assert_eq!(
        runtime.settings().to_configuration().resolve().unwrap(),
        configuration
    );
    assert_eq!(
        source.explicit_capabilities(),
        runtime.explicit_capabilities()
    );
    assert_eq!(
        source.normalized_capabilities(),
        runtime.normalized_capabilities()
    );
    assert_eq!(
        source.required_capabilities(),
        runtime.required_capabilities()
    );
    assert_eq!(
        source.compiled_capabilities(),
        runtime.compiled_capabilities()
    );
    assert_eq!(
        source.plugin_plan().fingerprint().as_bytes(),
        runtime.plugin_plan().fingerprint().as_bytes()
    );
    assert_eq!(
        source.schema_validation().composition_fingerprint(),
        runtime.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        source.schema_validation().executable_fingerprint(),
        runtime.schema_validation().executable_fingerprint()
    );
    assert_eq!(
        source.schema_validation().provider_receipts(),
        runtime.schema_validation().provider_receipts()
    );
    assert!(
        runtime
            .schema_validation()
            .provider_ids()
            .contains(&TRANSFORM_SCHEMA_PROVIDER_ID)
    );
    let native = ComponentTypeId::new("nara.transform.Transform2d");
    let data = ComponentTypeId::new(DATA_COMPONENT);
    for bindings in [
        source.schema_validation().bindings(),
        runtime.schema_validation().bindings(),
    ] {
        assert!(bindings.native_snapshot().schema(&native).is_some());
        assert!(bindings.native_snapshot().schema(&data).is_none());
        assert!(bindings.data_owner(&native).is_none());
        assert!(bindings.data_owner(&data).is_some());
    }
    assert!(
        source.schema_validation().bindings().data_owners()[0]
            .ptr_eq(&runtime.schema_validation().bindings().data_owners()[0])
    );
}

#[test]
fn standalone_runtime_composition_needs_only_admitted_configuration_and_recipe() {
    let configuration = RuntimeConfiguration::default().resolve().unwrap();
    let runtime = resolve_runtime_configuration(&configuration, mixed_recipe()).unwrap();
    let cloned = runtime.clone();
    assert_eq!(runtime.settings(), &configuration);
    assert_eq!(
        runtime.plugin_plan().fingerprint(),
        cloned.plugin_plan().fingerprint()
    );
    assert!(
        runtime
            .schema_validation()
            .bindings()
            .same_definitions(cloned.schema_validation().bindings())
    );
    assert!(
        runtime
            .schema_validation()
            .bindings()
            .schema(&ComponentTypeId::new(DATA_COMPONENT))
            .is_some()
    );
}

#[test]
fn runtime_settings_change_executable_identity_without_changing_schema_selection() {
    let configuration = RuntimeConfiguration::default();
    let baseline =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), ProductRecipe::new())
            .unwrap();
    let mut paused = configuration.clone();
    paused.runtime.paused = true;
    let mut timestep = configuration.clone();
    timestep.runtime.fixed_timestep = Duration::from_micros(12_500);
    let mut diagnostics = configuration.clone();
    diagnostics.diagnostics.runtime_capacity = 7;
    let mut tasks = configuration.clone();
    tasks.tasks.io.pending_capacity += 1;

    for changed in [paused, timestep, diagnostics, tasks] {
        let changed = changed.resolve().unwrap();
        let runtime = resolve_runtime_configuration(&changed, ProductRecipe::new()).unwrap();
        assert_eq!(runtime.settings(), &changed);
        assert_ne!(
            baseline.plugin_plan().fingerprint(),
            runtime.plugin_plan().fingerprint()
        );
        assert_eq!(
            baseline.schema_validation().composition_fingerprint(),
            runtime.schema_validation().composition_fingerprint()
        );
    }
}

struct FactoryProbe;

impl Plugin for FactoryProbe {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.runtime-configuration.factory-probe"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        panic!("composition must finish before plugin build")
    }
}

fn probe_recipe(instances: &Arc<AtomicUsize>) -> ProductRecipe {
    let instances = Arc::clone(instances);
    ProductRecipe::new()
        .add_configured_plugin((), move |_: &()| {
            instances.fetch_add(1, Ordering::SeqCst);
            FactoryProbe
        })
        .unwrap()
}

#[derive(Default)]
struct InputCategoryOnly;

impl Plugin for InputCategoryOnly {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.runtime-configuration.input-category-only"),
            PluginCategory::Input,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        panic!("composition must finish before plugin build")
    }
}

#[test]
fn source_input_requires_the_actual_selected_input_plugin_before_app_creation() {
    let manifest = TestManifest::new(
        r#"
schema_version = 1
[project]
source_universe = 'ab6dcb23-33ca-59eb-85d5-31234eb0e2ea'
native_document_identity = 'project_location'
name = 'Source input owner selection'
[runtime]
preset = 'server'
[input]
action_map = 'input/game.actions.json'
[profiles.without_input.input]
clear_action_map = true
"#,
    );
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    let input_path = ProjectPath::new("input/game.actions.json").unwrap();
    assert_eq!(candidate.input_requirement(), Some(&input_path));
    let instances = Arc::new(AtomicUsize::new(0));
    for recipe in [
        probe_recipe(&instances),
        probe_recipe(&instances)
            .add_plugin::<InputCategoryOnly>()
            .unwrap(),
    ] {
        let error = resolve_product_recipe(&candidate, recipe).unwrap_err();
        assert_eq!(
            error,
            RuntimePlanError::Composition(CompositionError::InputOwnerRequired)
        );
        assert_eq!(instances.load(Ordering::SeqCst), 0);
    }

    let source = resolve_product_recipe(
        &candidate,
        probe_recipe(&instances)
            .add_plugin::<InputPlugin>()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(source.input_requirement(), Some(&input_path));
    assert!(
        source
            .plugin_plan()
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == INPUT_PLUGIN_ID)
    );
    assert_eq!(instances.load(Ordering::SeqCst), 0);

    let runtime =
        resolve_runtime_configuration(candidate.runtime_configuration(), probe_recipe(&instances))
            .unwrap();
    assert_eq!(source.settings(), runtime.settings());
    assert!(
        !runtime
            .plugin_plan()
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == INPUT_PLUGIN_ID)
    );

    let cleared = ingest_project_manifest(&manifest.capability(), Some("without_input")).unwrap();
    assert!(cleared.input_requirement().is_none());
    let cleared_source = resolve_product_recipe(&cleared, probe_recipe(&instances)).unwrap();
    assert!(cleared_source.input_requirement().is_none());
    assert_eq!(
        cleared_source.plugin_plan().fingerprint(),
        runtime.plugin_plan().fingerprint()
    );
    assert_eq!(instances.load(Ordering::SeqCst), 0);
}

#[test]
fn runtime_capability_admission_rejects_before_any_app_or_plugin_factory() {
    let instances = Arc::new(AtomicUsize::new(0));
    let mut incomplete = RuntimeConfiguration::default();
    incomplete.capabilities.requested = vec![ProductCapability::Presentation2d];
    let mut conflict = RuntimeConfiguration::default();
    conflict.runtime.preset = RuntimePreset::Server;
    conflict.capabilities.requested = vec![ProductCapability::Runtime2d];
    for (record, kind) in [
        (
            incomplete,
            ProjectCandidateErrorKind::InvalidCapabilityClosure,
        ),
        (conflict, ProjectCandidateErrorKind::PresetConflict),
    ] {
        let effective = record.resolve().unwrap();
        let error =
            resolve_runtime_configuration(&effective, probe_recipe(&instances)).unwrap_err();
        let RuntimeConfigurationPlanError::Capabilities(error) = error else {
            panic!("expected capability admission rejection")
        };
        assert_eq!(error.kind(), kind);
        assert!(error.diagnostics().has_errors());
        assert_eq!(instances.load(Ordering::SeqCst), 0);
    }
    resolve_runtime_configuration(
        &RuntimeConfiguration::default().resolve().unwrap(),
        probe_recipe(&instances),
    )
    .unwrap();
    assert_eq!(instances.load(Ordering::SeqCst), 0);
}

#[derive(Default)]
struct UnrequestedPlugin;

impl Plugin for UnrequestedPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.runtime-configuration.unrequested"),
            PluginCategory::Runtime,
        )
        .requires_product_capabilities(&[PluginProductCapability::new("runtime-2d")]);
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        panic!("the unrequested plugin must not be instantiated")
    }
}

#[test]
fn runtime_configuration_enforces_recipe_product_requirements_before_app_creation() {
    let error = resolve_runtime_configuration(
        &RuntimeConfiguration::default().resolve().unwrap(),
        ProductRecipe::new()
            .add_plugin::<UnrequestedPlugin>()
            .unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        RuntimeConfigurationPlanError::Resolution(RuntimePlanError::Composition(
            CompositionError::UnrequestedProductCapability {
                plugin: UnrequestedPlugin::declaration().id,
                capability: ProductCapability::Runtime2d,
            }
        ))
    );
}

#[cfg(not(feature = "runtime-ui"))]
#[test]
fn runtime_configuration_uses_the_actual_compiled_capability_ceiling() {
    let mut configuration = RuntimeConfiguration::default();
    configuration.capabilities.requested = vec![ProductCapability::RuntimeUi];
    let instances = Arc::new(AtomicUsize::new(0));
    assert!(
        !compiled_product_capabilities()
            .capabilities()
            .contains(ProductCapability::RuntimeUi)
    );
    let error =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), probe_recipe(&instances))
            .unwrap_err();
    let RuntimeConfigurationPlanError::Capabilities(error) = error else {
        panic!("expected compiled capability rejection")
    };
    assert_eq!(
        error.kind(),
        ProjectCandidateErrorKind::UnavailableCapabilities
    );
    assert_eq!(instances.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "runtime-ui")]
#[test]
fn runtime_configuration_enforces_the_same_unsupported_default_gate() {
    let font = nara::ui::FontFace::from_bytes(
        include_bytes!("../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let recipe = ProductRecipe::new().with_ui_font(font).unwrap();
    let error =
        resolve_runtime_configuration(&RuntimeConfiguration::default().resolve().unwrap(), recipe)
            .unwrap_err();
    assert_eq!(
        error,
        RuntimeConfigurationPlanError::Resolution(RuntimePlanError::Composition(
            CompositionError::UnsupportedDefaultConfiguration {
                plugin: nara::ui::UI_PLUGIN_ID,
            }
        ))
    );
}

static MANIFEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestManifest {
    directory: PathBuf,
}

impl TestManifest {
    fn new(contents: &str) -> Self {
        let sequence = MANIFEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "nara_runtime_configuration_composition_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("nara.toml"), contents).unwrap();
        Self { directory }
    }

    fn capability(&self) -> FileCapability {
        FileCapability::from_host_handle(
            File::open(self.directory.join("nara.toml")).unwrap(),
            TrustMode::TrustedLocal,
            1,
        )
        .unwrap()
    }
}

impl Drop for TestManifest {
    fn drop(&mut self) {
        fs::remove_file(self.directory.join("nara.toml")).unwrap();
        fs::remove_dir(&self.directory).unwrap();
    }
}
