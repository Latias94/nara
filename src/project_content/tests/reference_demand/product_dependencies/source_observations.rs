use super::*;

use crate::project_content::{ProjectSceneLoadAttempt, ProjectSourceObservations};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapFileCandidate,
    ActionMapLimits, InputBinding, KeyCode,
};

fn observed_paths(observations: &ProjectSourceObservations) -> Vec<&str> {
    observations
        .paths()
        .iter()
        .map(AssetSourcePath::as_str)
        .collect()
}

fn select_shader(project: &LazyContentProject, shader: &str) {
    let path = project.root.join("assets/surface.material");
    let mut source: ProductSource = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    source.shader = AssetPath::new(shader).unwrap();
    fs::write(path, serde_json::to_vec(&source).unwrap()).unwrap();
}

#[test]
fn changed_material_retains_missing_shader_after_failure_and_recovers_without_generation_custody() {
    let project = dependency_project();
    let owner = reference_owner();
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(&project, "surface.material", B, vec![], vec![]);
    fs::write(project.root.join("assets/a.wgsl"), SHADER).unwrap();
    select_shader(&project, "a.wgsl");
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();

    let initial = loader.load(&candidate, &plan).unwrap();
    assert!(initial.content_closure().members().iter().any(|member| {
        member.kind() == ProjectContentMemberKind::AssetSource
            && member.path().as_str() == "assets/a.wgsl"
    }));
    drop(initial);
    assert_loader_idle(&loader);

    select_shader(&project, "b.wgsl");
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    assert!(result.is_err());
    drop(result);
    assert!(observations.is_complete());
    assert_eq!(
        observations.project_root_identity(),
        loader.project_root().identity(),
    );
    let paths = observed_paths(&observations);
    for expected in [
        "nara.toml",
        "scenes/startup.scene.json",
        "assets/surface.material.meta",
        "assets/surface.material",
        "assets/b.wgsl",
    ] {
        assert!(paths.contains(&expected), "missing observation: {expected}");
    }
    assert!(!paths.contains(&"assets/a.wgsl"));
    assert!(!paths.contains(&"b.wgsl"));
    let budget = loader.budget_snapshot();
    assert_eq!(budget.active_generations(), 0);
    assert_eq!(budget.active_reservations(), 1);
    for kind in [
        ProjectContentBudgetKind::OpenHandles,
        ProjectContentBudgetKind::EncodedBytes,
        ProjectContentBudgetKind::WorkBytes,
        ProjectContentBudgetKind::ArtifactBytes,
    ] {
        assert_eq!(budget.active(kind), 0, "unexpected retention: {kind:?}");
    }
    assert!(budget.active(ProjectContentBudgetKind::RetainedBytes) > 0);

    fs::write(project.root.join("assets/b.wgsl"), SHADER).unwrap();
    assert_eq!(observed_paths(&observations), paths);
    drop(observations);
    assert_loader_idle(&loader);

    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    let recovered = result.unwrap();
    assert!(observations.is_complete());
    assert!(observed_paths(&observations).contains(&"assets/b.wgsl"));
    assert_asset_sources(
        &project,
        recovered.content_closure().members(),
        &["assets/surface.material", "assets/b.wgsl"],
    );
    drop(recovered);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop(observations);
    assert_loader_idle(&loader);
}

#[test]
fn failed_default_image_source_and_metadata_opens_remain_in_the_attempt_cut() {
    for missing in ["texture.png", "texture.png.meta"] {
        let project = dependency_project();
        let owner = reference_owner();
        write_startup(&project, &owner);
        write_product::<MaterialDomain>(
            &project,
            "surface.material",
            B,
            vec![(
                AssetRef::path("texture.png").unwrap(),
                AssetSourceKind::Image,
                AssetReferenceDemand::Immediate,
            )],
            vec![],
        );
        write_image(&project, "texture.png", TEXTURE);
        fs::remove_file(project.root.join(format!("assets/{missing}"))).unwrap();
        let recipe = data_recipe(&owner)
            .add_asset_domain::<MaterialDomain>()
            .unwrap();
        let (candidate, plan, loader) = demand_inputs(&project, recipe);
        let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
        let ProjectSceneLoadAttempt {
            result,
            observations,
        } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
        assert!(result.is_err());
        drop(result);
        assert!(observations.is_complete());
        let expected = format!("assets/{missing}");
        assert!(observed_paths(&observations).contains(&expected.as_str()));
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        drop(observations);
        assert_loader_idle(&loader);
    }
}

