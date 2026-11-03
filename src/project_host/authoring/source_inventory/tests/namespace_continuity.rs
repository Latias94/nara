use nara_core::ItemLimit;

use super::*;

const ORIGINAL: &str = "scenes/original.scene.json";
const Q: &str = "prefabs/new.prefab.ron";
const SIDECAR: &str = "prefabs/new.prefab.ron.meta";

fn capture_configured(
    schema: &AuthoringSchemaReceipt,
    manifest_bytes: &[u8],
    profile: Option<&str>,
    observed_files: &[ObservedFileClaim<'_>],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> SourceInventory {
    with_candidate_storage(budget, |budget| {
        let manifest = SourceInventoryManifest::parse(manifest_bytes, budget)?;
        let settings = manifest
            .value
            .resolve_profile(profile)
            .map_err(|_| SourceInventoryError::ProfileInvalid)?;
        let root = schema.project_root();
        SourceInventory::capture_admitted(
            SourceInventoryInput {
                root: &root,
                manifest_bytes,
                manifest_identity: schema.manifest_identity(),
                manifest_digest: ContentDigest::of_bytes(manifest_bytes),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(&settings),
            },
            manifest,
            &[],
            &[],
            observed_files,
            SceneFileLimits::default(),
            budget,
        )
    })
    .unwrap()
}

#[test]
fn ordinary_new_participant_and_sidecar_preserve_definitions_after_raw_observations() {
    for q_present in [false, true] {
        for sidecar_present in [false, true] {
            let project = Fixture::new("stable_metadata", "");
            project.scene(ORIGINAL);
            project.metadata(
                ORIGINAL,
                "original.scene.json",
                FIRST_ID,
                AssetSourceKind::Scene,
                0,
            );
            if q_present {
                project.prefab(Q);
            }
            if sidecar_present {
                project.metadata(Q, "new.prefab.ron", SECOND_ID, AssetSourceKind::Prefab, 0);
            }
            let schema = project.receipt(None);
            let mut budget = schema.budget;
            let original = capture_configured(
                &schema,
                schema.manifest_bytes(),
                None,
                &[
                    ObservedFileClaim {
                        path: Q,
                        maximum_bytes: 4096,
                    },
                    ObservedFileClaim {
                        path: SIDECAR,
                        maximum_bytes: 4096,
                    },
                ],
                &mut budget,
            );
            for (path, present) in [(Q, q_present), (SIDECAR, sidecar_present)] {
                assert_eq!(
                    matches!(
                        original.file_observation(path),
                        Some(SourceFileObservation::Present { .. })
                    ),
                    present,
                );
            }
            project.prefab(Q);
            project.metadata(Q, "new.prefab.ron", SECOND_ID, AssetSourceKind::Prefab, 0);
            let current =
                SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
                    .unwrap();
            assert_eq!(paths(&original), [ORIGINAL]);
            assert_eq!(paths(&current), [Q, ORIGINAL]);
            assert_eq!(original.metadata().len(), 1);
            assert_eq!(current.metadata().len(), 2);
            assert!(current.file_observation(Q).is_none());
            assert!(current.file_observation(SIDECAR).is_none());
            assert_ne!(current.configurations(), original.configurations());
            assert!(current.configurations()[0].same_definition(&original.configurations()[0]));
            assert_ne!(current, original);
            let before = budget.usage();
            current
                .validate_namespace_continuity(&original, &mut budget)
                .unwrap();
            assert_eq!(
                budget.usage().retained_candidate_bytes(),
                before.retained_candidate_bytes(),
            );
            assert_eq!(
                budget.usage().retained_open_handles(),
                before.retained_open_handles(),
            );
            assert!(budget.usage().decode_work() > before.decode_work());
        }
    }
}

#[test]
fn another_actual_project_root_is_an_authority_refusal() {
    let project = Fixture::new("project_location", "");
    let other = Fixture::new("project_location", "");
    let schema = project.receipt(None);
    let other_schema = other.receipt(None);
    assert_eq!(schema.manifest_bytes(), other_schema.manifest_bytes());
    let mut budget = schema.budget;
    let original =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let current =
        SourceInventory::capture(&other_schema, &[], SceneFileLimits::default(), &mut budget)
            .unwrap();
    let original_limit = budget.limits.decode_work;
    budget.limits.decode_work = ItemLimit::new(budget.usage().decode_work()).unwrap();
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    budget.limits.decode_work = original_limit;
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::AuthorityChanged)
    ));
}

#[test]
fn an_original_missing_root_cannot_become_an_available_root() {
    let project = Fixture::new(
        "project_location",
        "[profiles.unused.paths]\nscenes = 'late/scenes'",
    );
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let original =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    fs::create_dir_all(project.root.join("late/scenes")).unwrap();
    let current =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::MembershipChanged)
    ));
}

