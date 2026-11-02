use nara_core::ItemLimit;

use super::*;
use crate::project_host::authoring::source_upgrade_storage::RecoveryStorageHost;

/// Only disk and verified recovery data survive fixture preparation. Fresh admission receives a
/// newly opened root; no live receipt, nominal basis, open source or history is available to it.
struct RecoveryFixture {
    project: Fixture,
}

impl RecoveryFixture {
    fn new(new_bundle: bool, new_sources: usize) -> Self {
        let SuccessorFixture {
            project,
            incumbent,
            previous,
            original,
            recovery,
        } = SuccessorFixture::with_source_options(false, true);
        drop(original);
        drop(previous);
        drop(incumbent);
        Self::from_recovery(project, recovery, new_bundle, new_sources)
    }

    fn from_recovery(
        project: Fixture,
        recovery: SourceUpgradeRecovery,
        new_bundle: bool,
        new_sources: usize,
    ) -> Self {
        project.write(RECOVERY, recovery.encoded_bytes());
        let mut manifest = ProjectManifest::parse_toml_bytes(recovery.manifest().successor.bytes())
            .manifest
            .unwrap();
        manifest.authoring.state = ProjectAuthoringState::Pending(
            ProjectAuthoringRecovery::new(ProjectPath::new(RECOVERY).unwrap(), recovery.digest())
                .unwrap(),
        );
        project.write("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
        if new_bundle {
            project.write(BUNDLE, recovery.bundle().successor.bytes());
        }
        for member in recovery.members().iter().take(new_sources) {
            project.write(&member.path, member.successor.bytes());
        }
        drop(recovery);
        Self { project }
    }

    fn pending(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> ObservedPendingRecoveryStorage {
        let root = crate::test_host_fs::writable_directory(&self.project.root).unwrap();
        RecoveryStorageHost::load_pending(root, SourceUpgradeRecoveryLimits::default(), budget)
            .unwrap()
    }

    fn replace(&self, pending: &ObservedPendingRecoveryStorage, path: &str, bytes: &[u8]) {
        let root = pending.root();
        let file = root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let maximum = nara_project::DEFAULT_MANIFEST_BYTE_LIMIT;
        let digest = file.digest(maximum).unwrap();
        let mut publisher =
            FilePublicationHost::bind_existing(root, path, &file, digest, maximum).unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (_, capability) = accepted.into_parts();
                publisher.commit_published(capability);
            }
            FilePublicationOutcome::Rejected(reason) => {
                panic!("fixture replacement refused: {reason:?}")
            }
            FilePublicationOutcome::Failed(stage) => {
                panic!("fixture replacement failed: {stage:?}")
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("fixture replacement uncertain: {evidence:?}")
            }
        }
    }
}

fn recovery_budget() -> AuthoringProjectAdmissionBudget {
    let mut budget = operation_budget();
    // Loading the verified package and confirming complete sets share one finite allowance.
    // This explicitly selected test policy does not change ordinary or upgrade defaults.
    budget.limits.decode_work = ItemLimit::new(500_000_000).unwrap();
    budget
}

