use super::*;
use crate::project_host::authoring::{
    source_upgrade_publication::{
        SourceUpgradePublication, SourceUpgradePublicationError, SourceUpgradePublicationStep,
    },
    source_upgrade_storage::{
        PendingRecoveryStorage, RecoveryPendingPublication, RecoveryStorageHost,
        RecoveryStoragePublication,
    },
};
use nara_project::ProjectManifest;

fn pending(
    fixture: &Fixture,
    recovery: Arc<SourceUpgradeRecovery>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> PendingRecoveryStorage {
    fs::create_dir_all(fixture.root.join(".nara/upgrades")).unwrap();
    let prepared = RecoveryStorageHost::capture(fixture.schema.project_root(), budget)
        .unwrap()
        .prepare(
            recovery,
            ProjectPath::new(".nara/upgrades/publication.json").unwrap(),
            budget,
        )
        .unwrap();
    let stored = match prepared.publish(budget) {
        RecoveryStoragePublication::Published(stored) => stored,
        _ => panic!("recovery storage did not publish"),
    };
    match stored.bind_pending(budget).unwrap().publish(budget) {
        RecoveryPendingPublication::Published(pending) => pending,
        _ => panic!("Pending did not publish"),
    }
}

fn prepare(
    fixture: &Fixture,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> (
    PreparedAuthoredSourceUpgrade,
    SourceUpgradePublication,
    PendingRecoveryStorage,
) {
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = fixture.capture_workspace(&mut work);
    let inventory = fixture.capture_inventory(budget);
    let recovery = fixture.recovery(false, false).into_shared(budget).unwrap();
    let selected =
        PreparedSourceUpgradeSchema::prepare(&fixture.schema, &fixture.previous, &recovery, budget)
            .unwrap();
    let reviewed = PreparedAuthoredSourceUpgrade::prepare(
        &fixture.schema,
        &selected,
        &recovery,
        inventory,
        input,
        &fixture.open_sources(),
        budget,
        &mut work,
    )
    .unwrap();
    let publication = SourceUpgradePublication::prepare(&reviewed, budget).unwrap();
    let pending = pending(fixture, recovery, budget);
    (reviewed, publication, pending)
}

fn publish_all(
    publication: &mut SourceUpgradePublication,
    pending: &PendingRecoveryStorage,
    budget: &mut AuthoringProjectAdmissionBudget,
) {
    while !publication.is_complete() {
        match publication.publish_next(pending.observed(), budget) {
            SourceUpgradePublicationStep::Published { .. }
            | SourceUpgradePublicationStep::Observed { .. } => {}
            SourceUpgradePublicationStep::Refused { error, .. } => {
                panic!("publication refused: {error:?}")
            }
            _ => panic!("publication did not complete a step"),
        }
    }
}

#[test]
fn exact_reviewed_sources_precede_the_bundle_and_actual_saved_observations() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    assert_eq!(publication.operation(), reviewed.recovery().digest());
    assert!(matches!(
        publication.observe_source(0, &mut budget),
        Err(SourceUpgradePublicationError::Incomplete)
    ));
    let count = reviewed.recovery().members().len();
    for index in 0..count {
        assert!(
            matches!(publication.publish_next(pending.observed(), &mut budget), SourceUpgradePublicationStep::Published { index: actual } if actual == index)
        );
        assert_eq!(publication.completed(), index + 1);
        assert_eq!(
            fs::read(fixture.root.join("schema.json")).unwrap(),
            reviewed.recovery().bundle().old_disk.bytes()
        );
        assert_eq!(
            fs::read(
                fixture
                    .root
                    .join(&reviewed.recovery().members()[index].path)
            )
            .unwrap(),
            reviewed.recovery().members()[index].successor.bytes()
        );
        assert!(publication.publication(index).is_some());
    }
    assert!(
        matches!(publication.publish_next(pending.observed(), &mut budget), SourceUpgradePublicationStep::Published { index } if index == count)
    );
    assert!(publication.is_complete());
    publication
        .validate_complete(pending.observed(), &mut budget)
        .unwrap();
    for (index, member) in reviewed.recovery().members().iter().enumerate() {
        let actual = publication.observe_source(index, &mut budget).unwrap();
        assert_eq!(actual.source_key(), &member.key);
        assert_eq!(actual.file.identity(), actual.source.identity());
        assert_eq!(
            actual.source.digest(),
            ContentDigest::of_bytes(member.successor.bytes())
        );
        assert_eq!(
            publication
                .publication(index)
                .unwrap()
                .receipt()
                .candidate_identity(),
            actual.file.identity()
        );
        drop(actual);
        budget.release_open_handles(1);
    }
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.schema.project_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn an_external_original_edit_stops_the_next_step_without_replacing_the_bundle_or_previous_evidence()
{
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    assert!(matches!(
        publication.publish_next(pending.observed(), &mut budget),
        SourceUpgradePublicationStep::Published { index: 0 }
    ));
    let next = &reviewed.recovery().members()[1];
    fs::write(
        fixture.root.join(&next.path),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    assert!(matches!(
        publication.publish_next(pending.observed(), &mut budget),
        SourceUpgradePublicationStep::Refused {
            index: 1,
            error: SourceUpgradePublicationError::SourceChanged
        }
    ));
    assert_eq!(publication.completed(), 1);
    assert!(publication.publication(0).is_some());
    assert_eq!(
        fs::read(fixture.root.join("schema.json")).unwrap(),
        reviewed.recovery().bundle().old_disk.bytes()
    );
    assert!(!publication.is_complete());
}

#[test]
fn pending_changes_and_exhausted_work_refuse_before_the_first_destructive_write() {
    for change_manifest in [true, false] {
        let fixture = Fixture::new();
        let mut budget = budget();
        let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
        if change_manifest {
            fs::write(fixture.root.join("nara.toml"), b"schema_version = 1\n").unwrap();
        } else {
            budget.limits.decode_work = ItemLimit::new(budget.usage().decode_work()).unwrap();
        }
        assert!(matches!(
            publication.publish_next(pending.observed(), &mut budget),
            SourceUpgradePublicationStep::Refused { index: 0, .. }
        ));
        assert_eq!(publication.completed(), 0);
        for member in reviewed.recovery().members() {
            assert_eq!(
                fs::read(fixture.root.join(&member.path)).unwrap(),
                member.old_disk.bytes()
            );
        }
    }
}

#[test]
fn complete_output_checks_detect_later_external_replacement() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    publish_all(&mut publication, &pending, &mut budget);
    let member = &reviewed.recovery().members()[0];
    fs::write(fixture.root.join(&member.path), member.old_disk.bytes()).unwrap();
    assert!(matches!(
        publication.validate_complete(pending.observed(), &mut budget),
        Err(SourceUpgradePublicationError::SourceChanged)
    ));
    assert!(publication.publication(0).is_some());
}

#[test]
fn partial_publication_keeps_the_failed_publisher_and_cleanup_until_retry() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    assert!(matches!(
        publication.publish_next(pending.observed(), &mut budget),
        SourceUpgradePublicationStep::Published { index: 0 }
    ));
    let handles = budget.usage().retained_open_handles();
    let control = publication.test_fail_next_write();
    assert!(matches!(
        publication.publish_next(pending.observed(), &mut budget),
        SourceUpgradePublicationStep::Failed { index: 1, .. }
    ));
    assert!(publication.cleanup_pending());
    assert_eq!(budget.usage().retained_open_handles(), handles + 2);
    assert!(!publication.retry_cleanup());
    assert_eq!(budget.usage().retained_open_handles(), handles + 2);
    assert_eq!(publication.completed(), 1);
    assert!(publication.publication(0).is_some());
    assert!(publication.publication(1).is_none());
    let failed = &reviewed.recovery().members()[1];
    assert_eq!(
        fs::read(fixture.root.join(&failed.path)).unwrap(),
        failed.old_disk.bytes()
    );
    control.release();
    assert!(publication.retry_cleanup());
    // The active owner keeps both admitted slots until the next publication succeeds.
    assert_eq!(budget.usage().retained_open_handles(), handles + 2);
    publish_all(&mut publication, &pending, &mut budget);
    assert!(!publication.cleanup_pending());
    assert!(!publication.uncertain());
    assert_eq!(budget.usage().retained_open_handles(), handles);
    publication
        .validate_complete(pending.observed(), &mut budget)
        .unwrap();
}

#[test]
fn the_batch_keeps_original_custody_after_the_reviewed_source_owner_drops() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    let original = reviewed.inventory().shared_file(PATHS[3]).unwrap();
    let original_lifetime = Arc::downgrade(&original);
    drop(original);
    let handles = budget.usage().retained_open_handles();
    let original_handles = reviewed.inventory().retained_handle_count();
    drop(reviewed);
    assert!(original_lifetime.upgrade().is_some());
    assert_eq!(budget.usage().retained_open_handles(), handles);
    publish_all(&mut publication, &pending, &mut budget);
    assert!(original_lifetime.upgrade().is_some());
    assert_eq!(budget.usage().retained_open_handles(), handles);
    drop(publication);
    assert!(original_lifetime.upgrade().is_none());
    budget.release_open_handles(original_handles);
    assert_eq!(
        budget.usage().retained_open_handles(),
        handles - original_handles
    );
}

