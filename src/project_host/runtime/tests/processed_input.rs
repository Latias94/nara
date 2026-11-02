use nara_asset::{
    AssetPathScope, AssetProductRef, ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec,
};
use nara_core::{FormatKind, FormatVersion};
use nara_fs::ContentDigest;
use nara_identity::SceneEntityId;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapAsset, ActionMapBindings,
    ActionMapBuilder, ActionMapDomain, ActionMapLimits, ActionMapRuntimeCodec, InputBinding,
};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SourceDocumentKey, SourceDocumentKind,
};

use super::*;
use crate::{
    ProductRecipe,
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentBudgetKind, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
        RuntimeInputContent,
    },
};

const INPUT_FILE: &str = "input.runtime.json";
const SCENE_FILE: &str = "startup.runtime.json";
static NEXT_INPUT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn input_product() -> AssetProductRef {
    product("7c220000-0000-4000-8000-000000000001")
}

fn startup_product() -> AssetProductRef {
    product("7c220000-0000-4000-8000-000000000002")
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("nara.test.processed-input.host", 1),
        )
        .unwrap()
        .add_plugin::<StartupInputProbePlugin>()
        .unwrap()
}

struct InputFixture {
    root: PathBuf,
    recipe: ProductRecipe,
    expected: RuntimeContentExpectation,
}

impl InputFixture {
    fn new(recipe: ProductRecipe) -> Self {
        Self::new_with_input(recipe, None)
    }

    fn new_with_input(recipe: ProductRecipe, input_override: Option<&[u8]>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_processed_input_host_{}_{}",
            std::process::id(),
            NEXT_INPUT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.input.action_map = Some(input_product());
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let source =
            SourceDocumentKey::stable_asset(startup_product().source(), SourceDocumentKind::Scene);
        let scene = crate::startup_scene::compile_test_scene(
            &SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("startup").unwrap(),
            )]),
            plan.schema_validation().bindings(),
            source.clone(),
        );
        let scene = RuntimeSceneCandidate::cook(
            &scene,
            plan.schema_validation().bindings(),
            ProjectAssetDatabase::default().resolver(&AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .declare_context(ActionContext::new("menu").unwrap(), false)
            .unwrap();
        builder
            .bind(ActionBinding::key(
                ActionId::new("file.jump").unwrap(),
                nara_input::KeyCode::Space,
            ))
            .unwrap();
        let input = ActionMapRuntimeCodec::encode_runtime(
            &ActionMapAsset::new(input_product(), builder.compile().unwrap()).unwrap(),
        )
        .unwrap();
        let input = input_override.unwrap_or(&input);
        fs::write(root.join(SCENE_FILE), &scene).unwrap();
        fs::write(root.join(INPUT_FILE), input).unwrap();
        let target = RuntimeContentTarget::new("test-host", "processed-input-host").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: startup_product(),
                scenes: vec![startup_product()],
                addressable_roots: vec![],
                members: vec![
                    RuntimeContentMember::new(
                        SCENE_FILE,
                        ContentDigest::of_bytes(&scene),
                        RuntimeContentMemberKind::Scene { source },
                        vec![startup_product()],
                        vec![],
                    )
                    .unwrap(),
                    RuntimeContentMember::new(
                        INPUT_FILE,
                        ContentDigest::of_bytes(input),
                        RuntimeContentMemberKind::Asset {
                            format: FormatKind::new(ActionMapRuntimeCodec::RUNTIME_KIND).unwrap(),
                            version: FormatVersion::ONE,
                        },
                        vec![input_product()],
                        vec![],
                    )
                    .unwrap(),
                ],
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let catalog = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &catalog).unwrap();
        Self {
            root,
            recipe,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&catalog), target),
        }
    }

    fn open(&self) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap()
    }
}

impl Drop for InputFixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_processed_input_host_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn retained_charge(base: &RuntimeContentBase) -> usize {
    let probe = RuntimeContentLimits::default().retained_bytes().get();
    match base.reserve_retained(probe).err().unwrap() {
        RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            ..
        } => required - probe,
        other => panic!("unexpected retention probe failure: {other:?}"),
    }
}

fn selected_input(attempt: &RuntimeStartAttempt) -> &RuntimeInputContent {
    match &attempt.inputs.as_ref().unwrap().content {
        RuntimeStartContent::Processed {
            input: Some(input), ..
        } => input,
        _ => panic!("the processed attempt must retain its global Input acquisition"),
    }
}

fn exchange_input(attempt: &mut RuntimeStartAttempt, replacement: Option<RuntimeInputContent>) {
    let RuntimeStartContent::Processed { input, .. } =
        &mut attempt.inputs.as_mut().unwrap().content
    else {
        unreachable!()
    };
    *input = replacement;
}

fn finish_retirement(host: &mut ProjectHost, base: &RuntimeContentBase, baseline: usize) {
    assert!(host.has_cleanup_owner());
    let (failed, diagnostics) = drive_host_cleanup_to_completion(host);
    assert!(!failed, "{diagnostics:?}");
    assert!(!host.has_runtime_custody());
    assert_eq!(retained_charge(base), baseline);
}

