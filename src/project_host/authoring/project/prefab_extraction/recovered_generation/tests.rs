//! Wired as a child of prefab_extraction::tests to exercise the actual Project producer.

use nara_project::{ProjectAuthoringRecovery, ProjectAuthoringState, ProjectManifest};

use super::super::{
    outputs::storage::ProjectPrefabExtractionStoragePhase,
    recovered_generation::{
        RecoveredPrefabExtractionGeneration, RecoveredPrefabExtractionGenerationError,
        RecoveredPrefabExtractionGenerationPhase as Phase,
    },
    recovered_inventory::RecoveredPrefabExtractionInventory,
    recovery_data::{
        PrefabExtractionRecovery, PrefabExtractionRecoveryInput, RecoveryBundleInput,
        RecoveryFileState, RecoveryMetadataGuardInput, RecoveryNewSourceInput,
        RecoveryOriginalSourceInput, RecoveryOutputInput, RecoveryRootInput,
        RecoveryRootObservation, RecoverySelectionInput, RecoverySourceDisposition,
        storage::load_pending,
    },
};
use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionLimits, recovery_storage::ObservedPendingRecoveryStorage,
        source_inventory::SourceFileObservation,
    },
    file_publication::FilePublicationOutcome,
};

#[path = "tests/fixture.rs"]
mod fixture;
use fixture::{
    budget, canonical, complete, generation, load, publish_pending, replace,
    validation_decoded_bytes,
};

