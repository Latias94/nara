#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use nara::{
    asset::AssetRef,
    project_host::{
        ProjectContentBudgetHost, ProjectContentBudgetKind, ProjectContentBudgetSnapshot,
        ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
    },
    scene::{PrefabDocument, PrefabInstance, SceneDocument, SceneEntityRecord, ScenePatchDocument},
};
use project_content_fixture::{TestProject, scene_id};

const PORTABLE_FIXTURE_HIGH_WATER: [(ProjectContentBudgetKind, usize); 10] = [
    (ProjectContentBudgetKind::DirectoryDepth, 2),
    (ProjectContentBudgetKind::DirectoryEntries, 12),
    (ProjectContentBudgetKind::PathBytes, 133),
    (ProjectContentBudgetKind::OpenHandles, 6),
    (ProjectContentBudgetKind::Files, 4),
    (ProjectContentBudgetKind::QueuedJobs, 1),
    (ProjectContentBudgetKind::InFlightJobs, 1),
    (ProjectContentBudgetKind::DependencyEdges, 2),
    (ProjectContentBudgetKind::EncodedBytes, 16_780_307),
    (ProjectContentBudgetKind::ArtifactBytes, 4),
];

// These three modeled allocation values depend on pointer width.
#[cfg(target_pointer_width = "64")]
const LAYOUT_FIXTURE_HIGH_WATER: [(ProjectContentBudgetKind, usize); 3] = [
    (ProjectContentBudgetKind::WorkBytes, 393_075_070),
    (ProjectContentBudgetKind::RetainedBytes, 3_944),
    (ProjectContentBudgetKind::AggregateBytes, 393_078_159),
];

#[test]
fn portable_loader_budgets_match_the_independent_fixture_oracle() {
    for (kind, expected) in PORTABLE_FIXTURE_HIGH_WATER {
        assert_budget_boundary(kind, expected);
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn layout_dependent_loader_budgets_match_the_64_bit_fixture_oracle() {
    for (kind, expected) in LAYOUT_FIXTURE_HIGH_WATER {
        assert_budget_boundary(kind, expected);
    }
}

fn assert_budget_boundary(kind: ProjectContentBudgetKind, expected: usize) {
    let observed = successful_high_water(kind, |_| {});
    assert_eq!(observed, expected, "the {kind:?} budget model drifted");

    let exact = ProjectContentLimits::default()
        .with_limit(kind, expected)
        .unwrap();
    let exact_observed = successful_high_water_with_limits(kind, exact, |_| {});
    assert_eq!(
        exact_observed, expected,
        "exact {kind:?} high water drifted"
    );

    if expected == 1 {
        return;
    }
    let rejected = ProjectContentLimits::default()
        .with_limit(kind, expected - 1)
        .unwrap();
    let (project, loader, result) = load_with_limits(rejected, |_| {});
    let error = result.expect_err("limit+1 input must reject");
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "requested")
    }));
    assert_no_active_charges(loader.budget_snapshot());
    drop(loader);
    drop(project);
}

#[test]
fn queued_jobs_budget_counts_materialized_unique_work_before_processing() {
    let observed = successful_high_water(ProjectContentBudgetKind::QueuedJobs, two_prefabs);
    assert_eq!(observed, 2);

    let exact = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::QueuedJobs, observed)
        .unwrap();
    assert_eq!(
        successful_high_water_with_limits(ProjectContentBudgetKind::QueuedJobs, exact, two_prefabs,),
        observed,
    );

    let rejected = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::QueuedJobs, observed - 1)
        .unwrap();
    let (project, loader, result) = load_with_limits(rejected, two_prefabs);
    let error = result.expect_err("the second materialized job must reject");
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_no_active_charges(loader.budget_snapshot());
    drop(loader);
    drop(project);
}

#[test]
fn failed_load_releases_every_charge_and_the_same_host_accepts_later_work() {
    let limits = ProjectContentLimits::default();
    let budget_host = ProjectContentBudgetHost::new(limits);
    let hostile = TestProject::with_prefab_startup();
    hostile.write_scene_bytes(br#"{"kind":"scene","format_version":1,"payload":[]}"#);
    let (hostile_candidate, hostile_plan, hostile_root) = hostile.candidate_plan_and_root();
    let hostile_loader =
        ProjectContentLoader::with_budget_host(hostile_root, limits, budget_host.clone()).unwrap();

    let error = hostile_loader
        .load(&hostile_candidate, &hostile_plan)
        .unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::SceneFormat);
    assert_no_active_charges(budget_host.snapshot());

    let valid = TestProject::with_prefab_startup();
    let (valid_candidate, valid_plan, valid_root) = valid.candidate_plan_and_root();
    let valid_loader =
        ProjectContentLoader::with_budget_host(valid_root, limits, budget_host.clone()).unwrap();
    let snapshot = valid_loader.load(&valid_candidate, &valid_plan).unwrap();
    assert_eq!(budget_host.snapshot().active_reservations(), 1);
    drop(snapshot);
    assert_no_active_charges(budget_host.snapshot());
}

