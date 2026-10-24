#![cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "project-workflow"
))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{collections::BTreeSet, fs};

use nara::{
    ProductRecipe,
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    fs::{ContentDigest, RelativePath},
    project::ProjectPath,
    project_host::{
        ProjectContentBudgetKind, ProjectContentBudgetSnapshot, ProjectContentErrorKind,
        ProjectContentLimits, ProjectContentLoader, ProjectContentMemberKind,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckErrorKind, ProjectCheckRequest, check_project},
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        ScenePatchDocument,
    },
};
use project_content_fixture::{
    TestProject, identity_transform_record, image_meta, scene_id, sprite_value, tileset_meta,
    valid_png_bytes,
};

const SECOND_SCENE: &str = "second.scene.json";
const SECOND_IMAGE: &str = "textures/second.png";

#[test]
fn stable_scene_roots_cannot_claim_the_same_source_identity() {
    let project = TestProject::with_prefab_startup();
    project.use_stable_native_identity();
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("local"))]);
    project.write_scene_source(&scene);
    fs::write(
        project.path().join("scenes").join(SECOND_SCENE),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let stable_id = StableAssetId::parse_str("4c2a2c0e-25dd-4b3e-9e6b-52f0a04e8de1").unwrap();
    project.write_scene_meta(stable_id);
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            stable_id,
            AssetPath::new(SECOND_SCENE).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        project.path().join(format!("scenes/{SECOND_SCENE}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    declare_scenes(&project, &[SECOND_SCENE]);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load_delivery(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
}

#[test]
fn selected_scene_content_is_truthful_without_granting_startup_or_delivery_authority() {
    let project = two_scene_project();
    let spare = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("spare"))]);
    fs::write(
        project.path().join("scenes/spare.scene.json"),
        spare.to_json_string().unwrap(),
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    assert_eq!(startup.selected_scene_path().as_str(), "startup.scene.json");
    assert!(startup.startup_content_closure().is_some());
    let second = loader
        .load_scene(&candidate, &plan, &ProjectPath::new(SECOND_SCENE).unwrap())
        .unwrap();
    assert_eq!(second.selected_scene_path().as_str(), SECOND_SCENE);
    assert!(second.startup_content_closure().is_none());
    assert_eq!(second.scene(), &second_scene());
    assert!(second.expanded_scene_provenance().values().any(|source| {
        source.source_entity_id() == &scene_id("second-enemy")
            && source
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &scene_id("second-anchor"))
    }));
    assert!(
        second
            .images()
            .iter()
            .any(|image| image.path().as_str() == SECOND_IMAGE)
    );
    assert_eq!(
        second.content_closure().selected_scene_path().as_str(),
        SECOND_SCENE
    );
    drop((startup, second));

    // A valid explicit source load is independent of the project's finite delivery declaration.
    let spare = loader
        .load_scene(
            &candidate,
            &plan,
            &ProjectPath::new("spare.scene.json").unwrap(),
        )
        .unwrap();
    assert_eq!(spare.scene().roots, [scene_id("spare")]);
    assert!(spare.startup_content_closure().is_none());
    drop(spare);
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    assert!(
        delivery
            .scene_roots()
            .iter()
            .all(|path| path.as_str() != "spare.scene.json")
    );
    assert!(
        delivery
            .members()
            .iter()
            .all(|member| member.path().as_str() != "scenes/spare.scene.json")
    );
}

