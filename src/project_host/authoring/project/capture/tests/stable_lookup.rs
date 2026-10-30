use nara_asset::{AssetProductRef, ImportedProductId};
use nara_reflect::PersistentComponentSchemaOwnerId;
use nara_scene::{
    PrefabProjectionStep, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
};

use super::*;
use crate::project_host::authoring::tests::TestAuthoringProject;

fn prefab_id() -> StableAssetId {
    StableAssetId::parse_str("762d41eb-607a-468d-9e1a-219029b58fe4").unwrap()
}

fn asset_id() -> StableAssetId {
    StableAssetId::parse_str("7ef3f24f-5b03-4f93-ac4c-1343af327d40").unwrap()
}

fn unrelated_id() -> StableAssetId {
    StableAssetId::parse_str("ce0f40a2-6716-498b-ad28-02bf34c8c2e8").unwrap()
}

fn write_metadata(
    project: &TestAuthoringProject,
    directory: &str,
    path: &str,
    stable_id: StableAssetId,
    kind: AssetSourceKind,
) {
    fs::write(
        project.root.join(directory).join(format!("{path}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(stable_id, AssetPath::new(path).unwrap(), kind),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
}

fn prefab_scene(stable_id: StableAssetId) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::StableId(stable_id),
        overrides: None,
    });
    SceneDocument::from_ordered_roots([anchor])
}

fn stable_project(prefab: bool) -> TestAuthoringProject {
    let project = independent_project();
    // This fixture's unused malformed canary is intentionally outside Path-only capture.
    // Stable lookup must instead enumerate a valid catalogue before the review begins.
    write_metadata(
        &project,
        "assets",
        "unused.png",
        unrelated_id(),
        AssetSourceKind::Image,
    );
    write_metadata(
        &project,
        "prefabs",
        "outer.prefab.json",
        prefab_id(),
        AssetSourceKind::Prefab,
    );
    fs::write(
        project.root.join("prefabs/outer.prefab.json"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("leaf").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let document = if prefab {
        prefab_scene(prefab_id())
    } else {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("image").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("nara.test.IndependentImage"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("stable_id".into())),
                        ("value", ComponentValue::String(asset_id().to_string())),
                    ]),
                )]),
            )
            .with_owner(
                PersistentComponentSchemaOwnerId::try_new("nara.test.independent-capture").unwrap(),
            ),
        )])
    };
    fs::write(
        project.root.join(SECOND_SCENE),
        document.to_json_string().unwrap(),
    )
    .unwrap();
    project
}

#[test]
fn primary_product_prefab_preserves_location_identity_on_fresh_and_cached_loads() {
    let product =
        AssetRef::Product(AssetProductRef::new(prefab_id(), ImportedProductId::PRIMARY).unwrap());
    for first in [
        product.clone(),
        AssetRef::path("outer.prefab.json").unwrap(),
    ] {
        let project = stable_project(true);
        let document = SceneDocument::from_ordered_roots(
            [first, product.clone(), AssetRef::StableId(prefab_id())]
                .into_iter()
                .enumerate()
                .map(|(index, reference)| {
                    let mut anchor = SceneEntityRecord::new(
                        SceneEntityId::new(format!("anchor-{index}")).unwrap(),
                    );
                    anchor.prefab = Some(PrefabInstance {
                        source: reference,
                        overrides: None,
                    });
                    anchor
                }),
        );
        fs::write(
            project.root.join(SECOND_SCENE),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        let (snapshot, source, session) = open_second_scene(&project);
        let mut capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.prefabs().len(), 1);
        assert_eq!(capture.stable_lookups.len(), 1);
        let key = capture.prefabs()[0].native_source().key().clone();
        assert!(matches!(
            key.origin(),
            SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == "prefabs/outer.prefab.json"
        ));
        let preview = capture.take_preview().unwrap();
        assert_eq!(preview.candidate.document().entities.len(), 6);
        let mut projections = 0;
        for provenance in preview.candidate.provenance().values() {
            if let Some(path) = provenance.projection_path() {
                projections += 1;
                assert_eq!(path.steps().len(), 1);
                assert!(matches!(
                    &path.steps()[0],
                    PrefabProjectionStep::SourceEntity { canonical_source, .. }
                        if canonical_source == &key
                ));
            }
        }
        assert_eq!(projections, 3);
        assert!(
            capture
                .source_facts()
                .any(|fact| { fact.project_path().as_str() == "prefabs/outer.prefab.json.meta" })
        );
        capture.source_validation().validate().unwrap();
        write_metadata(
            &project,
            "prefabs",
            "outer.prefab.json",
            StableAssetId::parse_str("072c21c6-8790-4cca-9e3c-0d1f51cb20ac").unwrap(),
            AssetSourceKind::Prefab,
        );
        assert!(matches!(
            capture.source_validation().validate(),
            Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: AuthoringSourceKind::AssetMetadata,
                path,
            }) if path.as_str() == "prefabs/outer.prefab.json.meta"
        ));
        assert_eq!(capture.prefabs()[0].native_source().key(), &key);
    }
}

