//! Child of extraction::tests: the actual live producer supplies every fresh-process package.

use super::super::{
    outputs::storage::ProjectPrefabExtractionStoragePhase,
    resume::{
        PrefabExtractionRecovery as Recovery, PrefabExtractionRecoveryErrorKind as ErrorKind,
        PrefabExtractionRecoveryMemberState as MemberState, PrefabExtractionRecoveryPhase as Phase,
    },
};
use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionError, AuthoringSourceOperationLimits,
    pending_recovery::{PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryKind},
    source_inventory::SourceInventoryError,
};

const RECOVERY: &str = "recovery/process-extraction.json";

fn pending(fixture: &Fixture) {
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    let mut storage = fixture
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
        .unwrap()
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    for _ in 0..8 {
        if storage.phase() == ProjectPrefabExtractionStoragePhase::Confirmed {
            return;
        }
        storage = storage.advance().unwrap();
    }
    panic!("the real producer did not reach actual Pending");
}

fn inspect(fixture: &Fixture, handles: usize) -> Recovery {
    let limits = AuthoringSourceOperationLimits::default()
        .with_open_handles(ItemLimit::new(handles).unwrap());
    let inspection = PendingAuthoringRecoveryInspection::inspect(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        limits,
    )
    .unwrap();
    assert_eq!(
        inspection.kind(),
        PendingAuthoringRecoveryKind::PrefabExtraction
    );
    let used = inspection.usage();
    let recovery = Recovery::inspect(inspection, SceneCompilationLimits::default()).unwrap();
    assert!(recovery.view().usage.host_work >= used.host_work);
    assert!(recovery.view().usage.peak_open_handles >= used.peak_open_handles);
    recovery
}

fn reviewed(mut recovery: Recovery) -> Recovery {
    for _ in 0..8 {
        if recovery.view().phase == Phase::Review {
            return recovery;
        }
        let result = recovery.advance();
        assert!(result.is_ok(), "{:?}", recovery.failure());
    }
    panic!("read-only stored generation admission did not reach Review");
}

fn complete(recovery: &mut Recovery) {
    for _ in 0..96 {
        if recovery.view().phase == Phase::Complete {
            return;
        }
        let result = recovery.advance();
        assert!(result.is_ok(), "{:?}", recovery.failure());
    }
    panic!("the finite recovery did not publish actual Ready");
}

fn prepared(recovery: &mut Recovery) {
    for _ in 0..96 {
        if recovery.test_prepared_project().is_some() {
            return;
        }
        let result = recovery.advance();
        assert!(result.is_ok(), "{:?}", recovery.failure());
    }
    panic!("the finite recovery did not prepare its actual Project");
}

fn snapshot_allocations(
    snapshot: &AuthoringProjectSessionSnapshot,
) -> (usize, usize, usize, usize) {
    (
        Arc::as_ptr(&snapshot.startup_scene) as usize,
        Arc::as_ptr(&snapshot.source_facts) as *const () as usize,
        Arc::as_ptr(&snapshot.asset_database) as usize,
        Arc::as_ptr(&snapshot.source_observations) as usize,
    )
}

fn inventory_cause(error: &dyn std::error::Error) -> Option<&SourceInventoryError> {
    let mut cause = error.source();
    while let Some(error) = cause {
        if let Some(inventory) = error.downcast_ref::<SourceInventoryError>() {
            return Some(inventory);
        }
        cause = error.source();
    }
    None
}

