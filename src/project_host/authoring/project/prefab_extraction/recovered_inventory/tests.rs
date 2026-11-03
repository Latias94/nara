//! Wired by the parent as a child of prefab_extraction::tests, so the real Fixture is shared.

use nara_project::{
    NativeDocumentIdentity, ProjectAuthoringRecovery, ProjectAuthoringState, ProjectManifest,
};

use super::super::{
    outputs::storage::{ProjectPrefabExtractionStorage, ProjectPrefabExtractionStoragePhase},
    recovered_inventory::{
        RecoveredPrefabExtractionInventory, RecoveredPrefabExtractionInventoryError,
    },
    recovery_data::{
        PrefabExtractionRecovery, PrefabExtractionRecoveryInput, RecoveryBundleInput,
        RecoveryFileState, RecoveryMetadataGuardInput, RecoveryNewSourceInput,
        RecoveryOriginalSourceInput, RecoveryOutputInput, RecoveryRootInput,
        RecoverySelectionInput, RecoverySourceDisposition, RecoverySourceEncoding,
        storage::load_pending,
    },
};
use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionLimits,
        recovery_storage::ObservedPendingRecoveryStorage,
        source_inventory::{SourceFileObservation, SourceRootObservation},
    },
    file_publication::FilePublicationOutcome,
};

#[path = "tests/fixture.rs"]
mod fixture;
use fixture::{BUNDLE, RECOVERY, load, raw_pending, replace};

fn budget() -> AuthoringProjectAdmissionBudget {
    // Loading, initial complete capture and confirmation share one operation allowance, selected
    // before the first IO. Ordinary one-shot project admission retains its independent20M limit.
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
        .for_source_operation(
            nara_core::ItemLimit::new(200_000_000).unwrap(),
            nara_core::ItemLimit::new(64).unwrap(),
        )
}

