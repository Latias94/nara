#![cfg(feature = "project-workflow")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    ProductRecipe,
    project::ProjectPath,
    project_host::{ProjectContentErrorKind, ProjectContentLoader},
    project_workflow::{
        ProjectCheckErrorKind, ProjectCheckRequest, ProjectContentMemberKind,
        ProjectStartupSceneRepairError, ProjectStartupSceneRepairOutcome,
        ProjectStartupSceneRepairRequest, check_project, repair_startup_scene,
    },
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};
use project_content_fixture::{TestProject, image_meta, scene_id, valid_png_bytes};

#[test]
fn public_check_returns_the_exact_loader_owned_startup_closure() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let root_identity = root.identity();
    let raw_snapshot = ProjectContentLoader::new(root.share())
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();

    let receipt = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap();

    assert_eq!(receipt.project_root_identity(), root_identity);
    assert_eq!(receipt.manifest_digest(), candidate.manifest_digest());
    assert_eq!(receipt.settings_lineage(), candidate.lineage());
    assert_eq!(
        receipt.startup_content().content_digest(),
        raw_snapshot.content_digest()
    );
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
                ProjectContentMemberKind::AssetSource,
            ),
            (
                "assets/textures/player.png.meta",
                ProjectContentMemberKind::AssetMetadata,
            ),
            ("nara.toml", ProjectContentMemberKind::ProjectManifest),
            (
                "prefabs/enemy.prefab.json",
                ProjectContentMemberKind::Prefab,
            ),
            ("scenes/startup.scene.json", ProjectContentMemberKind::Scene,),
            (
                "schema/authoring.bundle.json",
                ProjectContentMemberKind::AuthoringSchemaBundle,
            ),
        ]
    );
}

#[test]
fn unreferenced_import_does_not_enter_the_checked_startup_closure() {
    let project = TestProject::with_prefab_startup();
    let baseline = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap();

    project.write_asset(
        "textures/unreferenced.png",
        valid_png_bytes(),
        &image_meta(
            "1f112c40-b33b-44aa-97c0-198b51e10c58",
            "textures/unreferenced.png",
        ),
    );

    let successor = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap();

    assert_eq!(successor.manifest_digest(), baseline.manifest_digest());
    assert_eq!(
        successor.startup_content().content_digest(),
        baseline.startup_content().content_digest()
    );
    assert_eq!(
        successor.startup_content().closure_plan_digest(),
        baseline.startup_content().closure_plan_digest()
    );
    assert_eq!(
        successor.schema_fingerprint(),
        baseline.schema_fingerprint()
    );
    assert_eq!(successor.members(), baseline.members());
    assert!(successor.members().iter().all(|member| {
        !member
            .path()
            .as_str()
            .starts_with("assets/textures/unreferenced.png")
    }));
}

#[test]
fn public_check_rejects_a_bundle_missing_one_selected_native_owner() {
    let project = TestProject::with_prefab_startup();
    let (_candidate, plan, root) = project.candidate_plan_and_root();
    let selected_owner = plan
        .schema_validation()
        .owner_receipts()
        .next()
        .unwrap()
        .owner();
    let bundle_path = project.path().join("schema/authoring.bundle.json");
    let encoded = fs::read(&bundle_path).unwrap();
    let bundle = AuthoringSchemaBundle::from_json_bytes(&encoded).unwrap();
    let retained_owners = bundle
        .owners()
        .iter()
        .filter(|owner| !owner.owner_id().matches_executable(selected_owner).unwrap())
        .cloned()
        .collect::<Vec<AuthoringSchemaBundleOwner>>();
    let divergent = AuthoringSchemaBundle::new(retained_owners).unwrap();
    fs::write(bundle_path, divergent.to_json_string().unwrap()).unwrap();

    let error = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap_err();

    assert_eq!(error.kind(), ProjectCheckErrorKind::Content);
    let startup_error = error.content_error().unwrap();
    assert_eq!(
        startup_error.kind(),
        ProjectContentErrorKind::SchemaMismatch
    );
    assert!(startup_error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.runtime-agreement.selected-owner-missing"
    }));
}

#[test]
fn runtime_agreement_accepts_a_bundle_superset_and_rejects_divergence_or_unbound_source() {
    let custom_owner = AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("studio.signal.components").unwrap(),
        ComponentSchemaCatalog {
            components: vec![ComponentSchema::new(
                ComponentTypeId::new("studio.signal.Pulse"),
                "Pulse",
                ComponentSchemaVersion::ONE,
            )],
            ..ComponentSchemaCatalog::default()
        },
        None,
    );

    let superset = TestProject::with_prefab_startup();
    let superset_path = superset.path().join("schema/authoring.bundle.json");
    let superset_bundle =
        AuthoringSchemaBundle::from_json_bytes(&fs::read(&superset_path).unwrap()).unwrap();
    let mut superset_owners = superset_bundle.owners().to_vec();
    superset_owners.push(custom_owner.clone());
    fs::write(
        &superset_path,
        AuthoringSchemaBundle::new(superset_owners)
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    check_project(ProjectCheckRequest::new(
        superset.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap();

    let divergent = TestProject::with_prefab_startup();
    let divergent_path = divergent.path().join("schema/authoring.bundle.json");
    let divergent_bundle =
        AuthoringSchemaBundle::from_json_bytes(&fs::read(&divergent_path).unwrap()).unwrap();
    let scene_owner = PersistentComponentSchemaOwnerId::try_new(
        nara::scene::SCENE_COMPONENTS_SCHEMA_PROVIDER
            .owner()
            .as_str(),
    )
    .unwrap();
    let mut divergent_owners = divergent_bundle.owners().to_vec();
    let selected = divergent_owners
        .iter_mut()
        .find(|owner| owner.owner_id() == &scene_owner)
        .unwrap();
    let mut divergent_catalog = selected.current().clone();
    divergent_catalog
        .type_tombstones
        .push(ComponentTypeId::new("zz.runtime_agreement.Divergent"));
    *selected = AuthoringSchemaBundleOwner::new(
        scene_owner,
        divergent_catalog,
        selected.predecessor().cloned(),
    );
    fs::write(
        &divergent_path,
        AuthoringSchemaBundle::new(divergent_owners)
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let error = check_project(ProjectCheckRequest::new(
        divergent.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();
    assert!(
        error
            .content_error()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "project.runtime-agreement.selected-owner-divergent")
    );

    let unbound = TestProject::with_prefab_startup();
    let unbound_path = unbound.path().join("schema/authoring.bundle.json");
    let unbound_bundle =
        AuthoringSchemaBundle::from_json_bytes(&fs::read(&unbound_path).unwrap()).unwrap();
    let mut unbound_owners = unbound_bundle.owners().to_vec();
    unbound_owners.push(custom_owner);
    fs::write(
        &unbound_path,
        AuthoringSchemaBundle::new(unbound_owners)
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    unbound.write_scene_source(&SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("signal-node")).with_component(
            ComponentTypeId::new("studio.signal.Pulse"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::f64(0.75).unwrap(),
            ),
        ),
    ]));
    let error = check_project(ProjectCheckRequest::new(
        unbound.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();
    let codes = error
        .content_error()
        .unwrap()
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect::<Vec<_>>();
    assert!(
        codes.iter().any(|code| *code == "scene.unknown-component"),
        "unexpected diagnostics: {codes:?}"
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

    assert_eq!(error.kind(), ProjectCheckErrorKind::Content);
    assert_eq!(
        error.content_error().unwrap().kind(),
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