#[test]
fn product_selected_scene_preserves_the_failed_read_cut_on_decode_failure() {
    let project = dependency_project();
    let owner = reference_owner();
    write_scene(
        &project,
        "scenes/startup.scene.json",
        C,
        &SceneDocument::default(),
    );
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        b"invalid scene",
    )
    .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_product_observed_reusing_input(&candidate, &plan, product(C), None);
    assert_eq!(
        result.unwrap_err().kind(),
        ProjectContentErrorKind::SceneFormat,
    );
    assert!(observations.is_complete());
    let paths = observed_paths(&observations);
    assert!(paths.contains(&"scenes/startup.scene.json.meta"));
    assert!(paths.contains(&"scenes/startup.scene.json"));
    assert!(!paths.contains(&"assets/startup.scene.json"));
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop(observations);
    assert_loader_idle(&loader);
}

#[test]
fn missing_nested_prefab_keeps_the_compilers_attempted_source_paths() {
    let project = LazyContentProject::new();
    fs::remove_file(project.root.join("prefabs/r.prefab.json")).unwrap();
    let (candidate, plan, loader) = project.inputs();
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    assert!(result.is_err());
    drop(result);
    assert!(observations.is_complete());
    let paths = observed_paths(&observations);
    for expected in [
        "scenes/startup.scene.json",
        "prefabs/p.prefab.json",
        "prefabs/r.prefab.json",
    ] {
        assert!(paths.contains(&expected), "missing observation: {expected}");
    }
    assert!(!paths.contains(&"prefabs/missing-q.prefab.json"));
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop(observations);
    assert_loader_idle(&loader);
}

#[test]
fn observed_loader_refuses_an_incomplete_cut_when_retained_capacity_is_exhausted() {
    let project = dependency_project();
    let (candidate, plan, original) = project.inputs();
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::RetainedBytes, 1)
        .unwrap();
    let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    assert_eq!(
        result.unwrap_err().kind(),
        ProjectContentErrorKind::BudgetExceeded,
    );
    assert!(!observations.is_complete());
    assert!(observations.paths().is_empty());
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop(observations);
    assert_loader_idle(&loader);
}

#[test]
fn observation_count_and_retained_capacity_refusals_are_explicit_and_release_on_drop() {
    let project = dependency_project();
    let (_, _, original) = project.inputs();
    for kind in [
        ProjectContentBudgetKind::Files,
        ProjectContentBudgetKind::RetainedBytes,
    ] {
        let limits = ProjectContentLimits::default().with_limit(kind, 1).unwrap();
        let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
        let mut observations = ProjectSourceObservations::new(&loader);
        if kind == ProjectContentBudgetKind::Files {
            observations.record(None, "nara.toml").unwrap();
        }
        let error = observations.record(None, "assets/b.wgsl").unwrap_err();
        assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
        assert!(!observations.is_complete());
        assert!(!observed_paths(&observations).contains(&"assets/b.wgsl"));
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        drop(observations);
        assert_loader_idle(&loader);
    }
}

#[test]
fn dependency_edge_refusal_retains_the_requested_raw_path_in_a_complete_cut() {
    let project = dependency_project();
    let owner = reference_owner();
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(&project, "surface.material", B, vec![], vec![]);
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, original) = demand_inputs(&project, recipe);
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::DependencyEdges, 1)
        .unwrap();
    let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    assert_eq!(
        result.unwrap_err().kind(),
        ProjectContentErrorKind::BudgetExceeded,
    );
    assert!(observations.is_complete());
    assert!(observed_paths(&observations).contains(&"assets/shared.wgsl"));
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    assert_eq!(
        loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::WorkBytes),
        0,
    );
    drop(observations);
    assert_loader_idle(&loader);
}