#[test]
fn delivery_unions_both_complete_closures_and_deduplicates_shared_members() {
    let project = two_scene_project();
    // Explicitly listing startup once is valid; the implicit root does not duplicate it.
    declare_scenes(&project, &["startup.scene.json", SECOND_SCENE]);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let identity = root.identity();
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    let startup_closure = startup.startup_content_closure().unwrap();
    drop(startup);
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();

    assert_eq!(delivery.project_root_identity(), identity);
    assert_eq!(delivery.settings_lineage(), candidate.lineage());
    assert_eq!(delivery.manifest_digest(), candidate.manifest_digest());
    assert_eq!(
        delivery.schema_fingerprint(),
        plan.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        delivery.startup_content().content_digest(),
        startup_closure.content_digest()
    );
    // The bare load validates startup data. Delivery additionally checks the runtime schema
    // agreement, so its startup receipt also owns the exact authoring bundle source.
    for member in startup_closure.members() {
        assert_eq!(
            delivery
                .startup_content()
                .members()
                .iter()
                .find(|checked| checked.path() == member.path()),
            Some(member)
        );
    }
    let additional_members = delivery
        .startup_content()
        .members()
        .iter()
        .filter(|member| {
            startup_closure
                .members()
                .iter()
                .all(|bare| bare.path() != member.path())
        })
        .collect::<Vec<_>>();
    assert_eq!(additional_members.len(), 1);
    let bundle = additional_members[0];
    assert_eq!(bundle.path().as_str(), "schema/authoring.bundle.json");
    assert_eq!(
        bundle.kind(),
        ProjectContentMemberKind::AuthoringSchemaBundle
    );
    let bundle_bytes = fs::read(project.path().join(bundle.path().as_str())).unwrap();
    assert_eq!(bundle.digest(), ContentDigest::of_bytes(&bundle_bytes));
    assert_eq!(bundle.byte_len(), bundle_bytes.len() as u64);
    assert_eq!(delivery.scene_roots().len(), 2);
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(ProjectPath::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([SECOND_SCENE, "startup.scene.json"]),
    );

    let members = delivery
        .members()
        .iter()
        .map(|member| (member.path().as_str(), member.kind()))
        .collect::<Vec<_>>();
    assert_eq!(
        members,
        vec![
            (
                "assets/textures/player.png",
                ProjectContentMemberKind::AssetSource
            ),
            (
                "assets/textures/player.png.meta",
                ProjectContentMemberKind::AssetMetadata
            ),
            (
                "assets/textures/second.png",
                ProjectContentMemberKind::AssetSource
            ),
            (
                "assets/textures/second.png.meta",
                ProjectContentMemberKind::AssetMetadata
            ),
            ("nara.toml", ProjectContentMemberKind::ProjectManifest),
            (
                "prefabs/enemy.prefab.json",
                ProjectContentMemberKind::Prefab
            ),
            (
                "prefabs/second.prefab.json",
                ProjectContentMemberKind::Prefab
            ),
            (
                "prefabs/second.prefab.json.meta",
                ProjectContentMemberKind::AssetMetadata
            ),
            ("scenes/second.scene.json", ProjectContentMemberKind::Scene),
            ("scenes/startup.scene.json", ProjectContentMemberKind::Scene),
            (
                "schema/authoring.bundle.json",
                ProjectContentMemberKind::AuthoringSchemaBundle
            ),
        ]
    );
    for member in delivery.members() {
        let bytes = fs::read(project.path().join(member.path().as_str())).unwrap();
        assert_eq!(member.digest(), ContentDigest::of_bytes(&bytes));
        assert_eq!(member.byte_len(), bytes.len() as u64);
    }
    let checked = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap();
    assert_eq!(checked.delivery_digest(), delivery.delivery_digest());
    assert_eq!(checked.members(), delivery.members());
}

