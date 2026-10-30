//! Source admission regressions over real production decoders, compiler output and sidecars.

use nara_reflect::encode_asset_reference_value;

use super::scene_dependencies::{fixture, id, links, metadata, open_scene};
use super::*;

#[test]
fn whitespace_padded_scene_and_prefab_dependencies_fit_default_admission_in_both_encodings() {
    for ron in [false, true] {
        let project = fixture(&[2], &[], &[], &[]);
        let extension = if ron { "ron" } else { "json" };
        let prefab_path = format!("p.prefab.{extension}");
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("prefab-leaf").unwrap(),
        )]);
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path(&prefab_path).unwrap(),
            overrides: None,
        });
        let scene = SceneDocument::from_ordered_roots([anchor]);
        if ron {
            for suffix in ["", ".meta"] {
                fs::rename(
                    project.root.join(format!("scenes/b.scene.json{suffix}")),
                    project.root.join(format!("scenes/b.scene.ron{suffix}")),
                )
                .unwrap();
            }
            metadata(&project, "scenes/b.scene.ron", 2, AssetSourceKind::Scene);
        }
        let scene_path = format!("scenes/b.scene.{extension}");
        for (path, encoded) in [
            (
                scene_path.clone(),
                if ron {
                    scene.to_ron_string().unwrap()
                } else {
                    scene.to_json_string().unwrap()
                },
            ),
            (
                format!("prefabs/{prefab_path}"),
                if ron {
                    prefab.to_ron_string().unwrap()
                } else {
                    prefab.to_json_string().unwrap()
                },
            ),
        ] {
            let mut bytes = encoded.into_bytes();
            assert!(bytes.len() < 32 * 1024);
            bytes.resize(32 * 1024, b' ');
            fs::write(project.root.join(path), bytes).unwrap();
        }
        let (snapshot, source, session) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.scenes().len(), 2);
        assert_eq!(capture.prefabs().len(), 1);
        assert_eq!(capture.prefabs()[0].document(), &prefab);
        assert_eq!(capture.prefabs()[0].source().encoded_bytes(), 32 * 1024);
        let dependency = capture
            .scenes()
            .iter()
            .find(|scene| scene.source().project_path().as_str() == scene_path)
            .unwrap();
        assert_eq!(dependency.document(), &scene);
        assert_eq!(dependency.source().encoded_bytes(), 32 * 1024);
        capture.source_validation().validate().unwrap();
    }
}