struct ReaderDomain<const READ_DEPENDENCY: bool>;

impl<const READ_DEPENDENCY: bool> AssetDomain for ReaderDomain<READ_DEPENDENCY> {
    type Product = ();
}

impl<const READ_DEPENDENCY: bool> SourceAssetImporter for ReaderDomain<READ_DEPENDENCY> {
    type Settings = ();

    const SOURCE_KIND: &'static str = "test.source-observations.reader";

    fn limits() -> AssetImportLimits {
        MaterialDomain::limits()
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        if READ_DEPENDENCY {
            let path = AssetPath::new("missing.wgsl").unwrap();
            assert!(
                source
                    .read_dependency(&path, ByteLimit::new(1024).unwrap())
                    .is_err()
            );
        }
        Ok(())
    }
}

#[test]
fn importer_handled_io_and_edge_errors_preserve_success_and_complete_observations() {
    for edge_limit in [None, Some(1)] {
        let project = dependency_project();
        let owner = reference_owner();
        write_startup(&project, &owner);
        write_product::<ReaderDomain<true>>(&project, "surface.material", B, vec![], vec![]);
        let recipe = data_recipe(&owner)
            .add_asset_domain::<ReaderDomain<true>>()
            .unwrap();
        let (candidate, plan, original) = demand_inputs(&project, recipe);
        let limits = match edge_limit {
            Some(limit) => ProjectContentLimits::default()
                .with_limit(ProjectContentBudgetKind::DependencyEdges, limit)
                .unwrap(),
            None => ProjectContentLimits::default(),
        };
        let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
        let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
        let ordinary = loader.load_scene(&candidate, &plan, &scene).unwrap();
        let closure = ordinary.content_closure();
        drop(ordinary);
        let ProjectSceneLoadAttempt {
            result,
            observations,
        } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
        let snapshot = result.unwrap();
        assert!(observations.is_complete());
        assert!(observed_paths(&observations).contains(&"assets/missing.wgsl"));
        assert_eq!(snapshot.content_closure().members(), closure.members());
        assert_eq!(snapshot.content_digest(), closure.content_digest());
        assert_asset_sources(
            &project,
            snapshot.content_closure().members(),
            &["assets/surface.material"],
        );
        assert!(
            !snapshot
                .content_closure()
                .members()
                .iter()
                .any(|member| { member.path().as_str() == "assets/missing.wgsl" })
        );
        drop((snapshot, closure));
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        drop(observations);
        assert_loader_idle(&loader);
    }
}

#[test]
fn importer_without_dependency_reads_does_not_fabricate_raw_observations() {
    let project = dependency_project();
    let owner = reference_owner();
    write_startup(&project, &owner);
    write_product::<ReaderDomain<false>>(&project, "surface.material", B, vec![], vec![]);
    let recipe = data_recipe(&owner)
        .add_asset_domain::<ReaderDomain<false>>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
    let ProjectSceneLoadAttempt {
        result,
        observations,
    } = loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, None);
    let snapshot = result.unwrap();
    assert!(observations.is_complete());
    let paths = observed_paths(&observations);
    assert!(paths.contains(&"assets/surface.material"));
    assert!(!paths.contains(&"assets/shared.wgsl"));
    assert!(!paths.contains(&"assets/missing.wgsl"));
    assert_asset_sources(
        &project,
        snapshot.content_closure().members(),
        &["assets/surface.material"],
    );
    drop((snapshot, observations));
    assert_loader_idle(&loader);
}

