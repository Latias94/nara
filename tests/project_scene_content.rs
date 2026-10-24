#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    asset::AssetRef,
    project::ProjectPath,
    project_host::{
        ProjectContentBudgetKind, ProjectContentBudgetSnapshot, ProjectContentErrorKind,
        ProjectContentLoader, ProjectContentMemberKind,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
    },
};
use project_content_fixture::{
    TestProject, identity_transform_record, image_meta, scene_id, sprite_value, valid_png_bytes,
};

#[cfg(windows)]
#[test]
fn native_source_admission_rejects_case_aliases_instead_of_minting_location_identity() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    assert!(
        loader
            .load_scene(
                &candidate,
                &plan,
                &ProjectPath::new("STARTUP.scene.json").unwrap(),
            )
            .is_err()
    );
    let before = loader.load(&candidate, &plan).unwrap();
    let mut aliased = before.scene().clone();
    aliased.entities[0].prefab.as_mut().unwrap().source =
        AssetRef::path("ENEMY.prefab.json").unwrap();
    project.write_scene_source(&aliased);
    assert!(loader.load(&candidate, &plan).is_err());
    assert_eq!(
        before.scene().entities[0].prefab.as_ref().unwrap().source,
        AssetRef::path("enemy.prefab.json").unwrap()
    );
    drop(before);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn selected_scene_loads_only_its_dependency_closure_without_retargeting_startup() {
    let project = TestProject::with_prefab_startup();
    let selected = ProjectPath::new("second.scene.json").unwrap();
    let image = AssetRef::path("textures/second.png").unwrap();
    project.write_asset(
        "textures/second.png",
        valid_png_bytes(),
        &image_meta(
            "2e78c2d1-82e6-40ee-96f2-8dcd28a6e3e1",
            "textures/second.png",
        ),
    );
    let child = SceneEntityRecord::new(scene_id("second-enemy"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
        );
    project.write_prefab(
        "second.prefab.json",
        &PrefabDocument::from_ordered_roots([child]),
    );
    let mut anchor = SceneEntityRecord::new(scene_id("second-anchor")).with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    );
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("second.prefab.json").unwrap(),
        overrides: Default::default(),
    });
    fs::write(
        project.path().join("scenes").join(selected.as_str()),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let manifest = fs::read(project.path().join("nara.toml")).unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    let second = loader.load_scene(&candidate, &plan, &selected).unwrap();

    assert_eq!(second.selected_scene_path(), &selected);
    assert_eq!(second.lineage(), startup.lineage());
    assert_eq!(second.schema_fingerprint(), startup.schema_fingerprint());
    assert_eq!(second.scene().roots, [scene_id("second-anchor")]);
    assert_eq!(second.expanded_scene().entities.len(), 2);
    assert_eq!(second.expanded_scene_provenance().len(), 2);
    assert!(second.expanded_scene_provenance().values().any(|source| {
        source.source_entity_id() == &scene_id("second-enemy")
            && source
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &scene_id("second-anchor"))
    }));
    assert_eq!(second.prefabs().len(), 1);
    assert_eq!(second.prefabs()[0].path().as_str(), "second.prefab.json");
    assert_eq!(second.images().len(), 1);
    assert_eq!(second.images()[0].path().as_str(), "textures/second.png");
    assert!(second.startup_content_closure().is_none());
    assert!(startup.startup_content_closure().is_some());
    let closure = second.content_closure();
    assert_eq!(closure.selected_scene_path(), &selected);
    assert_eq!(
        closure.project_root_identity(),
        startup.content_closure().project_root_identity()
    );
    for expected in [
        (ProjectContentMemberKind::Scene, "scenes/second.scene.json"),
        (
            ProjectContentMemberKind::Prefab,
            "prefabs/second.prefab.json",
        ),
        (
            ProjectContentMemberKind::AssetSource,
            "assets/textures/second.png",
        ),
        (
            ProjectContentMemberKind::AssetMetadata,
            "assets/textures/second.png.meta",
        ),
    ] {
        assert!(
            closure.members().iter().any(|member| {
                member.kind() == expected.0 && member.path().as_str() == expected.1
            })
        );
    }
    for absent in [
        "scenes/startup.scene.json",
        "prefabs/enemy.prefab.json",
        "assets/textures/player.png",
    ] {
        assert!(
            closure
                .members()
                .iter()
                .all(|member| member.path().as_str() != absent)
        );
    }
    assert_eq!(
        candidate
            .settings()
            .startup
            .default_scene
            .as_ref()
            .unwrap()
            .as_str(),
        "startup.scene.json"
    );
    assert_eq!(
        fs::read(project.path().join("nara.toml")).unwrap(),
        manifest
    );
}

