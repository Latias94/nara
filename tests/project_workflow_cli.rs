#![cfg(all(
    feature = "project-workflow-cli",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[path = "support/project_tooling_caption_game.rs"]
mod caption_game;
#[path = "support/project_content_fixture.rs"]
mod content_fixture;
#[path = "support/project_tooling_game.rs"]
mod custom_game;
#[path = "fixtures/module-consumer/asset-domain-consumer/src/lib.rs"]
mod text_domain;

#[test]
#[ignore = "runs nested Cargo builds and a real custom-schema/external-domain Check/Run/Repair/Export trajectory"]
fn fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay() {
    use nara::{
        fs::RelativePath,
        project::ProjectPath,
        project_host::{ProjectContentLoader, ingest_project_manifest, resolve_product_recipe},
    };
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(checkout.join(format!(
        "target/nara-custom-project-tooling-{}-{sequence}",
        std::process::id()
    )));
    let created = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "custom-pulse", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let project = parent.path().join("custom-pulse");
    let library = project.join("src/lib.rs");
    let source = fs::read_to_string(&library).unwrap().replace(
        "    nara::ProductRecipe::new()",
        "    caption_game::with_caption(custom_recipe().expect(\"the project's contribution is valid\")).expect(\"the external resource contribution is valid\")",
    );
    fs::write(
        &library,
        format!(
            "{source}\nuse nara_asset_domain_consumer as text_domain;\n{}\nmod caption_game {{\n{}\n}}\n",
            include_str!("support/project_tooling_game.rs"),
            include_str!("support/project_tooling_caption_game.rs"),
        ),
    )
    .unwrap();
    let cargo_path = project.join("Cargo.toml");
    let mut cargo: toml::Value = toml::from_str(&fs::read_to_string(&cargo_path).unwrap()).unwrap();
    let domain_path = checkout.join("tests/fixtures/module-consumer/asset-domain-consumer");
    cargo["dependencies"].as_table_mut().unwrap().insert(
        "nara-asset-domain-consumer".to_owned(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "path".to_owned(),
            toml::Value::String(domain_path.to_str().unwrap().to_owned()),
        )])),
    );
    fs::write(&cargo_path, toml::to_string_pretty(&cargo).unwrap()).unwrap();
    let root = nara::project_tooling::host_fs::readable_directory(&project).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let recipe = || caption_game::with_caption(custom_game::custom_recipe().unwrap()).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
    fs::write(
        project.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut scene: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("scenes/startup.scene.json")).unwrap())
            .unwrap();
    scene["payload"]["entities"].as_array_mut().unwrap().push(serde_json::json!({
        "id":"pulse", "children":[], "components":{"studio.pulse.Pulse":{"version":1,"value":{"type":"map","value":{"value":{"type":"i64","value":41}}}}}
    }));
    scene["payload"]["roots"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("pulse"));
    install_caption_source(&project, &mut scene);
    fs::write(
        project.join("scenes/startup.scene.json"),
        serde_json::to_vec_pretty(&scene).unwrap(),
    )
    .unwrap();
    let expected_second_scene = install_declared_second_scene(&project);
    let marker = parent.path().join("gameplay.txt");
    let text_marker = parent.path().join("domain-gameplay.txt");
    assert!(!project.join("Cargo.lock").exists());
    let checked = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("check")
        .arg(&project)
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(project.join("Cargo.lock").is_file());
    assert!(!marker.exists(), "Check must not execute startup systems");
    assert!(
        !text_marker.exists(),
        "Check must not execute domain consumers"
    );
    let text_source = project.join("assets/captions/greeting.txt");
    let original_text = fs::read(&text_source).unwrap();
    let private_text = "NARA_PRIVATE_DOMAIN_SOURCE_CANARY_".repeat(4);
    fs::write(&text_source, format!("captions/suffix.txt\n{private_text}")).unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("check")
        .arg(&project)
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let failure = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        failure.contains("consumer.text.too-long"),
        "the compiled project's domain code must reach the CLI: {failure}"
    );
    assert!(failure.contains("Text exceeds max_characters"));
    assert!(!failure.contains("NARA_PRIVATE_DOMAIN_SOURCE_CANARY_"));
    assert!(!marker.exists());
    assert!(!text_marker.exists());
    let error = nara::project_tooling::invoke_project_tool(
        nara::project_tooling::ProjectToolRequest::Check {
            project: project.clone(),
            profile: None,
        },
    )
    .unwrap_err();
    let nara::project_tooling::ProjectToolInvocationError::Project(failure) = error else {
        panic!("the public caller must retain the compiled domain failure");
    };
    assert_eq!(failure.operation(), "check");
    assert_eq!(
        failure.stage(),
        nara::project_tooling::ProjectToolFailureStage::ContentAdmission
    );
    assert_eq!(failure.diagnostics().len(), 1);
    assert_eq!(failure.diagnostics()[0].code(), "consumer.text.too-long");
    assert_eq!(
        failure.diagnostics()[0].severity(),
        nara::diagnostic::DiagnosticSeverity::Error
    );
    assert!(
        !failure
            .to_string()
            .contains("NARA_PRIVATE_DOMAIN_SOURCE_CANARY_")
    );
    fs::write(&text_source, original_text).unwrap();

    let metadata_path = project.join("assets/captions/greeting.txt.meta");
    let original_metadata = fs::read(&metadata_path).unwrap();
    let mut invalid_settings: serde_json::Value =
        serde_json::from_slice(&original_metadata).unwrap();
    invalid_settings["payload"]["settings"]["max_characters"] = serde_json::json!(0);
    fs::write(
        &metadata_path,
        serde_json::to_vec(&invalid_settings).unwrap(),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("check")
        .arg(&project)
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let failure = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        failure.contains("consumer.text.settings-invalid"),
        "{failure}"
    );
    assert!(failure.contains("Set max_characters between 1 and 4096"));
    assert!(!marker.exists());
    assert!(!text_marker.exists());
    let error = nara::project_tooling::invoke_project_tool(
        nara::project_tooling::ProjectToolRequest::Export {
            project: project.clone(),
            parent: parent.path().to_owned(),
            destination: "invalid-domain-candidate".into(),
        },
    )
    .unwrap_err();
    let nara::project_tooling::ProjectToolInvocationError::Project(failure) = error else {
        panic!("Export's preliminary Check must retain the same domain failure");
    };
    assert_eq!(failure.operation(), "export");
    assert_eq!(
        failure.stage(),
        nara::project_tooling::ProjectToolFailureStage::ContentAdmission
    );
    assert_eq!(failure.diagnostics().len(), 1);
    assert_eq!(
        failure.diagnostics()[0].code(),
        "consumer.text.settings-invalid"
    );
    assert!(!parent.path().join("invalid-domain-candidate").exists());
    fs::write(metadata_path, &original_metadata).unwrap();
    let renamed = fs::read_to_string(&library)
        .unwrap()
        .replace("RenamedPulse", "AnotherNativePulseName");
    fs::write(&library, renamed).unwrap();
    let run = Command::new(env!("CARGO"))
        .args(["run", "--locked", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .args(["--bin", "headless"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "migrated-pulse=42\n");
    assert_eq!(fs::read_to_string(&text_marker).unwrap(), "Hello world\n");
    fs::remove_file(&marker).unwrap();
    fs::remove_file(&text_marker).unwrap();
    fs::rename(
        project.join("scenes/startup.scene.json"),
        project.join("scenes/recovered.scene.json"),
    )
    .unwrap();
    let old_metadata = project.join("scenes/startup.scene.json.meta");
    let identity = nara::asset::AssetMetaIdentityCandidate::decode_json_bytes(
        &fs::read(&old_metadata).unwrap(),
    )
    .unwrap();
    let moved_metadata = nara::asset::AssetMeta::new(
        nara::asset::AssetMetaIdentity::new(
            identity.identity().stable_id(),
            nara::asset::AssetPath::new("recovered.scene.json").unwrap(),
            nara::asset::AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        project.join("scenes/recovered.scene.json.meta"),
        moved_metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::remove_file(old_metadata).unwrap();
    let manifest_path = project.join("nara.toml");
    let original_manifest = fs::read(&manifest_path).unwrap();
    for profile in [None, Some("tools".to_owned()), Some("a".repeat(128))] {
        if let Some(profile) = &profile {
            let mut manifest = String::from_utf8(original_manifest.clone()).unwrap();
            manifest.push_str(&format!(
                "\n[profiles.{profile}.startup]\ndefault_scene = \"startup.scene.json\"\n"
            ));
            fs::write(&manifest_path, manifest).unwrap();
        }
        let error = nara::project_tooling::invoke_project_tool(
            nara::project_tooling::ProjectToolRequest::Check {
                project: project.clone(),
                profile: profile.clone(),
            },
        )
        .unwrap_err();
        let nara::project_tooling::ProjectToolInvocationError::Project(failure) = error else {
            panic!("missing startup must retain its structured repair hint");
        };
        let hint = failure.repair_hint().unwrap();
        assert_eq!(hint.scene(), Some("startup.scene.json"));
        assert_eq!(hint.scenes_root(), Some("scenes"));
        assert_eq!(hint.profile_selected(), profile.is_some());
        if profile.as_ref().is_some_and(|profile| profile.len() > 96) {
            assert_eq!(hint.profile(), None);
            assert_eq!(failure.loss().withheld_repair_fields, 1);
            assert!(failure.to_string().contains("--profile <name>"));
            assert!(!format!("{failure:?}").contains(profile.as_ref().unwrap()));
        } else {
            assert_eq!(hint.profile(), profile.as_deref());
            assert_eq!(failure.loss().withheld_repair_fields, 0);
        }
        fs::write(&manifest_path, &original_manifest).unwrap();
    }
    let mut invalid_settings: serde_json::Value =
        serde_json::from_slice(&original_metadata).unwrap();
    invalid_settings["payload"]["settings"]["max_characters"] = serde_json::json!(0);
    fs::write(
        project.join("assets/captions/greeting.txt.meta"),
        serde_json::to_vec(&invalid_settings).unwrap(),
    )
    .unwrap();
    let error = nara::project_tooling::invoke_project_tool(
        nara::project_tooling::ProjectToolRequest::RepairStartupScene {
            project: project.clone(),
            profile: None,
            replacement: "recovered.scene.json".to_owned(),
        },
    )
    .unwrap_err();
    let nara::project_tooling::ProjectToolInvocationError::Project(failure) = error else {
        panic!("replacement admission must retain the domain failure");
    };
    assert_eq!(
        failure.stage(),
        nara::project_tooling::ProjectToolFailureStage::Repair
    );
    assert_eq!(failure.code(), "project.tool.repair-failed");
    assert_eq!(
        failure.diagnostics()[0].code(),
        "consumer.text.settings-invalid"
    );
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest);
    fs::write(
        project.join("assets/captions/greeting.txt.meta"),
        original_metadata,
    )
    .unwrap();
    let repaired = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["repair", "startup-scene"])
        .arg(&project)
        .args(["--replacement", "recovered.scene.json"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(!marker.exists(), "Repair must not execute startup systems");
    assert!(
        !text_marker.exists(),
        "Repair must not execute domain consumers"
    );
    let exported = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("export")
        .arg(&project)
        .arg("--parent")
        .arg(parent.path())
        .args(["--destination", "custom-candidate"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .env("NARA_TEST_TEXT_MARKER", &text_marker)
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&exported.stdout),
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(String::from_utf8_lossy(&exported.stdout).contains("2 exact product observations"));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "migrated-pulse=42\n");
    assert_eq!(fs::read_to_string(&text_marker).unwrap(), "Hello world\n");
    let candidate = parent.path().join("custom-candidate");
    for path in [
        "captions/greeting.txt",
        "captions/greeting.txt.meta",
        "captions/suffix.txt",
    ] {
        assert_eq!(
            fs::read(candidate.join("project/assets").join(path)).unwrap(),
            fs::read(project.join("assets").join(path)).unwrap(),
        );
    }
    assert!(
        candidate
            .join("project/scenes/recovered.scene.json")
            .is_file()
    );
    assert!(
        candidate
            .join(format!("bin/headless{}", std::env::consts::EXE_SUFFIX))
            .is_file()
    );
    assert!(
        candidate
            .join(format!("bin/desktop{}", std::env::consts::EXE_SUFFIX))
            .is_file()
    );

    // Remove the development coordinate before consuming the independently published product.
    // Both this source and the destination belong to the test's private generated-project parent.
    fs::rename(&project, parent.path().join("detached-development-project")).unwrap();
    assert!(!project.exists());
    let published_project = candidate.join("project");
    let published_root =
        nara::project_tooling::host_fs::readable_directory(&published_project).unwrap();
    assert_ne!(published_root.identity(), root.identity());
    let manifest = published_root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let settings = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&settings, recipe()).unwrap();
    let loader = ProjectContentLoader::new(published_root).unwrap();
    let second = loader
        .load_scene(
            &settings,
            &plan,
            &ProjectPath::new("second.scene.json").unwrap(),
        )
        .unwrap();
    assert_eq!(second.scene(), &expected_second_scene);
    assert!(second.startup_content_closure().is_none());
    assert!(
        second
            .expanded_scene_provenance()
            .values()
            .any(|source| source.local_entity_id() == &content_fixture::scene_id("second-sprite"))
    );
    assert_eq!(second.images().len(), 1);
    assert_eq!(
        second.images()[0].path().as_str(),
        "assets/textures/second.png"
    );
    assert_eq!(second.images()[0].image().pixels(), &[24, 120, 220, 255]);
    let delivery = loader.load_delivery(&settings, &plan).unwrap();
    assert!(String::from_utf8_lossy(&exported.stdout).contains(&format!(
        "candidate with {} content-delivery members",
        delivery.members().len()
    )));
    assert!(
        delivery
            .scene_roots()
            .iter()
            .any(|scene| scene.as_str() == "second.scene.json")
    );
    assert!(
        delivery
            .members()
            .iter()
            .any(|member| member.path().as_str() == "prefabs/second.prefab.json")
    );
    assert!(
        delivery
            .members()
            .iter()
            .any(|member| member.path().as_str() == "assets/textures/second.png.meta")
    );
    for path in [
        "assets/captions/greeting.txt",
        "assets/captions/greeting.txt.meta",
        "assets/captions/suffix.txt",
    ] {
        assert_eq!(
            delivery
                .members()
                .iter()
                .filter(|member| member.path().as_str() == path)
                .count(),
            1,
        );
    }
    assert!(
        !published_project
            .join("scenes/undeclared.scene.json")
            .exists()
    );

    let unrelated = TestRoot::new("exported-second-scene-cwd");
    let published_marker = parent.path().join("published-gameplay.txt");
    let published_text_marker = parent.path().join("published-domain-gameplay.txt");
    let run = Command::new(candidate.join(format!("bin/headless{}", std::env::consts::EXE_SUFFIX)))
        .current_dir(unrelated.path())
        .env("NARA_TEST_PULSE_MARKER", &published_marker)
        .env("NARA_TEST_TEXT_MARKER", &published_text_marker)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "published product failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(&published_marker).unwrap(),
        "migrated-pulse=42\n"
    );
    assert_eq!(
        fs::read_to_string(&published_text_marker).unwrap(),
        "Hello world\n"
    );

    fs::remove_file(&published_marker).unwrap();
    fs::remove_file(&published_text_marker).unwrap();
    fs::remove_file(published_project.join("assets/captions/suffix.txt")).unwrap();
    let rejected =
        Command::new(candidate.join(format!("bin/headless{}", std::env::consts::EXE_SUFFIX)))
            .current_dir(unrelated.path())
            .env("NARA_TEST_PULSE_MARKER", &published_marker)
            .env("NARA_TEST_TEXT_MARKER", &published_text_marker)
            .output()
            .unwrap();
    assert!(
        !rejected.status.success(),
        "a required published dependency is missing"
    );
    assert!(!published_marker.exists());
    assert!(!published_text_marker.exists());
}

fn install_caption_source(project: &Path, scene: &mut serde_json::Value) {
    use nara::{
        asset::{
            AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
        },
        reflect::PersistentComponentProvider,
    };
    fs::create_dir_all(project.join("assets/captions")).unwrap();
    fs::write(
        project.join("assets/captions/greeting.txt"),
        b"captions/suffix.txt\nHello",
    )
    .unwrap();
    fs::write(project.join("assets/captions/suffix.txt"), b" world").unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("a8149311-df08-47aa-84c2-3a77e47c77ef").unwrap(),
            AssetPath::new("captions/greeting.txt").unwrap(),
            AssetSourceKind::Other("consumer.text".to_owned()),
        ),
        text_domain::TextSettings { max_characters: 64 },
    );
    fs::write(
        project.join("assets/captions/greeting.txt.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    let caption = caption_game::Caption {
        text: AssetRef::path("captions/greeting.txt").unwrap(),
    };
    scene["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id":"caption", "children":[], "components":{"studio.caption.Caption":{
                "version":1, "value":caption.__encode_persistent_component().unwrap()
            }}
        }));
    scene["payload"]["roots"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("caption"));
}