#[test]
fn long_root_observation_retention_accepts_exact_capacity_and_refuses_one_byte_short() {
    let project = dependency_project();
    let (_, _, original) = project.inputs();
    let source_root = nara_project::ProjectPath::new("a".repeat(100)).unwrap();
    let canonical = format!("{}/x", source_root.as_str());
    let overhead = crate::scene_retention::STRING_ALLOCATION_OVERHEAD;
    let prefix_bytes = std::mem::size_of::<AssetSourcePath>() + "nara.toml".len() + overhead;
    let required =
        prefix_bytes + std::mem::size_of::<AssetSourcePath>() + canonical.len() + overhead;

    for limit in [required, required - 1] {
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::RetainedBytes, limit)
            .unwrap();
        let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
        let mut observations = ProjectSourceObservations::new(&loader);
        observations.record(None, "nara.toml").unwrap();
        let result = observations.record(Some(&source_root), "x");
        if limit == required {
            result.unwrap();
            assert!(observations.is_complete());
            let recorded = observations
                .paths()
                .iter()
                .find(|path| path.as_str() == canonical)
                .unwrap();
            assert_eq!(recorded.retained_bytes(), canonical.len());
            assert_eq!(
                loader
                    .budget_snapshot()
                    .active(ProjectContentBudgetKind::RetainedBytes),
                required,
            );
            let mut pressure = loader.budget_host.reserve();
            pressure
                .set(
                    ProjectContentBudgetKind::WorkBytes,
                    limits.limit(ProjectContentBudgetKind::WorkBytes),
                )
                .unwrap();
            observations.record(Some(&source_root), "x").unwrap();
            observations.record(None, &canonical).unwrap();
            assert_eq!(observations.paths().len(), 2);
            assert_eq!(
                loader
                    .budget_snapshot()
                    .active(ProjectContentBudgetKind::Files),
                2,
            );
            drop(pressure);
        } else {
            assert_eq!(
                result.unwrap_err().kind(),
                ProjectContentErrorKind::BudgetExceeded,
            );
            assert!(!observations.is_complete());
            assert_eq!(observed_paths(&observations), ["nara.toml"]);
            assert_eq!(
                loader
                    .budget_snapshot()
                    .active(ProjectContentBudgetKind::Files),
                1,
            );
            assert_eq!(
                loader
                    .budget_snapshot()
                    .active(ProjectContentBudgetKind::RetainedBytes),
                prefix_bytes,
            );
        }
        assert_eq!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::WorkBytes),
            0,
        );
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        drop(observations);
        assert_loader_idle(&loader);
    }
}

#[test]
fn observation_allocation_failure_refunds_new_path_retains_prefix_and_refuses_candidate() {
    let project = dependency_project();
    let (candidate, plan, original) = project.inputs();
    let snapshot = original.load(&candidate, &plan).unwrap();
    let loader = ProjectContentLoader::new(original.project_root()).unwrap();
    let mut observations = ProjectSourceObservations::new(&loader);
    observations.record(None, "nara.toml").unwrap();
    let before = loader.budget_snapshot();
    observations.test_fail_next_path_allocation();
    observations.record(None, "nara.toml").unwrap();
    let error = observations.record(None, "assets/b.wgsl").unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AllocationFailed);
    assert!(!observations.is_complete());
    assert_eq!(observed_paths(&observations), ["nara.toml"]);
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(
            loader.budget_snapshot().active(kind),
            before.active(kind),
            "{kind:?}",
        );
    }
    assert_eq!(
        observations
            .record(None, "assets/c.wgsl")
            .unwrap_err()
            .kind(),
        error.kind(),
    );
    let attempt = ProjectSceneLoadAttempt::new(Ok(snapshot), observations);
    assert_eq!(
        attempt.result.unwrap_err().kind(),
        ProjectContentErrorKind::AllocationFailed,
    );
    assert_loader_idle(&original);
    assert_eq!(observed_paths(&attempt.observations), ["nara.toml"]);
    drop(attempt.observations);
    assert_loader_idle(&loader);
}

#[test]
fn observation_path_and_work_refusals_preserve_the_recorded_prefix() {
    let project = dependency_project();
    let (_, _, original) = project.inputs();
    for (path, work_limit, expected) in [
        (
            "../escape.wgsl",
            1024,
            ProjectContentErrorKind::InvalidLogicalPath,
        ),
        (
            "assets/over-budget.wgsl",
            32,
            ProjectContentErrorKind::BudgetExceeded,
        ),
    ] {
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::WorkBytes, work_limit)
            .unwrap();
        let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
        let mut observations = ProjectSourceObservations::new(&loader);
        observations.record(None, "nara.toml").unwrap();
        let before = loader.budget_snapshot();
        assert_eq!(
            observations.record(None, path).unwrap_err().kind(),
            expected
        );
        assert!(!observations.is_complete());
        assert_eq!(observed_paths(&observations), ["nara.toml"]);
        for &kind in ProjectContentBudgetKind::ALL {
            assert_eq!(
                loader.budget_snapshot().active(kind),
                before.active(kind),
                "{kind:?}",
            );
        }
        drop(observations);
        assert_loader_idle(&loader);
    }
}

