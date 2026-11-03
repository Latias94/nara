//! Child of prefab_extraction::tests: every Pending package comes from the actual live producer.

use nara_project::{ProjectAuthoringState, ProjectManifest};
use nara_scene::{
    SceneAuthoringContentState, SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage,
    SceneAuthoringRevision,
};
use nara_tooling::{EditorDocumentDigest, EditorPrefabExtractionDisposition};

use super::super::{
    outputs::storage::{ProjectPrefabExtractionStorage, ProjectPrefabExtractionStoragePhase},
    publication::{
        ExtractionFileMember, ExtractionFileResult, ProjectPrefabExtractionPublication,
        ProjectPrefabExtractionPublicationError, ProjectPrefabExtractionPublicationPhase as Phase,
    },
    recovered_generation::{
        RecoveredPrefabExtractionGeneration, RecoveredPrefabExtractionGenerationPhase,
    },
    recovered_inventory::RecoveredPrefabExtractionInventory,
    recovery_data::{PrefabExtractionRecovery, RecoverySourceDisposition, storage::load_pending},
};
use super::*;
use crate::project_host::{
    authoring::recovery_storage::ObservedPendingRecoveryStorage,
    file_publication::{
        AcceptedFilePublication, FilePublicationFailureStage, FilePublicationOutcome,
        FilePublicationRejection,
    },
};

#[path = "tests/checkpoints.rs"]
mod checkpoints;
#[path = "tests/fixture.rs"]
mod fixture;
#[path = "tests/future_inventory.rs"]
mod future_inventory;
#[path = "tests/future_project.rs"]
mod future_project;
#[path = "tests/installation.rs"]
mod installation;
use fixture::{
    CLOSED_REWRITE, RECOVERY, add_closed_rewrite, allocations, assert_pending, assert_published,
    budget, confirmed, fixed_files, fresh_publication, generation, live_state, load, replace,
};

#[test]
fn live_files_keep_dirty_saved_guards_and_original_encoded_checkpoints_until_later_acceptance() {
    let fixture = Fixture::new();
    add_closed_rewrite(&fixture);
    let live = live_state(&fixture);
    let active = fixture.workspace.active_document();
    let storage = confirmed(&fixture);
    let recovery = storage.recovery().unwrap();
    let files = fixed_files(recovery);
    let package = fs::read(fixture.root.join(RECOVERY)).unwrap();
    let pending = fs::read(fixture.root.join("nara.toml")).unwrap();
    let before = fixture.disk_state();
    let fixed = allocations(storage.outputs());
    let review = storage.outputs().review();
    let prepared_work = review.persistence.usage();
    let root = fixture.project.project_root();
    let previous: Vec<_> = files
        .iter()
        .map(|(member, path, _)| match member {
            ExtractionFileMember::Original(_) => Some(
                review
                    .context
                    .inventory
                    .participant(path)
                    .unwrap()
                    .file()
                    .identity(),
            ),
            _ => None,
        })
        .collect();
    for (path, disposition) in [
        (P, EditorPrefabExtractionDisposition::ReplaceSource),
        (INACTIVE, EditorPrefabExtractionDisposition::SaveCurrent),
        (GUARD, EditorPrefabExtractionDisposition::Guard),
    ] {
        assert_eq!(
            review.persistence.disposition(fixture.document(path)),
            Some(disposition)
        );
    }
    for (index, stored) in recovery.complete_original_sources().iter().enumerate() {
        assert_eq!(review.saved.published()[index], stored.output().is_some());
        if stored.path() == CLOSED_REWRITE {
            assert_eq!(
                stored.disposition(),
                RecoverySourceDisposition::RewriteClosed
            );
        }
        if stored.path() == GUARD {
            assert_ne!(
                stored.original_disk().bytes(),
                stored.reviewed_working().bytes()
            );
            assert!(stored.output().is_none());
        }
        if stored.path() == INACTIVE {
            assert_eq!(
                stored.output().unwrap().bytes(),
                stored.reviewed_working().bytes()
            );
            assert_ne!(
                stored.original_disk().bytes(),
                stored.output().unwrap().bytes()
            );
        }
    }
    for path in [P, INACTIVE] {
        let output = storage
            .outputs()
            .outputs()
            .find(|output| output.path() == path)
            .unwrap();
        assert!(output.encoded_checkpoint().is_some());
        assert!(output.checkpoint().is_some());
    }
    let closed = storage
        .outputs()
        .outputs()
        .find(|output| output.path() == CLOSED_REWRITE)
        .unwrap();
    assert!(closed.encoded_source().is_some());
    assert!(closed.encoded_checkpoint().is_none());

    let mut publication = storage.into_publication().unwrap();
    assert_eq!(publication.phase(), Phase::Files);
    assert_eq!(publication.position(), 0);
    assert_eq!(publication.file_count(), files.len());
    assert!(publication.recovered_generation().is_none());
    assert_eq!(fixture.disk_state(), before);
    // Initial handoff revalidates the saved generation under the original source Work.
    let work = publication
        .live_outputs()
        .unwrap()
        .review()
        .persistence
        .usage();
    assert!(work.work_bytes >= prepared_work.work_bytes);
    for (index, (member, path, bytes)) in files.iter().enumerate() {
        assert!(publication.result(index).is_none());
        publication = publication.advance().unwrap();
        assert_eq!(publication.position(), index + 1);
        let (actual, ExtractionFileResult::Published(accepted)) =
            publication.result(index).unwrap()
        else {
            panic!("a missing or original live member needs an actual publication receipt")
        };
        assert_eq!(actual, *member);
        assert_published(&root, path, bytes, accepted, previous[index]);
        assert_eq!(allocations(publication.live_outputs().unwrap()), fixed);
        assert_eq!(
            publication
                .live_outputs()
                .unwrap()
                .review()
                .persistence
                .usage(),
            work
        );
        assert_eq!(live_state(&fixture), live);
        assert_eq!(fixture.workspace.active_document(), active);
    }
    assert_eq!(publication.phase(), Phase::FilesComplete);
    assert!(publication.result(files.len()).is_none());
    assert!(!publication.cleanup_pending());
    assert!(!publication.uncertain());
    for (path, bytes) in before {
        if !files.iter().any(|(_, output, _)| path == Path::new(output)) {
            assert_eq!(fs::read(fixture.root.join(path)).unwrap(), bytes);
        }
    }
    assert_eq!(fs::read(fixture.root.join(RECOVERY)).unwrap(), package);
    assert_eq!(fs::read(fixture.root.join("nara.toml")).unwrap(), pending);
    assert_pending(&fixture);
}