#[test]
fn delivery_includes_the_nonstartup_scenes_tileset_and_its_image_dependencies() {
    let project = TestProject::with_prefab_startup();
    declare_scenes(&project, &[SECOND_SCENE]);
    let atlas_path = "textures/second-atlas.png";
    project.write_asset(
        atlas_path,
        valid_png_bytes(),
        &image_meta("5b6802dc-c438-421f-9f07-870f514c551b", atlas_path),
    );
    let tileset_path = "tilesets/second-terrain.tileset.ron";
    project.write_tileset_source(
        tileset_path,
        &nara::tilemap::TileSetSource::new(
            AssetRef::path(atlas_path).unwrap(),
            nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
        ),
        &tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", tileset_path),
    );
    let terrain = SceneEntityRecord::new(scene_id("second-terrain"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.tilemap.Tilemap"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "tile_size",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                    (
                        "tileset",
                        ComponentValue::map([
                            ("kind", ComponentValue::String("path".to_owned())),
                            ("value", ComponentValue::String(tileset_path.to_owned())),
                        ]),
                    ),
                    ("cells", ComponentValue::List(Vec::new())),
                ]),
            ),
        );
    fs::write(
        project.path().join("scenes").join(SECOND_SCENE),
        SceneDocument::from_ordered_roots([terrain])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let delivery = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap();
    assert!(delivery.members().iter().any(|member| {
        member.path().as_str() == "scenes/second.scene.json"
            && member.kind() == ProjectContentMemberKind::Scene
    }));
    for (path, kind) in [
        (
            "assets/tilesets/second-terrain.tileset.ron",
            ProjectContentMemberKind::AssetSource,
        ),
        (
            "assets/tilesets/second-terrain.tileset.ron.meta",
            ProjectContentMemberKind::AssetMetadata,
        ),
        (
            "assets/textures/second-atlas.png",
            ProjectContentMemberKind::AssetSource,
        ),
        (
            "assets/textures/second-atlas.png.meta",
            ProjectContentMemberKind::AssetMetadata,
        ),
    ] {
        let member = delivery
            .members()
            .iter()
            .find(|member| member.path().as_str() == path)
            .unwrap_or_else(|| panic!("delivery is missing {path}"));
        assert_eq!(member.kind(), kind);
        let bytes = fs::read(project.path().join(path)).unwrap();
        assert_eq!(member.digest(), ContentDigest::of_bytes(&bytes));
        assert_eq!(member.byte_len(), bytes.len() as u64);
        assert!(
            delivery
                .startup_content()
                .members()
                .iter()
                .all(|member| { member.path().as_str() != path }),
            "startup must not own the second Scene's dependency {path}"
        );
    }
}

#[test]
fn undeclared_invalid_scene_is_excluded_but_declaring_it_rejects_check_and_refunds_work() {
    let project = two_scene_project();
    fs::write(
        project.path().join("scenes/broken.scene.json"),
        b"not a Scene",
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    assert!(
        delivery
            .members()
            .iter()
            .all(|member| member.path().as_str() != "scenes/broken.scene.json")
    );
    let old_digest = delivery.delivery_digest();
    declare_scenes(&project, &[SECOND_SCENE, "broken.scene.json"]);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let failing = ProjectContentLoader::new(root).unwrap();
    let baseline = failing.budget_snapshot();
    let error = failing.load_delivery(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::SceneFormat);
    assert_active_budget_eq(failing.budget_snapshot(), baseline);
    let error = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap_err();
    assert_eq!(error.kind(), ProjectCheckErrorKind::Content);
    assert_eq!(delivery.delivery_digest(), old_digest);
    assert_eq!(delivery.scene_roots().len(), 2);
}

#[test]
fn changed_second_scene_produces_a_new_delivery_without_mutating_retained_facts() {
    let project = two_scene_project();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let first = loader.load_delivery(&candidate, &plan).unwrap();
    let first_digest = first.delivery_digest();
    let first_members = first.members().to_vec();
    let mut changed = second_scene();
    changed
        .entities
        .push(SceneEntityRecord::new(scene_id("later-second-object")));
    changed.roots.push(scene_id("later-second-object"));
    fs::write(
        project.path().join("scenes").join(SECOND_SCENE),
        changed.to_json_string().unwrap(),
    )
    .unwrap();
    let second = loader.load_delivery(&candidate, &plan).unwrap();
    assert_ne!(second.delivery_digest(), first_digest);
    assert_eq!(first.delivery_digest(), first_digest);
    assert_eq!(first.members(), first_members);
    assert_eq!(
        first.startup_content().content_digest(),
        second.startup_content().content_digest()
    );
    let current = fs::read(project.path().join("scenes").join(SECOND_SCENE)).unwrap();
    let member_digest = |receipt: &nara::project_host::ProjectContentDeliveryReceipt| {
        receipt
            .members()
            .iter()
            .find(|member| member.path().as_str() == "scenes/second.scene.json")
            .unwrap()
            .digest()
    };
    assert_ne!(member_digest(&first), ContentDigest::of_bytes(&current));
    assert_eq!(member_digest(&second), ContentDigest::of_bytes(&current));
}

#[test]
fn project_wide_scene_roots_use_the_selected_profiles_effective_scenes_directory() {
    let project = two_scene_project();
    fs::create_dir(project.path().join("alternate-scenes")).unwrap();
    let alternate =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("alternate-start"))]);
    fs::write(
        project.path().join("alternate-scenes/alternate.scene.json"),
        alternate.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.path().join("alternate-scenes").join(SECOND_SCENE),
        second_scene().to_json_string().unwrap(),
    )
    .unwrap();
    let manifest = project.path().join("nara.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{source}\n[profiles.alternate.paths]\nscenes = \"alternate-scenes\"\n[profiles.alternate.startup]\ndefault_scene = \"alternate.scene.json\"\n")).unwrap();
    let root = project.root_capability();
    let file = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&file, Some("alternate")).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(ProjectPath::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["alternate.scene.json", SECOND_SCENE])
    );
    let paths = delivery
        .members()
        .iter()
        .map(|member| member.path().as_str())
        .collect::<BTreeSet<_>>();
    assert!(paths.contains("alternate-scenes/alternate.scene.json"));
    assert!(paths.contains("alternate-scenes/second.scene.json"));
    assert!(!paths.iter().any(|path| path.starts_with("scenes/")));
    let selected = loader.load(&candidate, &plan).unwrap();
    assert_eq!(selected.scene(), &alternate);
    assert_eq!(
        selected.startup_content_closure().unwrap().content_digest(),
        delivery.startup_content().content_digest()
    );
}

