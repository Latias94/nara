#![cfg(feature = "project-workflow")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    ProductRecipe,
    project_host::ProjectContentErrorKind,
    project_workflow::{
        ProjectCheckErrorKind, ProjectCheckRequest, StartupContentMemberKind, check_project,
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

#[test]
fn public_check_classifies_missing_startup_content_without_publishing_a_receipt() {
    let project = TestProject::with_prefab_startup();
    fs::remove_file(project.path().join("scenes/startup.scene.json")).unwrap();

    let error = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();

    assert_eq!(error.kind(), ProjectCheckErrorKind::StartupContent);
    assert_eq!(
        error.startup_content_error().unwrap().kind(),
        ProjectContentErrorKind::HostIo
    );
    assert!(error.manifest_error().is_none());
    assert!(error.product_composition_error().is_none());
}