fn capture_recovered(
    pending: &ObservedPendingRecoveryStorage,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RecoveredSourceUpgradeInventory, SourceUpgradeInventoryError> {
    RecoveredSourceUpgradeInventory::capture(pending, SceneFileLimits::default(), budget)
}

#[test]
fn fresh_capture_accepts_old_mixed_and_new_sources_with_old_or_new_bundle() {
    for new_bundle in [false, true] {
        for new_sources in [0, 2, 4] {
            let fixture = RecoveryFixture::new(new_bundle, new_sources);
            let mut budget = recovery_budget();
            let pending = fixture.pending(&mut budget);
            let retained_handles = budget.usage().retained_open_handles();
            let proof = capture_recovered(&pending, &mut budget).unwrap();
            assert_eq!(proof.inventory().participants().len(), 4);
            assert_eq!(proof.inventory().metadata().len(), 2);
            assert_eq!(proof.inventory().open_sources.len(), 0);
            let expected_bundle = if new_bundle {
                &pending.recovery().bundle().successor
            } else {
                &pending.recovery().bundle().old_disk
            };
            assert_eq!(
                proof.bundle_digest(),
                ContentDigest::of_bytes(expected_bundle.bytes())
            );
            assert_eq!(
                proof
                    .bundle_file()
                    .read_to_end_bounded(expected_bundle.bytes().len() as u64)
                    .unwrap(),
                expected_bundle.bytes(),
            );
            assert_eq!(
                proof.bundle_file().parent_identity(),
                Some(proof.bundle_parent().identity()),
            );
            for (index, (observed, member)) in proof
                .inventory()
                .participants()
                .zip(pending.recovery().members())
                .enumerate()
            {
                let expected = if member.path == CATALOGUED {
                    RecoveryMemberState::Unchanged
                } else if index < new_sources {
                    RecoveryMemberState::New
                } else {
                    RecoveryMemberState::Old
                };
                assert_eq!(member.classify(Some(observed.file().digest())), expected);
                assert_eq!(observed.source_key(), &member.key);
                assert_eq!(observed.file().path(), member.path);
            }
            assert_eq!(
                budget.usage().retained_open_handles(),
                retained_handles + proof.inventory().retained_handle_count() + 1,
            );
            let retained = budget.usage().retained_candidate_bytes();
            let handles = budget.usage().retained_open_handles();
            let work = budget.usage().decode_work();
            proof.validate(&pending, &mut budget).unwrap();
            assert_eq!(budget.usage().retained_candidate_bytes(), retained);
            assert_eq!(budget.usage().retained_open_handles(), handles);
            assert!(budget.usage().decode_work() > work);
            assert!(matches!(
                AuthoringProjectAdmissionHost::default().prepare(pending.root().share(), None),
                Err(AuthoringProjectAdmissionError::SourceAuthoringPending),
            ));
        }
    }
}

#[test]
fn fresh_capture_retains_a_recorded_scripts_member_without_inventing_an_open_source() {
    const SCRIPT: &str = "scripts/nested/open.scene.json";
    let SuccessorFixture {
        project,
        incumbent,
        previous,
        original,
        recovery: earlier,
    } = SuccessorFixture::new();
    project.scene(SCRIPT);
    project.metadata(
        SCRIPT,
        "nested/open.scene.json",
        "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
        AssetSourceKind::Scene,
        0,
    );
    let root = incumbent.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(SCRIPT).unwrap())
        .unwrap();
    let native = NativeDocumentSourcePolicy::new(incumbent.settings())
        .admit(&root, SCRIPT, SourceDocumentKind::Scene, file.identity())
        .unwrap();
    let open = AuthoringDocumentSourceFact::from_native_observation(
        SCRIPT,
        file.identity(),
        file.parent_identity(),
        file.digest(4096).unwrap(),
        &native,
    )
    .unwrap();
    let mut budget = recovery_budget();
    let inventory = SourceUpgradeInventory::capture(
        &incumbent,
        &[SourceUpgradeOpenSource::Bound(&open)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    assert_eq!(inventory.participants().len(), 5);
    let members: Vec<_> = inventory
        .participants()
        .map(|participant| {
            let successor = earlier
                .members()
                .iter()
                .find(|member| member.path == participant.file().path())
                .map_or(participant.bytes(), |member| member.successor.bytes());
            SourceUpgradeRecoveryMemberInput {
                path: participant.file().path(),
                key: participant.source_key(),
                encoding: match participant.encoding() {
                    SourceUpgradeEncoding::Json => RecoverySourceEncoding::Json,
                    SourceUpgradeEncoding::Ron => RecoverySourceEncoding::Ron,
                },
                old_disk: participant.bytes(),
                reviewed_working: participant.bytes(),
                successor,
            }
        })
        .collect();
    let metadata: Vec<_> = inventory
        .metadata()
        .map(|metadata| SourceUpgradeRecoveryMetadataInput {
            path: metadata.file().path(),
            observed_bytes: metadata.bytes(),
        })
        .collect();
    let roots: Vec<_> = inventory
        .roots()
        .map(|root| SourceUpgradeRecoveryRootInput {
            path: root.path(),
            missing_ancestor: match root.observation() {
                SourceUpgradeRootObservation::Available { .. } => None,
                SourceUpgradeRootObservation::Missing { ancestor_path, .. } => {
                    Some(ancestor_path.as_str())
                }
            },
        })
        .collect();
    let missing_sidecars: Vec<_> = inventory.missing_sidecars().collect();
    let recovery = SourceUpgradeRecovery::prepare(
        SourceUpgradeRecoveryInput {
            old_ready_manifest: earlier.manifest().old_disk.bytes(),
            future_ready_manifest: earlier.manifest().successor.bytes(),
            bundle_path: BUNDLE,
            old_bundle: earlier.bundle().old_disk.bytes(),
            successor_bundle: earlier.bundle().successor.bytes(),
            component: &ComponentTypeId::new(COMPONENT),
            field: &ComponentFieldId::new("health"),
            successor_property: &ComponentPropertyPath::new(ComponentFieldId::new("health"))
                .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                    "milli",
                ))]),
            members: &members,
            metadata: &metadata,
            roots: &roots,
            missing_sidecars: &missing_sidecars,
        },
        SourceUpgradeRecoveryLimits::default(),
        &mut budget,
    )
    .unwrap();
    drop(members);
    drop(metadata);
    drop(roots);
    drop(missing_sidecars);
    drop(inventory);
    drop(original);
    drop(previous);
    drop(incumbent);
    drop(earlier);
    drop(open);
    drop(native);
    drop(file);
    drop(root);
    let fixture = RecoveryFixture::from_recovery(project, recovery, true, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    let proof = capture_recovered(&pending, &mut budget).unwrap();
    assert_eq!(proof.inventory().open_sources.len(), 0);
    assert_eq!(proof.inventory().participants().len(), 5);
    assert_eq!(proof.inventory().metadata().len(), 3);
    let source = proof
        .inventory()
        .participants()
        .find(|source| source.file().path() == SCRIPT)
        .unwrap();
    assert!(proof.shared_custody().file(SCRIPT).is_some());
    assert_eq!(
        source.file().parent_identity(),
        Some(proof.shared_custody().parent(SCRIPT).unwrap().identity()),
    );
    proof.validate(&pending, &mut budget).unwrap();
}

