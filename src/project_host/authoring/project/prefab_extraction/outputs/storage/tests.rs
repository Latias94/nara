use std::fs;

use nara_core::ItemLimit;
use nara_project::{ProjectAuthoringRecovery, ProjectAuthoringState};

use super::super::super::recovery_data::{
    PrefabExtractionRecoveryLimits,
    storage::{PrefabExtractionStorageLoadError, load_pending},
};
use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
    recovery_storage::{RecoveryPackageData, RecoveryStorageSource, validate_storage_path},
};

mod fixture;
use fixture::{Fixture, P, Q, RECOVERY, SIDECAR, until};

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

fn allocations(outputs: &PreparedProjectPrefabExtractionOutputs) -> Vec<usize> {
    let review = outputs.review();
    let corpus = review.persistence.sources().prepared().corpus();
    let mut pointers = vec![
        corpus.original_sources().as_ptr() as usize,
        corpus.sources().as_ptr() as usize,
        review.context.disk.as_ptr() as usize,
        review.saved.sources().as_ptr() as usize,
        review.saved.published().as_ptr() as usize,
        review.saved.extracted().document() as *const PrefabDocument as usize,
    ];
    pointers.extend(
        outputs
            .outputs()
            .map(|output| output.bytes().unwrap().as_ptr() as usize),
    );
    pointers
}

#[test]
fn actual_fixed_owner_publishes_package_then_pending_and_fresh_root_binds_concrete_data() {
    let fixture = Fixture::new();
    let original = fixture.originals();
    let ready = fs::read(fixture.directory.join("nara.toml")).unwrap();
    let live = fixture
        .workspace
        .prefab(fixture.document)
        .unwrap()
        .session();
    let live_allocation = live.document() as *const PrefabDocument;
    let live_revision = live.revision();
    let live_history = live.history_status();
    let live_usage = live.history_usage();
    let prepared = fixture.recovery();
    let fixed = allocations(prepared.outputs());
    let encoded = prepared.recovery().unwrap().encoded_bytes().as_ptr();
    let digest = prepared.recovery().unwrap().digest();
    let spent = prepared.outputs().review().persistence.usage();
    let storage = prepared
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    let storage = until(storage, ProjectPrefabExtractionStoragePhase::Recovery);
    assert!(!fixture.directory.join(RECOVERY).exists());
    fixture.assert_destinations_missing();
    assert_eq!(
        fs::read(fixture.directory.join("nara.toml")).unwrap(),
        ready
    );
    let stored = storage.advance().unwrap();
    assert_eq!(stored.phase(), ProjectPrefabExtractionStoragePhase::Stored);
    assert_eq!(
        fs::read(fixture.directory.join(RECOVERY)).unwrap(),
        stored.recovery().unwrap().encoded_bytes()
    );
    assert_eq!(
        fs::read(fixture.directory.join("nara.toml")).unwrap(),
        ready
    );
    fixture.assert_destinations_missing();
    let confirmed = until(stored, ProjectPrefabExtractionStoragePhase::Confirmed);
    let pending = confirmed.pending().unwrap();
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    assert_eq!(allocations(confirmed.outputs()), fixed);
    assert_eq!(
        confirmed.recovery().unwrap().encoded_bytes().as_ptr(),
        encoded
    );
    assert!(confirmed.outputs().review().persistence.usage().work_bytes >= spent.work_bytes);
    assert_eq!(fixture.originals(), original);
    fixture.assert_destinations_missing();
    assert_eq!(
        pending.recovery_publication().receipt().previous_identity(),
        None
    );
    assert_eq!(
        pending.pending_publication().receipt().candidate_identity(),
        pending.capability().identity()
    );
    let session = pending.observed().root().identity();
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.fresh_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
    drop(confirmed);
    let root = fixture.fresh_root();
    assert_ne!(root.identity(), session);
    let mut work = budget();
    let observed = load_pending(root, Default::default(), &mut work).unwrap();
    assert_eq!(observed.recovery().digest(), digest);
    assert_eq!(observed.reference().recovery_path().as_str(), RECOVERY);
    assert_eq!(observed.recovery().q().path(), Q);
    assert_eq!(observed.recovery().q_sidecar().path(), SIDECAR);
    assert!(
        observed
            .recovery()
            .complete_original_sources()
            .iter()
            .any(|source| source.path() == P)
    );
    assert_eq!(work.retained_open_handles, 3);
    let retained = work.retained_candidate_bytes;
    observed.validate(&mut work).unwrap();
    assert_eq!(work.retained_candidate_bytes, retained);
    assert_eq!(fixture.originals(), original);
    fixture.assert_destinations_missing();
    let live = fixture
        .workspace
        .prefab(fixture.document)
        .unwrap()
        .session();
    assert_eq!(live.document() as *const PrefabDocument, live_allocation);
    assert_eq!(live.revision(), live_revision);
    assert_eq!(live.history_status(), live_history);
    assert_eq!(live.history_usage(), live_usage);
    assert_eq!(
        fixture.workspace.document_is_dirty(fixture.document),
        Some(true)
    );
}

