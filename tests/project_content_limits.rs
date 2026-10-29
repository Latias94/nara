#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use nara::{
    asset::AssetRef,
    core::ByteLimit,
    diagnostic::DiagnosticValueRef,
    project_host::{
        ProjectContentBudgetHost, ProjectContentBudgetKind, ProjectContentBudgetSnapshot,
        ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId},
    scene::{
        PrefabDocument, PrefabExpansionLimits, PrefabInstance, SceneComponentRecord, SceneDocument,
        SceneEntityRecord,
    },
};
use project_content_fixture::{TestProject, image_meta, scene_id, sprite_value, valid_png_bytes};

#[test]
fn every_content_budget_accepts_its_peak_and_rejects_one_less_without_leaking() {
    // Struct layout and canonical encodings can change. The contract is bounded admission,
    // atomic rejection and complete release, rather than a particular allocation/job count.
    for kind in ProjectContentBudgetKind::ALL.iter().copied() {
        let required = successful_high_water(kind, |_| {});
        assert!(required > 0, "the fixture must exercise {kind:?}");
        assert_budget_boundary(kind, required);
    }
}

fn assert_budget_boundary(kind: ProjectContentBudgetKind, expected: usize) {
    let observed = successful_high_water(kind, |_| {});
    assert_eq!(observed, expected, "the {kind:?} budget model drifted");

    let exact = fixture_limits().with_limit(kind, expected).unwrap();
    let exact_observed = successful_high_water_with_limits(kind, exact, |_| {});
    assert_eq!(
        exact_observed, expected,
        "exact {kind:?} high water drifted"
    );

    if expected == 1 {
        return;
    }
    let rejected = fixture_limits().with_limit(kind, expected - 1).unwrap();
    let (project, loader, result) = load_with_limits(rejected, |_| {});
    let error = result.expect_err(&format!("limit+1 {kind:?} input must reject"));
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
fn spare_compiler_work_reservation_shrinks_without_exceeding_the_host_limit() {
    for kind in [
        ProjectContentBudgetKind::WorkBytes,
        ProjectContentBudgetKind::AggregateBytes,
    ] {
        let peak = successful_high_water_with_limits(kind, ProjectContentLimits::default(), |_| {});
        let limit = peak - 1;
        let limits = ProjectContentLimits::default()
            .with_limit(kind, limit)
            .unwrap();
        let (project, loader, result) = load_with_limits(limits, |_| {});
        let snapshot = result.unwrap();
        assert!(loader.budget_snapshot().high_water(kind) <= limit);
        assert_eq!(snapshot.prefabs().len(), 1);
        assert_eq!(snapshot.images().len(), 1);
        drop(snapshot);
        assert_no_active_charges(loader.budget_snapshot());
        drop(loader);
        drop(project);
    }
}

#[test]
fn synchronous_prefab_loading_does_not_add_queued_jobs() {
    let single = successful_high_water(ProjectContentBudgetKind::QueuedJobs, |_| {});
    let exact = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::QueuedJobs, single)
        .unwrap();
    let (project, loader, result) = load_with_limits(exact, two_prefabs);
    let snapshot = result.unwrap();
    assert_eq!(snapshot.prefabs().len(), 2);
    assert_eq!(snapshot.images().len(), 1);
    let budget = loader.budget_snapshot();
    // Prefabs are requested synchronously; only the shared image occupies the queue.
    assert_eq!(single, 1);
    assert_eq!(
        budget.high_water(ProjectContentBudgetKind::QueuedJobs),
        single
    );
    assert_eq!(budget.high_water(ProjectContentBudgetKind::InFlightJobs), 1);
    drop(snapshot);
    assert_no_active_charges(loader.budget_snapshot());
    drop(loader);
    drop(project);
}