#[derive(Default, Resource)]
struct StartupInputProbe {
    calls: usize,
    bindings: Option<ActionMapBindings>,
    gameplay_enabled: bool,
    menu_enabled: bool,
}

#[derive(Default)]
struct StartupInputProbePlugin;

impl Plugin for StartupInputProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.processed-input.startup-observer"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<StartupInputProbe>()?;
        app.add_systems(
            StartupStage::Core,
            |map: Res<ActionMap>, mut probe: nara_ecs::ResMut<StartupInputProbe>| {
                probe.calls += 1;
                probe.bindings = Some(map.bindings_handle());
                probe.gameplay_enabled = map.is_context_enabled(&ActionContext::gameplay());
                probe.menu_enabled = map.is_context_enabled(&ActionContext::new("menu").unwrap());
            },
        )?;
        Ok(())
    }
}

#[derive(Default)]
struct ExplicitEmptyInputPlugin;

impl Plugin for ExplicitEmptyInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.processed-input.explicit-empty"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ActionMap::default())?;
        Ok(())
    }
}

#[test]
fn processed_selection_is_visible_in_first_startup_from_the_captured_base_without_files() {
    let fixture = InputFixture::new(recipe());
    let base = fixture.open();
    let baseline = retained_charge(&base);
    for path in [INPUT_FILE, SCENE_FILE, RUNTIME_CONTENT_CATALOG_FILE] {
        fs::remove_file(fixture.root.join(path)).unwrap();
    }
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let expected = selected_input(&attempt).bindings();
    let prepared = host.prepare_start(&mut attempt).unwrap();
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    assert!(prepared.source().is_none());
    let world = prepared.runtime.runtime().unwrap().world();
    let probe = world.resource::<StartupInputProbe>();
    assert_eq!(probe.calls, 1);
    assert!(probe.gameplay_enabled);
    assert!(!probe.menu_enabled);
    assert!(
        probe
            .bindings
            .as_ref()
            .unwrap()
            .shares_owner_with(&expected)
    );
    assert_eq!(
        expected.definition().bindings()[0].action.as_str(),
        "file.jump",
    );
    assert_eq!(
        expected.definition().bindings()[0].input,
        InputBinding::Key(nara_input::KeyCode::Space),
    );
    assert!(!host.commit_prepared_start(prepared).has_errors());
    host.with_running_world(|world| {
        assert!(
            world
                .resource::<ActionMap>()
                .bindings_handle()
                .shares_owner_with(&expected)
        );
        assert!(
            world
                .resource::<CurrentSceneOwner>()
                .hosted_runtime_base()
                .unwrap()
                .same_base(&base)
        );
    })
    .unwrap();
    drop(expected);
    host.retire_running();
    finish_retirement(&mut host, &base, baseline);
    drop(attempt);
}

#[test]
fn an_explicit_empty_code_map_refuses_processed_selection_before_startup() {
    let fixture = InputFixture::new(recipe().add_plugin::<ExplicitEmptyInputPlugin>().unwrap());
    let base = fixture.open();
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let error = host.prepare_start(&mut attempt).err().unwrap();
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.input-selection-rejected",
    );
    let ProjectHostSlot::Cleaning { owner, .. } = &host.slot else {
        panic!("the rejected candidate must stay in existing Host cleanup custody")
    };
    let world = owner.world().unwrap();
    assert!(world.resource::<ActionMap>().bindings().is_empty());
    assert_eq!(world.resource::<StartupInputProbe>().calls, 0);
    finish_retirement(&mut host, &base, baseline);
    drop(attempt);
}

#[test]
fn input_attempt_stamp_refuses_new_acquisition_missing_selection_and_equal_catalog_foreign_base() {
    let fixture = InputFixture::new(recipe());
    let base = fixture.open();
    let foreign = fixture.open();
    assert_eq!(base.catalog_digest(), foreign.catalog_digest());
    assert!(!base.same_base(&foreign));
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let original = selected_input(&attempt).clone();
    let replacement = base.acquire_startup_input().unwrap().unwrap();
    assert_eq!(original.product(), replacement.product());
    assert!(!original.shares_owner_with(&replacement));
    for replacement in [
        Some(replacement),
        foreign.acquire_startup_input().unwrap(),
        None,
    ] {
        exchange_input(&mut attempt, replacement);
        let error = attempt.take_for(&host).err().unwrap();
        assert_eq!(first_code(&error.diagnostics), "project.run.stale-start");
        assert!(attempt.inputs.is_some());
        assert!(host.start_claim.is_active(attempt.epoch));
    }
    exchange_input(&mut attempt, Some(original));
    let RuntimeStartContent::Processed { base: selected, .. } =
        &mut attempt.inputs.as_mut().unwrap().content
    else {
        unreachable!()
    };
    *selected = foreign;
    assert!(attempt.take_for(&host).is_err());
    drop(attempt);
    assert!(!host.has_runtime_custody());
    assert_eq!(retained_charge(&base), baseline);
}