#[test]
fn cancelling_prepublication_storage_restores_original_package_and_only_dropped_reservations() {
    for phase in [
        ProjectPrefabExtractionStoragePhase::Capturing,
        ProjectPrefabExtractionStoragePhase::Preparing,
        ProjectPrefabExtractionStoragePhase::Recovery,
    ] {
        let fixture = Fixture::new();
        let prepared = fixture.recovery();
        let fixed = allocations(prepared.outputs());
        let encoded = prepared.recovery().unwrap().encoded_bytes().as_ptr();
        let retained = prepared
            .outputs()
            .review()
            .context
            .budget
            .retained_candidate_bytes;
        let handles = prepared
            .outputs()
            .review()
            .context
            .budget
            .retained_open_handles;
        let work = prepared.outputs().review().context.budget.decode_work;
        let storage = until(
            prepared
                .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
                .unwrap(),
            phase,
        );
        let cancellation = storage.cancel().unwrap();
        assert!(cancellation.orphan_publication().is_none());
        let restored = cancellation.into_recovery();
        assert_eq!(allocations(restored.outputs()), fixed);
        assert_eq!(
            restored.recovery().unwrap().encoded_bytes().as_ptr(),
            encoded
        );
        assert_eq!(
            restored
                .outputs()
                .review()
                .context
                .budget
                .retained_candidate_bytes,
            retained
        );
        assert_eq!(
            restored
                .outputs()
                .review()
                .context
                .budget
                .retained_open_handles,
            handles
        );
        assert!(restored.outputs().review().context.budget.decode_work > work);
        restored.cancel().unwrap().cancel().unwrap();
        assert!(!fixture.directory.join(RECOVERY).exists());
        fixture.assert_destinations_missing();
        assert!(matches!(
            fixture.manifest().authoring.state,
            ProjectAuthoringState::Ready
        ));
    }
}

#[test]
fn arc_admission_refusal_returns_original_package_and_cancellation_waits_for_escaped_arc() {
    let fixture = Fixture::new();
    let mut prepared = fixture.recovery();
    let fixed = allocations(prepared.outputs());
    let package = prepared.recovery.take().unwrap();
    let encoded = package.encoded_bytes().as_ptr();
    let work = &mut prepared.outputs.review.context.budget;
    let limits = work.limits;
    let retained = work.retained_candidate_bytes;
    work.limits.retained_candidate_bytes = ByteLimit::new(retained).unwrap();
    let failure = package.into_shared(work).unwrap_err();
    assert_eq!(failure.recovery.encoded_bytes().as_ptr(), encoded);
    assert_eq!(work.retained_candidate_bytes, retained);
    work.limits = limits;
    prepared.recovery = Some(failure.recovery);
    let storage = prepared
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    // A consumer retaining transport data must prevent the original package reservation from
    // being refunded. The field is private outside this aggregate; this exercises its fallback.
    let escaped = Arc::clone(storage.recovery.as_ref().unwrap());
    let retained = storage
        .owner
        .outputs
        .review
        .context
        .budget
        .retained_candidate_bytes;
    let failure = storage.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::RecoveryShared
    ));
    assert_eq!(
        failure
            .storage()
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes,
        retained
    );
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    drop(escaped);
    let restored = failure.cancel().unwrap().into_recovery();
    assert_eq!(
        restored.recovery().unwrap().encoded_bytes().as_ptr(),
        encoded
    );
    restored.cancel().unwrap().cancel().unwrap();
}