#[test]
fn identical_scene_bytes_at_different_paths_have_distinct_admitted_identity() {
    let project = TestProject::with_prefab_startup();
    let startup_bytes = fs::read(project.path().join("scenes/startup.scene.json")).unwrap();
    fs::write(
        project.path().join("scenes/copy.scene.json"),
        &startup_bytes,
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    let copy_path = ProjectPath::new("copy.scene.json").unwrap();
    let copy = loader.load_scene(&candidate, &plan, &copy_path).unwrap();
    assert_eq!(startup.scene(), copy.scene());
    assert_ne!(startup.revision(), copy.revision());
    assert_ne!(
        startup.content_closure().closure_plan_digest(),
        copy.content_closure().closure_plan_digest()
    );
    let source_digest = |snapshot: &nara::project_host::ProjectContentSnapshot| {
        snapshot
            .content_closure()
            .members()
            .iter()
            .find(|member| member.kind() == ProjectContentMemberKind::Scene)
            .unwrap()
            .digest()
    };
    assert_eq!(source_digest(&startup), source_digest(&copy));
    assert!(copy.startup_content_closure().is_none());
    drop(copy);

    let explicit_default = loader
        .load_scene(
            &candidate,
            &plan,
            &ProjectPath::new("startup.scene.json").unwrap(),
        )
        .unwrap();
    assert_eq!(startup.revision(), explicit_default.revision());
    assert_eq!(startup.content_digest(), explicit_default.content_digest());
    assert_eq!(
        startup.content_closure().closure_plan_digest(),
        explicit_default.content_closure().closure_plan_digest()
    );
    let facts = explicit_default.content_closure();
    let attestation = explicit_default.startup_content_closure().unwrap();
    assert_eq!(facts.members().as_ptr(), attestation.members().as_ptr());
    assert_eq!(
        attestation.content_closure().selected_scene_path(),
        facts.selected_scene_path()
    );
}

#[test]
fn source_only_receipts_release_decoded_content_and_generation_capacity() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert!(
        loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::ArtifactBytes)
            > 0
    );
    let facts = snapshot.content_closure();
    let startup = snapshot.startup_content_closure().unwrap();
    let cloned = facts.clone();
    assert_eq!(loader.budget_snapshot().active_reservations(), 2);
    drop(snapshot);
    let retained = loader.budget_snapshot();
    assert_eq!(retained.active_generations(), 0);
    assert_eq!(retained.active_reservations(), 1);
    assert_eq!(retained.active(ProjectContentBudgetKind::ArtifactBytes), 0);
    assert!(retained.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(facts.members().as_ptr(), startup.members().as_ptr());
    drop(facts);
    drop(startup);
    assert_eq!(loader.budget_snapshot().active_reservations(), 1);
    drop(cloned);
    assert_no_active_charges(loader.budget_snapshot());
}

#[test]
fn rejected_selected_loads_preserve_the_existing_snapshot_and_budget() {
    let project = TestProject::with_prefab_startup();
    fs::write(
        project.path().join("scenes/invalid.scene.json"),
        b"not a Scene",
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let revision = snapshot.revision();
    let before = loader.budget_snapshot();
    for (path, kind) in [
        (
            "missing.scene.json",
            ProjectContentErrorKind::MissingSelectedScene,
        ),
        ("invalid.scene.json", ProjectContentErrorKind::SceneFormat),
    ] {
        let error = loader
            .load_scene(&candidate, &plan, &ProjectPath::new(path).unwrap())
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(snapshot.revision(), revision);
        assert!(snapshot.startup_content_closure().is_some());
        let after = loader.budget_snapshot();
        assert_eq!(after.active_reservations(), before.active_reservations());
        assert_eq!(after.active_generations(), before.active_generations());
        for kind in ProjectContentBudgetKind::ALL {
            assert_eq!(after.active(*kind), before.active(*kind), "leaked {kind:?}");
        }
    }
    let other_project = TestProject::with_prefab_startup();
    let (other_candidate, other_plan, _) = other_project.candidate_plan_and_root();
    let error = loader
        .load_scene(
            &other_candidate,
            &other_plan,
            &ProjectPath::new("startup.scene.json").unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::ProjectRootMismatch);
    let error = loader
        .load_scene(
            &candidate,
            &other_plan,
            &ProjectPath::new("startup.scene.json").unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::ProjectLineageMismatch
    );
    drop(snapshot);
    assert_no_active_charges(loader.budget_snapshot());
}

fn assert_no_active_charges(snapshot: ProjectContentBudgetSnapshot) {
    assert_eq!(snapshot.active_reservations(), 0);
    assert_eq!(snapshot.active_generations(), 0);
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(snapshot.active(*kind), 0, "retained {kind:?}");
    }
}