#[test]
fn fresh_semantic_generation_observes_existing_outputs_and_publishes_only_waiting_members() {
    for existing in [
        vec![],
        vec![Q, P],
        vec![Q, Q_SIDECAR, P, INACTIVE, CLOSED_REWRITE],
    ] {
        let mut fixture = Fixture::new();
        add_closed_rewrite(&fixture);
        let live_basis = fixture.project.startup_scene.schema_basis();
        drop(confirmed(&fixture));
        fixture.open.clear();
        fixture.workspace = EditorWorkspace::new();
        let mut host = budget();
        let pending = load(&fixture, &mut host);
        let files = fixed_files(pending.recovery());
        // Originals already have retained capabilities. Replace them through the same real
        // publication primitive; raw creation is only used for the two absent new leaves.
        for (_, path, bytes) in &files {
            if existing.contains(&path.as_str()) {
                if matches!(path.as_str(), Q | Q_SIDECAR) {
                    fs::write(fixture.root.join(path), bytes).unwrap();
                } else {
                    replace(pending.root(), path, bytes);
                }
            }
        }
        let owner = generation(pending, host);
        assert_ne!(owner.observation().basis(), &live_basis);
        assert_eq!(
            owner
                .observation()
                .inventory()
                .captured_open_sources()
                .len(),
            0
        );
        let basis = owner.observation().basis().clone();
        let root = owner.pending().root().share();
        let operation = owner.pending().recovery().digest();
        let package = owner.pending().recovery().encoded_bytes().as_ptr();
        let custody = owner.observation().shared_custody().weak_owner();
        let (corpus, saved) = owner.verified().unwrap();
        let sources = corpus.sources().as_ptr();
        let saved_sources = saved.sources().as_ptr();
        let mask = saved.published().as_ptr();
        let q = corpus.extracted().document() as *const PrefabDocument;
        assert!(std::ptr::eq(
            corpus.extracted().document(),
            saved.extracted().document()
        ));
        assert_eq!(corpus.root(), &id("branch"));
        assert_eq!(corpus.extracted().document().entities.len(), 2);
        assert!(
            corpus
                .extracted()
                .document()
                .entities
                .iter()
                .any(|entity| entity.id == id("desktop-leaf"))
        );
        for (index, stored) in owner
            .pending()
            .recovery()
            .complete_original_sources()
            .iter()
            .enumerate()
        {
            assert_eq!(saved.published()[index], stored.output().is_some());
            assert_eq!(saved.sources()[index].key(), stored.key());
            let bytes = match &saved.sources()[index] {
                CapturedExtractionSource::Scene { document, .. } => {
                    document.to_json_string().unwrap().into_bytes()
                }
                CapturedExtractionSource::Prefab { source } => {
                    if stored.path().ends_with(".ron") {
                        source.document().to_ron_string().unwrap().into_bytes()
                    } else {
                        source.document().to_json_string().unwrap().into_bytes()
                    }
                }
            };
            assert_eq!(
                bytes,
                stored.output().unwrap_or(stored.original_disk()).bytes()
            );
        }
        let previous: Vec<_> = files
            .iter()
            .map(|(_, path, _)| {
                root.open_file_exact(&RelativePath::new(path).unwrap())
                    .ok()
                    .map(|file| file.identity())
            })
            .collect();
        let mut publication = owner.into_publication().unwrap();
        let source_work = publication.recovered_generation().unwrap().source_usage();
        assert!(publication.live_outputs().is_none());
        assert_eq!(publication.phase(), Phase::Files);
        assert_eq!(publication.file_count(), files.len());
        for (index, (member, path, bytes)) in files.iter().enumerate() {
            assert_eq!(publication.position(), index);
            assert!(publication.result(index).is_none());
            publication = publication.advance().unwrap();
            let (actual, result) = publication.result(index).unwrap();
            assert_eq!(actual, *member);
            if existing.contains(&path.as_str()) {
                let ExtractionFileResult::Observed { file, digest } = result else {
                    panic!("an already-output observation must not manufacture a receipt")
                };
                assert_eq!(Some(file.identity()), previous[index]);
                assert_eq!(*digest, ContentDigest::of_bytes(bytes));
                let current = root
                    .open_file_exact(&RelativePath::new(path).unwrap())
                    .unwrap();
                assert_eq!(current.identity(), file.identity());
                assert_eq!(
                    current.read_to_end_bounded(bytes.len() as u64).unwrap(),
                    *bytes
                );
            } else {
                let ExtractionFileResult::Published(accepted) = result else {
                    panic!("a waiting member needs actual publication evidence")
                };
                assert_published(&root, path, bytes, accepted, previous[index]);
            }
            let retained = publication.recovered_generation().unwrap();
            assert_eq!(retained.source_usage(), source_work);
            assert_eq!(retained.observation().basis(), &basis);
            assert_eq!(retained.pending().recovery().digest(), operation);
            assert_eq!(
                retained.pending().recovery().encoded_bytes().as_ptr(),
                package
            );
            assert!(std::sync::Weak::ptr_eq(
                &retained.observation().shared_custody().weak_owner(),
                &custody,
            ));
            let (corpus, saved) = retained.verified().unwrap();
            assert_eq!(corpus.sources().as_ptr(), sources);
            assert_eq!(saved.sources().as_ptr(), saved_sources);
            assert_eq!(saved.published().as_ptr(), mask);
            assert_eq!(corpus.extracted().document() as *const _, q);
        }
        assert_eq!(publication.phase(), Phase::FilesComplete);
        assert_pending(&fixture);
        assert!(custody.upgrade().is_some());
        drop(publication);
        assert!(custody.upgrade().is_none());
    }
}

