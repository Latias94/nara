//! Fixed executable definitions and real product consumers of already-published EnemyStats.
//!
//! These helpers never write project sources or derive executable authority from their bundle.

use std::{
    fs,
    num::NonZeroU32,
    path::Path,
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};

use nara_engine::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{
        CurrentScene, STARTUP_SCENE_ACTIVATION_PLUGIN_ID, add_startup_scene_activation_systems,
    },
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginProductCapability,
    },
    diagnostic::DiagnosticReport,
    ecs::{Resource, World},
    fs::ContentDigest,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport,
        ProjectContentDeliveryReceipt, advanced::built_in_schema_providers,
        open_explicit_product_project_root,
    },
    project_workflow::{ProjectCheckRequest, check_project},
};
use nara_identity::EntityLookup;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentPropertyPath,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    PersistentComponentSchemaOwnerId, resolve_component_property,
};
use nara_scene::{SceneEntitySource, scene_component_bindings};

use crate::authored_upgrade_command::{
    COMPONENT, OWNER, V2, health_property_v2, successor_catalog, v1_catalog,
};

const AGREEMENT_REFUSAL: &str = "project.runtime-agreement.selected-owner-divergent";
const TIMEOUT: Duration = Duration::from_secs(30);

/// One actual materialized record, identified without depending on flattened Prefab ordinals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnemyStatsSample {
    pub source_entity: String,
    pub projected: bool,
    pub health: i64,
    pub armor: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Resource)]
pub struct EnemyStatsOutcome {
    pub activations: u32,
    pub ticks: u32,
    pub samples: Vec<EnemyStatsSample>,
    pub health_total: i64,
}

#[derive(Clone, Resource)]
struct EnemyStatsConsumer {
    version: ComponentSchemaVersion,
    observations: Option<SyncSender<EnemyStatsOutcome>>,
}

impl ProductConfiguration for EnemyStatsConsumer {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.version.get().to_le_bytes());
        output.push(u8::from(self.observations.is_some()));
    }
}

impl Plugin for EnemyStatsConsumer {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("trial.enemy-stats.product"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[STARTUP_SCENE_ACTIVATION_PLUGIN_ID])
        .requires_any_product_capabilities(&[PluginProductCapability::new("runtime-2d")]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.clone())?
            .init_resource::<EnemyStatsOutcome>()?;
        add_startup_scene_activation_systems(app, observe_activation)?;
        app.add_systems(CoreStage::FixedUpdate, consume_stats)?;
        Ok(())
    }
}

fn fixed_owner(version: ComponentSchemaVersion) -> ComponentSchemaOwnerSnapshot {
    let previous = v1_catalog();
    let (current, predecessor) = if version == ComponentSchemaVersion::ONE {
        (previous, None)
    } else {
        assert_eq!(version, V2);
        (successor_catalog(&previous).unwrap(), Some(previous))
    };
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        current,
        predecessor,
    )
    .unwrap()
}

fn recipe(version: ComponentSchemaVersion) -> ProductRecipe {
    configured_recipe(version, None)
}

fn configured_recipe(
    version: ComponentSchemaVersion,
    observations: Option<SyncSender<EnemyStatsOutcome>>,
) -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<EnemyStatsConsumer>::data_configured(
                EnemyStatsConsumer {
                    version,
                    observations,
                },
                Clone::clone,
                [fixed_owner(version)],
            )
            .unwrap(),
        )
        .unwrap()
}

pub fn v1_recipe() -> ProductRecipe {
    recipe(ComponentSchemaVersion::ONE)
}

pub fn v2_recipe() -> ProductRecipe {
    recipe(V2)
}

/// Bounded observation of the first real fixed tick in a trusted compiled Editor Play owner.
pub fn v2_play_recipe(observations: SyncSender<EnemyStatsOutcome>) -> ProductRecipe {
    configured_recipe(V2, Some(observations))
}

/// Initial fixture setup only: include the complete native product owners before upgrading.
pub fn v1_bundle() -> AuthoringSchemaBundle {
    let builtin =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap();
    AuthoringSchemaBundle::new(
        builtin
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::from(&fixed_owner(
                ComponentSchemaVersion::ONE,
            ))]),
    )
    .unwrap()
}

fn observe_activation(world: &mut World) {
    assert!(CurrentScene::get(world).is_some());
    world.resource_mut::<EnemyStatsOutcome>().activations += 1;
}

fn consume_stats(world: &mut World) {
    let configuration = world.resource::<EnemyStatsConsumer>();
    let version = configuration.version;
    let observations = configuration.observations.clone();
    let current = CurrentScene::get(world).expect("the product must activate its actual Scene");
    let bindings = scene_component_bindings(world).unwrap();
    let component = ComponentTypeId::new(COMPONENT);
    let schema = bindings.schema(&component).unwrap();
    assert_eq!(schema.version(), version);
    let health = if version == V2 {
        health_property_v2()
    } else {
        ComponentPropertyPath::new(ComponentFieldId::new("health"))
    };
    let armor = ComponentPropertyPath::new(ComponentFieldId::new("armor"));
    let mut samples = Vec::new();
    for source in &current.source().entities {
        let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &source.id) else {
            panic!("the activated source entity must be live: {}", source.id);
        };
        let Some(record) = bindings.data_component(world, entity, &component).unwrap() else {
            continue;
        };
        assert_eq!(record.version, version);
        assert_eq!(record.owner().unwrap().as_str(), OWNER);
        let origin = world.get::<SceneEntitySource>(entity).unwrap().provenance();
        samples.push(EnemyStatsSample {
            source_entity: origin.local_entity_id().as_str().to_owned(),
            projected: origin.projection_path().is_some(),
            health: resolve_component_property(schema, version, &record.value, &health)
                .unwrap()
                .value
                .as_i64()
                .unwrap(),
            armor: resolve_component_property(schema, version, &record.value, &armor)
                .unwrap()
                .value
                .as_i64()
                .unwrap(),
        });
    }
    samples.sort_unstable();
    let health_total = samples
        .iter()
        .try_fold(0_i64, |sum, sample| sum.checked_add(sample.health))
        .unwrap();
    let mut outcome = world.resource_mut::<EnemyStatsOutcome>();
    outcome.ticks += 1;
    outcome.samples = samples;
    outcome.health_total = health_total;
    if outcome.ticks == 1 {
        if let Some(observations) = observations {
            let _ = observations.try_send(outcome.clone());
        }
    }
}