#[test]
fn failed_startup_retains_global_input_until_host_retirement_then_refunds_all_charges() {
    let fixture = InputFixture::new(recipe().add_plugin::<RecipeStartupFailurePlugin>().unwrap());
    let base = fixture.open();
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let error = host.prepare_start(&mut attempt).err().unwrap();
    assert_eq!(first_code(&error.diagnostics), "project.run.startup-failed");
    let ProjectHostSlot::Cleaning { owner, .. } = &host.slot else {
        panic!("startup failure must retain the existing Runtime cleanup owner")
    };
    let world = owner.world().unwrap();
    assert_eq!(world.resource::<StartupInputProbe>().calls, 1);
    assert_eq!(
        world.resource::<ActionMap>().bindings()[0].action.as_str(),
        "file.jump",
    );
    assert!(retained_charge(&base) >= baseline + ActionMap::required_state_bytes());
    finish_retirement(&mut host, &base, baseline);
    // Keep the consumed attempt alive through cleanup: its stamp must not retain the acquisition.
    drop(attempt);
}

#[test]
fn cancelled_attempt_and_aborted_prepared_start_release_input_in_the_existing_owners() {
    let fixture = InputFixture::new(recipe());
    let base = fixture.open();
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    assert!(retained_charge(&base) > baseline);
    drop(attempt);
    assert!(!host.has_runtime_custody());
    assert_eq!(retained_charge(&base), baseline);

    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let prepared = host.prepare_start(&mut attempt).unwrap();
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    host.abort_prepared_start(prepared);
    assert!(retained_charge(&base) >= baseline + ActionMap::required_state_bytes());
    finish_retirement(&mut host, &base, baseline);
    drop(attempt);
}

fn diagnostic_field<'a>(diagnostic: &'a Diagnostic, key: &str) -> Option<DiagnosticValueRef<'a>> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .map(|field| field.value())
}

#[test]
fn processed_input_context_budget_survives_the_codec_and_host_diagnostic_bridges() {
    let contexts: Vec<_> = (0..65)
        .map(|index| serde_json::json!({ "context": format!("context-{index}"), "enabled": true }))
        .collect();
    let encoded = serde_json::to_vec(&serde_json::json!({
        "kind": "runtime_action_map",
        "format_version": 1,
        "engine_min_version": "0.1.0",
        "generator": { "name": "nara", "version": "0.1.0" },
        "payload": { "product": input_product(), "contexts": contexts, "bindings": [] },
    }))
    .unwrap();
    let fixture = InputFixture::new_with_input(recipe(), Some(&encoded));
    let base = fixture.open();
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let error = host
        .begin_processed_start(base.clone(), vec![])
        .err()
        .unwrap();
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.runtime-content-rejected"
    );
    let domain = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "input.runtime-budget")
        .expect("the final report must preserve the bounded Input diagnostic");
    assert_eq!(
        diagnostic_field(domain, "budget-kind"),
        Some(DiagnosticValueRef::Identifier("contexts"))
    );
    assert_eq!(
        diagnostic_field(domain, "required"),
        Some(DiagnosticValueRef::Unsigned(65))
    );
    assert_eq!(
        diagnostic_field(domain, "limit"),
        Some(DiagnosticValueRef::Unsigned(64))
    );
    assert_eq!(host.next_epoch, 1);
    assert!(!host.has_runtime_custody());
    assert_eq!(retained_charge(&base), baseline);
}

#[test]
fn processed_input_state_budget_keeps_exact_host_numbers_and_cleanup_custody() {
    let fixture = InputFixture::new(recipe());
    let base = fixture.open();
    let baseline = retained_charge(&base);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_processed_start(base.clone(), vec![]).unwrap();
    let required_state = ActionMap::required_state_bytes();
    let before = retained_charge(&base);
    let limit = RuntimeContentLimits::default().retained_bytes().get();
    let pressure = base
        .reserve_retained(limit - before - (required_state - 1))
        .unwrap();
    let error = host.prepare_start(&mut attempt).err().unwrap();
    let diagnostic = error.diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "project.run.runtime-content-rejected"
    );
    assert_eq!(
        diagnostic_field(diagnostic, "reason"),
        Some(DiagnosticValueRef::Identifier("budget-exceeded"))
    );
    assert_eq!(
        diagnostic_field(diagnostic, "budget-kind"),
        Some(DiagnosticValueRef::Identifier("retained-bytes"))
    );
    assert_eq!(
        diagnostic_field(diagnostic, "required"),
        Some(DiagnosticValueRef::Unsigned(limit as u64 + 1))
    );
    assert_eq!(
        diagnostic_field(diagnostic, "limit"),
        Some(DiagnosticValueRef::Unsigned(limit as u64))
    );
    let ProjectHostSlot::Cleaning { owner, .. } = &host.slot else {
        panic!("refused state admission must retain the unpublished candidate for cleanup")
    };
    let world = owner.world().unwrap();
    assert_eq!(world.resource::<StartupInputProbe>().calls, 0);
    assert!(world.resource::<ActionMap>().bindings().is_empty());
    drop(pressure);
    finish_retirement(&mut host, &base, baseline);
    drop(attempt);
}