#[test]
fn shared_budget_host_contention_rejects_without_retry_or_partial_publication() {
    let aggregate = successful_high_water(ProjectContentBudgetKind::AggregateBytes, |_| {});
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::AggregateBytes, aggregate)
        .unwrap();
    let budget_host = ProjectContentBudgetHost::new(limits);
    let first_project = TestProject::with_prefab_startup();
    let (first_candidate, first_plan, first_root) = first_project.candidate_plan_and_root();
    let first_loader =
        ProjectContentLoader::with_budget_host(first_root, limits, budget_host.clone()).unwrap();
    let second_project = TestProject::with_prefab_startup();
    let (second_candidate, second_plan, second_root) = second_project.candidate_plan_and_root();
    let second_loader =
        ProjectContentLoader::with_budget_host(second_root, limits, budget_host.clone()).unwrap();

    let first = first_loader.load(&first_candidate, &first_plan).unwrap();
    let first_revision = first.revision();
    let error = second_loader
        .load(&second_candidate, &second_plan)
        .unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_eq!(first.revision(), first_revision);
    assert_eq!(budget_host.snapshot().active_reservations(), 1);

    drop(first);
    assert_no_active_charges(budget_host.snapshot());
    let retried_after_release = second_loader.load(&second_candidate, &second_plan).unwrap();
    drop(retried_after_release);
    assert_no_active_charges(budget_host.snapshot());
}

#[test]
fn duplicate_dependency_edges_share_unique_prefab_and_image_work() {
    let project = TestProject::with_prefab_startup();
    duplicate_prefab_edges(&project);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();

    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    let budget = loader.budget_snapshot();
    assert_eq!(budget.high_water(ProjectContentBudgetKind::QueuedJobs), 1);
    assert_eq!(budget.high_water(ProjectContentBudgetKind::Files), 4);
    assert_eq!(
        budget.high_water(ProjectContentBudgetKind::DependencyEdges),
        4,
    );

    drop(snapshot);
    assert_no_active_charges(loader.budget_snapshot());
}

fn successful_high_water(kind: ProjectContentBudgetKind, configure: fn(&TestProject)) -> usize {
    successful_high_water_with_limits(kind, ProjectContentLimits::default(), configure)
}

fn successful_high_water_with_limits(
    kind: ProjectContentBudgetKind,
    limits: ProjectContentLimits,
    configure: fn(&TestProject),
) -> usize {
    let (project, loader, result) = load_with_limits(limits, configure);
    let snapshot = result.unwrap();
    let observed = loader.budget_snapshot().high_water(kind);
    drop(snapshot);
    assert_no_active_charges(loader.budget_snapshot());
    drop(loader);
    drop(project);
    observed
}

fn assert_no_active_charges(snapshot: ProjectContentBudgetSnapshot) {
    assert_eq!(snapshot.active_reservations(), 0);
    for kind in ProjectContentBudgetKind::ALL.iter().copied() {
        assert_eq!(snapshot.active(kind), 0, "{kind:?} charge leaked");
    }
}

fn load_with_limits(
    limits: ProjectContentLimits,
    configure: fn(&TestProject),
) -> (
    TestProject,
    ProjectContentLoader,
    Result<nara::project_host::ProjectContentSnapshot, nara::project_host::ProjectContentError>,
) {
    let project = TestProject::with_prefab_startup();
    configure(&project);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::with_limits(root, limits).unwrap();
    let result = loader.load(&candidate, &plan);
    (project, loader, result)
}

fn two_prefabs(project: &TestProject) {
    let mut enemy = SceneEntityRecord::new(scene_id("enemy-anchor"));
    enemy.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    let mut bonus = SceneEntityRecord::new(scene_id("bonus-anchor"));
    bonus.prefab = Some(PrefabInstance {
        source: AssetRef::path("bonus.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    project.write_prefab("bonus.prefab.json", &PrefabDocument::default());
    project.write_scene_source(&SceneDocument::new([enemy, bonus]));
}

fn duplicate_prefab_edges(project: &TestProject) {
    let mut left = SceneEntityRecord::new(scene_id("left"));
    left.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    let mut right = SceneEntityRecord::new(scene_id("right"));
    right.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    project.write_scene_source(&SceneDocument::new([left, right]));
}
