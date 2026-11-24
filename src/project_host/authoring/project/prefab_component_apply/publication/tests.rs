use std::fs;

use nara_project::{ProjectAuthoringState, ProjectManifest};
use nara_scene::SceneFileLimits;

use super::*;
use crate::project_host::authoring::{
    project::prefab_component_apply::physical::{
        capture_pending_inventory,
        tests::{budget, load, raw_pending},
    },
    source_inventory::test_support::Fixture,
};
use crate::project_host::file_publication::CleanupTestControl;

fn writer(fixture: &Fixture) -> ComponentApplyPublication {
    let mut budget = budget();
    let pending = load(fixture, &mut budget);
    let physical =
        capture_pending_inventory(&pending, SceneFileLimits::default(), &mut budget).unwrap();
    ComponentApplyPublication::new(pending, physical, budget)
}

fn prepare(
    _: ComponentApplyReadyInput<'_>,
    _: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplyPublicationError> {
    Ok(())
}

#[test]
fn fresh_writer_keeps_the_first_actual_output_and_publishes_ready_after_the_second() {
    let fixture = raw_pending();
    let mut original = writer(&fixture);
    let [(_, _), (destination, _)] = original.pending.recovery().outputs();
    let second_path = fixture.root.join(destination.path());
    let second_original = fs::read(&second_path).unwrap();
    let pending = fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap();

    assert_eq!(original.advance().unwrap(), Some(0));
    let first = original.accepted_output(0).unwrap();
    assert!(first.evidence().is_some());
    let first_file = Arc::clone(first.file());
    assert_eq!(fs::read(&second_path).unwrap(), second_original);
    assert!(matches!(
        original.prepare_ready(prepare),
        Err(ComponentApplyPublicationError::Phase)
    ));
    assert_eq!(
        fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap(),
        pending
    );
    original.close().unwrap();

    let mut resumed = writer(&fixture);
    assert_eq!(resumed.advance().unwrap(), Some(0));
    let observed = resumed.accepted_output(0).unwrap();
    assert!(observed.evidence().is_none());
    assert!(observed.file().same_object(&first_file).unwrap());
    assert_eq!(
        observed.file().parent_identity(),
        Some(observed.parent().identity())
    );
    assert_eq!(resumed.advance().unwrap(), Some(1));
    assert!(resumed.accepted_output(1).unwrap().evidence().is_some());
    assert_eq!(resumed.advance().unwrap(), None);

    let prepared = resumed
        .prepare_ready(|input, _| {
            assert_eq!(
                input.outputs[1].digest(),
                input.pending.recovery().outputs()[1].1.digest()
            );
            Ok::<_, ComponentApplyPublicationError>(Arc::clone(input.outputs[0].file()))
        })
        .unwrap();
    assert_eq!(
        fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap(),
        pending
    );
    let (_, ready) = resumed
        .publish_ready(|input, _| {
            assert!(input.outputs[0].file().same_object(&prepared).unwrap());
            assert!(input.outputs[0].file().same_object(&first_file).unwrap());
            assert_eq!(
                fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap(),
                pending
            );
            Ok::<_, ComponentApplyPublicationError>(())
        })
        .unwrap();
    assert!(!resumed.is_pending());
    let ready_bytes = fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap();
    assert_eq!(
        ContentDigest::of_bytes(&ready_bytes),
        ready.source().digest()
    );
    assert_eq!(
        ProjectManifest::parse_toml_bytes(&ready_bytes)
            .manifest
            .unwrap()
            .authoring
            .state,
        ProjectAuthoringState::Ready
    );
}

#[derive(Debug, thiserror::Error)]
enum TestReadyError {
    #[error(transparent)]
    Publication(#[from] ComponentApplyPublicationError),
    #[error("caller refused binding")]
    Caller,
}

#[test]
fn caller_refusal_and_guard_drift_after_prepare_keep_pending_and_both_accepted_outputs() {
    let fixture = raw_pending();
    let mut publication = writer(&fixture);
    assert_eq!(publication.advance().unwrap(), Some(0));
    assert_eq!(publication.advance().unwrap(), Some(1));
    let manifest = fixture.root.join(PROJECT_MANIFEST_PATH);
    let pending = fs::read(&manifest).unwrap();
    let first = Arc::clone(publication.accepted_output(0).unwrap().file());
    let guard = fixture.root.join("scenes/dependent.scene.json");
    let original = fs::read(&guard).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;

    assert!(matches!(
        publication.publish_ready(|_, _| Err::<(), _>(TestReadyError::Caller)),
        Err(TestReadyError::Caller)
    ));
    let error = publication
        .publish_ready(|_, _| {
            fs::write(&guard, &changed).unwrap();
            Ok::<_, ComponentApplyPublicationError>(())
        })
        .unwrap_err();
    assert!(matches!(
        error,
        ComponentApplyPublicationError::Inventory(SourceInventoryError::SourceChanged)
    ));
    assert!(publication.is_pending());
    assert_eq!(fs::read(&manifest).unwrap(), pending);
    assert!(
        publication
            .accepted_output(0)
            .unwrap()
            .file()
            .same_object(&first)
            .unwrap()
    );
    assert!(publication.accepted_output(1).is_some());

    fs::write(&guard, &original).unwrap();
    let (_, ready) = publication.publish_ready(prepare).unwrap();
    assert_eq!(
        ready.source().digest(),
        ContentDigest::of_bytes(&fs::read(manifest).unwrap())
    );
}

#[test]
fn failed_output_holds_cleanup_and_retries_the_same_existing_leaf() {
    let fixture = raw_pending();
    let mut publication = writer(&fixture);
    let control = hold_failed_output(&mut publication);
    let mut publication = publication.close().unwrap_err();
    assert!(publication.cleanup_pending());
    assert!(!publication.retry_cleanup().unwrap());
    assert!(matches!(
        publication.advance(),
        Err(ComponentApplyPublicationError::CleanupPending)
    ));

    control.release();
    assert!(publication.retry_cleanup().unwrap());
    assert_eq!(publication.advance().unwrap(), Some(0));
    assert!(publication.accepted_output(0).unwrap().evidence().is_some());
    assert!(publication.can_close());
}

#[cfg(feature = "tooling")]
#[test]
fn exhausted_operation_can_release_its_actual_publisher_to_file_custody() {
    let fixture = raw_pending();
    let mut publication = writer(&fixture);
    let control = hold_failed_output(&mut publication);
    let first_path = fixture
        .root
        .join(publication.pending.recovery().outputs()[0].0.path());
    let first = fs::read(&first_path).unwrap();
    let manifest = fixture.root.join(PROJECT_MANIFEST_PATH);
    let pending = fs::read(&manifest).unwrap();
    publication.budget.limits.decode_work =
        nara_core::ItemLimit::new(publication.budget.decode_work).unwrap();
    assert!(matches!(
        publication.retry_cleanup(),
        Err(ComponentApplyPublicationError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    let mut publisher = publication.into_cleanup_publisher().unwrap();
    assert!(!publisher.retry_cleanup());
    control.release();
    assert!(publisher.retry_cleanup());
    assert!(!publisher.has_cleanup());
    assert_eq!(fs::read(first_path).unwrap(), first);
    assert_eq!(fs::read(manifest).unwrap(), pending);
}

fn hold_failed_output(publication: &mut ComponentApplyPublication) -> CleanupTestControl {
    let (member, _) = publication.pending.recovery().outputs()[0];
    let path = member.path();
    let current = publication.physical.inventory();
    let mut active = ActivePublication::bind(
        publication.pending.root(),
        path,
        &current.shared_file(path).unwrap(),
        &current.shared_parent(path).unwrap(),
        member.original_disk().digest(),
        current.scene_limits().encoded_bytes().get() as u64,
        &mut publication.budget,
    )
    .unwrap();
    let control = active.publisher.test_fail_after_write();
    publication.active = Some(active);

    assert!(matches!(
        publication.advance(),
        Err(ComponentApplyPublicationError::Failed(
            FilePublicationFailureStage::WriteTemporary
        ))
    ));
    assert!(publication.accepted_output(0).is_none());
    control
}
