#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

#[cfg(feature = "project-workflow")]
#[path = "project_content_boundary/format_diagnostics.rs"]
mod format_diagnostics;

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    diagnostic::{DiagnosticFieldClass, DiagnosticValueRef},
    project_host::{ProjectContentError, ProjectContentErrorKind, ProjectContentLoader},
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
    },
};

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
use project_content_fixture::{
    TestProject, image_meta, player_image_meta, scene_id, sprite_value, valid_png_bytes,
};

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
#[test]
fn stable_id_prefab_references_survive_source_rename() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let stable_id = StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap();
    let metadata = |path| {
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        )
        .to_json_string()
        .unwrap()
    };
    std::fs::write(
        project.path().join("prefabs/enemy.prefab.json.meta"),
        metadata("enemy.prefab.json"),
    )
    .unwrap();
    let mut anchor = SceneEntityRecord::new(scene_id("enemy-anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::StableId(stable_id),
        overrides: None,
    });
    let scene = SceneDocument::from_ordered_roots([anchor]);
    std::fs::write(
        project.path().join("scenes/startup.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();

    let before = loader.load(&candidate, &plan).unwrap();
    assert_eq!(
        before.prefabs()[0].path().as_str(),
        "prefabs/enemy.prefab.json"
    );
    std::fs::rename(
        project.path().join("prefabs/enemy.prefab.json"),
        project.path().join("prefabs/renamed.prefab.json"),
    )
    .unwrap();
    std::fs::rename(
        project.path().join("prefabs/enemy.prefab.json.meta"),
        project.path().join("prefabs/renamed.prefab.json.meta"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("prefabs/renamed.prefab.json.meta"),
        metadata("renamed.prefab.json"),
    )
    .unwrap();
    let after = loader.load(&candidate, &plan).unwrap();
    assert_eq!(
        after.prefabs()[0].path().as_str(),
        "prefabs/renamed.prefab.json"
    );
    assert_eq!(before.scene(), after.scene());
    let members = after
        .startup_content_closure()
        .expect("the loader selected the manifest's startup Scene");
    assert!(members.members().iter().any(|member| {
        member.path().as_str() == "prefabs/renamed.prefab.json.meta"
            && member.kind() == nara::project_host::ProjectContentMemberKind::AssetMetadata
    }));
    drop((before, after, members));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn settings_from_another_project_root_reject_before_content_access() {
    let source = TestProject::with_prefab_startup();
    let other = TestProject::with_prefab_startup();
    let (candidate, plan, _source_root) = source.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(other.root_capability()).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::ProjectRootMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn settings_and_runtime_plan_from_different_lineages_reject_before_content_access() {
    let project = TestProject::with_prefab_startup();
    let other = TestProject::with_prefab_startup();
    let (candidate, _plan, root) = project.candidate_plan_and_root();
    let (_other_candidate, other_plan, _other_root) = other.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &other_plan).unwrap_err();

    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::ProjectLineageMismatch
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn stable_id_component_asset_references_survive_source_rename() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let stable_id = player_image_meta().identity().stable_id();
    let enemy = SceneEntityRecord::new(scene_id("enemy")).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            sprite_value(&AssetRef::StableId(stable_id)),
        ),
    );
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([enemy]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let before = loader.load(&candidate, &plan).unwrap();
    let original_pixels = before.images()[0].image().pixels().to_vec();
    std::fs::rename(
        project.path().join("assets/textures/player.png"),
        project.path().join("assets/textures/renamed.png"),
    )
    .unwrap();
    std::fs::rename(
        project.path().join("assets/textures/player.png.meta"),
        project.path().join("assets/textures/renamed.png.meta"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("assets/textures/renamed.png.meta"),
        image_meta(&stable_id.to_string(), "textures/renamed.png")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let after = loader.load(&candidate, &plan).unwrap();
    assert_eq!(
        after.images()[0].path().as_str(),
        "assets/textures/renamed.png"
    );
    assert_eq!(after.images()[0].image().pixels(), original_pixels);
    assert_eq!(before.scene(), after.scene());
    assert_eq!(
        before.prefabs()[0].document(),
        after.prefabs()[0].document()
    );
    drop((before, after));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn stable_reference_catalogue_excludes_unreferenced_sources_from_the_runtime_closure() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let image_id = player_image_meta().identity().stable_id();
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("sprite"))
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::StableId(image_id)),
            ),
        )]);
    project.write_scene_source(&scene);
    let unrelated = image_meta(
        "42cb6e01-1885-455a-bd8d-8c6f6ee2bcc6",
        "textures/unreferenced.png",
    );
    std::fs::write(
        project.path().join("assets/textures/unreferenced.png.meta"),
        unrelated.to_json_string().unwrap(),
    )
    .unwrap();
    // A catalogue entry is a locator, not a request to read or import that source.
    std::fs::write(
        project.path().join("assets/textures/unreferenced.png"),
        b"not an image",
    )
    .unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let before = loader.load(&candidate, &plan).unwrap();
    assert_eq!(before.images().len(), 1);
    assert!(before.prefabs().is_empty());
    assert!(
        before
            .startup_content_closure()
            .expect("the loader selected the manifest's startup Scene")
            .members()
            .iter()
            .all(|member| !member.path().as_str().contains("unreferenced"))
    );
    std::fs::write(
        project.path().join("assets/textures/unreferenced.png"),
        b"changed outside closure",
    )
    .unwrap();
    let after = loader.load(&candidate, &plan).unwrap();
    assert_eq!(before.content_digest(), after.content_digest());
    drop((before, after));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn stable_reference_catalogue_rejects_duplicate_uuid_without_publishing_residency() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let image_id = player_image_meta().identity().stable_id();
    project.write_scene_source(&SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("sprite")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::StableId(image_id)),
            ),
        ),
    ]));
    let duplicate = image_meta(&image_id.to_string(), "textures/duplicate.png");
    std::fs::write(
        project.path().join("assets/textures/duplicate.png.meta"),
        duplicate.to_json_string().unwrap(),
    )
    .unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "project.content.asset-identity-conflict")
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn stable_reference_catalogue_uses_the_existing_project_directory_budget() {
    use nara::project_host::{ProjectContentBudgetKind, ProjectContentLimits};

    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let image_id = player_image_meta().identity().stable_id();
    project.write_scene_source(&SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("sprite")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::StableId(image_id)),
            ),
        ),
    ]));
    for index in 0..20 {
        std::fs::write(
            project.path().join(format!("assets/unused-{index}.txt")),
            b"bounded enumeration",
        )
        .unwrap();
    }
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::DirectoryEntries, 16)
        .unwrap();
    let loader = ProjectContentLoader::with_limits(root, limits).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn escaping_component_asset_paths_reject_before_source_access() {
    let project = TestProject::with_prefab_startup();
    let prefab_path = project.path().join("prefabs/enemy.prefab.json");
    let mut encoded =
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(prefab_path).unwrap()).unwrap();
    replace_json_string(&mut encoded, "textures/player.png", "../outside.png");
    project.write_prefab_bytes("enemy.prefab.json", &serde_json::to_vec(&encoded).unwrap());
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    // The shared compiler validates surviving root and Prefab values before asset capture.
    assert_eq!(error.kind(), ProjectContentErrorKind::PrefabExpansion);
    assert!(
        error.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "failure-kind"
                        && field.display_value().as_ref() == "component"
                })
        }),
        "{error:?}"
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn document_catalog_mismatch_rejects_without_partial_publication() {
    let project = TestProject::with_prefab_startup();
    let unknown = SceneEntityRecord::new(scene_id("unknown")).with_component(
        ComponentTypeId::new("nara.test.MissingFromFrozenCatalog"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    );
    project.write_scene_source(&SceneDocument::from_ordered_roots([unknown]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::PrefabExpansion);
    assert!(
        error.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.unknown-component"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "component-id"
                        && field.display_value().as_ref() == "nara.test.MissingFromFrozenCatalog"
                })
        }),
        "{error:?}"
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn image_metadata_must_match_the_referenced_path_kind_and_typed_recipe() {
    let path_mismatch = TestProject::with_prefab_startup();
    let (candidate, plan, root) = path_mismatch.candidate_plan_and_root();
    let meta = player_image_meta();
    let (identity, settings) = meta.into_parts();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            identity.stable_id(),
            AssetPath::new("textures/other.png").unwrap(),
            identity.source_kind().clone(),
        ),
        settings,
    );
    path_mismatch.write_player_image_meta(&meta);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let kind_mismatch = TestProject::with_prefab_startup();
    let (candidate, plan, root) = kind_mismatch.candidate_plan_and_root();
    let meta = player_image_meta();
    let (identity, settings) = meta.into_parts();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            identity.stable_id(),
            identity.path().clone(),
            AssetSourceKind::Other("binary".to_owned()),
        ),
        settings,
    );
    kind_mismatch.write_player_image_meta(&meta);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::UnsupportedAssetKind);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let invalid_recipe = TestProject::with_prefab_startup();
    let (candidate, plan, root) = invalid_recipe.candidate_plan_and_root();
    let meta_path = invalid_recipe
        .path()
        .join("assets/textures/player.png.meta");
    let mut encoded =
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&meta_path).unwrap()).unwrap();
    encoded["payload"]["settings"]["color_space"] =
        serde_json::Value::String("display_p3".to_owned());
    std::fs::write(&meta_path, serde_json::to_vec_pretty(&encoded).unwrap()).unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMeta);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn duplicate_stable_asset_ids_reject_the_complete_closure_atomically() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let duplicate = image_meta("3c7c5be4-fd4e-4b65-b8d4-c671f5982186", "textures/other.png");
    project.write_asset("textures/other.png", valid_png_bytes(), &duplicate);
    let player = AssetRef::path("textures/player.png").unwrap();
    let other = AssetRef::path("textures/other.png").unwrap();
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy-player")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&player)),
        ),
        SceneEntityRecord::new(scene_id("enemy-other")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&other)),
        ),
    ]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn cyclic_prefab_closure_rejects_without_requeue_or_partial_publication() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let mut cyclic = SceneEntityRecord::new(scene_id("cycle"));
    cyclic.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: None,
    });
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([cyclic]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::PrefabExpansion);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn hostile_scene_and_image_inputs_publish_no_snapshot_or_residency() {
    let hostile_scene = TestProject::with_prefab_startup();
    let (candidate, plan, root) = hostile_scene.candidate_plan_and_root();
    hostile_scene.write_scene_bytes(br#"{"kind":"scene","format_version":1,"payload":[]}"#);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::SceneFormat);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let hostile_image = TestProject::with_prefab_startup();
    let (candidate, plan, root) = hostile_image.candidate_plan_and_root();
    hostile_image.write_asset("textures/player.png", b"not a png", &player_image_meta());
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::ImageImport);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(feature = "project-workflow")]
#[test]
fn ordinary_check_preserves_safe_scene_and_prefab_format_reasons() {
    for prefab in [false, true] {
        for reason in [
            "syntax-invalid",
            "format-version-unsupported",
            "budget-exceeded",
        ] {
            let project = TestProject::with_prefab_startup();
            let relative = if prefab {
                "prefabs/enemy.prefab.json"
            } else {
                "scenes/startup.scene.json"
            };
            let original = std::fs::read(project.path().join(relative)).unwrap();
            let bytes = match reason {
                "syntax-invalid" => br#"{"private-document-sentinel":"unclosed"#.to_vec(),
                "format-version-unsupported" => {
                    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
                    value["format_version"] = serde_json::json!(999);
                    serde_json::to_vec(&value).unwrap()
                }
                _ => {
                    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
                    value["private-document-sentinel"] =
                        serde_json::Value::String("x".repeat(1024 * 1024 + 1));
                    serde_json::to_vec(&value).unwrap()
                }
            };
            std::fs::write(project.path().join(relative), bytes).unwrap();
            let error = nara::project_workflow::check_project(
                nara::project_workflow::ProjectCheckRequest::new(
                    project.root_capability(),
                    nara::ProductRecipe::new(),
                ),
            )
            .unwrap_err();
            let diagnostic = error
                .content_error()
                .unwrap()
                .diagnostics()
                .iter()
                .next()
                .unwrap();
            assert_eq!(
                diagnostic.code().as_str(),
                if prefab {
                    "project.content.prefab-format-invalid"
                } else {
                    "project.content.scene-format-invalid"
                }
            );
            assert!(
                diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "reason"
                        && field.value() == DiagnosticValueRef::Identifier(reason)
                }),
                "{reason}: {diagnostic:?}"
            );
            for key in match reason {
                "syntax-invalid" => ["line", "column"].as_slice(),
                "format-version-unsupported" => {
                    ["observed_version", "supported_version"].as_slice()
                }
                _ => ["budget", "maximum"].as_slice(),
            } {
                assert!(
                    diagnostic
                        .fields()
                        .iter()
                        .any(|field| field.key().as_str() == *key),
                    "missing {key}: {diagnostic:?}"
                );
            }
            assert!(!format!("{diagnostic:?}").contains("private-document-sentinel"));
        }
    }
}