#[test]
fn actual_pending_reopens_complete_stored_outputs_with_one_fresh_basis_and_fixed_q_inputs() {
    let mut fixture = Fixture::new();
    fixture::add_outward_arguments(&mut fixture);
    let live_basis = fixture.project.startup_scene.schema_basis();
    publish_pending(&fixture, vec![fixture.document(INACTIVE)]);
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let disk = fixture.disk_state();

    let mut host = budget();
    let pending = load(&fixture, &mut host);
    let operation = pending.recovery().digest();
    let observation = RecoveredPrefabExtractionInventory::capture(
        &pending,
        SceneFileLimits::default(),
        &mut host,
    )
    .unwrap();
    let basis = observation.basis().clone();
    assert_ne!(basis, live_basis);
    assert_eq!(observation.inventory().captured_open_sources().len(), 0);
    let mut owner = complete(generation(pending, observation, host));
    assert_eq!(owner.pending().recovery().digest(), operation);
    let (corpus, saved) = owner.verified().unwrap();
    let recovery = owner.pending().recovery();
    assert_eq!(corpus.schema_basis(), &basis);
    assert_eq!(owner.observation().basis(), &basis);
    assert_eq!(corpus.selected_source(), recovery.selection().source());
    assert_eq!(corpus.root(), recovery.selection().root());
    assert_eq!(corpus.extracted().key(), recovery.q().key());
    let stored_q = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        recovery.q().output().bytes(),
        SceneFileLimits::default(),
    )
    .unwrap()
    .into_composition_source()
    .unwrap();
    assert_eq!(
        stored_q.inputs.len(),
        2,
        "both outward arguments must have fixed names"
    );
    assert_eq!(corpus.extracted().document(), &stored_q);
    assert_eq!(saved.extracted().document(), &stored_q);
    assert!(std::ptr::eq(
        corpus.extracted().document(),
        saved.extracted().document()
    ));
    assert_eq!(
        corpus
            .extracted()
            .document()
            .to_json_string()
            .unwrap()
            .as_bytes(),
        recovery.q().output().bytes(),
    );
    assert_eq!(
        corpus.original_sources().len(),
        recovery.complete_original_sources().len()
    );
    for (index, stored) in recovery.complete_original_sources().iter().enumerate() {
        assert_eq!(corpus.original_sources()[index].key(), stored.key());
        assert_eq!(
            canonical(&corpus.original_sources()[index], stored.encoding().into()),
            stored.reviewed_working().bytes(),
        );
        assert_eq!(
            canonical(&corpus.sources()[index], stored.encoding().into()),
            stored.output().unwrap_or(stored.reviewed_working()).bytes(),
        );
        assert_eq!(saved.published()[index], stored.output().is_some());
        assert_eq!(
            canonical(&saved.sources()[index], stored.encoding().into()),
            stored.output().unwrap_or(stored.original_disk()).bytes(),
        );
        if let CapturedExtractionSource::Scene {
            basis: source_basis,
            ..
        } = &saved.sources()[index]
        {
            assert_eq!(source_basis, &basis);
        }
    }
    owner.validate().unwrap();
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(owner.pending().root().share(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn mixed_original_output_and_q_sidecar_progress_keep_the_first_native_corpus_on_validation() {
    for sidecar_present in [false, true] {
        let fixture = Fixture::new();
        publish_pending(&fixture, vec![fixture.document(INACTIVE)]);
        let mut host = budget();
        let pending = load(&fixture, &mut host);
        let selected = pending
            .recovery()
            .complete_original_sources()
            .iter()
            .find(|source| source.path() == P)
            .unwrap();
        replace(&pending, P, selected.output().unwrap().bytes());
        fs::write(
            fixture.root.join(Q),
            pending.recovery().q().output().bytes(),
        )
        .unwrap();
        if sidecar_present {
            fs::write(
                fixture.root.join(Q_SIDECAR),
                pending.recovery().q_sidecar().output().bytes(),
            )
            .unwrap();
        }
        let disk = fixture.disk_state();
        let observation = RecoveredPrefabExtractionInventory::capture(
            &pending,
            SceneFileLimits::default(),
            &mut host,
        )
        .unwrap();
        assert_eq!(
            selected.classify(Some(
                observation
                    .inventory()
                    .participant(P)
                    .unwrap()
                    .file()
                    .digest(),
            )),
            RecoveryFileState::Output
        );
        let inactive = pending
            .recovery()
            .complete_original_sources()
            .iter()
            .find(|source| source.path() == INACTIVE)
            .unwrap();
        assert_eq!(
            inactive.classify(Some(
                observation
                    .inventory()
                    .participant(INACTIVE)
                    .unwrap()
                    .file()
                    .digest(),
            )),
            RecoveryFileState::Original
        );
        assert!(observation.inventory().participant(Q).is_none());
        assert!(matches!(
            observation.inventory().file_observation(Q),
            Some(SourceFileObservation::Present { .. })
        ));
        assert_eq!(
            matches!(
                observation.inventory().file_observation(Q_SIDECAR),
                Some(SourceFileObservation::Present { .. })
            ),
            sidecar_present
        );
        let basis = observation.basis().clone();
        let adapters = observation.models().adapters.as_ptr();
        let bundle = Arc::as_ptr(observation.bundle_file());
        let custody = observation.shared_custody().weak_owner();
        let bytes = observation
            .inventory()
            .participant(P)
            .unwrap()
            .bytes()
            .as_ptr();
        let validation_decoded = validation_decoded_bytes(&pending, &observation, &mut host);
        // A second full live capture cannot fit under the first observation's measured peak.
        // Confirmation has only the original custody plus its normal bounded native probes.
        host.limits.peak_open_handles = ItemLimit::new(host.usage().peak_open_handles()).unwrap();
        let mut owner = complete(generation(pending, observation, host));
        let before = owner.host_usage();
        let outputs = owner.verified().unwrap().0.sources().as_ptr();
        let q = owner.verified().unwrap().0.extracted().document() as *const PrefabDocument;
        for _ in 0..2 {
            owner.validate().unwrap();
        }
        assert_eq!(owner.observation().basis(), &basis);
        assert_eq!(owner.verified().unwrap().0.schema_basis(), &basis);
        assert_eq!(owner.observation().models().adapters.as_ptr(), adapters);
        assert_eq!(Arc::as_ptr(owner.observation().bundle_file()), bundle);
        assert!(std::sync::Weak::ptr_eq(
            &owner.observation().shared_custody().weak_owner(),
            &custody
        ));
        assert_eq!(
            owner
                .observation()
                .inventory()
                .participant(P)
                .unwrap()
                .bytes()
                .as_ptr(),
            bytes
        );
        assert_eq!(owner.verified().unwrap().0.sources().as_ptr(), outputs);
        assert_eq!(
            owner.verified().unwrap().0.extracted().document() as *const _,
            q
        );
        let after = owner.host_usage();
        assert_eq!(
            after.retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            after.retained_open_handles(),
            before.retained_open_handles()
        );
        assert_eq!(after.peak_open_handles(), before.peak_open_handles());
        assert_eq!(
            after.decoded_bytes(),
            before.decoded_bytes() + 2 * validation_decoded
        );
        assert!(after.decode_work() > before.decode_work());
        assert_eq!(fixture.disk_state(), disk);
    }
}

#[test]
fn grammar_valid_but_false_q_refuses_at_verify_and_retry_retains_inputs_and_spent_work() {
    let fixture = Fixture::new();
    publish_pending(&fixture, vec![]);
    let pending = load(&fixture, &mut budget());
    let mut q = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        pending.recovery().q().output().bytes(),
        SceneFileLimits::default(),
    )
    .unwrap()
    .into_composition_source()
    .unwrap();
    q.placements.push(SceneEntityPlacement::root(id("forged")));
    q.entities.push(SceneEntityRecord::new(id("forged")));
    let q_bytes = q.to_json_string().unwrap();
    let falsified = fixture::rebuild(pending.recovery(), Some(q_bytes.as_bytes()), false);
    fixture::install_package(&pending, &falsified);
    drop(pending);
    let disk = fixture.disk_state();
    let mut host = budget();
    let pending = load(&fixture, &mut host);
    assert_eq!(pending.recovery().digest(), falsified.digest());
    let observation = RecoveredPrefabExtractionInventory::capture(
        &pending,
        SceneFileLimits::default(),
        &mut host,
    )
    .unwrap();
    let basis = observation.basis().clone();
    let validation_decoded = validation_decoded_bytes(&pending, &observation, &mut host);
    let decoded = generation(pending, observation, host).advance().unwrap();
    assert_eq!(decoded.phase(), Phase::Verify);
    let before = decoded.source_usage();
    let failure = decoded.advance().unwrap_err();
    assert!(
        matches!(failure.error(), RecoveredPrefabExtractionGenerationError::Correspondence {
        report, ..
    } if !report.diagnostics.is_empty())
    );
    assert_eq!(failure.owner().phase(), Phase::Verify);
    assert!(failure.owner().verified().is_none());
    assert!(failure.owner().source_usage().work_nodes > before.work_nodes);
    let source = failure.owner().source_usage();
    let host = failure.owner().host_usage();
    let package = failure
        .owner()
        .pending()
        .recovery()
        .encoded_bytes()
        .as_ptr();
    let q = failure
        .owner()
        .pending()
        .recovery()
        .q()
        .output()
        .bytes()
        .as_ptr();
    let failure = failure.retry().unwrap_err();
    assert!(matches!(
        failure.error(),
        RecoveredPrefabExtractionGenerationError::Correspondence { .. }
    ));
    assert_eq!(failure.owner().phase(), Phase::Verify);
    assert_eq!(failure.owner().observation().basis(), &basis);
    assert_eq!(
        failure
            .owner()
            .pending()
            .recovery()
            .encoded_bytes()
            .as_ptr(),
        package
    );
    assert_eq!(
        failure
            .owner()
            .pending()
            .recovery()
            .q()
            .output()
            .bytes()
            .as_ptr(),
        q
    );
    assert!(failure.owner().source_usage().work_nodes > source.work_nodes);
    assert!(failure.owner().source_usage().work_bytes > source.work_bytes);
    assert_eq!(
        failure.owner().host_usage().decoded_bytes(),
        host.decoded_bytes() + validation_decoded
    );
    assert_eq!(
        failure.owner().host_usage().retained_open_handles(),
        host.retained_open_handles()
    );
    assert_eq!(
        failure.owner().host_usage().retained_candidate_bytes(),
        host.retained_candidate_bytes()
    );
    assert!(failure.owner().host_usage().decode_work() > host.decode_work());
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn old_disk_guard_route_refuses_only_at_saved_and_keeps_the_verified_working_pair_for_retry() {
    let fixture = Fixture::with_guard_route("mobile/assets", "", true);
    publish_pending(&fixture, vec![fixture.document(GUARD)]);
    let pending = load(&fixture, &mut budget());
    let guard = pending
        .recovery()
        .complete_original_sources()
        .iter()
        .find(|source| source.path() == GUARD)
        .unwrap();
    assert_eq!(guard.disposition(), RecoverySourceDisposition::SaveCurrent);
    assert_ne!(
        guard.original_disk().bytes(),
        guard.reviewed_working().bytes()
    );
    let falsified = fixture::rebuild(pending.recovery(), None, true);
    fixture::install_package(&pending, &falsified);
    drop(pending);
    let disk = fixture.disk_state();
    let mut host = budget();
    let pending = load(&fixture, &mut host);
    let guard_index = pending
        .recovery()
        .complete_original_sources()
        .iter()
        .position(|source| source.path() == GUARD)
        .unwrap();
    let observation = RecoveredPrefabExtractionInventory::capture(
        &pending,
        SceneFileLimits::default(),
        &mut host,
    )
    .unwrap();
    let basis = observation.basis().clone();
    let validation_decoded = validation_decoded_bytes(&pending, &observation, &mut host);
    let decoded = generation(pending, observation, host).advance().unwrap();
    let verified = decoded.advance().unwrap();
    assert_eq!(verified.phase(), Phase::Saved);
    assert!(verified.verified().is_none());
    let before = verified.source_usage();
    let failure = verified.advance().unwrap_err();
    let RecoveredPrefabExtractionGenerationError::SavedGeneration(error) = failure.error() else {
        panic!("the original disk Guard must fail independent saved admission: {failure:?}");
    };
    assert_eq!(error.source_index(), Some(guard_index));
    assert!(error.report().diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-extraction-saved-guard-relocation"
    }));
    assert_eq!(failure.owner().phase(), Phase::Saved);
    assert!(failure.owner().source_usage().work_nodes > before.work_nodes);
    let source = failure.owner().source_usage();
    let host = failure.owner().host_usage();
    let package = failure
        .owner()
        .pending()
        .recovery()
        .encoded_bytes()
        .as_ptr();
    let failure = failure.retry().unwrap_err();
    assert!(matches!(
        failure.error(),
        RecoveredPrefabExtractionGenerationError::SavedGeneration(_)
    ));
    assert_eq!(failure.owner().phase(), Phase::Saved);
    assert!(failure.owner().verified().is_none());
    assert_eq!(failure.owner().observation().basis(), &basis);
    assert_eq!(
        failure
            .owner()
            .pending()
            .recovery()
            .encoded_bytes()
            .as_ptr(),
        package
    );
    assert!(failure.owner().source_usage().work_nodes > source.work_nodes);
    assert!(failure.owner().source_usage().work_bytes > source.work_bytes);
    assert_eq!(
        failure.owner().host_usage().decoded_bytes(),
        host.decoded_bytes() + validation_decoded
    );
    assert_eq!(
        failure.owner().host_usage().retained_candidate_bytes(),
        host.retained_candidate_bytes()
    );
    assert_eq!(
        failure.owner().host_usage().retained_open_handles(),
        host.retained_open_handles()
    );
    assert!(failure.owner().host_usage().decode_work() > host.decode_work());
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn complete_generation_rejects_equal_byte_replacement_and_retry_cannot_refresh_either_budget() {
    let fixture = Fixture::new();
    publish_pending(&fixture, vec![]);
    let mut host = budget();
    let pending = load(&fixture, &mut host);
    let observation = RecoveredPrefabExtractionInventory::capture(
        &pending,
        SceneFileLimits::default(),
        &mut host,
    )
    .unwrap();
    let validation_decoded = validation_decoded_bytes(&pending, &observation, &mut host);
    let owner = complete(generation(pending, observation, host));
    let basis = owner.observation().basis().clone();
    let (corpus, saved) = owner.verified().unwrap();
    let sources = corpus.sources().as_ptr();
    let q = corpus.extracted().document() as *const PrefabDocument;
    let saved_sources = saved.sources().as_ptr();
    let selected = owner
        .pending()
        .recovery()
        .complete_original_sources()
        .iter()
        .find(|source| source.path() == P)
        .unwrap();
    replace(owner.pending(), P, selected.original_disk().bytes());
    let disk = fixture.disk_state();
    let source = owner.source_usage();
    let host = owner.host_usage();
    let failure = owner.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        RecoveredPrefabExtractionGenerationError::Inventory(_)
    ));
    assert_eq!(failure.owner().phase(), Phase::Complete);
    assert_eq!(failure.owner().source_usage(), source);
    assert!(failure.owner().host_usage().decode_work() > host.decode_work());
    let host = failure.owner().host_usage();
    let failure = failure.retry().unwrap_err();
    assert!(matches!(
        failure.error(),
        RecoveredPrefabExtractionGenerationError::Inventory(_)
    ));
    assert_eq!(failure.owner().phase(), Phase::Complete);
    assert_eq!(failure.owner().observation().basis(), &basis);
    let (corpus, saved) = failure.owner().verified().unwrap();
    assert_eq!(corpus.sources().as_ptr(), sources);
    assert_eq!(corpus.extracted().document() as *const _, q);
    assert_eq!(saved.sources().as_ptr(), saved_sources);
    assert_eq!(failure.owner().source_usage(), source);
    assert_eq!(
        failure.owner().host_usage().decoded_bytes(),
        host.decoded_bytes() + validation_decoded
    );
    assert_eq!(
        failure.owner().host_usage().retained_candidate_bytes(),
        host.retained_candidate_bytes()
    );
    assert_eq!(
        failure.owner().host_usage().retained_open_handles(),
        host.retained_open_handles()
    );
    assert!(failure.owner().host_usage().decode_work() > host.decode_work());
    assert_eq!(fixture.disk_state(), disk);
}