#[test]
fn occupied_package_refusal_keeps_owner_and_budget_for_retry_or_cancel() {
    for retry in [false, true] {
        let fixture = Fixture::new();
        let storage = until(
            fixture.storage(),
            ProjectPrefabExtractionStoragePhase::Preparing,
        );
        let fixed = allocations(storage.outputs());
        let retained = storage
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes;
        let handles = storage
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_open_handles;
        let spent = storage.owner.outputs.review.context.budget.decode_work;
        fs::write(fixture.directory.join(RECOVERY), b"occupied").unwrap();
        let failure = storage.advance().unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionStorageError::Storage(
                RecoveryStorageError::RecoveryAlreadyExists
            )
        ));
        assert_eq!(
            failure
                .storage()
                .owner
                .outputs
                .review
                .context
                .budget
                .retained_candidate_bytes,
            retained
        );
        assert_eq!(
            failure
                .storage()
                .owner
                .outputs
                .review
                .context
                .budget
                .retained_open_handles,
            handles
        );
        assert!(
            failure
                .storage()
                .owner
                .outputs
                .review
                .context
                .budget
                .decode_work
                > spent
        );
        assert_eq!(allocations(failure.storage().outputs()), fixed);
        assert_eq!(
            fs::read(fixture.directory.join(RECOVERY)).unwrap(),
            b"occupied"
        );
        if retry {
            fs::remove_file(fixture.directory.join(RECOVERY)).unwrap();
            let storage = until(
                failure.retry().unwrap(),
                ProjectPrefabExtractionStoragePhase::Confirmed,
            );
            assert_eq!(allocations(storage.outputs()), fixed);
            assert_eq!(
                storage
                    .pending()
                    .unwrap()
                    .observed()
                    .reference()
                    .recovery_path()
                    .as_str(),
                RECOVERY
            );
        } else {
            failure
                .cancel()
                .unwrap()
                .into_recovery()
                .cancel()
                .unwrap()
                .cancel()
                .unwrap();
        }
        fixture.assert_destinations_missing();
    }
}

#[test]
fn common_protected_paths_cover_every_extraction_role_and_directory() {
    let fixture = Fixture::new();
    let prepared = fixture.recovery();
    let package = prepared.recovery().unwrap();
    let protected = package.protected_paths().collect::<Vec<_>>();
    for expected in [
        "nara.toml",
        "schema.json",
        P,
        "prefabs/actor.prefab.json.meta",
        Q,
        SIDECAR,
        "assets",
        "prefabs",
        "scenes",
    ] {
        assert!(
            protected.contains(&expected),
            "missing protected role {expected}"
        );
    }
    for path in [
        "NARA.TOML",
        "schema.json/child",
        "prefabs/recovery.json",
        "assets",
        "scenes/start.scene.json.meta",
    ] {
        assert!(matches!(
            validate_storage_path(path, package, &mut budget()),
            Err(RecoveryStorageError::RecoveryPathConflict)
        ));
    }
    for path in ["recovery/../x.json", "recovery//x.json", "recovery\\x.json"] {
        assert!(matches!(
            validate_storage_path(path, package, &mut budget()),
            Err(RecoveryStorageError::RecoveryPathInvalid)
        ));
    }
    validate_storage_path(RECOVERY, package, &mut budget()).unwrap();
}

#[test]
fn publication_failures_retain_physical_stage_and_original_outputs() {
    let fixture = Fixture::new();
    let mut storage = until(
        fixture.storage(),
        ProjectPrefabExtractionStoragePhase::Recovery,
    );
    let fixed = allocations(storage.outputs());
    let limits = storage.owner.outputs.review.context.budget.limits;
    let spent = storage.owner.outputs.review.context.budget.decode_work;
    storage
        .owner
        .outputs
        .review
        .context
        .budget
        .limits
        .decode_work = ItemLimit::new(spent).unwrap();
    let mut failure = storage.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Storage(RecoveryStorageError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    assert_eq!(
        failure.storage().phase(),
        ProjectPrefabExtractionStoragePhase::Recovery
    );
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    assert!(!fixture.directory.join(RECOVERY).exists());
    failure.storage.owner.outputs.review.context.budget.limits = limits;
    let stored = failure.retry().unwrap();
    assert_eq!(stored.phase(), ProjectPrefabExtractionStoragePhase::Stored);
    let mut bound = stored.advance().unwrap();
    let retained = bound
        .owner
        .outputs
        .review
        .context
        .budget
        .retained_candidate_bytes;
    let handles = bound
        .owner
        .outputs
        .review
        .context
        .budget
        .retained_open_handles;
    let spent = bound.owner.outputs.review.context.budget.decode_work;
    bound.owner.outputs.review.context.budget.limits.decode_work = ItemLimit::new(spent).unwrap();
    let mut failure = bound.advance().unwrap_err();
    assert_eq!(
        failure.storage().phase(),
        ProjectPrefabExtractionStoragePhase::Pending
    );
    assert_eq!(
        failure
            .storage()
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes,
        retained
    );
    assert_eq!(
        failure
            .storage()
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_open_handles,
        handles
    );
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Ready
    ));
    failure.storage.owner.outputs.review.context.budget.limits = limits;
    let confirmed = failure.retry().unwrap();
    assert_eq!(
        confirmed.phase(),
        ProjectPrefabExtractionStoragePhase::Confirmed
    );
    let escaped = confirmed.pending().unwrap().observed().shared_recovery();
    let failure = confirmed.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::PendingAttempted
    ));
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    assert_eq!(
        escaped.digest(),
        failure.storage().recovery().unwrap().digest()
    );
    fixture.assert_destinations_missing();
}

