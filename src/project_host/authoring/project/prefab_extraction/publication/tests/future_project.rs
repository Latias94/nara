//! Actual future Project and final-file preparation, independently of Session installation.

use super::super::super::publication::{
    PrefabExtractionProjectOutcome, ProjectPrefabExtractionFutureInventory,
};
use super::future_inventory::{checkpoints_complete, recovered_with_budget};
use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionLimits,
    recovery_storage::RecoveryStorageError,
};

fn future(fixture: &mut Fixture, recovered: bool) -> ProjectPrefabExtractionFutureInventory {
    let owner = if recovered {
        recovered_with_budget(fixture, budget())
    } else {
        checkpoints_complete(confirmed(fixture).into_publication().unwrap())
    };
    owner.into_future_inventory().unwrap()
}

fn actual_basis(
    future: &ProjectPrefabExtractionFutureInventory,
) -> nara_scene::AuthoringSchemaBasis {
    let files = future.publication();
    if let Some(outputs) = files.live_outputs() {
        outputs
            .review()
            .persistence
            .sources()
            .prepared()
            .corpus()
            .schema_basis()
            .clone()
    } else {
        files
            .recovered_generation()
            .unwrap()
            .observation()
            .basis()
            .clone()
    }
}

#[test]
fn future_projects_reuse_the_actual_live_or_recovered_basis_and_remain_private_under_pending() {
    for stable in [false, true] {
        for recovered in [false, true] {
            let mut fixture = Fixture::with_native_identity(if stable {
                "stable_metadata"
            } else {
                "project_location"
            });
            let future = future(&mut fixture, recovered);
            let basis = actual_basis(&future);
            let files = fixture.disk_state();
            let working = live_state(&fixture);
            let inventory_custody = future.inventory().shared_custody();
            let source_allocations = future
                .inventory()
                .participants()
                .map(|source| source.bytes().as_ptr() as usize)
                .collect::<Vec<_>>();
            let mut host = AuthoringProjectAdmissionHost::default();
            let mut prepared = future.prepare_project(&mut host).unwrap();
            assert_eq!(prepared.basis(), &basis);
            assert_eq!(
                prepared
                    .pending_project()
                    .fork_startup_scene()
                    .schema_basis(),
                basis
            );
            assert_eq!(
                basis == fixture.project.startup_scene.schema_basis(),
                !recovered
            );
            assert!(
                prepared
                    .future_inventory()
                    .inventory()
                    .shared_custody()
                    .same_owner(&inventory_custody)
            );
            assert_eq!(
                prepared
                    .future_inventory()
                    .inventory()
                    .participants()
                    .map(|source| source.bytes().as_ptr() as usize)
                    .collect::<Vec<_>>(),
                source_allocations
            );
            let project = prepared.pending_project();
            assert!(matches!(
                project.settings().authoring_state,
                ProjectAuthoringState::Pending(_)
            ));
            assert!(
                project
                    .prefab_sources()
                    .any(|prefab| prefab.source().project_path().as_str() == Q)
            );
            assert_eq!(project.readiness(), AuthoringProjectReadiness::Complete);
            prepared.validate().unwrap();
            assert_eq!(fixture.disk_state(), files);
            assert_eq!(live_state(&fixture), working);
            assert_pending(&fixture);
        }
    }
}

