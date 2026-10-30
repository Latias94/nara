#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        AssetSourcePath, ImportedProductId, SourceHash, StableAssetId,
    },
    fs::RelativePath,
    project::ProjectPath,
    project_host::{
        ProjectContentBudgetKind, ProjectContentBudgetSnapshot, ProjectContentErrorKind,
        ProjectContentLoader, ProjectContentMemberKind,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        SourceDocumentOrigin,
    },
};
use project_content_fixture::{
    TestProject, identity_transform_record, image_meta, scene_id, sprite_value, tileset_meta,
    valid_png_bytes,
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
fn late_prefab_metadata_discovery_checks_the_binding_before_snapshot_publication() {
    let project = TestProject::with_prefab_startup();
    let prefab_id: StableAssetId = "30ebf595-f6e3-4df8-ad26-0b979486c670".parse().unwrap();
    let image = AssetRef::StableId(
        project_content_fixture::player_image_meta()
            .identity()
            .stable_id(),
    );
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("enemy"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
        )]);
    project.write_prefab("enemy.prefab.json", &prefab);
    project.write_prefab_meta("enemy.prefab.json", prefab_id);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let before = loader.load(&candidate, &plan).unwrap();
    let key = before.prefabs()[0].source_key().clone();
    let baseline = loader.budget_snapshot();

    // The Prefab Path loads before effective Image references require a full catalogue.
    // Its adjacent metadata must not become a different association only on later expansion.
    fs::write(
        project.path().join("prefabs/enemy.prefab.json.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                prefab_id,
                AssetPath::new("enemy.prefab.json").unwrap(),
                AssetSourceKind::Image,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.prefab-source-association-invalid"
    }));
    assert_eq!(before.prefabs()[0].source_key(), &key);
    assert_eq!(before.prefabs()[0].document(), &prefab);
    assert_eq!(
        before
            .prefab_resolution()
            .resolve_ref(&AssetRef::StableId(prefab_id))
            .unwrap()
            .source_kind(),
        &AssetSourceKind::Prefab
    );
    let after = loader.budget_snapshot();
    assert_eq!(after.active_reservations(), baseline.active_reservations());
    for kind in [
        ProjectContentBudgetKind::RetainedBytes,
        ProjectContentBudgetKind::ArtifactBytes,
    ] {
        assert_eq!(after.active(kind), baseline.active(kind));
    }
    drop(before);
    assert_no_active_charges(loader.budget_snapshot());
}