#[test]
fn package_prepublisher_capacity_refusal_can_cancel_without_package_io() {
    let fixture = Fixture::new();
    let prepared = fixture.recovery();
    let fixed = allocations(prepared.outputs());
    let encoded = prepared.recovery().unwrap().encoded_bytes().as_ptr();
    let retained = prepared
        .outputs()
        .review()
        .context
        .budget
        .retained_candidate_bytes;
    let handles = prepared
        .outputs()
        .review()
        .context
        .budget
        .retained_open_handles;
    let mut storage = until(
        prepared
            .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
            .unwrap(),
        ProjectPrefabExtractionStoragePhase::Recovery,
    );
    let work = &mut storage.owner.outputs.review.context.budget;
    work.limits.retained_candidate_bytes = ByteLimit::new(work.retained_candidate_bytes).unwrap();
    let spent = work.decode_work;
    let failure = storage.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Storage(RecoveryStorageError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    assert!(!fixture.directory.join(RECOVERY).exists());
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Ready
    ));
    let cancellation = failure.cancel().unwrap();
    assert!(cancellation.orphan_publication().is_none());
    let restored = cancellation.into_recovery();
    assert_eq!(allocations(restored.outputs()), fixed);
    assert_eq!(
        restored.recovery().unwrap().encoded_bytes().as_ptr(),
        encoded
    );
    assert_eq!(
        restored
            .outputs()
            .review()
            .context
            .budget
            .retained_candidate_bytes,
        retained
    );
    assert_eq!(
        restored
            .outputs()
            .review()
            .context
            .budget
            .retained_open_handles,
        handles
    );
    assert!(restored.outputs().review().context.budget.decode_work > spent);
    restored.cancel().unwrap().cancel().unwrap();
    fixture.assert_destinations_missing();
}

#[test]
fn cancelling_a_confirmed_orphan_or_unattempted_pending_retains_its_actual_receipt() {
    for phase in [
        ProjectPrefabExtractionStoragePhase::Stored,
        ProjectPrefabExtractionStoragePhase::Pending,
    ] {
        let fixture = Fixture::new();
        let prepared = fixture.recovery();
        let fixed = allocations(prepared.outputs());
        let encoded = prepared.recovery().unwrap().encoded_bytes().as_ptr();
        let digest = prepared.recovery().unwrap().digest();
        let retained = prepared
            .outputs()
            .review()
            .context
            .budget
            .retained_candidate_bytes;
        let handles = prepared
            .outputs()
            .review()
            .context
            .budget
            .retained_open_handles;
        let storage = until(
            prepared
                .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
                .unwrap(),
            phase,
        );
        let cancellation = storage.cancel().unwrap();
        let receipt = cancellation.orphan_publication().unwrap().receipt();
        assert_eq!(receipt.previous_identity(), None);
        let root = fixture.project.project_root();
        let file = root
            .open_file_exact(&RelativePath::new(RECOVERY).unwrap())
            .unwrap();
        assert_eq!(receipt.candidate_identity(), file.identity());
        assert_eq!(cancellation.prepared().recovery().unwrap().digest(), digest);
        assert_eq!(allocations(cancellation.prepared().outputs()), fixed);
        assert_eq!(
            cancellation
                .prepared()
                .outputs()
                .review()
                .context
                .budget
                .retained_candidate_bytes,
            retained + size_of::<FilePublicationEvidence>()
        );
        assert_eq!(
            cancellation
                .prepared()
                .outputs()
                .review()
                .context
                .budget
                .retained_open_handles,
            handles
        );
        let restored = cancellation.into_recovery();
        assert_eq!(
            restored.recovery().unwrap().encoded_bytes().as_ptr(),
            encoded
        );
        assert_eq!(
            restored
                .outputs()
                .review()
                .context
                .budget
                .retained_candidate_bytes,
            retained
        );
        assert_eq!(
            fs::read(fixture.directory.join(RECOVERY)).unwrap(),
            restored.recovery().unwrap().encoded_bytes()
        );
        restored.cancel().unwrap().cancel().unwrap();
        assert!(fixture.directory.join(RECOVERY).exists());
        assert!(matches!(
            fixture.manifest().authoring.state,
            ProjectAuthoringState::Ready
        ));
        fixture.assert_destinations_missing();
    }
}