#[test]
fn refused_future_project_returns_the_same_native_inventory_and_semantics_for_retry() {
    let mut fixture = Fixture::new();
    let mut future = future(&mut fixture, false);
    let before = future.host_usage().unwrap();
    let basis = actual_basis(&future);
    let bodies = future
        .inventory()
        .participants()
        .map(|source| source.bytes().as_ptr() as usize)
        .collect::<Vec<_>>();
    let original = fs::read(fixture.root.join(CLOSED)).unwrap();
    fs::write(fixture.root.join(CLOSED), b"changed closed source").unwrap();
    let mut host = AuthoringProjectAdmissionHost::default();
    let failure = future.prepare_project(&mut host).unwrap_err();
    assert_eq!(actual_basis(failure.owner()), basis);
    assert_eq!(
        failure
            .owner()
            .inventory()
            .participants()
            .map(|source| source.bytes().as_ptr() as usize)
            .collect::<Vec<_>>(),
        bodies
    );
    // The owner itself may move; its frozen allocations, observations and nominal basis do not.
    let mut future = failure.into_owner();
    let after = future.host_usage().unwrap();
    assert_eq!(
        after.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        after.retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(after.decode_work() > before.decode_work());
    fs::write(fixture.root.join(CLOSED), original).unwrap();
    let mut prepared = future.prepare_project(&mut host).unwrap();
    assert_eq!(prepared.basis(), &basis);
    prepared.validate().unwrap();
    assert_pending(&fixture);
}

#[test]
fn future_project_capacity_refusal_drops_only_new_storage_and_does_not_restart_work() {
    let mut probe = Fixture::new();
    let future = future(&mut probe, true);
    let mut prepared = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    let required = prepared.host_usage().unwrap().aggregate_encoded_bytes();
    drop(prepared);
    let mut exact_fixture = Fixture::new();
    let exact_budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::new(
        ByteLimit::new(required).unwrap(),
    ))
    .for_source_operation(
        ItemLimit::new(200_000_000).unwrap(),
        ItemLimit::new(128).unwrap(),
    );
    let exact_future = recovered_with_budget(&mut exact_fixture, exact_budget)
        .into_future_inventory()
        .unwrap();
    let mut exact_prepared = exact_future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    assert_eq!(
        exact_prepared
            .host_usage()
            .unwrap()
            .aggregate_encoded_bytes(),
        required
    );
    drop(exact_prepared);
    let limits = AuthoringProjectAdmissionLimits::new(ByteLimit::new(required - 1).unwrap());
    let host_budget = AuthoringProjectAdmissionBudget::new(limits).for_source_operation(
        ItemLimit::new(200_000_000).unwrap(),
        ItemLimit::new(128).unwrap(),
    );
    let mut fixture = Fixture::new();
    let owner = recovered_with_budget(&mut fixture, host_budget);
    let mut future = owner.into_future_inventory().unwrap();
    let before = future.host_usage().unwrap();
    let basis = actual_basis(&future);
    let disk = fixture.disk_state();
    let failure = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap_err();
    assert_eq!(actual_basis(failure.owner()), basis);
    let mut future = failure.into_owner();
    let after = future.host_usage().unwrap();
    assert_eq!(
        after.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        after.retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(after.aggregate_encoded_bytes() > before.aggregate_encoded_bytes());
    assert!(after.decode_work() > before.decode_work());
    let failure = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap_err();
    let mut future = failure.into_owner();
    assert!(
        future.host_usage().unwrap().aggregate_encoded_bytes() >= after.aggregate_encoded_bytes()
    );
    assert_eq!(fixture.disk_state(), disk);
    assert_pending(&fixture);
}

#[test]
fn recovered_final_ready_uses_the_actual_publication_and_an_ordinary_reopen_reads_the_new_project()
{
    for stable in [false, true] {
        let mut fixture = Fixture::with_native_identity(if stable {
            "stable_metadata"
        } else {
            "project_location"
        });
        let future = future(&mut fixture, true);
        let basis = actual_basis(&future);
        let mut prepared = future
            .prepare_project(&mut AuthoringProjectAdmissionHost::default())
            .unwrap();
        let before = prepared.host_usage().unwrap();
        let source_files = fixture.disk_state();
        let bound = prepared.bind().unwrap();
        assert_pending(&fixture);
        // Recovery has no live Workspace or persistence lanes. Their installation set is empty;
        // this exercises only the concrete Project/file owner, not the public Session workflow.
        let PrefabExtractionProjectOutcome::Published(mut published) = bound.publish().unwrap()
        else {
            panic!("the retained actual Pending should publish its fixed Ready generation");
        };
        assert_eq!(published.session.fork_startup_scene().schema_basis(), basis);
        assert!(matches!(
            published.session.settings().authoring_state,
            ProjectAuthoringState::Ready
        ));
        let manifest = fs::read(fixture.root.join("nara.toml")).unwrap();
        let observed = published
            .session
            .project_root()
            .open_file_exact(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        assert_eq!(
            published.session.schema.manifest_identity(),
            observed.identity()
        );
        assert_eq!(
            published.session.schema.manifest_digest(),
            ContentDigest::of_bytes(&manifest)
        );
        assert_eq!(
            published.evidence.receipt().published_identity(),
            Some(observed.identity())
        );
        let independently_opened = fixture
            .project
            .project_root()
            .open_file_exact(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        assert!(observed.same_object(&independently_opened).unwrap());
        published
            .session
            .startup_source_validation()
            .validate()
            .unwrap();
        assert!(
            published.session.prefab_sources().any(|prefab| prefab
                .source()
                .project_path()
                .as_str()
                == Q)
        );
        let after = published.files.host_usage().unwrap();
        assert!(after.decode_work() > before.decode_work());
        for (path, bytes) in source_files {
            if path != PathBuf::from("nara.toml") {
                assert_eq!(fs::read(fixture.root.join(path)).unwrap(), bytes);
            }
        }
        let reopened = admit_project(&fixture.root);
        assert_eq!(reopened.readiness(), AuthoringProjectReadiness::Complete);
        assert!(
            reopened
                .prefab_sources()
                .any(|prefab| prefab.source().project_path().as_str() == Q)
        );
        assert_ne!(reopened.startup_scene.schema_basis(), basis);
    }
}

#[test]
fn recovered_ready_failure_retains_the_bound_candidate_until_physical_cleanup_and_retry() {
    let mut fixture = Fixture::new();
    let future = future(&mut fixture, true);
    let prepared = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    let mut bound = prepared.bind().unwrap();
    let gate = bound.test_fail_next_write();
    let PrefabExtractionProjectOutcome::Failed { mut bound, stage } = bound.publish().unwrap()
    else {
        panic!("controlled candidate write failure must preserve its exact bound owner");
    };
    assert_eq!(stage, FilePublicationFailureStage::WriteTemporary);
    assert!(bound.cleanup_pending());
    assert!(!bound.retry_cleanup().unwrap());
    assert_pending(&fixture);
    gate.release();
    assert!(bound.retry_cleanup().unwrap());
    assert!(!bound.cleanup_pending());
    assert!(!bound.uncertain());
    assert!(matches!(
        bound.publish().unwrap(),
        PrefabExtractionProjectOutcome::Published(_)
    ));
}

#[test]
fn final_ready_revalidates_other_profile_sources_and_keeps_the_same_bound_owner_for_retry() {
    let mut fixture = Fixture::new();
    let future = future(&mut fixture, true);
    let prepared = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    let bound = prepared.bind().unwrap();
    let original = fs::read(fixture.root.join(R)).unwrap();
    fs::write(fixture.root.join(R), b"changed mobile-only source").unwrap();
    let refusal = match bound.publish() {
        Err(refusal) => refusal,
        Ok(_) => panic!("a non-startup profile's changed member must block final Ready"),
    };
    assert!(matches!(
        refusal.error(),
        super::super::super::publication::PrefabExtractionProjectError::Inventory(_)
    ));
    assert_pending(&fixture);
    fs::write(fixture.root.join(R), original).unwrap();
    assert!(matches!(
        refusal.into_bound().publish().unwrap(),
        PrefabExtractionProjectOutcome::Published(_)
    ));
}

#[test]
fn final_ready_cleanup_spends_the_original_work_allowance_and_refuses_before_native_retry() {
    let failed_ready = |fixture: &mut Fixture, host_budget| {
        let future = recovered_with_budget(fixture, host_budget)
            .into_future_inventory()
            .unwrap();
        let prepared = future
            .prepare_project(&mut AuthoringProjectAdmissionHost::default())
            .unwrap();
        let mut bound = prepared.bind().unwrap();
        let gate = bound.test_fail_next_write();
        let PrefabExtractionProjectOutcome::Failed { bound, .. } = bound.publish().unwrap() else {
            panic!("controlled Ready write must retain its cleanup owner");
        };
        (bound, gate)
    };
    let mut probe = Fixture::new();
    let (mut bound, gate) = failed_ready(&mut probe, budget());
    let before_cleanup = bound.host_usage().unwrap().decode_work();
    gate.release();
    assert!(bound.retry_cleanup().unwrap());
    drop(bound);
    let cleanup_work = crate::project_host::authoring::publication_admission::CLEANUP_WORK;
    let mut fixture = Fixture::new();
    let limit = ItemLimit::new(before_cleanup.checked_add(cleanup_work).unwrap()).unwrap();
    let host_budget = AuthoringProjectAdmissionBudget::new(Default::default())
        .for_source_operation(limit, ItemLimit::new(128).unwrap());
    let (mut bound, gate) = failed_ready(&mut fixture, host_budget);
    let before = bound.host_usage().unwrap();
    assert_eq!(before.decode_work(), before_cleanup);
    assert!(!bound.retry_cleanup().unwrap());
    let admitted = bound.host_usage().unwrap();
    assert_eq!(admitted.decode_work(), limit.get());
    assert_eq!(
        admitted.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        admitted.retained_open_handles(),
        before.retained_open_handles()
    );
    gate.release();
    assert!(bound.retry_cleanup().is_err());
    // If the native cleanup closure ran after the gate was released, this would be false.
    assert!(bound.cleanup_pending());
    let refused = bound.host_usage().unwrap();
    assert_eq!(
        refused.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        refused.retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(refused.decode_work() > admitted.decode_work());
    assert_pending(&fixture);
}

#[test]
fn original_workspace_stage_moves_to_its_guard_while_the_final_project_keeps_native_authority() {
    let mut fixture = Fixture::new();
    let future = future(&mut fixture, false);
    let prepared = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    let basis = prepared.basis().clone();
    let working = live_state(&fixture);
    let disk = fixture.disk_state();
    let mut installation = prepared
        .bind()
        .unwrap()
        .into_workspace_installation()
        .unwrap();
    let corpus = installation.persistence().sources().prepared().corpus();
    let originals = corpus.original_sources().as_ptr();
    let sources = corpus.sources().as_ptr();
    assert_eq!(corpus.schema_basis(), &basis);
    installation.project_mut().validate().unwrap();
    let mut guarded = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    // The original persistence stage is now owned by the Workspace guard, not the publisher.
    // Full native/Pending validation remains usable while that guard borrows live Workspace.
    guarded.project_mut().validate().unwrap();
    let usage = guarded.project_mut().host_usage().unwrap();
    let mut restored = guarded.into_prepared();
    assert_eq!(restored.project_mut().host_usage().unwrap(), usage);
    let corpus = restored.persistence().sources().prepared().corpus();
    assert_eq!(corpus.original_sources().as_ptr(), originals);
    assert_eq!(corpus.sources().as_ptr(), sources);
    assert_eq!(corpus.schema_basis(), &basis);
    assert_eq!(live_state(&fixture), working);
    assert_eq!(fixture.disk_state(), disk);
    let guarded = restored
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    drop(guarded.into_prepared());
    assert_eq!(live_state(&fixture), working);
    assert_pending(&fixture);
}

#[test]
fn late_workspace_refusal_returns_the_exact_detached_stage_without_recapturing_the_project() {
    let mut fixture = Fixture::new();
    let future = future(&mut fixture, false);
    let prepared = future
        .prepare_project(&mut AuthoringProjectAdmissionHost::default())
        .unwrap();
    let mut installation = prepared
        .bind()
        .unwrap()
        .into_workspace_installation()
        .unwrap();
    let corpus = installation.persistence().sources().prepared().corpus();
    let sources = corpus.sources().as_ptr();
    let basis = corpus.schema_basis().clone();
    let before = installation.project_mut().host_usage().unwrap();
    let document = fixture.document(P);
    let source = fixture.workspace.prefab(document).unwrap().session();
    let patch = crate::scene_test_support::add_root_entity_patch(
        &source.document().placements,
        SceneEntityRecord::new(id("late-workspace-edit")),
    );
    let base_revision = fixture.workspace.document_revision(document).unwrap();
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::ApplyPrefabPatch {
                document,
                base_revision,
                patch,
            })
            .applied
    );
    let working = live_state(&fixture);
    let failure = match installation.bind_workspace(&fixture.authority, &mut fixture.workspace) {
        Err(failure) => failure,
        Ok(_) => panic!("the captured original source no longer owns the live Workspace revision"),
    };
    assert!(!failure.report().applied);
    let mut restored = failure.into_prepared();
    let corpus = restored.persistence().sources().prepared().corpus();
    assert_eq!(corpus.sources().as_ptr(), sources);
    assert_eq!(corpus.schema_basis(), &basis);
    assert_eq!(restored.project_mut().host_usage().unwrap(), before);
    restored.project_mut().validate().unwrap();
    assert_eq!(live_state(&fixture), working);
    assert_pending(&fixture);
}

#[test]
fn completed_stage_transfer_uses_the_original_allowance_at_exact_and_one_short_work() {
    let prepare = |fixture: &Fixture, work| {
        let request = fixture.request_with_host_limits(
            SceneCompilationLimits::default(),
            ItemLimit::new(128).unwrap(),
            ItemLimit::new(work).unwrap(),
        );
        let storage = fixture::confirmed_request(fixture, request);
        let future = checkpoints_complete(storage.into_publication().unwrap())
            .into_future_inventory()
            .unwrap();
        future
            .prepare_project(&mut AuthoringProjectAdmissionHost::default())
            .unwrap()
            .bind()
            .unwrap()
    };
    let probe = Fixture::new();
    let mut installation = prepare(&probe, 200_000_000)
        .into_workspace_installation()
        .unwrap();
    let required = installation
        .project_mut()
        .host_usage()
        .unwrap()
        .decode_work();
    drop(installation);
    for limit in [required, required - 1] {
        let fixture = Fixture::new();
        let mut bound = prepare(&fixture, limit);
        let before = bound.host_usage().unwrap();
        let corpus = bound
            .live_outputs()
            .unwrap()
            .review()
            .persistence
            .sources()
            .prepared()
            .corpus();
        let sources = corpus.sources().as_ptr();
        let basis = corpus.schema_basis().clone();
        let disk = fixture.disk_state();
        let working = live_state(&fixture);
        match bound.into_workspace_installation() {
            Ok(mut installation) => {
                assert_eq!(limit, required);
                assert_eq!(
                    installation
                        .project_mut()
                        .host_usage()
                        .unwrap()
                        .decode_work(),
                    required
                );
                let corpus = installation.persistence().sources().prepared().corpus();
                assert_eq!(corpus.sources().as_ptr(), sources);
                assert_eq!(corpus.schema_basis(), &basis);
            }
            Err(refusal) => {
                assert_eq!(limit, required - 1);
                assert!(matches!(refusal.error(),
                    super::super::super::publication::PrefabExtractionProjectError::Publication(
                        ProjectPrefabExtractionPublicationError::Installation(
                            super::super::super::outputs::storage::ProjectPrefabExtractionStorageError::Storage(
                                RecoveryStorageError::Admission(crate::project_host::authoring::AuthoringProjectAdmissionError::DecodeWorkExceeded { .. })
                            )
                        )
                    )
                ), "{:?}", refusal.error());
                let mut bound = refusal.into_bound();
                let after = bound.host_usage().unwrap();
                assert_eq!(
                    after.retained_candidate_bytes(),
                    before.retained_candidate_bytes()
                );
                assert_eq!(
                    after.retained_open_handles(),
                    before.retained_open_handles()
                );
                assert!(after.decode_work() > before.decode_work());
                let corpus = bound
                    .live_outputs()
                    .unwrap()
                    .review()
                    .persistence
                    .sources()
                    .prepared()
                    .corpus();
                assert_eq!(corpus.sources().as_ptr(), sources);
                assert_eq!(corpus.schema_basis(), &basis);
            }
        }
        assert_eq!(fixture.disk_state(), disk);
        assert_eq!(live_state(&fixture), working);
        assert_pending(&fixture);
    }
}