fn install_declared_second_scene(project: &Path) -> nara::scene::SceneDocument {
    use content_fixture::{
        identity_transform_record, image_meta, scene_id, sprite_value, valid_png_bytes,
    };
    use nara::{
        asset::AssetRef,
        reflect::{ComponentSchemaVersion, ComponentTypeId},
        scene::{
            PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        },
    };

    fs::create_dir_all(project.join("assets/textures")).unwrap();
    fs::create_dir_all(project.join("prefabs")).unwrap();
    fs::write(
        project.join("assets/textures/second.png"),
        valid_png_bytes(),
    )
    .unwrap();
    fs::write(
        project.join("assets/textures/second.png.meta"),
        image_meta(
            "d591edb8-3f1b-46b4-95bf-e9b2f77457a3",
            "textures/second.png",
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let prefab =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("second-sprite"))
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    sprite_value(&AssetRef::path("textures/second.png").unwrap()),
                ),
            )]);
    fs::write(
        project.join("prefabs/second.prefab.json"),
        prefab.to_json_string().unwrap(),
    )
    .unwrap();
    let mut anchor = SceneEntityRecord::new(scene_id("second-anchor")).with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    );
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("second.prefab.json").unwrap(),
        overrides: None,
    });
    let scene = SceneDocument::from_ordered_roots([anchor]);
    fs::write(
        project.join("scenes/second.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    for (root, path, kind, id) in [
        (
            "scenes",
            "second.scene.json",
            nara::asset::AssetSourceKind::Scene,
            "46199fbe-f0c4-4116-b6ae-d85b407f6cba",
        ),
        (
            "prefabs",
            "second.prefab.json",
            nara::asset::AssetSourceKind::Prefab,
            "d2634ab6-7f43-4bbb-b466-9b837404f3dd",
        ),
    ] {
        let metadata = nara::asset::AssetMeta::new(
            nara::asset::AssetMetaIdentity::new(
                nara::asset::StableAssetId::parse_str(id).unwrap(),
                nara::asset::AssetPath::new(path).unwrap(),
                kind,
            ),
            (),
        );
        fs::write(
            project.join(root).join(format!("{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    fs::write(project.join("scenes/undeclared.scene.json"), b"not a Scene").unwrap();
    let manifest_path = project.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!("{manifest}\n[content]\nscenes = [\"second.scene.json\"]\n"),
    )
    .unwrap();
    scene
}

#[test]
#[ignore = "builds the independent project's headless tooling executable"]
fn compiled_project_check_uses_custom_schema_without_starting_gameplay() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let unrelated = TestRoot::new("compiled-project-check");
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(unrelated.path())
        .arg("check")
        .arg(checkout.join("reference-game"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("NARA_REFERENCE_GAME_STARTUP_MARKER", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compiled project check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Project check passed"));
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("headless_first_authoritative_tick"),
        "strict Check must not start gameplay",
    );
}

#[test]
#[ignore = "builds a real legacy headless binary and exercises compiled-tool bootstrap failures"]
fn legacy_tool_entry_and_failed_bootstrap_never_launch_or_publish() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = TestRoot::new("legacy-tool-entry");
    fs::create_dir_all(root.path().join("src/bin")).unwrap();
    let manifest = "[package]\nname = \"legacy-tool-entry\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\nresolver = \"3\"\n";
    fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(root.path().join("src/bin/headless.rs"), "fn main() { std::fs::write(std::env::var_os(\"NARA_TEST_LEGACY_MARKER\").unwrap(), b\"gameplay\").unwrap(); }").unwrap();
    let build = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--manifest-path"])
        .arg(root.path().join("Cargo.toml"))
        .args(["--bin", "headless"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let marker = root.path().join("gameplay.txt");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_nara"))
            .arg("export")
            .arg(root.path())
            .arg("--parent")
            .arg(root.path())
            .args(["--destination", "must-stay-absent"])
            .env("CARGO_TARGET_DIR", checkout.join("target"))
            .env("CARGO_BUILD_JOBS", "1")
            .env("NARA_TEST_LEGACY_MARKER", &marker)
            .output()
            .unwrap()
    };
    for protocol in [None, Some(2), Some(999)] {
        let metadata = protocol.map_or(String::new(), |version| {
            format!("\n[package.metadata.nara]\nproject-tool-protocol = {version}\n")
        });
        fs::write(
            root.path().join("Cargo.toml"),
            format!("{manifest}{metadata}"),
        )
        .unwrap();
        let output = invoke();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("project-tool-protocol"));
        assert!(
            !marker.exists(),
            "the real legacy executable must not start"
        );
        assert!(!root.path().join("must-stay-absent").exists());
    }
    fs::write(
        root.path().join("Cargo.toml"),
        format!("{manifest}\n[package.metadata.nara]\nproject-tool-protocol = 1\n"),
    )
    .unwrap();
    fs::write(
        root.path().join("src/bin/headless.rs"),
        "compile_error!(\"bootstrap fixture failure\"); fn main() {}\n",
    )
    .unwrap();
    let output = invoke();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bootstrap build failed"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bootstrap fixture failure"),
        "the structured compiler diagnostic must remain actionable"
    );
    assert!(!marker.exists());
    assert!(!root.path().join("must-stay-absent").exists());
}

#[test]
fn new_command_creates_and_admits_a_local_project_from_an_unrelated_cwd() {
    let root = TestRoot::new("local-siblings");
    let checkout = root.path().join("engine checkout");
    let parent = root.path().join("games with spaces");
    fs::create_dir(&checkout).unwrap();
    fs::create_dir(&parent).unwrap();
    let unrelated = TestRoot::new("unrelated-cwd");

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(unrelated.path())
        .args(["new", "desktop-2d/v1", "cli-created", "--parent"])
        .arg(&parent)
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();

    let project = parent.join("cli-created");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("project creation was rejected: BeforeStaging"),
            "stdout={}\nstderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains(" is unsupported:")
                || stderr.contains(" cannot prove the required invariant:"),
            "unexpected pre-staging rejection: {stderr}"
        );
        assert!(!project.exists());
        assert!(fs::read_dir(&parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".nara-stage-")
        }));
        return;
    }
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Created cli-created from desktop-2d/v1.\n"
    );

    assert!(project.join("nara.toml").is_file());
    assert!(project.join("scenes/startup.scene.json").is_file());
    assert_created_source_identity(&project);
    let cargo_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let cargo: toml::Value = toml::from_str(&cargo_manifest).unwrap();
    let dependency = cargo["dependencies"]["nara"]["path"].as_str().unwrap();
    assert_eq!(dependency, "../../engine checkout");
    assert_eq!(
        fs::canonicalize(project.join(dependency)).unwrap(),
        fs::canonicalize(&checkout).unwrap()
    );
    assert!(cargo_manifest.contains("[workspace]\nresolver = \"3\""));
}