#[test]
fn nested_stable_native_sources_use_the_actual_sidecar_reader_for_scene_and_prefab() {
    let project = fixture(&[2], &[], &[], &[]);
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("assets = \"assets\"", "assets = \"data\"")
        .replace("prefabs = \"prefabs\"", "prefabs = \"data/prefabs\"")
        .replace("'project_location'", "'stable_metadata'");
    fs::write(manifest_path, manifest).unwrap();
    fs::rename(project.root.join("assets"), project.root.join("data")).unwrap();
    fs::rename(
        project.root.join("prefabs"),
        project.root.join("data/prefabs"),
    )
    .unwrap();
    metadata(
        &project,
        "scenes/startup.scene.json",
        0,
        AssetSourceKind::Scene,
    );
    for suffix in ["", ".meta"] {
        fs::rename(
            project.root.join(format!("scenes/b.scene.json{suffix}")),
            project
                .root
                .join(format!("data/prefabs/b.scene.json{suffix}")),
        )
        .unwrap();
    }
    // These declarations select the outer Assets reader, despite the nested Prefabs root.
    metadata(
        &project,
        "data/prefabs/b.scene.json",
        2,
        AssetSourceKind::Scene,
    );
    metadata(
        &project,
        "data/prefabs/p.prefab.json",
        5,
        AssetSourceKind::Prefab,
    );
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("actual-leaf").unwrap(),
    )]);
    fs::write(
        project.root.join("data/prefabs/p.prefab.json"),
        prefab.to_json_string().unwrap(),
    )
    .unwrap();
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::StableId(id(5)),
        overrides: None,
    });
    let dependency = SceneDocument::from_ordered_roots([anchor]);
    fs::write(
        project.root.join("data/prefabs/b.scene.json"),
        dependency.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/b.scene.json"),
        b"the wrong reader must remain unused",
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    let scene = capture
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == "data/prefabs/b.scene.json")
        .unwrap();
    assert_eq!(scene.document(), &dependency);
    assert_eq!(
        scene.native_source().key(),
        &nara_scene::SourceDocumentKey::stable_asset(id(2), nara_scene::SourceDocumentKind::Scene)
    );
    assert_eq!(capture.prefabs().len(), 1);
    assert_eq!(
        capture.prefabs()[0].source().project_path().as_str(),
        "data/prefabs/p.prefab.json"
    );
    assert_eq!(capture.prefabs()[0].document(), &prefab);
    assert_eq!(
        capture.prefabs()[0].native_source().key(),
        &nara_scene::SourceDocumentKey::stable_asset(id(5), nara_scene::SourceDocumentKind::Prefab)
    );
    capture.source_validation().validate().unwrap();

    // Ordinary indexed Open has no explicit reader. The same outer-root declarations must
    // retain their stable identity when a nested configured root also contains the files.
    let (opened_scene_source, opened_scene) = open_scene(&snapshot, "data/prefabs/b.scene.json");
    assert_eq!(opened_scene.document(), &dependency);
    assert_eq!(
        opened_scene_source.relative_path(),
        "data/prefabs/b.scene.json"
    );
    assert_eq!(
        opened_scene_source.source_key(),
        scene.native_source().key()
    );
    opened_scene_source
        .revalidate_native_source(&snapshot.project_root())
        .unwrap();
    let (opened_prefab_source, opened_prefab) =
        super::prefab_acquisition::open_prefab(&snapshot, "data/prefabs/p.prefab.json");
    assert_eq!(opened_prefab.document(), &prefab);
    assert_eq!(
        opened_prefab_source.relative_path(),
        "data/prefabs/p.prefab.json"
    );
    assert_eq!(
        opened_prefab_source.source_key(),
        capture.prefabs()[0].native_source().key()
    );
    opened_prefab_source
        .revalidate_native_source(&snapshot.project_root())
        .unwrap();

    // A narrow capability reads the same sidecar without changing its outer-root identity.
    let directory = snapshot
        .project_root()
        .open_directory_exact(&RelativePath::new("data/prefabs").unwrap())
        .unwrap();
    let native = NativeDocumentSourcePolicy::new(snapshot.settings())
        .admit_in_directory(
            &directory,
            &ProjectPath::new("data/prefabs").unwrap(),
            "p.prefab.json",
            nara_scene::SourceDocumentKind::Prefab,
            opened_prefab_source.identity(),
        )
        .unwrap();
    assert_eq!(native.key(), opened_prefab_source.source_key());
    assert_eq!(
        native.metadata_source().unwrap().project_path().as_str(),
        "data/prefabs/p.prefab.json.meta"
    );
    native.revalidate(&directory).unwrap();

    // Both orders must admit the first read and reuse one canonical source for the other alias.
    for references in [
        [
            AssetRef::path("p.prefab.json").unwrap(),
            AssetRef::StableId(id(5)),
        ],
        [
            AssetRef::StableId(id(5)),
            AssetRef::path("p.prefab.json").unwrap(),
        ],
    ] {
        let dependency = SceneDocument::from_ordered_roots(references.into_iter().enumerate().map(
            |(index, source)| {
                let mut anchor =
                    SceneEntityRecord::new(SceneEntityId::new(format!("anchor-{index}")).unwrap());
                anchor.prefab = Some(PrefabInstance {
                    source,
                    overrides: None,
                });
                anchor
            },
        ));
        fs::write(
            project.root.join("data/prefabs/b.scene.json"),
            dependency.to_json_string().unwrap(),
        )
        .unwrap();
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(capture.scenes().len(), 2);
        assert_eq!(capture.prefabs().len(), 1);
        let captured_prefab = &capture.prefabs()[0];
        assert_eq!(
            captured_prefab.source().project_path().as_str(),
            "data/prefabs/p.prefab.json"
        );
        assert_eq!(captured_prefab.document(), &prefab);
        assert_eq!(captured_prefab.native_source().key(), native.key());
        let scene = capture
            .scenes()
            .iter()
            .find(|scene| scene.source().project_path().as_str() == "data/prefabs/b.scene.json")
            .unwrap();
        assert_eq!(scene.document(), &dependency);
        assert_eq!(scene.effective_document().unwrap().entities.len(), 4);
        capture.source_validation().validate().unwrap();
    }
}