#[test]
fn queued_jobs_budget_counts_unique_images_before_processing() {
    let observed = successful_high_water(ProjectContentBudgetKind::QueuedJobs, two_images);
    assert_eq!(observed, 2);

    let exact = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::QueuedJobs, observed)
        .unwrap();
    assert_eq!(
        successful_high_water_with_limits(ProjectContentBudgetKind::QueuedJobs, exact, two_images),
        observed,
    );

    let rejected = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::QueuedJobs, observed - 1)
        .unwrap();
    let (project, loader, result) = load_with_limits(rejected, two_images);
    let error = result.expect_err("the second unique image must exceed the queue limit");
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.budget-exceeded"
            && diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "budget"
                    && field.value() == DiagnosticValueRef::Identifier("queued-jobs")
            })
    }));
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
    assert_eq!(budget_host.snapshot().active_reservations(), 2);
    drop(snapshot);
    assert_no_active_charges(budget_host.snapshot());
}

#[test]
fn shared_budget_host_contention_rejects_without_retry_or_partial_publication() {
    let aggregate = successful_high_water(ProjectContentBudgetKind::AggregateBytes, |_| {});
    let limits = fixture_limits()
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
    assert_eq!(budget_host.snapshot().active_reservations(), 2);

    drop(first);
    assert_no_active_charges(budget_host.snapshot());
    let retried_after_release = second_loader.load(&second_candidate, &second_plan).unwrap();
    drop(retried_after_release);
    assert_no_active_charges(budget_host.snapshot());
}

#[test]
fn shared_budget_host_retains_at_most_two_distinct_content_generations() {
    let limits = ProjectContentLimits::default();
    let budget_host = ProjectContentBudgetHost::new(limits);
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::with_budget_host(root, limits, budget_host.clone()).unwrap();

    let first = loader.load(&candidate, &plan).unwrap();
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy-second")),
    ]));
    let second = loader.load(&candidate, &plan).unwrap();
    assert_ne!(first.revision(), second.revision());
    assert_eq!(budget_host.snapshot().active_generations(), 2);

    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy-third")),
    ]));
    let rejected = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        rejected.kind(),
        ProjectContentErrorKind::GenerationCapacityExceeded
    );
    assert_eq!(budget_host.snapshot().active_generations(), 2);

    drop(first);
    let third = loader.load(&candidate, &plan).unwrap();
    assert_ne!(second.revision(), third.revision());
    assert_eq!(budget_host.snapshot().active_generations(), 2);
    assert_eq!(budget_host.snapshot().generation_high_water(), 2);

    drop(second);
    drop(third);
    assert_no_active_charges(budget_host.snapshot());
}

#[test]
fn escaped_scene_and_image_keep_their_generation_claim_until_the_last_owner_retires() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let first = loader.load(&candidate, &plan).unwrap();
    let first_revision = first.revision();
    let source = first.scene_source();
    let image = first.images()[0].image().share_retained().unwrap();
    let pixels = image.pixels().to_vec();
    let shared_image = image.share_retained().unwrap();
    drop(first);
    assert_eq!(loader.budget_snapshot().active_generations(), 1);
    assert_eq!(loader.budget_snapshot().active_reservations(), 1);

    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy-second")),
    ]));
    let second = loader.load(&candidate, &plan).unwrap();
    let second_revision = second.revision();
    assert_ne!(second_revision, first_revision);
    assert_eq!(loader.budget_snapshot().active_generations(), 2);

    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy-third")),
    ]));
    let before_refusal = loader.budget_snapshot();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::GenerationCapacityExceeded
    );
    assert_active_charges_eq(loader.budget_snapshot(), before_refusal);
    assert_eq!(second.revision(), second_revision);
    assert_eq!(image.pixels(), pixels);

    drop(source);
    drop(image);
    // A shared pixel allocation still owns A even after its Snapshot and Scene source retire.
    assert_eq!(loader.budget_snapshot().active_generations(), 2);
    assert_eq!(shared_image.pixels(), pixels);
    let before_refusal = loader.budget_snapshot();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::GenerationCapacityExceeded
    );
    assert_active_charges_eq(loader.budget_snapshot(), before_refusal);

    drop(shared_image);
    assert_eq!(loader.budget_snapshot().active_generations(), 1);
    let third = loader.load(&candidate, &plan).unwrap();
    assert_ne!(third.revision(), second_revision);
    assert_eq!(loader.budget_snapshot().active_generations(), 2);
    assert_eq!(loader.budget_snapshot().generation_high_water(), 2);
    drop((second, third));
    assert_no_active_charges(loader.budget_snapshot());
}