fn assert_created_source_identity(project: &Path) {
    let bytes = fs::read(project.join("nara.toml")).unwrap();
    let manifest = nara::project::ProjectManifest::parse_toml_bytes(&bytes)
        .manifest
        .unwrap();
    assert_eq!(
        manifest.project.native_document_identity,
        nara::project::NativeDocumentIdentity::StableMetadata
    );
    assert!(!manifest.project.source_universe.as_uuid().is_nil());
    let metadata = nara::asset::AssetMetaIdentityCandidate::decode_json_bytes(
        &fs::read(project.join("scenes/startup.scene.json.meta")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata.identity().path().as_str(), "startup.scene.json");
    assert_eq!(
        metadata.identity().source_kind(),
        &nara::asset::AssetSourceKind::Scene
    );
    assert!(!metadata.identity().stable_id().as_uuid().is_nil());
}

#[test]
#[cfg(windows)]
fn new_command_rejects_cross_volume_local_creation_before_publication() {
    let checkout = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let parent = TestRoot::new("cross-volume-parent");
    let parent_path = fs::canonicalize(parent.path()).unwrap();
    if checkout.components().next() == parent_path.components().next() {
        // This host has no second test volume; the same-volume cases run elsewhere.
        return;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "cross-volume-game", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cross-volume roots are unsupported"));
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn new_command_accepts_a_valid_immutable_git_source() {
    let root = TestRoot::new("git-source");
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(root.path())
        .args(["new", "desktop-2d/v1", "git-created", "--parent"])
        .arg(root.path())
        .args([
            "--git",
            "https://github.com/nara-engine/nara.git",
            "--rev",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();

    let project = root.path().join("git-created");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("project creation was rejected: BeforeStaging")
                && (stderr.contains(" is unsupported:")
                    || stderr.contains(" cannot prove the required invariant:")),
            "unexpected Git-source rejection: {stderr}"
        );
        assert!(!project.exists());
        return;
    }

    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Created git-created from desktop-2d/v1.\n"
    );
    let cargo_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_manifest.contains(
        "git = \"https://github.com/nara-engine/nara.git\", rev = \"0123456789abcdef0123456789abcdef01234567\""
    ));
    assert!(!cargo_manifest.contains("@"));
    assert_created_source_identity(&project);
}

#[test]
fn generated_project_is_an_independent_cargo_product() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(checkout.join(format!(
        "../nara-project-workflow-cli-cargo-{}-{sequence}",
        std::process::id()
    )));

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "cargo-product", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    let project = parent.path().join("cargo-product");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata = Command::new(env!("CARGO"))
        .current_dir(&checkout)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&metadata.stdout),
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    assert_eq!(metadata["packages"].as_array().unwrap().len(), 1);
    assert_eq!(metadata["packages"][0]["name"], "cargo-product");
    assert_eq!(
        fs::canonicalize(metadata["workspace_root"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&project).unwrap()
    );
    let targets = metadata["packages"][0]["targets"].as_array().unwrap();
    let mut target_names = targets
        .iter()
        .map(|target| target["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    target_names.sort_unstable();
    assert_eq!(target_names, ["desktop", "headless", "nara_game"]);

    let check = Command::new(env!("CARGO"))
        .current_dir(&checkout)
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .args([
            "check",
            "--offline",
            "--all-targets",
            "--all-features",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn new_command_rejects_mixed_dependency_sources_before_creation() {
    let checkout = TestRoot::new("mixed-source-checkout");
    fs::create_dir(checkout.path().join("projects")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "mixed-source", "--parent"])
        .arg(checkout.path().join("projects"))
        .arg("--local-checkout")
        .arg(checkout.path())
        .args([
            "--git",
            "https://example.invalid/nara.git",
            "--rev",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(!checkout.path().join("projects/mixed-source").exists());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("choose exactly one Nara dependency source")
    );
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara-project-workflow-cli-{label}-{}-{sequence}",
            std::process::id()
        ));
        Self::at(path)
    }

    fn at(path: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
