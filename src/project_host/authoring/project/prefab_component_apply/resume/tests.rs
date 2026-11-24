use std::fs;

use nara_asset::AssetRef;
use nara_fs::{ContentDigest, RelativePath};
use nara_scene::{PrefabInstance, SceneDocument, SceneEntityId, SceneEntityRecord};

use super::super::physical::tests::{
    CONTAINING, DESTINATION, GUARD, budget, load, raw_pending, raw_pending_with_outputs, replace,
};
use super::*;
use crate::{
    project_host::authoring::recovery_storage::RecoveryPackageData,
    test_host_fs::writable_directory,
};

#[test]
fn fresh_recovery_reviews_a_mixed_pair_and_returns_a_real_ready_receipt() {
    let fixture = raw_pending();
    let (ready, containing_output, destination_output, operation) = {
        let pending = load(&fixture, &mut budget());
        let [(containing, output), (destination, successor)] = pending.recovery().outputs();
        assert_eq!(containing.path(), CONTAINING);
        replace(&pending, destination.path(), successor.bytes());
        (
            pending.recovery().final_ready_manifest().to_vec(),
            output.bytes().to_vec(),
            successor.bytes().to_vec(),
            pending.reference().recovery_digest(),
        )
    };
    let guard = fs::read(fixture.root.join(GUARD)).unwrap();
    let pending_bytes = fs::read(fixture.root.join("nara.toml")).unwrap();
    let root = writable_directory(&fixture.root).unwrap();
    let destination_path = RelativePath::new(DESTINATION).unwrap();
    let destination_disk_path = fixture.root.join(DESTINATION);
    let destination_before = root.open_file_exact(&destination_path).unwrap();
    let destination_modified = fs::metadata(&destination_disk_path)
        .unwrap()
        .modified()
        .unwrap();
    let pending_identity = root
        .open_file_exact(&RelativePath::new("nara.toml").unwrap())
        .unwrap()
        .identity();
    let limits = AuthoringSourceOperationLimits::default();
    let inspection = PendingAuthoringRecoveryInspection::inspect(root.share(), limits).unwrap();
    let inspection_work = inspection.usage().host_work;
    let mut recovery = PrefabComponentApplyRecovery::from_inspection(inspection).unwrap();
    assert_eq!(
        recovery.view().phase,
        PrefabComponentApplyRecoveryPhase::Review
    );
    assert_eq!(recovery.view().source_count, 3);
    assert!(recovery.view().usage.host_work > inspection_work);
    let review = recovery.review().unwrap();
    assert_eq!(review.selected_profile(), Some("selected"));
    let [containing, destination] = review.outputs();
    assert_eq!(containing.project_path, CONTAINING);
    assert_eq!(destination.project_path, DESTINATION);
    assert_eq!(containing.key.kind(), nara_scene::SourceDocumentKind::Scene);
    assert_eq!(
        destination.key.kind(),
        nara_scene::SourceDocumentKind::Prefab
    );
    assert_eq!(containing.output, containing_output);
    assert_eq!(destination.output, destination_output);
    assert!(!containing.already_current);
    assert!(destination.already_current);
    let containing_original = containing.original_disk.to_vec();
    assert_eq!(
        recovery.advance().unwrap_err().kind(),
        PrefabComponentApplyRecoveryErrorKind::Phase
    );

    let confirmed = recovery.confirm().unwrap();
    assert_eq!(confirmed.completed_files, 0);
    assert_eq!(
        fs::read(fixture.root.join(CONTAINING)).unwrap(),
        containing_original
    );
    assert_eq!(
        fs::read(fixture.root.join("nara.toml")).unwrap(),
        pending_bytes
    );
    let first = recovery.advance().unwrap();
    assert_eq!(first.completed_files, 1);
    assert_eq!(
        fs::read(fixture.root.join(CONTAINING)).unwrap(),
        containing_output
    );
    let second = recovery.advance().unwrap();
    assert_eq!(second.completed_files, 2);
    assert_eq!(
        second.phase,
        PrefabComponentApplyRecoveryPhase::PublishingReady
    );
    assert_eq!(
        fs::read(fixture.root.join("nara.toml")).unwrap(),
        pending_bytes
    );
    let complete = recovery.advance().unwrap();
    assert_eq!(complete.phase, PrefabComponentApplyRecoveryPhase::Complete);
    let receipt = recovery.into_receipt().unwrap();

    assert_eq!(receipt.operation(), operation);
    assert_eq!(receipt.written_sources(), 1);
    assert_eq!(receipt.usage(), complete.usage);
    assert_eq!(
        receipt.ready_publication().previous_identity(),
        Some(pending_identity)
    );
    assert_eq!(
        receipt.ready_publication().published_content_digest(),
        Some(ContentDigest::of_bytes(&ready))
    );
    assert_eq!(fs::read(fixture.root.join("nara.toml")).unwrap(), ready);
    assert_eq!(fs::read(fixture.root.join(GUARD)).unwrap(), guard);
    let destination_after = root.open_file_exact(&destination_path).unwrap();
    assert_eq!(destination_after.identity(), destination_before.identity());
    assert!(destination_after.same_object(&destination_before).unwrap());
    assert_eq!(
        destination_after
            .digest(destination_output.len() as u64)
            .unwrap(),
        ContentDigest::of_bytes(&destination_output)
    );
    assert_eq!(
        fs::read(&destination_disk_path).unwrap(),
        destination_output
    );
    assert_eq!(
        fs::metadata(&destination_disk_path)
            .unwrap()
            .modified()
            .unwrap(),
        destination_modified
    );
}

#[test]
fn already_current_outputs_still_need_a_valid_saved_generation() {
    let fixture = raw_pending_with_outputs(|containing, _| {
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance::forest(
            AssetRef::path("missing.prefab.ron").unwrap(),
        ));
        *containing = SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap()
            .into_bytes();
    });
    let pending_bytes = fs::read(fixture.root.join("nara.toml")).unwrap();
    {
        let pending = load(&fixture, &mut budget());
        for (source, output) in pending.recovery().outputs() {
            replace(&pending, source.path(), output.bytes());
        }
    }

    let root = writable_directory(&fixture.root).unwrap();
    let failure =
        PrefabComponentApplyRecovery::inspect(root, AuthoringSourceOperationLimits::default())
            .unwrap_err();
    assert_eq!(
        failure.error().kind(),
        PrefabComponentApplyRecoveryErrorKind::SavedGeneration
    );
    assert_eq!(
        fs::read(fixture.root.join("nara.toml")).unwrap(),
        pending_bytes
    );
}