#[test]
fn delivery_and_escaped_scene_authority_retain_budget_until_their_last_drop() {
    let project = two_scene_project();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let budget = loader.budget_host();
    let baseline = budget.snapshot();
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    let retained = budget.snapshot();
    assert!(retained.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    // Delivery retains source facts, not the decoded generations of every declared Scene.
    assert_eq!(retained.active_generations(), baseline.active_generations());
    let clone = delivery.clone();
    assert_eq!(budget.snapshot(), retained);
    let second = loader
        .load_scene(&candidate, &plan, &ProjectPath::new(SECOND_SCENE).unwrap())
        .unwrap();
    let source = second.scene_source();
    let closure = second.content_closure();
    drop((loader, second, delivery));
    assert_eq!(budget.snapshot().active_generations(), 1);
    assert!(
        budget
            .snapshot()
            .active(ProjectContentBudgetKind::ArtifactBytes)
            > 0
    );
    drop(source);
    assert_eq!(budget.snapshot().active_generations(), 0);
    assert_eq!(
        budget
            .snapshot()
            .active(ProjectContentBudgetKind::ArtifactBytes),
        0
    );
    assert!(
        budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes)
            > 0
    );
    drop(closure);
    assert!(
        budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes)
            > 0
    );
    drop(clone);
    assert_active_budget_eq(budget.snapshot(), baseline);
}

#[test]
fn delivery_file_limit_rejects_the_union_even_when_each_scene_fits_and_refunds_work() {
    let project = TestProject::with_prefab_startup();
    let empty_scene = SceneDocument::default().to_json_string().unwrap();
    for scene in ["startup.scene.json", SECOND_SCENE] {
        fs::write(project.path().join("scenes").join(scene), &empty_scene).unwrap();
    }
    declare_scenes(&project, &[SECOND_SCENE]);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::Files, 3)
        .unwrap();
    let loader = ProjectContentLoader::with_limits(root, limits).unwrap();
    let baseline = loader.budget_snapshot();

    for scene in ["startup.scene.json", SECOND_SCENE] {
        let snapshot = loader
            .load_scene(&candidate, &plan, &ProjectPath::new(scene).unwrap())
            .unwrap();
        let expected_scene_path = format!("scenes/{scene}");
        assert_eq!(
            snapshot
                .content_closure()
                .members()
                .iter()
                .map(|member| { (member.path().as_str(), member.kind()) })
                .collect::<Vec<_>>(),
            [
                ("nara.toml", ProjectContentMemberKind::ProjectManifest),
                (
                    expected_scene_path.as_str(),
                    ProjectContentMemberKind::Scene
                ),
            ]
        );
        drop(snapshot);
        assert_active_budget_eq(loader.budget_snapshot(), baseline);
    }

    // Checked startup also retains its schema bundle. Adding B makes four distinct members,
    // although both individual raw Scene closures above fit within the three-file ceiling.
    let error = loader.load_delivery(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.delivery.member-limit" })
    );
    assert_active_budget_eq(loader.budget_snapshot(), baseline);
}