#[test]
fn fresh_capture_refuses_third_bundle_bytes_and_refunds_its_candidate() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    fixture.project.write(BUNDLE, b"{}");
    let before = budget.usage();
    assert!(matches!(
        capture_recovered(&pending, &mut budget),
        Err(SourceUpgradeInventoryError::RecoveryInventory),
    ));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        budget.usage().retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
}

#[test]
fn fresh_capture_refuses_third_source_bytes_and_complete_profile_membership_changes() {
    for change in 0..3 {
        let fixture = RecoveryFixture::new(false, 2);
        let mut budget = recovery_budget();
        let pending = fixture.pending(&mut budget);
        match change {
            0 => fixture.project.prefab(CLOSED),
            1 => fixture.project.scene("other/scenes/late.scene.json"),
            _ => fs::remove_file(fixture.project.root.join(OTHER_PROFILE)).unwrap(),
        }
        let before = budget.usage();
        let error = capture_recovered(&pending, &mut budget).err().unwrap();
        if change == 2 {
            assert!(
                matches!(error, SourceUpgradeInventoryError::Host(ref error) if is_missing(error))
            );
        } else {
            assert!(matches!(
                error,
                SourceUpgradeInventoryError::RecoveryInventory
            ));
        }
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(budget.usage().decode_work() > before.decode_work());
    }
}