#[test]
fn failed_write_keeps_cleanup_cursor_and_custody_then_resumes_with_the_same_spent_allowance() {
    let mut fixture = Fixture::new();
    let mut publication = fresh_publication(&mut fixture).advance().unwrap();
    assert_eq!(publication.position(), 1);
    let owner = publication.recovered_generation().unwrap();
    let custody = owner.observation().shared_custody().weak_owner();
    let package = owner.pending().recovery().encoded_bytes().as_ptr();
    let q = match publication.result(0).unwrap().1 {
        ExtractionFileResult::Published(accepted) => accepted.source().identity(),
        _ => panic!("Q must have a real receipt"),
    };
    let before = publication.host_usage().unwrap();
    let control = publication.test_fail_next_write();
    let failure = publication.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionPublicationError::Failed(
            FilePublicationFailureStage::WriteTemporary,
        )
    ));
    let mut publication = failure.into_owner();
    assert_eq!(publication.position(), 1);
    assert!(publication.result(1).is_none());
    assert!(publication.cleanup_pending());
    assert!(!publication.uncertain());
    let cleanup_before = publication.host_usage().unwrap();
    assert!(!publication.retry_cleanup().unwrap());
    let failed = publication.host_usage().unwrap();
    assert_eq!(
        failed.decode_work() - cleanup_before.decode_work(),
        crate::project_host::authoring::publication_admission::CLEANUP_WORK
    );
    assert_eq!(
        failed.retained_candidate_bytes(),
        cleanup_before.retained_candidate_bytes()
    );
    assert_eq!(
        failed.retained_open_handles(),
        before.retained_open_handles() + 2
    );
    assert!(failed.retained_candidate_bytes() > before.retained_candidate_bytes());
    assert!(failed.decode_work() > before.decode_work());
    let disk = fixture.disk_state();
    let failure = publication.advance().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionPublicationError::Rejected(FilePublicationRejection::CleanupPending,)
    ));
    assert_eq!(failure.owner().position(), 1);
    let mut publication = failure.into_owner();
    let blocked = publication.host_usage().unwrap();
    assert_eq!(
        blocked.retained_open_handles(),
        failed.retained_open_handles()
    );
    assert_eq!(
        blocked.retained_candidate_bytes(),
        failed.retained_candidate_bytes()
    );
    assert!(blocked.decode_work() > failed.decode_work());
    assert_eq!(fixture.disk_state(), disk);
    assert!(!fixture.root.join(Q_SIDECAR).exists());
    assert!(custody.upgrade().is_some());
    assert_eq!(
        publication
            .recovered_generation()
            .unwrap()
            .pending()
            .recovery()
            .encoded_bytes()
            .as_ptr(),
        package
    );
    control.release();
    assert!(publication.retry_cleanup().unwrap());
    assert_eq!(
        publication.host_usage().unwrap(),
        crate::project_host::authoring::AuthoringProjectAdmissionUsage {
            decode_work: blocked.decode_work()
                + crate::project_host::authoring::publication_admission::CLEANUP_WORK,
            ..blocked
        }
    );
    publication = publication.advance().unwrap();
    let resumed = publication.host_usage().unwrap();
    assert_eq!(publication.position(), 2);
    assert_eq!(
        resumed.retained_open_handles(),
        before.retained_open_handles() + 1
    );
    assert!(resumed.retained_candidate_bytes() < blocked.retained_candidate_bytes());
    assert!(resumed.decode_work() > blocked.decode_work());
    assert!(!publication.cleanup_pending());
    while publication.phase() != Phase::FilesComplete {
        publication = publication.advance().unwrap();
    }
    let ExtractionFileResult::Published(accepted) = publication.result(0).unwrap().1 else {
        panic!("the first receipt must survive cleanup of a later member")
    };
    assert_eq!(accepted.source().identity(), q);
    assert_pending(&fixture);
    drop(publication);
    assert!(custody.upgrade().is_none());
}