#[test]
fn delivery_budget_refusal_is_atomic_and_missing_selected_scene_is_not_a_startup_issue() {
    let project = two_scene_project();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::RetainedBytes, 1)
        .unwrap();
    let loader = ProjectContentLoader::with_limits(root.share(), limits).unwrap();
    let baseline = loader.budget_snapshot();
    assert_eq!(
        loader.load_delivery(&candidate, &plan).unwrap_err().kind(),
        ProjectContentErrorKind::BudgetExceeded
    );
    assert_active_budget_eq(loader.budget_snapshot(), baseline);
    let loader = ProjectContentLoader::new(root).unwrap();
    let baseline = loader.budget_snapshot();
    assert_eq!(
        loader
            .load_scene(
                &candidate,
                &plan,
                &ProjectPath::new("absent.scene.json").unwrap()
            )
            .unwrap_err()
            .kind(),
        ProjectContentErrorKind::MissingSelectedScene
    );
    assert_active_budget_eq(loader.budget_snapshot(), baseline);
}

fn two_scene_project() -> TestProject {
    let project = TestProject::with_prefab_startup();
    declare_scenes(&project, &[SECOND_SCENE]);
    project.write_asset(
        SECOND_IMAGE,
        valid_png_bytes(),
        &image_meta("5b6802dc-c438-421f-9f07-870f514c551b", SECOND_IMAGE),
    );
    let prefab =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("second-enemy"))
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    sprite_value(&AssetRef::path(SECOND_IMAGE).unwrap()),
                ),
            )]);
    project.write_prefab("second.prefab.json", &prefab);
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("02c296a0-f75f-4f76-bf88-a15eeb5a32e9").unwrap(),
            AssetPath::new("second.prefab.json").unwrap(),
            AssetSourceKind::Prefab,
        ),
        (),
    );
    fs::write(
        project.path().join("prefabs/second.prefab.json.meta"),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.path().join("scenes").join(SECOND_SCENE),
        second_scene().to_json_string().unwrap(),
    )
    .unwrap();
    project
}

fn second_scene() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        prefab_anchor(
            "shared-anchor",
            AssetRef::path("enemy.prefab.json").unwrap(),
        ),
        prefab_anchor(
            "second-anchor",
            AssetRef::stable_id("02c296a0-f75f-4f76-bf88-a15eeb5a32e9").unwrap(),
        ),
    ])
}

fn prefab_anchor(id: &str, source: AssetRef) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(scene_id(id)).with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    );
    entity.prefab = Some(PrefabInstance {
        source,
        overrides: ScenePatchDocument::default(),
    });
    entity
}

fn declare_scenes(project: &TestProject, scenes: &[&str]) {
    let path = project.path().join("nara.toml");
    let mut manifest: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    manifest.as_table_mut().unwrap().insert(
        "content".to_owned(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "scenes".to_owned(),
            toml::Value::Array(
                scenes
                    .iter()
                    .map(|scene| toml::Value::String((*scene).to_owned()))
                    .collect(),
            ),
        )])),
    );
    fs::write(path, toml::to_string_pretty(&manifest).unwrap()).unwrap();
}

fn assert_active_budget_eq(
    actual: ProjectContentBudgetSnapshot,
    expected: ProjectContentBudgetSnapshot,
) {
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    assert_eq!(actual.active_generations(), expected.active_generations());
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(actual.active(*kind), expected.active(*kind), "{kind:?}");
    }
}
