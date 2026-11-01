use super::*;

#[test]
fn stable_lookup_revalidation_accepts_replaced_equal_metadata_and_rejects_changed_content() {
    let fixture = CatalogueFixture::new();
    let root = fixture.capability();
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    let mut catalogue =
        ReferenceCatalogue::new(root.share(), fixture.directories(), &mut budget).unwrap();
    catalogue
        .resolve_resource(&AssetRef::StableId(fixture.id()), &mut budget)
        .unwrap();
    let observations = catalogue.into_reference_observations(&mut budget).unwrap();
    fs::rename(
        fixture.metadata_path(),
        fixture.root.join("prefabs/old-metadata"),
    )
    .unwrap();
    fs::write(fixture.metadata_path(), fixture.metadata()).unwrap();
    ReferenceCatalogue::revalidate_observations(
        &root,
        fixture.directories(),
        &observations,
        &mut budget,
    )
    .unwrap();
    fs::write(fixture.metadata_path(), format!("{} ", fixture.metadata())).unwrap();
    assert!(matches!(
        ReferenceCatalogue::revalidate_observations(
            &root,
            fixture.directories(),
            &observations,
            &mut budget,
        ),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { .. })
    ));
}

#[test]
fn stable_content_observations_preserve_kind_path_presence_and_global_uuid_checks() {
    for change in ["kind", "path", "missing", "uuid-conflict"] {
        let fixture = CatalogueFixture::new();
        let root = fixture.capability();
        let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
        let mut catalogue =
            ReferenceCatalogue::new(root.share(), fixture.directories(), &mut budget).unwrap();
        catalogue
            .resolve_resource(&AssetRef::StableId(fixture.id()), &mut budget)
            .unwrap();
        let observations = catalogue.into_reference_observations(&mut budget).unwrap();
        match change {
            "kind" | "path" => fs::write(
                fixture.metadata_path(),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        fixture.id(),
                        AssetPath::new(if change == "path" {
                            "elsewhere"
                        } else {
                            "enemy.prefab.json"
                        })
                        .unwrap(),
                        if change == "kind" {
                            AssetSourceKind::Scene
                        } else {
                            AssetSourceKind::Prefab
                        },
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap(),
            "missing" => fs::remove_file(fixture.metadata_path()).unwrap(),
            "uuid-conflict" => fs::write(
                fixture.root.join("assets/conflict.meta"),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        fixture.id(),
                        AssetPath::new("conflict").unwrap(),
                        AssetSourceKind::Image,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        assert!(
            ReferenceCatalogue::revalidate_observations(
                &root,
                fixture.directories(),
                &observations,
                &mut budget,
            )
            .is_err(),
            "{change}"
        );
    }
}

#[test]
fn deferred_binding_accepts_replacement_without_hashing_the_body_but_keeps_metadata_evidence() {
    let fixture = CatalogueFixture::new();
    let path = "assets/future.scene.json";
    let id = StableAssetId::parse_str("3653c8aa-ea3f-451e-92f4-860331f167af").unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            id,
            AssetPath::new("future.scene.json").unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    fs::write(fixture.root.join(format!("{path}.meta")), &metadata).unwrap();
    fs::write(fixture.root.join(path), vec![b'!'; 1024 * 1024]).unwrap();
    let root = fixture.capability();
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    let mut catalogue =
        ReferenceCatalogue::new(root.share(), fixture.directories(), &mut budget).unwrap();
    catalogue
        .resolve_resource(&AssetRef::StableId(id), &mut budget)
        .unwrap();
    let record = catalogue.database.record_for_stable_id(id).unwrap().clone();
    let opened = open_source_file(
        &root,
        ProjectPath::new(path).unwrap(),
        path,
        AuthoringSourceKind::Scene,
        0,
        &mut budget,
    )
    .unwrap();
    catalogue
        .observe_resolved_target(opened, &record, &mut budget)
        .unwrap();
    let observations = catalogue.into_reference_observations(&mut budget).unwrap();
    fs::rename(
        fixture.root.join(path),
        fixture.root.join("assets/previous-body"),
    )
    .unwrap();
    fs::write(
        fixture.root.join(path),
        b"a new malformed body with a different length",
    )
    .unwrap();
    ReferenceCatalogue::revalidate_observations(
        &root,
        fixture.directories(),
        &observations,
        &mut budget,
    )
    .unwrap();
    assert!(budget.usage().aggregate_encoded_bytes() < 64 * 1024);
    let metadata_path = fixture.root.join(format!("{path}.meta"));
    fs::rename(&metadata_path, fixture.root.join("assets/old-sidecar")).unwrap();
    fs::write(&metadata_path, &metadata).unwrap();
    ReferenceCatalogue::revalidate_observations(
        &root,
        fixture.directories(),
        &observations,
        &mut budget,
    )
    .unwrap();
    fs::write(&metadata_path, format!("{metadata} ")).unwrap();
    assert!(
        ReferenceCatalogue::revalidate_observations(
            &root,
            fixture.directories(),
            &observations,
            &mut budget,
        )
        .is_err()
    );
    fs::write(&metadata_path, &metadata).unwrap();
    fs::remove_file(fixture.root.join(path)).unwrap();
    assert!(
        ReferenceCatalogue::revalidate_observations(
            &root,
            fixture.directories(),
            &observations,
            &mut budget,
        )
        .is_err()
    );
}

#[test]
fn deferred_binding_revalidation_refusal_releases_its_temporary_source_storage() {
    let fixture = CatalogueFixture::new();
    let target = ResolvedTargetObservation {
        kind: AuthoringSourceKind::Scene,
        project_path: ProjectPath::new("assets/missing.scene.json").unwrap(),
        dependency_depth: 0,
    };
    let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
    assert!(
        target
            .revalidate(&fixture.capability(), &mut budget)
            .is_err()
    );
    assert_eq!(budget.usage().retained_candidate_bytes(), 0);
    assert!(budget.usage().decode_work() > 0);
}

#[test]
fn more_than_sixty_four_lookup_observations_revalidate_with_shared_bounded_handles() {
    let fixture = CatalogueFixture::new();
    let ids = (1..=72)
        .map(|index| {
            let id =
                StableAssetId::parse_str(&format!("00000000-0000-4000-8000-{index:012x}")).unwrap();
            let path = format!("source-{index}.scene.json");
            fs::write(
                fixture.root.join(format!("assets/{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        id,
                        AssetPath::new(path).unwrap(),
                        AssetSourceKind::Scene,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            id
        })
        .collect::<Vec<_>>();
    let mut budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits {
        // Isolate the independent handle ceiling from cumulative repeated scan/hash work.
        decode_work: ItemLimit::new(64 * 1024 * 1024).unwrap(),
        ..Default::default()
    });
    let root = fixture.capability();
    let mut catalogue =
        ReferenceCatalogue::new(root.share(), fixture.directories(), &mut budget).unwrap();
    for id in ids {
        catalogue
            .resolve_resource(&AssetRef::StableId(id), &mut budget)
            .unwrap()
            .unwrap();
    }
    let observations = catalogue.into_reference_observations(&mut budget).unwrap();
    assert_eq!(observations.len(), 72);
    for _ in 0..2 {
        ReferenceCatalogue::revalidate_observations(
            &root,
            fixture.directories(),
            &observations,
            &mut budget,
        )
        .unwrap();
    }
    assert_eq!(budget.retained_open_handles, 0);
    assert!(budget.usage().peak_open_handles() <= 64);
}