fn capture(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RecoveredPrefabExtractionInventory, RecoveredPrefabExtractionInventoryError> {
    RecoveredPrefabExtractionInventory::capture(pending, SceneFileLimits::default(), budget)
}

#[test]
fn actual_output_package_and_pending_stages_feed_fresh_physical_and_schema_admission() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    let prepared = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap()
        .prepare_outputs()
        .unwrap()
        .prepare_recovery(Default::default())
        .unwrap();
    let mut storage: ProjectPrefabExtractionStorage = prepared
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    while storage.phase() != ProjectPrefabExtractionStoragePhase::Confirmed {
        storage = storage.advance().unwrap();
    }
    let operation = storage.recovery().unwrap().digest();
    assert!(storage.pending().is_some());
    drop(storage);

    let mut budget = budget();
    let pending = load(&fixture, &mut budget);
    assert_eq!(pending.recovery().digest(), operation);
    let proof = capture(&pending, &mut budget).unwrap();
    assert_eq!(proof.inventory().participants().len(), 6);
    assert_eq!(proof.inventory().captured_open_sources().len(), 0);
    assert_ne!(proof.basis(), &fixture.project.startup_scene.schema_basis());
    assert_eq!(
        proof.bundle_digest(),
        ContentDigest::of_bytes(pending.recovery().unchanged_bundle().blob().bytes())
    );
    for path in [Q, Q_SIDECAR] {
        assert!(matches!(
            proof.inventory().file_observation(path),
            Some(SourceFileObservation::Missing { .. })
        ));
    }
    proof.validate(&pending, &mut budget).unwrap();
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(pending.root().share(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn actual_stable_metadata_pending_observes_all_four_q_states_in_the_selected_profile() {
    for q_present in [false, true] {
        for meta_present in [false, true] {
            let fixture = raw_pending(true, Some("mobile"));
            let mut budget = budget();
            let pending = load(&fixture, &mut budget);
            let recovery = pending.recovery();
            assert_eq!(recovery.selected_profile(), Some("mobile"));
            assert_eq!(recovery.q().path(), "mobile/prefabs/new.prefab.json");
            if q_present {
                fs::write(
                    fixture.root.join(recovery.q().path()),
                    recovery.q().output().bytes(),
                )
                .unwrap();
            }
            if meta_present {
                fs::write(
                    fixture.root.join(recovery.q_sidecar().path()),
                    recovery.q_sidecar().output().bytes(),
                )
                .unwrap();
            }
            let handles_before = budget.retained_open_handles;
            let proof = capture(&pending, &mut budget).unwrap();
            assert_eq!(proof.inventory().participants().len(), 6);
            assert_eq!(proof.inventory().metadata().len(), 6);
            assert_eq!(proof.inventory().missing_sidecars().len(), 0);
            assert!(proof.inventory().participant(recovery.q().path()).is_none());
            assert!(
                proof
                    .inventory()
                    .metadata_for_source(recovery.q().path())
                    .is_none()
            );
            assert!(
                proof
                    .inventory()
                    .configurations()
                    .iter()
                    .any(|configuration| {
                        configuration.profile_name() == Some("mobile")
                            && configuration.participant_paths().any(|path| path == R)
                            && !configuration.participant_paths().any(|path| path == P)
                    })
            );
            for (path, present) in [
                (recovery.q().path(), q_present),
                (recovery.q_sidecar().path(), meta_present),
            ] {
                let parent = proof.inventory().shared_parent(path).unwrap();
                match proof.inventory().file_observation(path).unwrap() {
                    SourceFileObservation::Missing { parent_identity } => {
                        assert!(!present);
                        assert_eq!(*parent_identity, parent.identity());
                    }
                    SourceFileObservation::Present { file, bytes } => {
                        assert!(present);
                        assert_eq!(file.parent_identity(), Some(parent.identity()));
                        assert_eq!(bytes, &fs::read(fixture.root.join(path)).unwrap());
                        assert_eq!(
                            proof.inventory().shared_file(path).unwrap().identity(),
                            file.identity()
                        );
                    }
                }
            }
            assert_eq!(
                budget.retained_open_handles,
                handles_before + proof.inventory().retained_handle_count() + 1
            );
            assert_eq!(
                proof.bundle_file().parent_identity(),
                Some(proof.bundle_parent().identity())
            );
            assert_eq!(
                proof
                    .bundle_file()
                    .read_to_end_bounded(recovery.unchanged_bundle().blob().digest().length())
                    .unwrap(),
                recovery.unchanged_bundle().blob().bytes()
            );
            let custody = proof.shared_custody();
            assert!(custody.has_directory("schema"));
            assert!(custody.has_directory("schema/native"));
            let basis = proof.basis().clone();
            let models = proof.models() as *const _;
            let bundle_model = &proof.models().bundle as *const _;
            let adapters = proof.models().adapters.as_ptr();
            let retained = budget.retained_candidate_bytes;
            let handles = budget.retained_open_handles;
            let work = budget.decode_work;
            proof.validate(&pending, &mut budget).unwrap();
            assert_eq!(proof.basis(), &basis);
            assert_eq!(proof.models() as *const _, models);
            assert_eq!(&proof.models().bundle as *const _, bundle_model);
            assert_eq!(proof.models().adapters.as_ptr(), adapters);
            assert_eq!(budget.retained_candidate_bytes, retained);
            assert_eq!(budget.retained_open_handles, handles);
            assert!(budget.decode_work > work);
            let weak = custody.weak_owner();
            drop(proof);
            assert!(weak.upgrade().is_some());
            assert!(custody.parent(BUNDLE).is_some());
        }
    }
}

#[test]
fn original_output_bytes_are_current_data_and_unchanged_guards_stay_original() {
    let fixture = raw_pending(false, None);
    let mut budget = budget();
    let pending = load(&fixture, &mut budget);
    let selected = pending
        .recovery()
        .complete_original_sources()
        .iter()
        .find(|source| source.path() == P)
        .unwrap();
    replace(&pending, P, selected.output().unwrap().bytes());
    let proof = capture(&pending, &mut budget).unwrap();
    assert_eq!(
        selected.classify(Some(
            proof.inventory().participant(P).unwrap().file().digest()
        )),
        RecoveryFileState::Output
    );
    for guard in pending
        .recovery()
        .complete_original_sources()
        .iter()
        .filter(|source| source.disposition() == RecoverySourceDisposition::Guard)
    {
        assert_eq!(
            proof.inventory().participant(guard.path()).unwrap().bytes(),
            guard.original_disk().bytes()
        );
    }
    proof.validate(&pending, &mut budget).unwrap();
}

#[test]
fn missing_extra_changed_sources_metadata_negative_guards_and_q_conflicts_refuse_capture() {
    for change in 0..10 {
        let fixture = raw_pending(false, Some("mobile"));
        let mut budget = budget();
        let pending = load(&fixture, &mut budget);
        match change {
            0 => fs::remove_file(fixture.root.join(P)).unwrap(),
            1 => fs::write(
                fixture.root.join("scenes/extra.scene.json"),
                SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("extra"))])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap(),
            2 => fs::write(fixture.root.join(P), b"different original").unwrap(),
            3 => {
                // An unrelated metadata declaration also belongs to the complete guard set.
                let identity = AssetMetaIdentity::new(
                    "b7827fb9-28fa-4c04-847f-000000000004".parse().unwrap(),
                    AssetPath::new("extra.bin").unwrap(),
                    AssetSourceKind::Image,
                );
                fs::write(fixture.root.join("assets/extra.bin"), b"image source").unwrap();
                fs::write(
                    fixture.root.join("assets/extra.bin.meta"),
                    AssetMeta::new(identity, 0_u32).to_json_string().unwrap(),
                )
                .unwrap();
            }
            4 => fs::remove_file(fixture.root.join(format!("{P}.meta"))).unwrap(),
            5 => {
                let identity = AssetMetaIdentity::new(
                    "b7827fb9-28fa-4c04-847f-000000000003".parse().unwrap(),
                    AssetPath::new("inactive.scene.json").unwrap(),
                    AssetSourceKind::Scene,
                );
                fs::write(
                    fixture.root.join(format!("{INACTIVE}.meta")),
                    AssetMeta::new(identity, 0_u32).to_json_string().unwrap(),
                )
                .unwrap();
            }
            6 => fs::create_dir_all(fixture.root.join("absent/prefabs")).unwrap(),
            7 => fs::write(
                fixture.root.join(pending.recovery().q().path()),
                b"conflicting Q",
            )
            .unwrap(),
            8 => fs::write(
                fixture.root.join(pending.recovery().q_sidecar().path()),
                b"conflicting metadata",
            )
            .unwrap(),
            9 => fs::write(fixture.root.join(BUNDLE), b"different unchanged bundle").unwrap(),
            _ => unreachable!(),
        }
        let retained = budget.retained_candidate_bytes;
        let handles = budget.retained_open_handles;
        let work = budget.decode_work;
        assert!(capture(&pending, &mut budget).is_err(), "change {change}");
        assert_eq!(budget.retained_candidate_bytes, retained, "change {change}");
        assert_eq!(budget.retained_open_handles, handles, "change {change}");
        assert!(budget.decode_work > work, "change {change}");
    }
}

#[test]
fn changed_selected_profile_configuration_cannot_bind_the_stored_pending_package() {
    let fixture = raw_pending(true, Some("mobile"));
    let mut manifest =
        ProjectManifest::parse_toml_bytes(&fs::read(fixture.root.join("nara.toml")).unwrap())
            .manifest
            .unwrap();
    manifest.profiles.get_mut("mobile").unwrap().paths.prefabs = Some("changed/prefabs".to_owned());
    fs::write(
        fixture.root.join("nara.toml"),
        manifest.to_canonical_toml_bytes().unwrap(),
    )
    .unwrap();
    let mut budget = budget();
    assert!(
        load_pending(
            crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
            Default::default(),
            &mut budget,
        )
        .is_err()
    );
    assert_eq!(budget.retained_candidate_bytes, 0);
    assert_eq!(budget.retained_open_handles, 0);
    assert!(budget.decode_work > 0);
}

#[test]
fn equal_bytes_replacements_and_changed_ancestors_refuse_validation_without_readmitting_models() {
    for change in 0..7 {
        let fixture = raw_pending(true, Some("mobile"));
        let mut budget = budget();
        let pending = load(&fixture, &mut budget);
        fs::write(
            fixture.root.join(pending.recovery().q().path()),
            pending.recovery().q().output().bytes(),
        )
        .unwrap();
        let proof = capture(&pending, &mut budget).unwrap();
        let basis = proof.basis().clone();
        let models = proof.models() as *const _;
        let retained = budget.retained_candidate_bytes;
        let handles = budget.retained_open_handles;
        let work = budget.decode_work;
        match change {
            0 => replace(
                &pending,
                BUNDLE,
                pending.recovery().unchanged_bundle().blob().bytes(),
            ),
            1 => replace(
                &pending,
                P,
                pending
                    .recovery()
                    .complete_original_sources()
                    .iter()
                    .find(|source| source.path() == P)
                    .unwrap()
                    .original_disk()
                    .bytes(),
            ),
            2 => replace(
                &pending,
                pending.recovery().q().path(),
                pending.recovery().q().output().bytes(),
            ),
            3 => {
                let renamed = fs::rename(
                    fixture.root.join("schema/native"),
                    fixture.root.join("schema/previous-native"),
                );
                if let Err(error) = renamed {
                    assert!(
                        cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied,
                        "{error}"
                    );
                    // Windows can prevent renaming a directory containing this held leaf.
                    // Prove the original names, bytes, nominal models and complete guards still
                    // hold rather than pretending an external replacement actually occurred.
                    assert!(!fixture.root.join("schema/previous-native").exists());
                    assert_eq!(
                        fs::read(fixture.root.join(BUNDLE)).unwrap(),
                        pending.recovery().unchanged_bundle().blob().bytes()
                    );
                    proof.validate(&pending, &mut budget).unwrap();
                    assert_eq!(proof.basis(), &basis);
                    assert_eq!(proof.models() as *const _, models);
                    assert_eq!(budget.retained_candidate_bytes, retained);
                    assert_eq!(budget.retained_open_handles, handles);
                    assert!(budget.decode_work > work);
                    continue;
                }
                fs::create_dir(fixture.root.join("schema/native")).unwrap();
                fs::write(
                    fixture.root.join(BUNDLE),
                    pending.recovery().unchanged_bundle().blob().bytes(),
                )
                .unwrap();
            }
            4 => replace(&pending, "nara.toml", pending.manifest_bytes()),
            5 => replace(&pending, RECOVERY, pending.recovery().encoded_bytes()),
            6 => fs::write(
                fixture.root.join(pending.recovery().q_sidecar().path()),
                pending.recovery().q_sidecar().output().bytes(),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        assert!(
            proof.validate(&pending, &mut budget).is_err(),
            "change {change}"
        );
        assert_eq!(proof.basis(), &basis);
        assert_eq!(proof.models() as *const _, models);
        assert_eq!(budget.retained_candidate_bytes, retained, "change {change}");
        assert_eq!(budget.retained_open_handles, handles, "change {change}");
        assert!(budget.decode_work > work, "change {change}");
    }
}