#[test]
fn pending_prepublisher_refusal_restores_cancellation_without_losing_orphan_evidence() {
    let fixture = Fixture::new();
    let mut storage = until(
        fixture.storage(),
        ProjectPrefabExtractionStoragePhase::Pending,
    );
    let fixed = allocations(storage.outputs());
    let work = &mut storage.owner.outputs.review.context.budget;
    work.limits.retained_candidate_bytes = ByteLimit::new(work.retained_candidate_bytes).unwrap();
    let failure = storage.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Storage(RecoveryStorageError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    let cancellation = failure.cancel().unwrap();
    assert!(cancellation.orphan_publication().is_some());
    assert_eq!(allocations(cancellation.prepared().outputs()), fixed);
    cancellation
        .into_recovery()
        .cancel()
        .unwrap()
        .cancel()
        .unwrap();
    assert!(fixture.directory.join(RECOVERY).exists());
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Ready
    ));
    fixture.assert_destinations_missing();
}

#[test]
fn package_cleanup_refusal_retains_custody_until_cancellation_can_discharge_it() {
    let fixture = Fixture::new();
    let mut storage = until(
        fixture.storage(),
        ProjectPrefabExtractionStoragePhase::Recovery,
    );
    let fixed = allocations(storage.outputs());
    let Stage::Recovery(prepared) = &mut storage.stage else {
        unreachable!()
    };
    let control = prepared.test_fail_after_write();
    let failure = storage.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Failed(FilePublicationFailureStage::WriteTemporary)
    ));
    let retained = failure
        .storage()
        .owner
        .outputs
        .review
        .context
        .budget
        .retained_candidate_bytes;
    let failure = failure.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::CleanupPending
    ));
    assert_eq!(
        failure.storage().phase(),
        ProjectPrefabExtractionStoragePhase::Recovery
    );
    assert_eq!(
        failure
            .storage()
            .owner
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes,
        retained
    );
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    assert!(!fixture.directory.join(RECOVERY).exists());
    control.release();
    let cancellation = failure.cancel().unwrap();
    assert!(cancellation.orphan_publication().is_none());
    cancellation
        .into_recovery()
        .cancel()
        .unwrap()
        .cancel()
        .unwrap();
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Ready
    ));
}

#[test]
fn native_pending_failure_keeps_its_fence_across_a_later_prepublisher_refusal() {
    let fixture = Fixture::new();
    let mut storage = until(
        fixture.storage(),
        ProjectPrefabExtractionStoragePhase::Pending,
    );
    let fixed = allocations(storage.outputs());
    let Stage::Pending(pending) = &mut storage.stage else {
        unreachable!()
    };
    let control = pending.test_fail_after_write();
    let failure = storage.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Failed(FilePublicationFailureStage::WriteTemporary)
    ));
    let mut failure = failure.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::PendingAttempted
    ));
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    control.release();
    failure.storage.cleanup().unwrap();
    let work = &mut failure.storage.owner.outputs.review.context.budget;
    work.limits.retained_candidate_bytes = ByteLimit::new(work.retained_candidate_bytes).unwrap();
    let failure = failure.retry().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::Storage(RecoveryStorageError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    let failure = failure.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionStorageError::PendingAttempted
    ));
    assert_eq!(allocations(failure.storage().outputs()), fixed);
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Ready
    ));
    fixture.assert_destinations_missing();
}