#[test]
fn active_and_fresh_output_handles_refuse_before_opening_when_the_allowance_is_short() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    let handles = budget.usage().retained_open_handles();
    // Pending validation fits; binding needs its own retained leaf/cleanup reservation in
    // addition to the same sixteen-slot transient path allowance.
    budget.limits.peak_open_handles = ItemLimit::new(handles + 17).unwrap();
    assert!(matches!(
        publication.publish_next(pending.observed(), &mut budget),
        SourceUpgradePublicationStep::Refused {
            index: 0,
            error: SourceUpgradePublicationError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
            )
        }
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(publication.completed(), 0);
    assert_eq!(
        fs::read(fixture.root.join(PATHS[0])).unwrap(),
        encoded(0, false, false)
    );
    budget.limits.peak_open_handles = ItemLimit::new(64).unwrap();
    publish_all(&mut publication, &pending, &mut budget);
    let actual = publication.observe_source(0, &mut budget).unwrap();
    assert_eq!(budget.usage().retained_open_handles(), handles + 1);
    assert_eq!(actual.source_key(), &reviewed.recovery().members()[0].key);
    drop(actual);
    budget.release_open_handles(1);
    publication
        .validate_complete(pending.observed(), &mut budget)
        .unwrap();
    assert_eq!(budget.usage().retained_open_handles(), handles);
    let retained = budget.usage().retained_candidate_bytes();
    let member = &reviewed.recovery().members()[0];
    fs::write(fixture.root.join(&member.path), member.old_disk.bytes()).unwrap();
    assert!(matches!(
        publication.observe_source(0, &mut budget),
        Err(SourceUpgradePublicationError::SourceChanged)
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
}

#[test]
fn a_new_equal_output_leaf_is_a_fresh_observation_and_keeps_historical_publication_separate() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, mut publication, pending) = prepare(&fixture, &mut budget);
    publish_all(&mut publication, &pending, &mut budget);
    let member = &reviewed.recovery().members()[0];
    let original_identity = publication
        .publication(0)
        .unwrap()
        .receipt()
        .candidate_identity();
    let original = fixture
        .schema
        .project_root()
        .open_file_exact(&RelativePath::new(&member.path).unwrap())
        .unwrap();
    let mut replacement =
        crate::project_host::file_publication::FilePublicationHost::bind_existing(
            &fixture.schema.project_root(),
            &member.path,
            &original,
            ContentDigest::of_bytes(member.successor.bytes()),
            SceneFileLimits::default().encoded_bytes().get() as u64,
        )
        .unwrap();
    assert!(matches!(
        replacement.publish(member.successor.bytes()),
        crate::project_host::file_publication::FilePublicationOutcome::Published(_)
    ));
    publication
        .validate_complete(pending.observed(), &mut budget)
        .unwrap();
    let current = publication.observe_source(0, &mut budget).unwrap();
    assert_ne!(current.file.identity(), original_identity);
    assert_eq!(
        current.source.digest(),
        ContentDigest::of_bytes(member.successor.bytes())
    );
    assert_eq!(
        publication
            .publication(0)
            .unwrap()
            .receipt()
            .candidate_identity(),
        original_identity
    );
}