#[test]
fn admitted_prefab_association_conflicts_report_the_actual_catalogue_path() {
    let project = stable_project(true);
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    let prefab = &capture.prefabs()[0];
    let SourceDocumentOrigin::ProjectLocation { universe, .. } =
        prefab.native_source().key().origin()
    else {
        panic!("this fixture requires location-authored identity");
    };
    let product =
        AssetRef::Product(AssetProductRef::new(prefab_id(), ImportedProductId::PRIMARY).unwrap());
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    let mut catalogue = ReferenceCatalogue::new(
        snapshot.project_root(),
        ReferenceCatalogue::project_bindings(snapshot.settings()),
        &mut budget,
    )
    .unwrap();
    assert_eq!(
        catalogue
            .resolve(&product, &catalogue.prefabs(), &mut budget)
            .unwrap(),
        Some(AssetPath::new("outer.prefab.json").unwrap()),
    );
    for key in [
        SourceDocumentKey::stable_asset(unrelated_id(), SourceDocumentKind::Prefab),
        SourceDocumentKey::project_location(
            *universe,
            AssetSourcePath::new("prefabs/different.prefab.json").unwrap(),
            SourceDocumentKind::Prefab,
        ),
    ] {
        let mismatched = ResolvedPrefabSource::bound(Arc::new(prefab.document().clone()), key);
        assert!(matches!(
            catalogue.bind_prefab_source(&product, mismatched),
            Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { path })
                if path.as_str() == "prefabs/outer.prefab.json"
        ));
    }
}

#[test]
fn unreferenced_metadata_kinds_do_not_block_open_or_activate_their_sources() {
    for prefab in [false, true] {
        let project = stable_project(prefab);
        let unreferenced = [
            (
                "assets",
                "unclassified.source",
                AssetSourceKind::Unknown,
                "e9f80da6-6ad7-496e-b8be-f476ca9c3f2a",
            ),
            (
                "prefabs",
                "unreferenced.scene.json",
                AssetSourceKind::Scene,
                "7ab290d6-158f-4713-afaa-88c4c56002f0",
            ),
            (
                "assets",
                "unregistered.source",
                AssetSourceKind::Other("unregistered-domain".into()),
                "d5adef10-e032-4471-bb3c-61958e732c1a",
            ),
        ];
        for (directory, path, kind, id) in &unreferenced {
            fs::write(
                project.root.join(directory).join(path),
                b"invalid unreferenced source",
            )
            .unwrap();
            write_metadata(
                &project,
                directory,
                path,
                StableAssetId::parse_str(id).unwrap(),
                kind.clone(),
            );
        }
        // The ordinary startup Open must discover identities for its real stable reference.
        fs::copy(
            project.root.join(SECOND_SCENE),
            project.root.join("scenes/startup.scene.json"),
        )
        .unwrap();
        let (snapshot, source, session) = open_second_scene(&project);
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
        snapshot.admit_startup_document().unwrap();
        snapshot.startup_source_validation().validate().unwrap();
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.prefabs().len(), usize::from(prefab));
        assert_eq!(capture.assets().len(), usize::from(!prefab));
        let facts = capture
            .source_facts()
            .map(|fact| fact.project_path().as_str())
            .collect::<Vec<_>>();
        for (directory, path, _, _) in &unreferenced {
            let source_path = format!("{directory}/{path}");
            assert!(!facts.contains(&source_path.as_str()));
            assert!(!facts.contains(&format!("{source_path}.meta").as_str()));
        }
        capture.source_validation().validate().unwrap();
    }
}