#[test]
fn retained_ancestors_outside_enumerated_roots_prevent_or_refuse_replacement() {
    let project = Fixture::new("project_location", "");
    fs::create_dir_all(project.root.join("outputs/nested")).unwrap();
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let observed = [ObservedFileClaim {
        path: "outputs/nested/new.prefab.ron",
        maximum_bytes: 4096,
    }];
    let original = capture_configured(
        &schema,
        schema.manifest_bytes(),
        None,
        &observed,
        &mut budget,
    );
    let replaced = match fs::rename(
        project.root.join("outputs"),
        project.root.join("previous-outputs"),
    ) {
        Ok(()) => true,
        Err(error) if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied => {
            false
        }
        Err(error) => panic!("ancestor replacement failed unexpectedly: {error}"),
    };
    if !replaced {
        // Windows may enforce the retained directory custody by refusing its rename. This
        // proves prevention, not a successfully injected ancestor-replacement rejection.
        assert!(project.root.join("outputs/nested").is_dir());
        assert!(!project.root.join("previous-outputs").exists());
        let current = capture_configured(
            &schema,
            schema.manifest_bytes(),
            None,
            &observed,
            &mut budget,
        );
        current
            .validate_namespace_continuity(&original, &mut budget)
            .unwrap();
        return;
    }
    fs::create_dir_all(project.root.join("outputs/nested")).unwrap();
    let current = capture_configured(
        &schema,
        schema.manifest_bytes(),
        None,
        &observed,
        &mut budget,
    );
    assert_eq!(current.roots, original.roots);
    assert_eq!(current.directories, original.directories);
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::MembershipChanged)
    ));
}

#[test]
fn an_original_missing_sidecar_remains_a_negative_fact() {
    let project = Fixture::new("project_location", "");
    project.scene(ORIGINAL);
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let original =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(original.missing_sidecars().len(), 1);
    project.metadata(
        ORIGINAL,
        "original.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let current =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(paths(&current), paths(&original));
    assert_eq!(current.missing_sidecars().len(), 0);
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::MembershipChanged)
    ));
}

#[test]
fn selected_profile_is_checked_even_when_all_configuration_definitions_match() {
    let project = Fixture::new("project_location", "[profiles.other]");
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let original =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let current = capture_configured(
        &schema,
        schema.manifest_bytes(),
        Some("other"),
        &[],
        &mut budget,
    );
    assert_eq!(current.configurations(), original.configurations());
    assert!(matches!(
        current.validate_namespace_continuity(&original, &mut budget),
        Err(SourceInventoryError::MembershipChanged)
    ));
}

#[test]
fn complete_definitions_include_unselected_profiles_and_native_identity_inputs() {
    for (before, after) in [
        (
            "984edb80-00a9-45fd-9f6c-d2dbf903ed48",
            "94692c80-85a1-49fb-bcb8-4ae59ea50d9a",
        ),
        ("project_location", "stable_metadata"),
        (
            "scripts = 'alternate/scripts'",
            "scripts = 'different/scripts'",
        ),
        ("[profiles.other.paths]", "[profiles.renamed.paths]"),
        (
            "[profiles.other.paths]",
            "[profiles.added]\n[profiles.other.paths]",
        ),
        ("assets = 'assets'", "assets = 'scenes'"),
    ] {
        let project = Fixture::new(
            "project_location",
            "[profiles.other.paths]\nassets = 'assets'\nscripts = 'alternate/scripts'",
        );
        let schema = project.receipt(None);
        let mut budget = schema.budget;
        let original =
            SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
                .unwrap();
        let manifest = std::str::from_utf8(schema.manifest_bytes())
            .unwrap()
            .replace(before, after);
        project.write("nara.toml", manifest.as_bytes());
        let current = capture_configured(&schema, manifest.as_bytes(), None, &[], &mut budget);
        assert!(
            current.configurations().len() != original.configurations().len()
                || current
                    .configurations()
                    .iter()
                    .zip(original.configurations())
                    .any(|(current, original)| !current.same_definition(original)),
            "{before}",
        );
        assert!(matches!(
            current.validate_namespace_continuity(&original, &mut budget),
            Err(SourceInventoryError::MembershipChanged)
        ));
    }
}

#[test]
fn comparison_is_preadmitted_and_refusal_keeps_the_original_cumulative_charge() {
    let project = Fixture::new("project_location", "");
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let before = budget;
    inventory
        .validate_namespace_continuity(&inventory, &mut budget)
        .unwrap();
    let cost = budget.usage().decode_work() - before.usage().decode_work();
    assert!(cost > 0);
    let mut limited = before;
    let maximum = before.usage().decode_work() + cost - 1;
    limited.limits.decode_work = ItemLimit::new(maximum).unwrap();
    assert!(matches!(
        inventory.validate_namespace_continuity(&inventory, &mut limited),
        Err(SourceInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: actual }
        )) if actual == maximum
    ));
    assert_eq!(limited.usage().decode_work(), maximum + 1);
    let retry_maximum = limited.usage().decode_work() + cost;
    limited.limits.decode_work = ItemLimit::new(retry_maximum).unwrap();
    inventory
        .validate_namespace_continuity(&inventory, &mut limited)
        .unwrap();
    assert_eq!(limited.usage().decode_work(), retry_maximum);
    assert_eq!(
        limited.usage().retained_candidate_bytes(),
        before.usage().retained_candidate_bytes(),
    );
    assert_eq!(
        limited.usage().retained_open_handles(),
        before.usage().retained_open_handles(),
    );
}