#[test]
fn custom_source_roots_resolve_same_local_aliases_through_the_complete_loader() {
    for prefab_root in ["content/templates", "content/art/templates"] {
        let project = TestProject::with_prefab_startup();
        let asset_root = "content/art";
        // Both concrete source owners accept RON; different roots give the same alias
        // distinct meanings. The Image dependency remains a real PNG source.
        let alias = AssetPath::new("shared.ron").unwrap();
        let image_alias = AssetPath::new("textures/shared.png").unwrap();
        let image_id = StableAssetId::parse_str("b7d0435a-b2a9-40a4-941e-d6db426a1674").unwrap();
        let prefab_id = StableAssetId::parse_str("7a08bfbd-fd51-4fef-9dc7-356d91af8f1f").unwrap();
        let tileset_id: StableAssetId = "e3b9883e-3e7b-4d0d-9749-2398fdf6c63f".parse().unwrap();
        let image_product = AssetProductRef::new(image_id, ImportedProductId::PRIMARY).unwrap();
        let prefab_product = AssetProductRef::new(prefab_id, ImportedProductId::PRIMARY).unwrap();
        let image_references = [
            AssetRef::Path(image_alias.clone()),
            AssetRef::StableId(image_id),
            AssetRef::Product(image_product),
        ];
        let prefab_references = [
            AssetRef::Path(alias.clone()),
            AssetRef::StableId(prefab_id),
            AssetRef::Product(prefab_product),
        ];
        let tileset_references = [
            AssetRef::Path(alias.clone()),
            AssetRef::StableId(tileset_id),
            AssetRef::Product(
                AssetProductRef::new(tileset_id, ImportedProductId::PRIMARY).unwrap(),
            ),
        ];
        let sprite_type = ComponentTypeId::new("nara.sprite.Sprite");
        let prefab = PrefabDocument::from_ordered_roots(image_references.iter().enumerate().map(
            |(index, reference)| {
                SceneEntityRecord::new(scene_id(&format!("image-{index}")))
                    .with_component(
                        ComponentTypeId::new("nara.transform.Transform2d"),
                        identity_transform_record(),
                    )
                    .with_component(
                        sprite_type.clone(),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            sprite_value(reference),
                        ),
                    )
                    .with_component(
                        ComponentTypeId::new("nara.tilemap.Tilemap"),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            nara::reflect::ComponentValue::map([
                                (
                                    "tile_size",
                                    nara::reflect::ComponentValue::map([
                                        ("x", nara::reflect::ComponentValue::f64(1.0).unwrap()),
                                        ("y", nara::reflect::ComponentValue::f64(1.0).unwrap()),
                                    ]),
                                ),
                                (
                                    "tileset",
                                    nara::reflect::encode_asset_reference_value(
                                        &tileset_references[index],
                                    ),
                                ),
                                ("cells", nara::reflect::ComponentValue::List(Vec::new())),
                            ]),
                        ),
                    )
            },
        ));
        let scene = SceneDocument::from_ordered_roots(prefab_references.iter().enumerate().map(
            |(index, reference)| {
                let mut anchor = SceneEntityRecord::new(scene_id(&format!("anchor-{index}")))
                    .with_component(
                        ComponentTypeId::new("nara.transform.Transform2d"),
                        identity_transform_record(),
                    );
                anchor.prefab = Some(PrefabInstance {
                    source: reference.clone(),
                    overrides: None,
                });
                anchor
            },
        ));
        project.write_asset(
            image_alias.as_str(),
            valid_png_bytes(),
            &image_meta(&image_id.to_string(), image_alias.as_str()),
        );
        project.write_tileset_source(
            alias.as_str(),
            &nara::tilemap::TileSetSource::new(
                AssetRef::StableId(image_id),
                nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
            ),
            &tileset_meta(&tileset_id.to_string(), alias.as_str()),
        );
        project.write_prefab_bytes(alias.as_str(), prefab.to_ron_string().unwrap().as_bytes());
        project.write_prefab_meta(alias.as_str(), prefab_id);
        project.write_scene_source(&scene);

        // Discovery must bind these sidecars by location, without activating their source kind.
        // In the nested configuration the Assets reader observes each through another local path.
        for (path, kind, id) in [
            (
                "unused.source",
                AssetSourceKind::Unknown,
                "428ad50e-b970-4b0a-b01f-72fa0b04cd90",
            ),
            (
                "unused.scene.json",
                AssetSourceKind::Scene,
                "adbeea2e-a9c5-4882-9642-085ce7fae3d9",
            ),
            (
                "unused.png",
                AssetSourceKind::Image,
                "7de4ebfc-56d4-4c75-8306-7f53fb238c9a",
            ),
        ] {
            project.write_prefab_bytes(path, b"unreferenced source must not be decoded");
            fs::write(
                project.path().join("prefabs").join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str(id).unwrap(),
                        AssetPath::new(path).unwrap(),
                        kind,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        fs::create_dir_all(project.path().join("content")).unwrap();
        fs::rename(
            project.path().join("assets"),
            project.path().join(asset_root),
        )
        .unwrap();
        fs::rename(
            project.path().join("prefabs"),
            project.path().join(prefab_root),
        )
        .unwrap();
        let manifest_path = project.path().join("nara.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("assets = \"assets\"", &format!("assets = \"{asset_root}\""))
            .replace(
                "prefabs = \"prefabs\"",
                &format!("prefabs = \"{prefab_root}\""),
            );
        fs::write(manifest_path, manifest).unwrap();

        let (candidate, plan, root) = project.candidate_plan_and_root();
        let loader = ProjectContentLoader::new(root.share()).unwrap();
        let snapshot = loader
            .load(&candidate, &plan)
            .unwrap_or_else(|error| panic!("{prefab_root}: {error:?}"));
        let image_path = AssetSourcePath::new(format!("{asset_root}/{image_alias}")).unwrap();
        let tileset_path = AssetSourcePath::new(format!("{asset_root}/{alias}")).unwrap();
        let prefab_path = AssetSourcePath::new(format!("{prefab_root}/{alias}")).unwrap();
        assert_ne!(image_path, prefab_path);
        assert_ne!(tileset_path, prefab_path);
        assert_eq!(snapshot.scene(), &scene);
        assert_eq!(snapshot.prefabs().len(), 1);
        assert_eq!(snapshot.prefabs()[0].path(), &prefab_path);
        assert_eq!(snapshot.prefabs()[0].document(), &prefab);
        assert!(matches!(
            snapshot.prefabs()[0].source_key().origin(),
            SourceDocumentOrigin::ProjectLocation { path, .. } if path == &prefab_path
        ));
        assert_eq!(snapshot.images().len(), 1);
        assert_eq!(snapshot.tilesets().len(), 1);
        assert_eq!(snapshot.tilesets()[0].path(), &tileset_path);
        assert_eq!(snapshot.tilesets()[0].image_path(), &image_alias);
        let image = &snapshot.images()[0];
        assert_eq!(image.path(), &image_path);
        assert_eq!(image.source().stable_id(), image_id);
        assert_eq!(
            image.source().source_hash(),
            SourceHash::from_bytes(valid_png_bytes()),
        );
        assert_eq!(image.image().product(), Some(image_product));
        assert_eq!(image.image().pixels().len(), 4);
        assert_eq!(
            snapshot
                .expanded_scene()
                .entities
                .iter()
                .filter(|entity| entity.components.contains_key(&sprite_type))
                .count(),
            9,
        );
        for (resolution, references, path, id, kind, local_alias) in [
            (
                snapshot.asset_resolution(),
                &image_references,
                &image_path,
                image_id,
                AssetSourceKind::Image,
                &image_alias,
            ),
            (
                snapshot.prefab_resolution(),
                &prefab_references,
                &prefab_path,
                prefab_id,
                AssetSourceKind::Prefab,
                &alias,
            ),
            (
                snapshot.asset_resolution(),
                &tileset_references,
                &tileset_path,
                tileset_id,
                AssetSourceKind::Other("tileset".to_owned()),
                &alias,
            ),
        ] {
            let record = resolution.resolve_ref(&references[0]).unwrap();
            assert_eq!(record.path(), path);
            assert_eq!(record.stable_id(), id);
            assert_eq!(record.source_kind(), &kind);
            assert_eq!(
                resolution.scope().relativize(path),
                Some(local_alias.clone())
            );
            for reference in references {
                assert!(std::ptr::eq(
                    record,
                    resolution.resolve_ref(reference).unwrap()
                ));
            }
        }

        let receipt = snapshot.content_closure();
        assert_eq!(receipt.project_root_identity(), root.identity());
        for (kind, path) in [
            (
                ProjectContentMemberKind::AssetSource,
                image_path.as_str().to_owned(),
            ),
            (
                ProjectContentMemberKind::Prefab,
                prefab_path.as_str().to_owned(),
            ),
            (
                ProjectContentMemberKind::AssetSource,
                tileset_path.as_str().to_owned(),
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                image_path.meta_path(),
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                prefab_path.meta_path(),
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                tileset_path.meta_path(),
            ),
        ] {
            assert_eq!(
                receipt
                    .members()
                    .iter()
                    .filter(|member| { member.kind() == kind && member.path().as_str() == path })
                    .count(),
                1,
            );
        }
        assert!(
            receipt
                .members()
                .iter()
                .all(|member| !member.path().as_str().contains("unused."))
        );
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        assert_eq!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::ArtifactBytes),
            0,
        );
        assert!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::RetainedBytes)
                > 0
        );
        for member in receipt.members() {
            root.open_file_exact(&RelativePath::new(member.path().as_str()).unwrap())
                .unwrap()
                .verify_digest(member.digest(), member.byte_len())
                .unwrap();
        }
        let image_receipt = receipt
            .members()
            .iter()
            .find(|member| {
                member.kind() == ProjectContentMemberKind::AssetSource
                    && member.path().as_str() == image_path.as_str()
            })
            .unwrap();
        fs::write(
            project.path().join(image_path.as_str()),
            b"changed actual Image source",
        )
        .unwrap();
        assert!(
            root.open_file_exact(&RelativePath::new(image_receipt.path().as_str()).unwrap())
                .unwrap()
                .verify_digest(image_receipt.digest(), image_receipt.byte_len())
                .is_err()
        );
        drop(receipt);
        assert_no_active_charges(loader.budget_snapshot());
    }
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
        source.local_entity_id() == &scene_id("second-enemy")
            && source
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &scene_id("second-anchor"))
    }));
    assert_eq!(second.prefabs().len(), 1);
    assert_eq!(
        second.prefabs()[0].path().as_str(),
        "prefabs/second.prefab.json"
    );
    assert_eq!(second.images().len(), 1);
    assert_eq!(
        second.images()[0].path().as_str(),
        "assets/textures/second.png"
    );
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
