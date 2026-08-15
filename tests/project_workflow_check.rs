#![cfg(feature = "project-workflow")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    ProductRecipe,
    project::ProjectPath,
    project_host::ProjectContentErrorKind,
    project_workflow::{
        ProjectCheckErrorKind, ProjectCheckRequest, ProjectStartupSceneRepairError,
        ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairRequest,
        StartupContentMemberKind, check_project, repair_startup_scene,
    },
};
use project_content_fixture::TestProject;

#[test]
fn public_check_returns_the_exact_loader_owned_startup_closure() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let root_identity = root.identity();

    let receipt = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap();

    assert_eq!(receipt.project_root_identity(), root_identity);
    assert_eq!(receipt.manifest_digest(), candidate.manifest_digest());
    assert_eq!(receipt.settings_lineage(), candidate.lineage());
    assert_eq!(
        receipt.schema_fingerprint(),
        plan.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        receipt
            .members()
            .iter()
            .map(|member| (member.path().as_str(), member.kind()))
            .collect::<Vec<_>>(),
        vec![
            (
                "assets/textures/player.png",
                StartupContentMemberKind::AssetSource,
            ),
            (
                "assets/textures/player.png.meta",
                StartupContentMemberKind::AssetMetadata,
            ),
            ("nara.toml", StartupContentMemberKind::ProjectManifest),
            (
                "prefabs/enemy.prefab.json",
                StartupContentMemberKind::Prefab,
            ),
            ("scenes/startup.scene.json", StartupContentMemberKind::Scene,),
        ]
    );
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn typed_missing_startup_scene_issue_drives_one_explicit_repair() {
    let project = TestProject::with_prefab_startup();
    fs::rename(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/recovered.scene.json"),
    )
    .unwrap();

    let error = check_project(ProjectCheckRequest::new(
        project.writable_root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();

    assert_eq!(error.kind(), ProjectCheckErrorKind::StartupContent);
    assert_eq!(
        error.startup_content_error().unwrap().kind(),
        ProjectContentErrorKind::MissingStartupScene
    );
    assert!(error.manifest_error().is_none());
    assert!(error.product_composition_error().is_none());
    let issue = error.into_startup_scene_issue().unwrap();
    assert_eq!(issue.selected_scene().as_str(), "startup.scene.json");

    let outcome = repair_startup_scene(ProjectStartupSceneRepairRequest::new(
        issue,
        ProjectPath::new("recovered.scene.json").unwrap(),
    ))
    .unwrap();
    let ProjectStartupSceneRepairOutcome::Repaired(receipt) = outcome else {
        panic!("expected a repaired project: {outcome:?}");
    };

    assert_eq!(
        receipt.publication().replacement().as_str(),
        "recovered.scene.json"
    );
    assert_eq!(
        receipt.startup_content().manifest_digest(),
        receipt.publication().manifest_digest()
    );
    let manifest = fs::read(project.path().join("nara.toml")).unwrap();
    let load = nara::project::ProjectManifest::parse_toml_bytes(&manifest);
    assert!(!load.has_errors());
    assert_eq!(
        load.manifest
            .unwrap()
            .resolve_profile(None)
            .unwrap()
            .startup
            .default_scene
            .unwrap()
            .as_str(),
        "recovered.scene.json"
    );
}

#[test]
fn repair_rejects_a_stale_missing_scene_issue_before_publication() {
    let project = TestProject::with_prefab_startup();
    fs::rename(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/recovered.scene.json"),
    )
    .unwrap();
    let error = check_project(ProjectCheckRequest::new(
        project.writable_root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();
    let issue = error.into_startup_scene_issue().unwrap();
    let manifest = project.path().join("nara.toml");
    let mut changed = fs::read_to_string(&manifest).unwrap();
    changed.push_str("\n# changed after issue\n");
    fs::write(&manifest, &changed).unwrap();

    let error = repair_startup_scene(ProjectStartupSceneRepairRequest::new(
        issue,
        ProjectPath::new("recovered.scene.json").unwrap(),
    ))
    .unwrap_err();

    assert!(matches!(error, ProjectStartupSceneRepairError::StaleIssue));
    assert_eq!(fs::read_to_string(manifest).unwrap(), changed);
}