#[test]
fn refused_batch_preparation_releases_its_partial_entries_and_keeps_the_reviewed_inputs() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let (reviewed, _, _) = prepare(&fixture, &mut budget);
    let retained = budget.usage().retained_candidate_bytes();
    budget.limits.retained_candidate_bytes = nara_core::ByteLimit::new(retained + 1).unwrap();
    let work = budget.usage().decode_work();
    for _ in 0..2 {
        assert!(matches!(
            SourceUpgradePublication::prepare(&reviewed, &mut budget),
            Err(SourceUpgradePublicationError::Admission(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
            ))
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    }
    assert!(budget.usage().decode_work() > work);
    assert_eq!(reviewed.recovery().members().len(), 4);
}

#[test]
fn known_final_inventory_overlap_is_checked_before_pending_without_recording_a_fake_peak() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = fixture.capture_workspace(&mut work);
    let inventory = fixture.capture_inventory(&mut budget);
    let recovery = fixture
        .recovery(false, false)
        .into_shared(&mut budget)
        .unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    let reviewed = PreparedAuthoredSourceUpgrade::prepare(
        &fixture.schema,
        &selected,
        &recovery,
        inventory,
        input,
        &fixture.open_sources(),
        &mut budget,
        &mut work,
    )
    .unwrap();
    let handles = budget.usage().retained_open_handles();
    let observed_peak = budget.usage().peak_open_handles();
    let required = handles
        + reviewed.inventory().retained_handle_count() * 2
        + reviewed.workspace().documents().len()
        // Independently count confirmed Pending's three handles and the Ready owner's six;
        // storage's earlier five-slot peak no longer overlaps this final confirmation.
        + 3 + 6
        + 16;
    assert!(required > observed_peak);
    let retained = budget.usage().retained_candidate_bytes();
    budget.limits.peak_open_handles = ItemLimit::new(required - 1).unwrap();
    assert!(
        matches!(SourceUpgradePublication::prepare(&reviewed, &mut budget),
            Err(SourceUpgradePublicationError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum }
            )) if maximum == required - 1
        )
    );
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(budget.usage().peak_open_handles(), observed_peak);
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    budget.limits.peak_open_handles = ItemLimit::new(required).unwrap();
    let publication = SourceUpgradePublication::prepare(&reviewed, &mut budget).unwrap();
    assert_eq!(publication.completed(), 0);
    assert_eq!(budget.usage().peak_open_handles(), observed_peak);
    let manifest =
        ProjectManifest::parse_toml_bytes(&fs::read(fixture.root.join("nara.toml")).unwrap())
            .manifest
            .unwrap();
    assert_eq!(
        manifest.authoring.state,
        nara_project::ProjectAuthoringState::Ready
    );
    for (index, path) in PATHS.iter().enumerate() {
        assert_eq!(
            fs::read(fixture.root.join(path)).unwrap(),
            encoded(index, false, false)
        );
    }
}