#[test]
fn overlapping_configured_roots_keep_exact_prefab_and_asset_locations_and_source_origins() {
    for stable_native in [false, true] {
        let project = stable_project(true);
        let manifest_path = project.root.join("nara.toml");
        let mut manifest = fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("assets = \"assets\"", "assets = \"data\"")
            .replace("prefabs = \"prefabs\"", "prefabs = \"data/prefabs\"");
        if stable_native {
            manifest = manifest.replace("'project_location'", "'stable_metadata'");
            for (name, id) in [
                ("startup.scene.json", "d2359c10-53b0-46b8-b3ab-b7530e2bf062"),
                ("second.scene.json", "5ae2741a-8d77-4bb2-87ce-445c3c6dd63f"),
            ] {
                write_metadata(
                    &project,
                    "scenes",
                    name,
                    StableAssetId::parse_str(id).unwrap(),
                    AssetSourceKind::Scene,
                );
            }
        }
        fs::write(&manifest_path, manifest).unwrap();
        fs::rename(project.root.join("assets"), project.root.join("data")).unwrap();
        fs::rename(
            project.root.join("prefabs"),
            project.root.join("data/prefabs"),
        )
        .unwrap();
        let prefab_bytes = fs::read(project.root.join("data/prefabs/leaf.prefab.json")).unwrap();
        fs::write(
            project.root.join("data/prefabs/outer.prefab.json"),
            &prefab_bytes,
        )
        .unwrap();
        fs::write(
            project.root.join("data/outer.prefab.json"),
            b"different source in Assets",
        )
        .unwrap();
        write_metadata(
            &project,
            "data",
            "outer.prefab.json",
            StableAssetId::parse_str("00548957-d79c-441a-9d6e-e4dfb00b11c5").unwrap(),
            AssetSourceKind::Other("text".into()),
        );

        let (snapshot, source, session) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.prefabs().len(), 1);
        assert_eq!(capture.assets().len(), 1);
        let prefab = &capture.prefabs()[0];
        assert_eq!(
            prefab.source().project_path().as_str(),
            "data/prefabs/outer.prefab.json",
        );
        assert_eq!(
            prefab.document().to_json_string().unwrap().as_bytes(),
            prefab_bytes.as_slice(),
        );
        match prefab.native_source().key().origin() {
            nara_scene::SourceDocumentOrigin::StableAsset { stable_id } => {
                assert!(stable_native);
                assert_eq!(*stable_id, prefab_id());
            }
            nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. } => {
                assert!(!stable_native);
                assert_eq!(path.as_str(), "data/prefabs/outer.prefab.json");
            }
            other => panic!("unexpected source origin: {other:?}"),
        }
        let asset = &capture.assets()[0];
        assert_eq!(asset.path().as_str(), "data/leaf.png");
        assert_eq!(asset.bytes(), b"leaf");
        assert_eq!(capture.asset_scope().prefix().unwrap().as_str(), "data");
        assert_eq!(
            capture.prefab_scope().prefix().unwrap().as_str(),
            "data/prefabs",
        );
        assert!(Arc::ptr_eq(&capture.asset_scope(), &capture.asset_scope()));
        assert_eq!(
            capture
                .asset_resolution()
                .record_for_path(&AssetPath::new("leaf.png").unwrap()),
            Some(asset.record()),
        );
        let prefab_record = capture
            .prefab_resolution()
            .resolve_ref(&AssetRef::StableId(prefab_id()))
            .unwrap();
        assert_eq!(
            prefab_record.path().as_str(),
            "data/prefabs/outer.prefab.json"
        );
        capture.source_validation().validate().unwrap();
        fs::write(
            project.root.join("data/prefabs/outer.prefab.json"),
            b"changed actual Prefab",
        )
        .unwrap();
        assert!(capture.source_validation().validate().is_err());
    }
}

#[test]
fn reviewed_prefab_lookup_rejects_new_conflicting_metadata_in_either_source_directory() {
    for (directory, path, kind) in [
        ("prefabs", "conflict.prefab.json", AssetSourceKind::Prefab),
        ("assets", "conflict.png", AssetSourceKind::Image),
    ] {
        let project = stable_project(true);
        let (snapshot, source, session) = open_second_scene(&project);
        let revision = session.revision();
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.stable_lookups.len(), 1);
        capture.source_validation().validate().unwrap();
        let existing = capture
            .source_facts()
            .map(|fact| {
                (
                    fact.project_path().clone(),
                    fs::read(project.root.join(fact.project_path().as_str())).unwrap(),
                )
            })
            .collect::<Vec<_>>();

        write_metadata(&project, directory, path, prefab_id(), kind);
        assert!(matches!(
            capture.source_validation().validate(),
            Err(AuthoringProjectSourceAdmissionError::AssetIdentity {
                source: AssetDatabaseError::DuplicateStableId { .. },
                ..
            }),
        ));
        for (path, bytes) in existing {
            assert_eq!(fs::read(project.root.join(path.as_str())).unwrap(), bytes);
        }
        assert_eq!(session.revision(), revision);
    }
}

#[test]
fn reviewed_asset_lookup_rejects_new_conflicting_prefab_metadata() {
    let project = stable_project(false);
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(capture.assets().len(), 1);
    assert_eq!(capture.assets()[0].path().as_str(), "assets/outer.png");
    assert_eq!(capture.stable_lookups.len(), 1);
    capture.source_validation().validate().unwrap();
    write_metadata(
        &project,
        "prefabs",
        "conflict.prefab.json",
        asset_id(),
        AssetSourceKind::Prefab,
    );
    assert!(matches!(
        capture.source_validation().validate(),
        Err(AuthoringProjectSourceAdmissionError::AssetIdentity {
            source: AssetDatabaseError::DuplicateStableId { .. },
            ..
        }),
    ));
}