#[test]
fn published_q_and_every_original_directory_prefix_stay_bound_before_later_outputs() {
    for change in 0..4 {
        let mut fixture = Fixture::new();
        let publication = fresh_publication(&mut fixture).advance().unwrap();
        let owner = publication.recovered_generation().unwrap();
        let root = owner.pending().root().share();
        let bytes = owner.pending().recovery().q().output().bytes().to_vec();
        let ExtractionFileResult::Published(accepted) = publication.result(0).unwrap().1 else {
            panic!("Q must have been published before the external change")
        };
        let identity = accepted.source().identity();
        let receipt = accepted.evidence().receipt() as *const _;
        let before_mutation = fixture.disk_state();
        let move_directory =
            |from: &str, to: &str| match fs::rename(fixture.root.join(from), fixture.root.join(to))
            {
                Ok(()) => true,
                Err(error) if cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied => {
                    false
                }
                Err(error) => panic!("directory mutation failed unexpectedly: {error}"),
            };
        let changed = match change {
            0 => {
                replace(&root, Q, &bytes);
                true
            }
            1 => {
                if move_directory("desktop/prefabs", "desktop/retired-prefabs") {
                    fs::create_dir(fixture.root.join("desktop/prefabs")).unwrap();
                    fs::write(fixture.root.join(Q), &bytes).unwrap();
                    true
                } else {
                    false
                }
            }
            2 => {
                // Keep the exact immediate parent and Q, but move them under a replacement
                // desktop. A check of only the leaf and its immediate parent would pass.
                if move_directory("desktop", "retired-desktop") {
                    fs::create_dir(fixture.root.join("desktop")).unwrap();
                    fs::rename(
                        fixture.root.join("retired-desktop/prefabs"),
                        fixture.root.join("desktop/prefabs"),
                    )
                    .unwrap();
                    let current = root
                        .open_file_exact(&RelativePath::new(Q).unwrap())
                        .unwrap();
                    assert_eq!(current.identity(), identity);
                    assert_eq!(
                        current.parent_identity(),
                        accepted.source().parent_identity()
                    );
                    true
                } else {
                    false
                }
            }
            3 => {
                fs::rename(
                    fixture.root.join(Q),
                    fixture.root.join("desktop/prefabs/NEW.prefab.json"),
                )
                .unwrap();
                true
            }
            _ => unreachable!(),
        };
        if !changed {
            // Windows may enforce this directory custody before an external rename can occur.
            // Distinguish prevention from a successful mutation followed by our own refusal.
            assert!(matches!(change, 1 | 2));
            assert_eq!(fixture.disk_state(), before_mutation);
            let current = root
                .open_file_exact(&RelativePath::new(Q).unwrap())
                .unwrap();
            assert_eq!(current.identity(), identity);
            assert_eq!(
                current.digest(bytes.len() as u64).unwrap(),
                ContentDigest::of_bytes(&bytes)
            );
            let publication = publication.advance().unwrap();
            assert_eq!(publication.position(), 2);
            let ExtractionFileResult::Published(accepted) = publication.result(0).unwrap().1 else {
                panic!("prevented mutation must preserve the actual Q receipt")
            };
            assert_eq!(accepted.evidence().receipt() as *const _, receipt);
            assert_pending(&fixture);
            continue;
        }
        let disk = fixture.disk_state();
        let failure = publication.advance().unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionPublicationError::SourceChanged
        ));
        assert_eq!(failure.owner().position(), 1);
        assert!(failure.owner().result(1).is_none());
        let failure = failure.retry().unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionPublicationError::SourceChanged
        ));
        let ExtractionFileResult::Published(accepted) = failure.owner().result(0).unwrap().1 else {
            panic!("prefix refusal must retain the accepted Q result")
        };
        assert_eq!(accepted.source().identity(), identity);
        assert_eq!(accepted.evidence().receipt() as *const _, receipt);
        assert_eq!(fixture.disk_state(), disk);
        assert!(!fixture.root.join(Q_SIDECAR).exists());
        assert_pending(&fixture);
    }
}
#[test]
fn destination_collision_and_original_deletion_refuse_without_overwriting_external_state() {
    for collision in [true, false] {
        let mut fixture = Fixture::new();
        let mut publication = fresh_publication(&mut fixture);
        let original_bytes = fs::read(fixture.root.join(P)).unwrap();
        if collision {
            fs::write(fixture.root.join(Q), b"external destination").unwrap();
        } else {
            publication = publication.advance().unwrap().advance().unwrap();
            let first = publication
                .recovered_generation()
                .unwrap()
                .pending()
                .recovery()
                .complete_original_sources()
                .iter()
                .find(|source| source.output().is_some())
                .unwrap()
                .path()
                .to_owned();
            fs::remove_file(fixture.root.join(&first)).unwrap();
        }
        let cursor = publication.position();
        let disk = fixture.disk_state();
        let failure = publication.advance().unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionPublicationError::SourceChanged
        ));
        assert_eq!(failure.owner().position(), cursor);
        assert!(failure.owner().result(cursor).is_none());
        let failure = failure.retry().unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionPublicationError::SourceChanged
        ));
        assert_eq!(failure.owner().position(), cursor);
        assert_eq!(fixture.disk_state(), disk);
        if collision {
            assert_eq!(
                fs::read(fixture.root.join(Q)).unwrap(),
                b"external destination"
            );
            assert_eq!(fs::read(fixture.root.join(P)).unwrap(), original_bytes);
            assert!(!fixture.root.join(Q_SIDECAR).exists());
        } else {
            assert!(failure.owner().result(0).is_some());
            assert!(failure.owner().result(1).is_some());
        }
        assert_pending(&fixture);
    }
}