#[test]
fn observed_path_and_product_loads_match_ordinary_closure_and_reuse_frozen_input() {
    let project = dependency_project();
    let owner = reference_owner();
    write_scene(
        &project,
        "scenes/startup.scene.json",
        C,
        &document(&owner, &[(IMMEDIATE, AssetRef::Product(product(B)))]),
    );
    write_product::<MaterialDomain>(&project, "surface.material", B, vec![], vec![]);
    let encode_input = |key| {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(ActionId::new("jump").unwrap(), key))
            .unwrap();
        ActionMapFileCandidate::new(product(AUXILIARY), builder.compile().unwrap())
            .unwrap()
            .to_json_string()
            .unwrap()
    };
    let input_path = project.root.join("game.actions.json");
    let input_bytes = encode_input(KeyCode::Space);
    fs::write(&input_path, &input_bytes).unwrap();
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!("{manifest}\n[input]\naction_map = 'game.actions.json'\n"),
    )
    .unwrap();
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);
    let scene = nara_project::ProjectPath::new("startup.scene.json").unwrap();
    let ordinary = loader.load_scene(&candidate, &plan, &scene).unwrap();
    let input = ordinary.input_action_map().unwrap().clone();
    let closure = ordinary.content_closure();

    for by_product in [false, true] {
        let attempt = if by_product {
            loader.load_scene_product_observed_reusing_input(
                &candidate,
                &plan,
                product(C),
                Some(&input),
            )
        } else {
            loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, Some(&input))
        };
        let snapshot = attempt.result.unwrap();
        assert!(attempt.observations.is_complete());
        assert!(input.shares_owner_with(snapshot.input_action_map().unwrap()));
        assert_eq!(snapshot.content_digest(), ordinary.content_digest());
        assert_eq!(snapshot.revision(), ordinary.revision());
        assert_eq!(snapshot.content_closure().members(), closure.members());
        assert_eq!(
            snapshot.content_closure().closure_plan_digest(),
            closure.closure_plan_digest(),
        );
        let paths = observed_paths(&attempt.observations);
        assert!(paths.contains(&"game.actions.json"));
        assert!(paths.contains(&"game.actions.json.meta"));
        assert!(!paths.contains(&"assets/game.actions.json"));
        drop((snapshot, attempt.observations));
    }

    fs::write(&input_path, encode_input(KeyCode::Enter)).unwrap();
    let before = loader.budget_snapshot();
    for by_product in [false, true] {
        let attempt = if by_product {
            loader.load_scene_product_observed_reusing_input(
                &candidate,
                &plan,
                product(C),
                Some(&input),
            )
        } else {
            loader.load_scene_observed_reusing_input(&candidate, &plan, &scene, Some(&input))
        };
        let error = attempt.result.unwrap_err();
        assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
        assert!(error.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.content.global-input-source-changed"
        }));
        assert!(attempt.observations.is_complete());
        assert_eq!(
            observed_paths(&attempt.observations),
            ["game.actions.json", "nara.toml"],
        );
        assert_eq!(
            input.source_digest(),
            ContentDigest::of_bytes(input_bytes.as_bytes()),
        );
        assert_eq!(
            input.definition().bindings()[0].input,
            InputBinding::Key(KeyCode::Space),
        );
        drop(attempt.observations);
        for &kind in ProjectContentBudgetKind::ALL {
            assert_eq!(
                loader.budget_snapshot().active(kind),
                before.active(kind),
                "{kind:?}",
            );
        }
    }
    drop((ordinary, input, closure));
    assert_loader_idle(&loader);
}
