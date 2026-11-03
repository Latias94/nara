//! Exact physical leaves remain separate from native source and metadata membership.

use super::*;

const ORIGINAL: &str = "scenes/original.scene.json";
const Q: &str = "prefabs/new.prefab.ron";
const SIDECAR: &str = "prefabs/new.prefab.ron.meta";
const OUTPUT: &[u8] = b"fixed Q output";

fn fixture() -> Fixture {
    let project = Fixture::new("stable_metadata", "");
    project.scene(ORIGINAL);
    project.metadata(
        ORIGINAL,
        "original.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    project
}

fn observations() -> [ObservedFileClaim<'static>; 2] {
    [
        ObservedFileClaim {
            path: Q,
            maximum_bytes: 4096,
        },
        ObservedFileClaim {
            path: SIDECAR,
            maximum_bytes: 4096,
        },
    ]
}

fn capture_files(
    schema: &AuthoringSchemaReceipt,
    open: &[OpenSource<'_>],
    claims: &[SourceClaim<'_>],
    observed: &[ObservedFileClaim<'_>],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceInventory, SourceInventoryError> {
    with_candidate_storage(budget, |budget| {
        let manifest = SourceInventoryManifest::parse(schema.manifest_bytes(), budget)?;
        let root = schema.project_root();
        SourceInventory::capture_admitted(
            SourceInventoryInput {
                root: &root,
                manifest_bytes: schema.manifest_bytes(),
                manifest_identity: schema.manifest_identity(),
                manifest_digest: schema.manifest_digest(),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(schema.settings()),
            },
            manifest,
            open,
            claims,
            observed,
            SceneFileLimits::default(),
            budget,
        )
    })
}

#[test]
fn stable_metadata_observes_all_four_independent_new_leaf_states_without_native_promotion() {
    for q_present in [false, true] {
        for sidecar_present in [false, true] {
            let project = fixture();
            if q_present {
                project.write(Q, OUTPUT);
            }
            if sidecar_present {
                project.metadata(Q, "new.prefab.ron", SECOND_ID, AssetSourceKind::Prefab, 0);
            }
            let schema = project.receipt(None);
            let mut budget = schema.budget;
            let inventory = capture_files(&schema, &[], &[], &observations(), &mut budget).unwrap();
            assert_eq!(paths(&inventory), [ORIGINAL]);
            assert_eq!(inventory.metadata().len(), 1);
            assert!(inventory.metadata_for_source(Q).is_none());
            assert_eq!(inventory.missing_sidecars().len(), 0);
            assert_eq!(
                inventory.configurations()[0]
                    .participant_paths()
                    .collect::<Vec<_>>(),
                [ORIGINAL]
            );
            for (path, present) in [(Q, q_present), (SIDECAR, sidecar_present)] {
                let parent = inventory.shared_parent(path).unwrap();
                match inventory.file_observation(path).unwrap() {
                    SourceFileObservation::Present { file, bytes } => {
                        assert!(present);
                        assert_eq!(file.path(), path);
                        assert_eq!(Some(parent.identity()), file.parent_identity());
                        assert_eq!(bytes, &fs::read(project.root.join(path)).unwrap());
                        let retained = inventory.shared_file(path).unwrap();
                        assert_eq!(retained.identity(), file.identity());
                        assert_eq!(retained.parent_identity(), file.parent_identity());
                        retained
                            .verify_digest(file.digest(), file.encoded_bytes())
                            .unwrap();
                    }
                    SourceFileObservation::Missing { parent_identity } => {
                        assert!(!present);
                        assert_eq!(*parent_identity, parent.identity());
                        assert!(inventory.shared_file(path).is_none());
                    }
                }
            }
            let retained = budget.usage().retained_candidate_bytes();
            let handles = budget.usage().retained_open_handles();
            let work = budget.usage().decode_work();
            inventory.validate(&schema, &[], &mut budget).unwrap();
            assert_eq!(budget.usage().retained_candidate_bytes(), retained);
            assert_eq!(budget.usage().retained_open_handles(), handles);
            assert!(budget.usage().decode_work() > work);

            let custody = inventory.shared_custody();
            let weak = custody.weak_owner();
            drop(inventory);
            assert!(weak.upgrade().is_some());
            assert!(custody.parent(Q).is_some());
            assert_eq!(custody.file(Q).is_some(), q_present);
            assert_eq!(custody.file(SIDECAR).is_some(), sidecar_present);

            let mut ordinary_budget = schema.budget;
            let ordinary = SourceInventory::capture(
                &schema,
                &[],
                SceneFileLimits::default(),
                &mut ordinary_budget,
            );
            if q_present == sidecar_present {
                let ordinary = ordinary.unwrap();
                assert_eq!(ordinary.participants().len(), 1 + usize::from(q_present));
                assert!(ordinary.file_observation(Q).is_none());
            } else {
                assert!(ordinary.is_err());
            }
        }
    }
}

#[test]
fn exact_observations_preserve_other_discovered_sources_and_reject_required_source_overlap() {
    let project = fixture();
    project.write(Q, OUTPUT);
    project.scene("scenes/extra.scene.json");
    project.metadata(
        "scenes/extra.scene.json",
        "extra.scene.json",
        SECOND_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory = capture_files(&schema, &[], &[], &observations(), &mut budget).unwrap();
    assert_eq!(paths(&inventory), ["scenes/extra.scene.json", ORIGINAL]);
    for path in [ORIGINAL, "scenes/original.scene.json.meta"] {
        let mut budget = schema.budget;
        let retained = budget.usage().retained_candidate_bytes();
        let handles = budget.usage().retained_open_handles();
        let result = capture_files(
            &schema,
            &[],
            &[SourceClaim {
                path: ORIGINAL,
                kind: SourceDocumentKind::Scene,
                explicit_native: false,
            }],
            &[ObservedFileClaim {
                path,
                maximum_bytes: 4096,
            }],
            &mut budget,
        );
        assert!(
            matches!(result, Err(SourceInventoryError::ObservedFileConflict)),
            "{path}: {result:?}"
        );
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
    }
    fs::remove_file(project.root.join(Q)).unwrap();
    let open = project.open_source(&schema, ORIGINAL);
    let mut budget = schema.budget;
    assert!(matches!(
        capture_files(
            &schema,
            &[OpenSource::Bound(&open)],
            &[],
            &[ObservedFileClaim {
                path: ORIGINAL,
                maximum_bytes: 4096,
            }],
            &mut budget,
        ),
        Err(SourceInventoryError::ObservedFileConflict)
    ));
    let configured = Fixture::new(
        "project_location",
        "[content]\nscenes = ['required.scene.json']",
    );
    let schema = configured.receipt(None);
    let mut budget = schema.budget;
    assert!(matches!(
        capture_files(
            &schema,
            &[],
            &[],
            &[ObservedFileClaim {
                path: "scenes/required.scene.json",
                maximum_bytes: 4096,
            }],
            &mut budget,
        ),
        Err(SourceInventoryError::ObservedFileConflict)
    ));
}

#[test]
fn new_leaf_presence_content_and_identity_changes_refuse_without_refunding_live_custody() {
    for change in 0..3 {
        let project = fixture();
        if change != 0 {
            project.write(Q, OUTPUT);
        }
        let schema = project.receipt(None);
        let mut budget = schema.budget;
        let inventory = capture_files(&schema, &[], &[], &observations(), &mut budget).unwrap();
        let retained = budget.usage().retained_candidate_bytes();
        let handles = budget.usage().retained_open_handles();
        let work = budget.usage().decode_work();
        if change == 2 {
            project.write("replacement.tmp", OUTPUT);
            fs::remove_file(project.root.join(Q)).unwrap();
            fs::rename(project.root.join("replacement.tmp"), project.root.join(Q)).unwrap();
            let file = inventory.shared_file(Q).unwrap();
            assert_eq!(file.read_to_end_bounded(4096).unwrap(), OUTPUT);
        } else {
            project.write(Q, b"changed output");
        }
        assert!(inventory.validate(&schema, &[], &mut budget).is_err());
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
}

#[test]
fn failed_observation_read_refunds_only_dropped_storage_and_handles() {
    let project = fixture();
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory = capture_files(&schema, &[], &[], &observations(), &mut budget).unwrap();
    let custody = inventory.shared_custody();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    let work = budget.usage().decode_work();
    project.write(Q, OUTPUT);
    let mut claims = observations();
    claims[0].maximum_bytes = 1;
    assert!(matches!(
        capture_files(&schema, &[], &[], &claims, &mut budget),
        Err(SourceInventoryError::Host(
            FsError::ByteLimitExceeded { .. }
        ))
    ));
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert!(budget.usage().decode_work() > work);
    assert!(custody.same_owner(&inventory.shared_custody()));
    assert!(custody.parent(Q).is_some());
}

#[test]
fn missing_leaf_outside_roots_keeps_its_exact_parent_and_rejects_replaced_ancestors() {
    let project = fixture();
    fs::create_dir_all(project.root.join("outputs/nested")).unwrap();
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let path = "outputs/nested/new.prefab.ron";
    let inventory = capture_files(
        &schema,
        &[],
        &[],
        &[ObservedFileClaim {
            path,
            maximum_bytes: 4096,
        }],
        &mut budget,
    )
    .unwrap();
    assert_eq!(paths(&inventory), [ORIGINAL]);
    assert!(inventory.shared_custody().has_directory("outputs"));
    assert!(inventory.shared_custody().has_directory("outputs/nested"));
    inventory.validate(&schema, &[], &mut budget).unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    fs::rename(
        project.root.join("outputs/nested"),
        project.root.join("outputs/previous-nested"),
    )
    .unwrap();
    fs::create_dir(project.root.join("outputs/nested")).unwrap();
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceInventoryError::SourceChanged)
    ));
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert_eq!(budget.usage().retained_open_handles(), handles);

    let mut budget = schema.budget;
    assert!(matches!(
        capture_files(
            &schema,
            &[],
            &[],
            &[ObservedFileClaim {
                path: "uncreated-parent/new.prefab.ron",
                maximum_bytes: 4096,
            }],
            &mut budget,
        ),
        Err(SourceInventoryError::Host(ref error)) if is_missing(error)
    ));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        schema.budget.retained_candidate_bytes
    );
    assert_eq!(
        budget.usage().retained_open_handles(),
        schema.budget.retained_open_handles
    );
}