#[test]
fn reviewed_successful_lookups_accept_unrelated_new_unique_metadata() {
    for prefab in [false, true] {
        let project = stable_project(prefab);
        let (snapshot, source, session) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        capture.source_validation().validate().unwrap();
        write_metadata(
            &project,
            "assets",
            "new-unique.png",
            StableAssetId::parse_str("7097845b-ea5c-478a-a96d-a97b8cc13831").unwrap(),
            AssetSourceKind::Image,
        );
        capture.source_validation().validate().unwrap();
        assert!(!capture.requires_recapture());
    }
}

#[test]
fn reviewed_successful_lookup_preserves_existing_file_receipts_on_removal_or_rebinding() {
    for rebound in [false, true] {
        let project = stable_project(true);
        let (snapshot, source, session) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        capture.source_validation().validate().unwrap();
        fs::remove_file(project.root.join("prefabs/outer.prefab.json.meta")).unwrap();
        if rebound {
            write_metadata(
                &project,
                "prefabs",
                "rebound.prefab.json",
                prefab_id(),
                AssetSourceKind::Prefab,
            );
        }
        assert!(matches!(
            capture.source_validation().validate(),
            Err(AuthoringProjectSourceAdmissionError::SourceHost { .. })
                | Err(AuthoringProjectSourceAdmissionError::SourceChanged { .. }),
        ));
    }
}

#[test]
fn startup_snapshot_carries_stable_lookup_observations_through_publication_and_session_transfer() {
    let project = stable_project(true);
    fs::copy(
        project.root.join(SECOND_SCENE),
        project.root.join("scenes/startup.scene.json"),
    )
    .unwrap();
    let (snapshot, _, _) = open_second_scene(&project);
    assert_eq!(snapshot.stable_lookups.len(), 1);
    let validation = snapshot.startup_source_validation();
    validation.clone().validate().unwrap();
    write_metadata(
        &project,
        "prefabs",
        "conflict.prefab.json",
        prefab_id(),
        AssetSourceKind::Prefab,
    );
    assert!(matches!(
        validation.validate(),
        Err(AuthoringProjectSourceAdmissionError::AssetIdentity {
            source: AssetDatabaseError::DuplicateStableId { .. },
            ..
        }),
    ));
}

#[test]
fn unresolved_stable_lookup_is_reobserved_and_still_requires_explicit_recapture() {
    let project = stable_project(true);
    let missing = StableAssetId::parse_str("b92f792e-f526-41c0-9fdf-21c55d890cdf").unwrap();
    fs::write(
        project.root.join(SECOND_SCENE),
        prefab_scene(missing).to_json_string().unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(capture.requires_recapture());
    assert_eq!(capture.unresolved_dependencies.len(), 1);
    assert_eq!(capture.stable_lookups.len(), 1);
    capture.source_validation().validate().unwrap();
    write_metadata(
        &project,
        "assets",
        "new-unique.png",
        StableAssetId::parse_str("7097845b-ea5c-478a-a96d-a97b8cc13831").unwrap(),
        AssetSourceKind::Image,
    );
    capture.source_validation().validate().unwrap();
    assert!(capture.requires_recapture());
    write_metadata(
        &project,
        "prefabs",
        "recovered.prefab.json",
        missing,
        AssetSourceKind::Prefab,
    );
    fs::copy(
        project.root.join("prefabs/outer.prefab.json"),
        project.root.join("prefabs/recovered.prefab.json"),
    )
    .unwrap();
    let changed = capture.source_validation().validate();
    assert!(
        matches!(
            &changed,
            Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { path })
                if path.as_str() == "prefabs/recovered.prefab.json"
        ),
        "{changed:?}"
    );
    assert!(capture.requires_recapture());
    let recovered = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(recovered.readiness(), AuthoringProjectReadiness::Complete);
    assert!(!recovered.requires_recapture());
    recovered.source_validation().validate().unwrap();
}

#[test]
fn path_only_capture_revalidation_does_not_activate_unused_metadata() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert!(capture.stable_lookups.is_empty());
    fs::write(
        project.root.join("prefabs/new-invalid.prefab.json.meta"),
        b"invalid metadata must remain unused",
    )
    .unwrap();
    capture.source_validation().validate().unwrap();
}

#[test]
fn source_revalidation_uses_the_same_bounded_budget_for_a_fresh_catalogue() {
    let project = stable_project(true);
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    let mut validation = capture.source_validation();
    drop(capture);
    drop(snapshot);
    let budget = &mut Arc::get_mut(&mut validation.schema)
        .expect("only the validation owns this Schema receipt")
        .budget;
    let maximum = budget.unique_files;
    budget.limits.unique_files = ItemLimit::new(maximum).unwrap();
    assert!(matches!(
        validation.validate(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::UniqueFileLimitExceeded { maximum: observed }
        )) if observed == maximum
    ));
}