#[test]
fn path_self_edge_reads_its_metadata_without_indexing_unrelated_sources_or_redecoding_the_root() {
    let project = fixture(&[], &[], &[], &[]);
    let mut document = links(&[1]);
    document.entities[0]
        .components
        .values_mut()
        .next()
        .unwrap()
        .value = nara_reflect::ComponentValue::map([(
        "image",
        encode_asset_reference_value(&AssetRef::path("self.scene.json").unwrap()),
    )]);
    fs::write(
        project.root.join("assets/self.scene.json"),
        document.to_json_string().unwrap(),
    )
    .unwrap();
    metadata(
        &project,
        "assets/self.scene.json",
        5,
        AssetSourceKind::Scene,
    );
    let (snapshot, _, _) = open_second_scene(&project);
    let (source, session) = open_scene(&snapshot, "assets/self.scene.json");
    fs::write(
        project.root.join("assets/unused.png.meta"),
        b"unused metadata must not be indexed",
    )
    .unwrap();
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(capture.scenes().len(), 1);
    assert_eq!(capture.scenes()[0].document(), session.document());
    assert!(capture.stable_lookups.is_empty());
    assert_eq!(
        capture
            .source_facts()
            .filter(|fact| fact.project_path().as_str() == "assets/self.scene.json")
            .count(),
        1
    );
    assert!(
        capture
            .source_facts()
            .any(|fact| fact.project_path().as_str() == "assets/self.scene.json.meta")
    );
    assert_eq!(
        capture
            .asset_database
            .record_for_stable_id(id(5))
            .unwrap()
            .source_kind(),
        &AssetSourceKind::Scene
    );
    capture.source_validation().validate().unwrap();
}

#[test]
fn real_compiler_output_transfers_its_exact_reservation_without_a_second_candidate_allowance() {
    let project = fixture(&[], &[], &[], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let mut report = nara_scene::__private::try_expand_authoring_prefabs_with_loader(
        session.document(),
        &session.authoring_schema_basis().unwrap(),
        Default::default(),
        None,
        |_, _| Ok::<_, AuthoringProjectSourceAdmissionError>(None),
    )
    .unwrap();
    let working = report.checked_work_bytes().unwrap();
    let candidate = report
        .candidate
        .take()
        .unwrap()
        .bind_source(source.source_key().clone());
    drop(report);
    let retained = crate::scene_retention::direct_startup_scene_retained_bytes(&candidate).unwrap();
    let mut reservation = working.max(retained);
    let baseline = snapshot.schema.budget.retained_candidate_bytes;
    let maximum = baseline + reservation;
    let mut budget = snapshot.schema.budget;
    budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    budget.charge_source_bytes(0, 0, 0, reservation).unwrap();
    assert!(retained > maximum - budget.retained_candidate_bytes);
    let pointer = std::ptr::from_ref(candidate.document());
    scenes::retain_compiler_result(&mut budget, &mut reservation, retained).unwrap();
    assert_eq!(reservation, 0);
    assert_eq!(budget.retained_candidate_bytes, baseline + retained);
    assert_eq!(std::ptr::from_ref(candidate.document()), pointer);
}

#[test]
fn borrowed_input_costs_match_captured_owners_and_unavailable_prefab_never_falls_back_to_disk() {
    let project = fixture(&[], &[], &[], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let (prefab_source, prefab) =
        super::prefab_acquisition::open_prefab(&snapshot, "prefabs/outer.prefab.json");
    let scene_input = AuthoringSceneSessionSnapshot::capture(&source, &session);
    assert_eq!(
        AuthoringSceneSessionSnapshot::capture_retained_bytes(&source, &session).unwrap(),
        scenes::open_scene_inputs_retained_bytes(&[scene_input]).unwrap()
    );
    let unavailable = AuthoringPrefabSessionSnapshot::unavailable(
        prefab_source.source_key().clone(),
        Arc::from(prefab_source.relative_path()),
        &prefab,
    );
    assert_eq!(
        AuthoringPrefabSessionSnapshot::unavailable_retained_bytes(
            prefab_source.source_key(),
            prefab_source.relative_path(),
            &prefab,
        )
        .unwrap(),
        prefabs::open_prefab_inputs_retained_bytes(std::slice::from_ref(&unavailable)).unwrap()
    );
    let mut document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("anchor").unwrap(),
    )]);
    document.entities[0].prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join(SECOND_SCENE),
        document.to_json_string().unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    assert!(
        matches!(snapshot.capture_scene_document(&source, &session, &[unavailable]),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { kind: AuthoringSourceKind::Prefab, path })
            if path.as_str() == "prefabs/outer.prefab.json")
    );
}