#[test]
fn duplicate_dependency_edges_share_unique_prefab_and_image_work() {
    let single_edge_files = successful_high_water(ProjectContentBudgetKind::Files, |_| {});
    let project = TestProject::with_prefab_startup();
    duplicate_prefab_edges(&project);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();

    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    let budget = loader.budget_snapshot();
    assert_eq!(budget.high_water(ProjectContentBudgetKind::QueuedJobs), 1);
    // Source-kind discovery reads metadata before exact load-time revalidation. Duplicate
    // references must add no file work beyond the same closure with a single Prefab edge.
    assert_eq!(
        budget.high_water(ProjectContentBudgetKind::Files),
        single_edge_files,
    );
    assert_eq!(
        budget.high_water(ProjectContentBudgetKind::DependencyEdges),
        4,
    );

    drop(snapshot);
    assert_no_active_charges(loader.budget_snapshot());
}

fn successful_high_water(kind: ProjectContentBudgetKind, configure: fn(&TestProject)) -> usize {
    successful_high_water_with_limits(kind, fixture_limits(), configure)
}

fn fixture_limits() -> ProjectContentLimits {
    // The compiler can lower its spare work reservation to the shared Host's remaining capacity.
    // Keep this small fixture's expansion allowance below its fixed decode peak, so exact
    // high-water/one-byte-short assertions exercise required work rather than spare capacity.
    ProjectContentLimits::default().with_prefab_expansion(
        PrefabExpansionLimits::default().with_working_bytes(ByteLimit::new(1024 * 1024).unwrap()),
    )
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
    assert_eq!(snapshot.active_generations(), 0);
    for kind in ProjectContentBudgetKind::ALL.iter().copied() {
        assert_eq!(snapshot.active(kind), 0, "{kind:?} charge leaked");
    }
}

fn assert_active_charges_eq(
    actual: ProjectContentBudgetSnapshot,
    expected: ProjectContentBudgetSnapshot,
) {
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    assert_eq!(actual.active_generations(), expected.active_generations());
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(
            actual.active(*kind),
            expected.active(*kind),
            "{kind:?} charge drifted"
        );
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
        overrides: None,
    });
    let mut bonus = SceneEntityRecord::new(scene_id("bonus-anchor"));
    bonus.prefab = Some(PrefabInstance {
        source: AssetRef::path("bonus.prefab.json").unwrap(),
        overrides: None,
    });
    let shared_image = SceneEntityRecord::new(scene_id("bonus"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            project_content_fixture::identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::path("textures/player.png").unwrap()),
            ),
        );
    project.write_prefab(
        "bonus.prefab.json",
        &PrefabDocument::from_ordered_roots([shared_image]),
    );
    project.write_scene_source(&SceneDocument::from_ordered_roots([enemy, bonus]));
}

fn two_images(project: &TestProject) {
    two_prefabs(project);
    let path = "textures/bonus.png";
    project.write_asset(
        path,
        valid_png_bytes(),
        &image_meta("39b646c5-9f42-43e6-8ea1-dca814d7c3a3", path),
    );
    let bonus = SceneEntityRecord::new(scene_id("bonus"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            project_content_fixture::identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::path(path).unwrap()),
            ),
        );
    project.write_prefab(
        "bonus.prefab.json",
        &PrefabDocument::from_ordered_roots([bonus]),
    );
}

fn duplicate_prefab_edges(project: &TestProject) {
    let mut left = SceneEntityRecord::new(scene_id("left"));
    left.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: None,
    });
    let mut right = SceneEntityRecord::new(scene_id("right"));
    right.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: None,
    });
    project.write_scene_source(&SceneDocument::from_ordered_roots([left, right]));
}