#[test]
fn legacy_scene_parent_shape_reports_an_actionable_bounded_diagnostic() {
    let project = TestProject::with_prefab_startup();
    project.write_scene_bytes(
        br#"{
            "kind":"scene",
            "format_version":1,
            "engine_min_version":"0.1.0",
            "generator":{"name":"test","version":"0.1.0"},
            "payload":{
                "roots":["current-root","legacy-root"],
                "entities":[
                    {"id":"current-root"},
                    {"id":"legacy-root","parent":null}
                ]
            }
        }"#,
    );
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_legacy_placement_diagnostic(
        &error,
        "project.content.scene-legacy-placement-shape",
        "Rewrite the legacy scene with ordered roots and parent-owned children; sibling order cannot be inferred",
        "scene",
        "startup.scene.json",
        1,
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn legacy_prefab_parent_shape_reports_an_actionable_bounded_diagnostic() {
    let project = TestProject::with_prefab_startup();
    project.write_prefab_bytes(
        "enemy.prefab.json",
        br#"{
            "kind":"prefab",
            "format_version":1,
            "engine_min_version":"0.1.0",
            "generator":{"name":"test","version":"0.1.0"},
            "payload":{
                "roots":["legacy-root"],
                "entities":[{"id":"legacy-root","parent":"removed-parent"}]
            }
        }"#,
    );
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_legacy_placement_diagnostic(
        &error,
        "project.content.prefab-legacy-placement-shape",
        "Rewrite the legacy prefab with ordered roots and parent-owned children; sibling order cannot be inferred",
        "prefab",
        "enemy.prefab.json",
        0,
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

fn assert_legacy_placement_diagnostic(
    error: &ProjectContentError,
    code: &str,
    summary: &str,
    source_key: &str,
    source_path: &str,
    entity_index: u64,
) {
    assert_eq!(error.kind(), ProjectContentErrorKind::SceneFormat);
    let diagnostic = error.diagnostics().iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), code);
    assert_eq!(diagnostic.summary().as_str(), summary);
    assert_eq!(diagnostic.fields().len(), 3);
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "reason"
            && field.class() == DiagnosticFieldClass::Public
            && field.value() == DiagnosticValueRef::Identifier("legacy-placement-shape")
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == source_key
            && field.class() == DiagnosticFieldClass::Public
            && field.value() == DiagnosticValueRef::Identifier(source_path)
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "entity_index"
            && field.class() == DiagnosticFieldClass::Public
            && field.value() == DiagnosticValueRef::Unsigned(entity_index)
    }));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn strict_content_loader_rejects_a_multi_link_source_before_publication() {
    let project = TestProject::with_prefab_startup();
    std::fs::hard_link(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/startup.alias.json"),
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn strict_content_loader_rejects_a_symlinked_source_before_publication() {
    use std::os::unix::fs::symlink;

    let project = TestProject::with_prefab_startup();
    std::fs::rename(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/startup.target.json"),
    )
    .unwrap();
    symlink(
        "startup.target.json",
        project.path().join("scenes/startup.scene.json"),
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn parented_transform_loads_while_inherited_visibility_remains_fail_closed() {
    let transform_project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = transform_project.candidate_plan_and_root();
    let transform_record = |x: f64, y: f64| {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                (
                    "translation",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(x).unwrap()),
                        ("y", ComponentValue::f64(y).unwrap()),
                    ]),
                ),
                ("rotation", ComponentValue::f64(0.0).unwrap()),
                (
                    "scale",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(1.0).unwrap()),
                        ("y", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        )
    };
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let parent = SceneEntityRecord::new(scene_id("parent"))
        .with_children([scene_id("child")])
        .with_component(transform_id.clone(), transform_record(10.0, 20.0));
    let child = SceneEntityRecord::new(scene_id("child"))
        .with_component(transform_id.clone(), transform_record(3.0, 4.0));
    transform_project
        .write_scene_source(&SceneDocument::new([scene_id("parent")], [parent, child]));
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let expanded = snapshot.expanded_scene();
    let parent = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("parent"))
        .unwrap();
    let child = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("child"))
        .unwrap();
    assert_eq!(parent.children, [scene_id("child")]);
    assert!(child.components.contains_key(&transform_id));
    drop(snapshot);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let visibility_project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = visibility_project.candidate_plan_and_root();
    let parent = SceneEntityRecord::new(scene_id("parent"))
        .with_children([scene_id("child")])
        .with_component(
            ComponentTypeId::new("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("hidden".to_owned()),
            ),
        );
    let child = SceneEntityRecord::new(scene_id("child"));
    visibility_project
        .write_scene_source(&SceneDocument::new([scene_id("parent")], [parent, child]));
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::UnsupportedHierarchySemantics
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn project_content_image_payload_cannot_escape_its_snapshot_lease_by_clone() {
    static_assertions::assert_not_impl_any!(nara::image::ImageAsset: Clone);
}

#[test]
fn content_snapshot_exposes_read_only_data_and_keeps_its_owner_private() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/project_content/private_owner.rs");
    cases.compile_fail("tests/ui/project_content/mutable_document.rs");
}

#[test]
fn startup_source_keeps_its_admitted_identity_context_charged() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let source = snapshot.scene_source();
    drop(snapshot);
    assert_eq!(loader.budget_snapshot().active_reservations(), 1);
    assert_eq!(loader.budget_snapshot().active_generations(), 1);
    assert!(
        loader
            .budget_snapshot()
            .active(nara::project_host::ProjectContentBudgetKind::RetainedBytes)
            > 0
    );
    drop(source);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
}

fn replace_json_string(value: &mut serde_json::Value, expected: &str, replacement: &str) {
    match value {
        serde_json::Value::String(current) if current == expected => {
            *current = replacement.to_owned();
        }
        serde_json::Value::Array(values) => {
            for value in values {
                replace_json_string(value, expected, replacement);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                replace_json_string(value, expected, replacement);
            }
        }
        _ => {}
    }
}