#[test]
fn authenticated_inspection_payload_limit_preserves_the_actual_admission_error_chain() {
    let fixture = Fixture::new();
    pending(&fixture);
    let disk = fixture.disk_state();
    let limits =
        AuthoringSourceOperationLimits::default().with_open_handles(ItemLimit::new(128).unwrap());
    let probe = PendingAuthoringRecoveryInspection::inspect(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        limits,
    )
    .unwrap();
    let required = probe.usage().host_work;
    drop(probe);
    let inspection = PendingAuthoringRecoveryInspection::inspect(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        limits.with_host_work(ItemLimit::new(required).unwrap()),
    )
    .unwrap();
    assert_eq!(inspection.usage().host_work, required);
    let failure = Recovery::inspect(inspection, SceneCompilationLimits::default()).unwrap_err();
    assert_eq!(failure.kind(), ErrorKind::HostWorkLimit);
    let mut cause = std::error::Error::source(&failure);
    let mut actual_work_limit = false;
    while let Some(error) = cause {
        if matches!(
            error.downcast_ref::<AuthoringProjectAdmissionError>(),
            Some(AuthoringProjectAdmissionError::DecodeWorkExceeded { .. })
        ) {
            actual_work_limit = true;
            break;
        }
        cause = error.source();
    }
    assert!(
        actual_work_limit,
        "the real payload refusal must preserve its original admission error"
    );
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn process_recovery_inspection_and_confirmation_are_read_only_and_close_preserves_pending() {
    let mut fixture = Fixture::new();
    pending(&fixture);
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let before = fixture.disk_state();
    let mut recovery = reviewed(inspect(&fixture, 128));
    let review = recovery.review().unwrap();
    assert_eq!(review.selected_root(), &id("branch"));
    assert_eq!(review.selected_profile(), None);
    assert_eq!(review.destination().project_path, Q);
    assert_eq!(review.destination().observed, MemberState::NotCreated);
    assert_eq!(
        review.destination().metadata_observed,
        MemberState::NotCreated
    );
    assert_eq!(review.unchanged_bundle_state(), MemberState::Unchanged);
    assert_eq!(review.sources().len(), recovery.view().source_count);
    assert_ne!(
        review.schema_basis(),
        &fixture.project.startup_scene.schema_basis()
    );
    assert_eq!(fixture.disk_state(), before);
    recovery.confirm().unwrap();
    assert_eq!(recovery.view().phase, Phase::PublishingSources);
    assert_eq!(fixture.disk_state(), before);
    recovery.close().unwrap();
    assert_eq!(recovery.view().phase, Phase::Closed);
    assert_eq!(recovery.confirm().unwrap_err(), ErrorKind::Closed);
    assert_eq!(fixture.disk_state(), before);
    assert!(!fixture.root.join(Q).exists());
    assert!(!fixture.root.join(Q_SIDECAR).exists());
    let recovery = recovery.into_outcome().unwrap_err();
    assert_eq!(recovery.view().phase, Phase::Closed);
}

#[test]
fn all_old_and_mixed_recovery_finish_with_exact_schema_and_keep_observed_outputs_unreplaced() {
    for mixed in [false, true] {
        let mut fixture = Fixture::new();
        pending(&fixture);
        fixture.open.clear();
        fixture.workspace = EditorWorkspace::new();
        let mut recovery = reviewed(inspect(&fixture, 128));
        if mixed {
            // Simulate physical progress before a fresh process. There are deliberately no
            // live publication receipts for the next inspection to claim it performed.
            let review = recovery.review().unwrap();
            let destination = review.destination();
            fs::write(
                fixture.root.join(destination.project_path),
                destination.output,
            )
            .unwrap();
            fs::write(
                fixture.root.join(destination.metadata_path),
                destination.metadata_output,
            )
            .unwrap();
            let p = review
                .sources()
                .find(|source| source.project_path == P)
                .unwrap();
            fs::write(fixture.root.join(P), p.output.unwrap()).unwrap();
            recovery.close().unwrap();
            drop(recovery);
            recovery = reviewed(inspect(&fixture, 128));
        }
        let root = crate::test_host_fs::writable_directory(&fixture.root).unwrap();
        let observed_q = mixed.then(|| {
            root.open_file_exact(&RelativePath::new(Q).unwrap())
                .unwrap()
                .identity()
        });
        let review = recovery.review().unwrap();
        let basis = review.schema_basis().clone();
        let q_key = review.destination().key.clone();
        let bundle = review.schema_bundle() as *const _ as usize;
        let expected: Vec<_> = review
            .sources()
            .filter_map(|source| {
                source
                    .output
                    .map(|bytes| (source.project_path.to_owned(), bytes.to_vec()))
            })
            .chain([
                (Q.to_owned(), review.destination().output.to_vec()),
                (
                    Q_SIDECAR.to_owned(),
                    review.destination().metadata_output.to_vec(),
                ),
            ])
            .collect();
        assert_eq!(
            review.destination().observed,
            if mixed {
                MemberState::Output
            } else {
                MemberState::NotCreated
            }
        );
        recovery.confirm().unwrap();
        complete(&mut recovery);
        let outcome = recovery.into_outcome().unwrap();
        assert_eq!(outcome.schema_basis(), &basis);
        assert_eq!(outcome.schema_bundle() as *const _ as usize, bundle);
        assert!(outcome.source_keys().any(|key| key == &q_key));
        for (path, bytes) in expected {
            assert_eq!(fs::read(fixture.root.join(path)).unwrap(), bytes);
        }
        if let Some(identity) = observed_q {
            assert_eq!(
                root.open_file_exact(&RelativePath::new(Q).unwrap())
                    .unwrap()
                    .identity(),
                identity
            );
        }
        let actual_ready = root
            .open_file_exact(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let admitted = &outcome.test_project().schema;
        assert_eq!(
            outcome.ready_receipt().published_identity(),
            Some(admitted.manifest_identity())
        );
        assert!(
            admitted
                .manifest
                .capability()
                .same_object(&actual_ready)
                .unwrap()
        );
        let reopened = admit_project(&fixture.root);
        assert_eq!(reopened.authoring_bundle(), outcome.schema_bundle());
        assert_ne!(reopened.startup_scene.schema_basis(), basis);
    }
}

#[test]
fn stale_confirmation_keeps_the_original_generation_and_retry_never_renews_work() {
    let mut fixture = Fixture::new();
    pending(&fixture);
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let mut recovery = reviewed(inspect(&fixture, 128));
    let basis = recovery.review().unwrap().schema_basis().clone();
    let bundle = recovery.review().unwrap().schema_bundle() as *const _ as usize;
    let disk = fixture.disk_state();
    let original = fs::read(fixture.root.join(R)).unwrap();
    fs::write(fixture.root.join(R), b"changed dormant-profile source").unwrap();
    let before = recovery.view().usage;
    assert!(recovery.confirm().is_err());
    assert_eq!(recovery.view().phase, Phase::Review);
    assert!(recovery.failure().is_some());
    assert!(matches!(
        inventory_cause(&recovery.failure().unwrap()),
        Some(SourceInventoryError::SourceChanged)
    ));
    assert_eq!(recovery.review().unwrap().schema_basis(), &basis);
    assert_eq!(
        recovery.review().unwrap().schema_bundle() as *const _ as usize,
        bundle
    );
    assert!(recovery.view().usage.host_work > before.host_work);
    fs::write(fixture.root.join(R), original).unwrap();
    let refused_usage = recovery.view().usage;
    recovery.retry().unwrap();
    recovery.confirm().unwrap();
    assert!(recovery.view().usage.host_work > refused_usage.host_work);
    assert_eq!(
        recovery.view().usage.source_work_bytes,
        refused_usage.source_work_bytes + size_of::<PrefabExtractionSavedGeneration>()
    );
    recovery.close().unwrap();
    assert_eq!(fixture.disk_state(), disk);
    drop(recovery);

    // Preserve the exact native cause through both the generation-failure owner and the
    // confirmation wrapper. Same-object content changes and replaced objects are distinct.
    for confirm in [false, true] {
        let mut recovery = inspect(&fixture, 128);
        if confirm {
            recovery = reviewed(recovery);
        } else {
            recovery.advance().unwrap();
            assert_eq!(recovery.view().phase, Phase::Inspecting);
        }
        let root = crate::test_host_fs::writable_directory(&fixture.root).unwrap();
        let original_guard = root
            .open_file_exact(&RelativePath::new(GUARD).unwrap())
            .unwrap();
        let bytes = fs::read(fixture.root.join(GUARD)).unwrap();
        let mut changed = bytes.clone();
        *changed.first_mut().unwrap() ^= 1;
        fs::write(fixture.root.join(GUARD), changed).unwrap();
        assert!(
            original_guard
                .same_object(
                    &root
                        .open_file_exact(&RelativePath::new(GUARD).unwrap())
                        .unwrap()
                )
                .unwrap()
        );
        let changed_contents = fixture.disk_state();
        let refusal = if confirm {
            recovery.confirm()
        } else {
            recovery.advance()
        };
        assert_eq!(refusal.unwrap_err(), ErrorKind::Inventory);
        assert!(
            matches!(
                inventory_cause(&recovery.failure().unwrap()),
                Some(SourceInventoryError::SourceChanged)
            ),
            "{:?}",
            recovery.failure()
        );
        assert_eq!(fixture.disk_state(), changed_contents);
        fs::write(fixture.root.join(GUARD), &bytes).unwrap();
        recovery.retry().unwrap();
        let original_view = recovery.view();
        fs::rename(
            fixture.root.join(GUARD),
            fixture.root.join(if confirm {
                "retired-guard-confirm"
            } else {
                "retired-guard-generation"
            }),
        )
        .unwrap();
        fs::write(fixture.root.join(GUARD), &bytes).unwrap();
        let replacement = root
            .open_file_exact(&RelativePath::new(GUARD).unwrap())
            .unwrap();
        assert!(!original_guard.same_object(&replacement).unwrap());
        assert_eq!(fs::read(fixture.root.join(GUARD)).unwrap(), bytes);
        let external_change = fixture.disk_state();

        let refusal = if confirm {
            recovery.confirm()
        } else {
            recovery.advance()
        };
        assert_eq!(refusal.unwrap_err(), ErrorKind::Inventory);
        assert!(matches!(
            inventory_cause(&recovery.failure().unwrap()),
            Some(SourceInventoryError::MembershipChanged)
        ));
        assert_eq!(recovery.view().operation, original_view.operation);
        assert_eq!(
            recovery.view().phase,
            if confirm {
                Phase::Review
            } else {
                Phase::Inspecting
            }
        );
        assert_eq!(
            recovery.view().usage.source_work_bytes,
            original_view.usage.source_work_bytes
        );
        assert!(recovery.view().usage.host_work > original_view.usage.host_work);
        assert_eq!(fixture.disk_state(), external_change);
        assert!(!fixture.root.join(Q).exists());
        assert!(!fixture.root.join(Q_SIDECAR).exists());
        recovery.close().unwrap();
        let recovery = recovery.into_outcome().unwrap_err();
        assert_eq!(recovery.view().phase, Phase::Closed);
        assert_eq!(fixture.disk_state(), external_change);
        drop(recovery);
    }
}

#[test]
fn a_late_native_capacity_refusal_keeps_the_same_owner_and_retry_or_close_cannot_raise_limits() {
    let mut fixture = Fixture::new();
    pending(&fixture);
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let mut recovery = reviewed(inspect(&fixture, 64));
    recovery.confirm().unwrap();
    let mut refused = false;
    for _ in 0..96 {
        if recovery.advance().is_err() {
            refused = true;
            break;
        }
        if recovery.view().phase == Phase::Complete {
            break;
        }
    }
    assert!(
        refused,
        "this complete fixture must exercise the explicit late64 control"
    );
    assert_eq!(
        recovery.failure().unwrap().kind(),
        ErrorKind::OpenHandleLimit
    );
    let usage = recovery.view().usage;
    let disk = fixture.disk_state();
    assert_eq!(recovery.retry().unwrap_err(), ErrorKind::OpenHandleLimit);
    assert_eq!(recovery.view().usage, usage);
    recovery.close().unwrap();
    assert_eq!(recovery.view().limits.open_handles().get(), 64);
    assert_eq!(fixture.disk_state(), disk);
    drop(recovery);
    let mut fresh = reviewed(inspect(&fixture, 128));
    fresh.confirm().unwrap();
    complete(&mut fresh);
    assert_eq!(fresh.view().phase, Phase::Complete);
}

#[test]
fn a_completed_outcome_keeps_the_exact_prepared_project_allocations_and_original_basis() {
    let mut fixture = Fixture::new();
    pending(&fixture);
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let mut recovery = reviewed(inspect(&fixture, 128));
    let basis = recovery.review().unwrap().schema_basis().clone();
    recovery.confirm().unwrap();
    prepared(&mut recovery);
    let candidate = recovery.test_prepared_project().unwrap();
    assert_eq!(candidate.startup_scene.schema_basis(), basis);
    assert!(matches!(
        candidate.settings().authoring_state,
        nara_project::ProjectAuthoringState::Pending(_)
    ));
    let allocations = snapshot_allocations(candidate);
    let inventory_keys = candidate
        .source_facts()
        .map(|source| source.project_path().clone())
        .collect::<Vec<_>>();
    complete(&mut recovery);
    let outcome = recovery.into_outcome().unwrap();
    let installed = outcome.test_project();
    assert_eq!(snapshot_allocations(installed), allocations);
    assert_eq!(installed.startup_scene.schema_basis(), basis);
    assert!(matches!(
        installed.settings().authoring_state,
        nara_project::ProjectAuthoringState::Ready
    ));
    assert_eq!(
        installed
            .source_facts()
            .map(|source| source.project_path().clone())
            .collect::<Vec<_>>(),
        inventory_keys
    );
    installed.startup_source_validation().validate().unwrap();
    assert_eq!(outcome.schema_basis(), &basis);
}

#[test]
fn ready_write_failure_keeps_physical_cleanup_and_the_prepared_project_for_retry_or_close() {
    for close in [false, true] {
        let mut fixture = Fixture::new();
        pending(&fixture);
        fixture.open.clear();
        fixture.workspace = EditorWorkspace::new();
        let mut recovery = reviewed(inspect(&fixture, 128));
        let basis = recovery.review().unwrap().schema_basis().clone();
        recovery.confirm().unwrap();
        prepared(&mut recovery);
        let allocations = snapshot_allocations(recovery.test_prepared_project().unwrap());
        recovery.advance().unwrap();
        assert_eq!(recovery.view().phase, Phase::PublishingReady);
        let before_write = fixture.disk_state();
        let root = crate::test_host_fs::writable_directory(&fixture.root).unwrap();
        let q_identity = root
            .open_file_exact(&RelativePath::new(Q).unwrap())
            .unwrap()
            .identity();
        let gate = recovery.test_fail_ready_write().unwrap();
        let write = recovery.advance();
        let failed = recovery.view();
        let blocked = if close {
            recovery.close()
        } else {
            recovery.retry()
        };
        let still_pending = recovery.view();
        let manifest_before_release = fs::read(fixture.root.join("nara.toml")).unwrap();
        // Release the native control before assertions so an unexpected result cannot strand
        // a deliberately blocked cleanup while the fixture tries to remove its real files.
        gate.release();
        let after_release = if close {
            recovery.close()
        } else {
            recovery.retry()
        };

        assert_eq!(write.unwrap_err(), ErrorKind::PublicationFailed);
        assert!(failed.cleanup_pending);
        assert_eq!(blocked.unwrap_err(), ErrorKind::CleanupPending);
        assert!(still_pending.cleanup_pending);
        assert!(still_pending.usage.host_work > failed.usage.host_work);
        assert_eq!(still_pending.limits.open_handles().get(), 128);
        let parsed = nara_project::ProjectManifest::parse_toml_str(
            std::str::from_utf8(&manifest_before_release).unwrap(),
        );
        assert!(matches!(
            parsed.manifest.unwrap().authoring.state,
            nara_project::ProjectAuthoringState::Pending(_)
        ));
        assert!(after_release.is_ok(), "{:?}", recovery.failure());
        assert!(!recovery.view().cleanup_pending);
        assert!(recovery.view().usage.host_work > still_pending.usage.host_work);
        assert_eq!(
            root.open_file_exact(&RelativePath::new(Q).unwrap())
                .unwrap()
                .identity(),
            q_identity
        );
        if close {
            assert_eq!(recovery.view().phase, Phase::Closed);
            assert_eq!(fixture.disk_state(), before_write);
            let owner = recovery.into_outcome().unwrap_err();
            assert_eq!(owner.view().phase, Phase::Closed);
        } else {
            assert_eq!(recovery.view().phase, Phase::Complete);
            let outcome = recovery.into_outcome().unwrap();
            assert_eq!(snapshot_allocations(outcome.test_project()), allocations);
            assert_eq!(outcome.test_project().startup_scene.schema_basis(), basis);
            outcome
                .test_project()
                .startup_source_validation()
                .validate()
                .unwrap();
        }
    }
}