#[test]
fn fresh_transport_checks_missing_length_digest_ceiling_then_concrete_grammar() {
    let fixture = Fixture::new();
    drop(fixture.pending());
    let original = fixture.originals();
    let bytes = fs::read(fixture.directory.join(RECOVERY)).unwrap();
    let mut small = budget();
    let limits = PrefabExtractionRecoveryLimits {
        encoded_bytes: ByteLimit::new(bytes.len() - 1).unwrap(),
        ..Default::default()
    };
    assert!(matches!(
        load_pending(fixture.fresh_root(), limits, &mut small),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::PackageEncodedBytesExceeded
        ))
    ));
    assert_eq!(small.retained_candidate_bytes, 0);
    assert_eq!(small.retained_open_handles, 0);
    fs::remove_file(fixture.directory.join(RECOVERY)).unwrap();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut budget()),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::PackageMissing
        ))
    ));
    fs::write(fixture.directory.join(RECOVERY), b"{").unwrap();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut budget()),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::PackageDigestMismatch
        ))
    ));
    let mut corrupt = bytes.clone();
    corrupt[0] = b'!';
    fs::write(fixture.directory.join(RECOVERY), &corrupt).unwrap();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut budget()),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::PackageDigestMismatch
        ))
    ));
    let digest = ContentDigest::of_bytes(b"{");
    let reference = ProjectAuthoringRecovery::new(
        ProjectPath::new(RECOVERY).unwrap(),
        nara_project::ProjectAuthoringRecoveryDigest::from_parts(
            digest.length(),
            *digest.as_bytes(),
        ),
    )
    .unwrap();
    let mut manifest = fixture.manifest();
    manifest.authoring.state = ProjectAuthoringState::Pending(reference);
    fixture.replace("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
    fs::write(fixture.directory.join(RECOVERY), b"{").unwrap();
    let mut grammar = budget();
    grammar.charge_source_bytes(0, 0, 0, 128).unwrap();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut grammar),
        Err(PrefabExtractionStorageLoadError::Data(_))
    ));
    assert_eq!(grammar.retained_candidate_bytes, 128);
    assert_eq!(grammar.retained_open_handles, 0);
    assert!(grammar.decode_work > 0);
    assert_eq!(fixture.originals(), original);
    fixture.assert_destinations_missing();
}

#[test]
fn fresh_bind_refuses_changed_complete_configuration_and_refunds_only_new_candidates() {
    let fixture = Fixture::new();
    drop(fixture.pending());
    let mut manifest = fixture.manifest();
    manifest.project.name = "Changed after Pending".to_owned();
    fixture.replace("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
    let mut work = budget();
    work.charge_source_bytes(0, 0, 0, 73).unwrap();
    let before = work.usage();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut work),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::ConfigurationMismatch
        ))
    ));
    assert_eq!(work.retained_candidate_bytes, 73);
    assert_eq!(work.retained_open_handles, 0);
    assert!(work.decode_work > before.decode_work());
    fixture.assert_destinations_missing();
}

#[test]
fn fresh_bind_refuses_a_package_moved_into_a_protected_source_root() {
    let fixture = Fixture::new();
    drop(fixture.pending());
    let mut manifest = fixture.manifest();
    let ProjectAuthoringState::Pending(reference) = &manifest.authoring.state else {
        panic!("the fixture must have published Pending");
    };
    let moved = "scenes/recovery.json";
    let reference = ProjectAuthoringRecovery::new(
        ProjectPath::new(moved).unwrap(),
        reference.recovery_digest(),
    )
    .unwrap();
    fs::copy(
        fixture.directory.join(RECOVERY),
        fixture.directory.join(moved),
    )
    .unwrap();
    manifest.authoring.state = ProjectAuthoringState::Pending(reference);
    fixture.replace("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
    let mut work = budget();
    assert!(matches!(
        load_pending(fixture.fresh_root(), Default::default(), &mut work),
        Err(PrefabExtractionStorageLoadError::Storage(
            RecoveryStorageError::RecoveryPathConflict
        ))
    ));
    assert_eq!(work.retained_candidate_bytes, 0);
    assert_eq!(work.retained_open_handles, 0);
    fixture.assert_destinations_missing();
}

#[test]
fn fresh_observation_refuses_later_same_bytes_in_a_different_manifest_incarnation() {
    let fixture = Fixture::new();
    drop(fixture.pending());
    let mut work = budget();
    let observed = load_pending(fixture.fresh_root(), Default::default(), &mut work).unwrap();
    fixture.replace("nara.toml", observed.manifest_bytes());
    assert!(matches!(
        observed.validate(&mut work),
        Err(RecoveryStorageError::SourceChanged(
            RecoveryStorageSource::Manifest
        ))
    ));
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    fixture.assert_destinations_missing();
}