#[test]
fn fresh_capture_refuses_metadata_payload_membership_and_negative_root_changes() {
    for change in 0..6 {
        let fixture = RecoveryFixture::new(true, 2);
        let mut budget = recovery_budget();
        let pending = fixture.pending(&mut budget);
        match change {
            0 => fixture.project.metadata(
                "assets/unused.png",
                "unused.png",
                SECOND_ID,
                AssetSourceKind::Image,
                1,
            ),
            1 => fixture.project.metadata(
                "assets/late.png",
                "late.png",
                "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
                AssetSourceKind::Image,
                0,
            ),
            2 => fs::remove_file(fixture.project.root.join("assets/unused.png.meta")).unwrap(),
            3 => fixture.project.metadata(
                OPEN,
                "open.scene.json",
                "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
                AssetSourceKind::Scene,
                0,
            ),
            4 => fs::create_dir(fixture.project.root.join("missing")).unwrap(),
            _ => fs::create_dir_all(fixture.project.root.join("missing/scenes")).unwrap(),
        }
        let before = budget.usage();
        assert!(matches!(
            capture_recovered(&pending, &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory),
        ));
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(budget.usage().decode_work() > before.decode_work());
    }
}

#[test]
fn fresh_confirmation_refuses_progress_or_equal_content_at_another_source_or_bundle_leaf() {
    for change in 0..3 {
        let fixture = RecoveryFixture::new(false, 0);
        let mut budget = recovery_budget();
        let pending = fixture.pending(&mut budget);
        let proof = capture_recovered(&pending, &mut budget).unwrap();
        let path = if change == 2 { BUNDLE } else { CLOSED };
        let original = if change == 2 {
            Arc::clone(proof.bundle_file())
        } else {
            proof.inventory().shared_file(path).unwrap()
        };
        let member = pending
            .recovery()
            .members()
            .iter()
            .find(|member| member.path == CLOSED)
            .unwrap();
        let bytes = match change {
            0 => member.successor.bytes(),
            1 => member.old_disk.bytes(),
            _ => pending.recovery().bundle().old_disk.bytes(),
        };
        fixture.replace(&pending, path, bytes);
        let current = pending
            .root()
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        assert!(!original.same_object(&current).unwrap());
        let before = budget.usage();
        assert!(matches!(
            proof.validate(&pending, &mut budget),
            Err(SourceUpgradeInventoryError::MembershipChanged),
        ));
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(budget.usage().decode_work() > before.decode_work());
    }
}

#[test]
fn fresh_confirmation_rechecks_pending_and_recovery_actual_leaves() {
    for path in ["nara.toml", RECOVERY] {
        let fixture = RecoveryFixture::new(false, 2);
        let mut budget = recovery_budget();
        let pending = fixture.pending(&mut budget);
        let proof = capture_recovered(&pending, &mut budget).unwrap();
        let bytes = fs::read(fixture.project.root.join(path)).unwrap();
        fixture.replace(&pending, path, &bytes);
        let before = budget.usage();
        assert!(matches!(
            proof.validate(&pending, &mut budget),
            Err(SourceUpgradeInventoryError::PendingStorage(
                RecoveryStorageError::SourceChanged(_)
            )),
        ));
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
    }
}

#[test]
fn fresh_confirmation_releases_temporary_storage_and_keeps_cumulative_work_on_refusal() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    let proof = capture_recovered(&pending, &mut budget).unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    for _ in 0..2 {
        let work = budget.usage().decode_work();
        proof.validate(&pending, &mut budget).unwrap();
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
    fixture.project.scene("other/scenes/late.scene.json");
    for _ in 0..2 {
        let work = budget.usage().decode_work();
        assert!(matches!(
            proof.validate(&pending, &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory),
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
}

#[test]
fn fresh_capture_admits_wrapper_storage_before_io_and_keeps_the_pending_owner_on_refusal() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    let before = budget.usage();
    let wrapper_bytes =
        size_of::<RecoveredSourceUpgradeInventory>() - size_of::<SourceUpgradeInventory>();
    let exact = before.retained_candidate_bytes() + wrapper_bytes;
    budget.limits.retained_candidate_bytes = ByteLimit::new(exact - 1).unwrap();
    assert!(matches!(
        capture_recovered(&pending, &mut budget),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum },
        )) if maximum == exact - 1,
    ));
    assert_eq!(budget.usage(), before);
    let actual = pending
        .capability()
        .read_to_end_bounded(pending.digest().length())
        .unwrap();
    assert_eq!(actual, pending.manifest_bytes());
}