fn check(
    root: &Path,
    profile: Option<&str>,
    recipe: ProductRecipe,
) -> ProjectContentDeliveryReceipt {
    let request =
        ProjectCheckRequest::new(open_explicit_product_project_root(root).unwrap(), recipe);
    let request = match profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };
    let checked = check_project(request).unwrap();
    for member in checked.members() {
        assert_eq!(
            member.digest(),
            ContentDigest::of_bytes(&fs::read(root.join(member.path().as_str())).unwrap()),
            "Check must observe the actual published source: {}",
            member.path(),
        );
    }
    checked
}

fn run(
    root: &Path,
    profile: Option<&str>,
    recipe: ProductRecipe,
) -> HeadlessRunReport<EnemyStatsOutcome> {
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .with_recipe(recipe)
        .with_cleanup_timeout(Duration::from_secs(2));
    let intent = match profile {
        Some(profile) => intent.with_profile(profile),
        None => intent,
    };
    let mut run = HeadlessRun::new(
        open_explicit_product_project_root(root).unwrap(),
        intent,
        Vec::new(),
    );
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let report = run.execute_bounded();
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        assert!(
            Instant::now() < deadline,
            "the actual product cleanup must finish",
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_saved_product(
    root: &Path,
    profile: Option<&str>,
    recipe: ProductRecipe,
    expected_scene_roots: &[&str],
    expected: &[EnemyStatsSample],
) -> EnemyStatsOutcome {
    let checked = check(root, profile, recipe.clone());
    let actual_roots: Vec<_> = checked
        .scene_roots()
        .iter()
        .map(|path| path.as_str())
        .collect();
    assert_eq!(actual_roots, expected_scene_roots);
    let report = run(root, profile, recipe);
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics(),
    );
    let startup = report.startup_content_closure().unwrap();
    assert_eq!(startup.manifest_digest(), checked.manifest_digest());
    assert_eq!(startup.schema_fingerprint(), checked.schema_fingerprint());
    assert_eq!(startup.members(), checked.startup_content().members());
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("the real product must finish its runtime tick and cleanup");
    };
    assert_outcome(&outcome, expected);
    outcome
}

pub fn assert_v1_saved_product(
    root: &Path,
    profile: Option<&str>,
    expected_scene_roots: &[&str],
    expected: &[EnemyStatsSample],
) -> EnemyStatsOutcome {
    assert_saved_product(root, profile, v1_recipe(), expected_scene_roots, expected)
}

/// Checks and executes already-published sources; expected samples must use literal test oracles.
pub fn assert_v2_saved_product(
    root: &Path,
    profile: Option<&str>,
    expected_scene_roots: &[&str],
    expected: &[EnemyStatsSample],
) -> EnemyStatsOutcome {
    assert_saved_product(root, profile, v2_recipe(), expected_scene_roots, expected)
}

pub fn assert_outcome(outcome: &EnemyStatsOutcome, expected: &[EnemyStatsSample]) {
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(outcome.activations, 1);
    assert_eq!(outcome.ticks, 1);
    assert_eq!(outcome.samples, expected);
    assert_eq!(
        outcome.health_total,
        expected.iter().map(|sample| sample.health).sum::<i64>(),
    );
}

fn assert_diagnostic(diagnostics: &DiagnosticReport, code: &str) {
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "expected {code}: {diagnostics:?}",
    );
}

/// Both product entries must reject these same v2 files before publishing a startup closure.
pub fn assert_v1_refuses_saved_v2(root: &Path, profile: Option<&str>) {
    let request = ProjectCheckRequest::new(
        open_explicit_product_project_root(root).unwrap(),
        v1_recipe(),
    );
    let request = match profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };
    let failure = check_project(request).unwrap_err();
    assert_diagnostic(
        failure.content_error().unwrap().diagnostics(),
        AGREEMENT_REFUSAL,
    );
    let report = run(root, profile, v1_recipe());
    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_none());
    assert_diagnostic(report.diagnostics(), AGREEMENT_REFUSAL);
}

/// A surviving unavailable record is still a runtime refusal after successful authoring upgrade.
/// Missing native owners or a divergent bundle cannot satisfy this content refusal.
pub fn assert_v2_unavailable_refusal(root: &Path, profile: Option<&str>) {
    let request = ProjectCheckRequest::new(
        open_explicit_product_project_root(root).unwrap(),
        v2_recipe(),
    );
    let request = match profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };
    let failure = check_project(request).unwrap_err();
    assert_diagnostic(
        failure.content_error().unwrap().diagnostics(),
        "scene.unknown-component",
    );
    let report = run(root, profile, v2_recipe());
    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_none());
    assert_diagnostic(report.diagnostics(), "scene.unknown-component");
}
