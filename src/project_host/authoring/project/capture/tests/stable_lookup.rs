use nara_reflect::PersistentComponentSchemaOwnerId;

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
    assert_eq!(capture.assets()[0].path().as_str(), "outer.png");
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
    assert!(matches!(
        capture.source_validation().validate(),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { path })
            if path.as_str() == "recovered.prefab.json"
    ));
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