#[test]
fn fresh_capture_handle_peak_is_exact_and_one_short_returns_all_new_custody() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut base = recovery_budget();
    let pending = fixture.pending(&mut base);
    let mut measured = base;
    let proof = capture_recovered(&pending, &mut measured).unwrap();
    let exact = measured.usage().peak_open_handles();
    assert!(exact > base.usage().peak_open_handles());
    drop(proof);
    for maximum in [exact - 1, exact] {
        let mut budget = base;
        budget.limits.peak_open_handles = ItemLimit::new(maximum).unwrap();
        let result = capture_recovered(&pending, &mut budget);
        if maximum == exact {
            let proof = result.unwrap();
            assert_eq!(budget.usage().peak_open_handles(), exact);
            assert_eq!(
                budget.usage().retained_open_handles(),
                base.usage().retained_open_handles()
                    + proof.inventory().retained_handle_count()
                    + 1,
            );
        } else {
            assert!(matches!(result, Err(SourceUpgradeInventoryError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: actual },
            )) if actual == maximum));
            assert_eq!(
                budget.usage().retained_open_handles(),
                base.usage().retained_open_handles()
            );
            assert_eq!(
                budget.usage().retained_candidate_bytes(),
                base.usage().retained_candidate_bytes()
            );
        }
    }
}

#[test]
fn fresh_capture_work_refusal_keeps_performed_work_and_refunds_only_candidate_resources() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    let before = budget.usage();
    budget.limits.decode_work = ItemLimit::new(before.decode_work() + 1).unwrap();
    assert!(matches!(
        capture_recovered(&pending, &mut budget),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. },
        )),
    ));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        budget.usage().retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
}

#[test]
fn fresh_bundle_and_shared_inventory_custody_keep_actual_leaves_and_ancestors_alive() {
    let fixture = RecoveryFixture::new(false, 2);
    let mut budget = recovery_budget();
    let pending = fixture.pending(&mut budget);
    let proof = capture_recovered(&pending, &mut budget).unwrap();
    let custody = proof.shared_custody();
    let bundle = Arc::clone(proof.bundle_file());
    let bundle_parent = proof.bundle_parent().share();
    let weak_custody = Arc::downgrade(&proof.inventory.custody);
    let weak_bundle = Arc::downgrade(proof.bundle_file());
    let before = budget.usage();
    drop(proof);
    assert!(weak_custody.upgrade().is_some());
    assert!(weak_bundle.upgrade().is_some());
    for path in [BUNDLE, OPEN, CLOSED, OTHER_PROFILE] {
        let parent = custody.parent(path).unwrap();
        if path == BUNDLE {
            assert!(parent.same_object(&bundle_parent).unwrap());
            assert_eq!(bundle.parent_identity(), Some(parent.identity()));
        } else {
            let file = custody.file(path).unwrap();
            assert_eq!(file.parent_identity(), Some(parent.identity()));
        }
    }
    assert!(custody.inner.directories.contains_key("other"));
    assert!(custody.inner.directories.contains_key("other/scenes"));
    assert!(custody.inner.directories.contains_key("schema"));
    assert_eq!(budget.usage(), before);
    drop(bundle);
    drop(bundle_parent);
    drop(custody);
    assert!(weak_custody.upgrade().is_none());
    assert!(weak_bundle.upgrade().is_none());
}
