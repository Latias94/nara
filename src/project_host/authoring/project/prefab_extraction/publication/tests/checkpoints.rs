//! Actual fixed file results are consumed once by their original live persistence stage.

use super::*;

fn files_complete(
    fixture: &Fixture,
) -> (
    ProjectPrefabExtractionPublication,
    Vec<(ExtractionFileMember, String, Vec<u8>)>,
) {
    let storage = confirmed(fixture);
    let files = fixed_files(storage.recovery().unwrap());
    let mut owner = storage.into_publication().unwrap();
    while owner.phase() != Phase::FilesComplete {
        owner = owner.advance().unwrap();
    }
    (owner, files)
}

fn current_files(
    fixture: &Fixture,
    files: &[(ExtractionFileMember, String, Vec<u8>)],
) -> Vec<(FileIdentity, Vec<u8>)> {
    let root = fixture.project.project_root();
    files
        .iter()
        .map(|(_, path, bytes)| {
            let file = root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            (
                file.identity(),
                file.read_to_end_bounded(bytes.len() as u64).unwrap(),
            )
        })
        .collect()
}

fn semantic_allocations(owner: &ProjectPrefabExtractionPublication) -> [usize; 6] {
    let review = owner.live_outputs().unwrap().review();
    let corpus = review.persistence.sources().prepared().corpus();
    [
        corpus.original_sources().as_ptr() as usize,
        corpus.sources().as_ptr() as usize,
        review.context.disk.as_ptr() as usize,
        review.saved.sources().as_ptr() as usize,
        review.saved.published().as_ptr() as usize,
        review.saved.extracted().document() as *const _ as usize,
    ]
}

fn unchanged_inactive(fixture: &mut Fixture) {
    let document = fixture.document(INACTIVE);
    let base_revision = fixture.workspace.document_revision(document).unwrap();
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            })
            .applied
    );
}

fn confirmed_with_encoded_limit(fixture: &Fixture, limit: usize) -> ProjectPrefabExtractionStorage {
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    let mut review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap();
    // Select the tighter operation allowance before output/package/file work starts. No
    // cumulative accounting is reset, and the running owner never acquires another allowance.
    review.context.budget.limits.aggregate_encoded_bytes = ByteLimit::new(limit).unwrap();
    let mut storage = review
        .prepare_outputs()
        .unwrap()
        .prepare_recovery(Default::default())
        .unwrap()
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    while storage.phase() != ProjectPrefabExtractionStoragePhase::Confirmed {
        storage = storage.advance().unwrap();
    }
    storage
}

#[test]
fn live_checkpoint_acceptance_keeps_actual_files_and_live_workspace_unchanged() {
    let fixture = Fixture::new();
    add_closed_rewrite(&fixture);
    let before = live_state(&fixture);
    let (mut owner, files) = files_complete(&fixture);
    let outputs = owner.live_outputs().unwrap();
    let pending = fs::read(fixture.root.join("nara.toml")).unwrap();
    let package = fs::read(fixture.root.join(RECOVERY)).unwrap();
    let physical = current_files(&fixture, &files);
    let semantics = semantic_allocations(&owner);
    let encoded_count = outputs
        .outputs()
        .filter(|output| output.encoded_checkpoint().is_some())
        .count();
    assert!(encoded_count >= 2);
    let mut work = outputs.review().persistence.usage();
    let mut accepted_count = 0;
    while owner.phase() != Phase::CheckpointsComplete {
        owner = owner.advance().unwrap();
        assert_eq!(owner.position(), files.len());
        let accepted: Vec<_> = (0..owner.file_count())
            .filter_map(|index| {
                let (member, ExtractionFileResult::Checkpoint(acceptance)) = owner.result(index)?
                else {
                    return None;
                };
                assert!(acceptance.accepted());
                let evidence = acceptance.evidence().unwrap();
                assert_eq!(evidence.published_identity(), Some(physical[index].0));
                assert_eq!(
                    evidence.digest(),
                    EditorDocumentDigest::new(
                        files[index].2.len() as u64,
                        *ContentDigest::of_bytes(&files[index].2).as_bytes(),
                    )
                );
                Some(member)
            })
            .collect();
        assert!(accepted.len() >= accepted_count);
        accepted_count = accepted.len();
        let current = owner.live_outputs().unwrap();
        assert_eq!(
            current
                .outputs()
                .filter(|output| output.encoded_checkpoint().is_some())
                .count(),
            encoded_count - accepted_count,
        );
        let next = current.review().persistence.usage();
        assert!(next.work_bytes >= work.work_bytes);
        work = next;
        assert_eq!(semantic_allocations(&owner), semantics);
        assert_eq!(current_files(&fixture, &files), physical);
        assert_eq!(live_state(&fixture), before);
        assert_eq!(fs::read(fixture.root.join("nara.toml")).unwrap(), pending);
        assert_eq!(fs::read(fixture.root.join(RECOVERY)).unwrap(), package);
    }
    assert_eq!(accepted_count, encoded_count);
    assert_eq!(owner.checkpoint_position(), Some(files.len()));
    assert!(matches!(
        owner.result(0).unwrap().1,
        ExtractionFileResult::Published(_)
    ));
    assert!(matches!(
        owner.result(1).unwrap().1,
        ExtractionFileResult::Published(_)
    ));
    assert_pending(&fixture);
}

#[test]
fn unchanged_live_save_uses_actual_observation_without_manufacturing_a_receipt() {
    let mut fixture = Fixture::new();
    unchanged_inactive(&mut fixture);
    let before = live_state(&fixture);
    let root = fixture.project.project_root();
    let original = root
        .open_file_exact(&RelativePath::new(INACTIVE).unwrap())
        .unwrap();
    let (mut owner, _) = files_complete(&fixture);
    let index = (0..owner.file_count())
        .find(|index| {
            let (_, result) = owner.result(*index).unwrap();
            result.observation().0 == original.identity()
        })
        .unwrap();
    assert!(matches!(
        owner.result(index).unwrap().1,
        ExtractionFileResult::Observed { .. }
    ));
    let after_files = fixture.disk_state();
    while owner.phase() != Phase::CheckpointsComplete {
        owner = owner.advance().unwrap();
    }
    let (_, ExtractionFileResult::Checkpoint(acceptance)) = owner.result(index).unwrap() else {
        panic!("the exact original checkpoint must consume the observed current file")
    };
    assert!(acceptance.accepted());
    assert!(acceptance.evidence().is_none());
    assert_eq!(acceptance.observation().0, original.identity());
    assert_eq!(fixture.disk_state(), after_files);
    assert_eq!(live_state(&fixture), before);
    assert_pending(&fixture);
}

#[test]
fn observation_acceptance_admits_exact_read_bound_and_retains_transfer_on_one_short() {
    let mut probe = Fixture::new();
    unchanged_inactive(&mut probe);
    let (mut measured, _) = files_complete(&probe);
    while measured.phase() != Phase::CheckpointsComplete {
        measured = measured.advance().unwrap();
    }
    let exact = measured.host_usage().unwrap().aggregate_encoded_bytes();
    drop(measured);
    for limit in [exact, exact - 1] {
        let mut fixture = Fixture::new();
        unchanged_inactive(&mut fixture);
        let before = live_state(&fixture);
        let storage = confirmed_with_encoded_limit(&fixture, limit);
        let files = fixed_files(storage.recovery().unwrap());
        let mut owner = storage.into_publication().unwrap();
        while owner.phase() != Phase::FilesComplete {
            owner = owner.advance().unwrap();
        }
        let physical = current_files(&fixture, &files);
        loop {
            match owner.advance() {
                Ok(next) if next.phase() == Phase::CheckpointsComplete => {
                    assert_eq!(limit, exact);
                    assert_eq!(current_files(&fixture, &files), physical);
                    break;
                }
                Ok(next) => owner = next,
                Err(failure) => {
                    assert_eq!(limit, exact - 1);
                    assert!(matches!(
                        failure.error(),
                        ProjectPrefabExtractionPublicationError::Admission(_)
                    ));
                    let mut owner = failure.into_owner();
                    let cursor = owner.checkpoint_position().unwrap();
                    let (_, ExtractionFileResult::Checkpoint(acceptance)) =
                        owner.result(cursor).unwrap()
                    else {
                        panic!(
                            "observation admission must retain the transferred original checkpoint"
                        )
                    };
                    assert!(!acceptance.accepted());
                    assert!(acceptance.evidence().is_none());
                    assert!(acceptance.observation_digest().is_some());
                    let observation = acceptance.observation();
                    let usage = owner.host_usage().unwrap();
                    let failure = owner.advance().unwrap_err();
                    let mut owner = failure.into_owner();
                    assert_eq!(owner.checkpoint_position(), Some(cursor));
                    let (_, ExtractionFileResult::Checkpoint(acceptance)) =
                        owner.result(cursor).unwrap()
                    else {
                        panic!("a refused retry must not drop the transfer")
                    };
                    assert!(!acceptance.accepted());
                    assert_eq!(acceptance.observation(), observation);
                    let retried = owner.host_usage().unwrap();
                    assert!(retried.aggregate_encoded_bytes() >= usage.aggregate_encoded_bytes());
                    assert!(retried.decode_work() >= usage.decode_work());
                    assert_eq!(
                        retried.retained_candidate_bytes(),
                        usage.retained_candidate_bytes()
                    );
                    assert_eq!(
                        retried.retained_open_handles(),
                        usage.retained_open_handles()
                    );
                    assert_eq!(current_files(&fixture, &files), physical);
                    break;
                }
            }
        }
        assert_eq!(live_state(&fixture), before);
        assert_pending(&fixture);
    }
}

#[test]
fn late_leaf_replacement_retains_accepted_prefix_and_unconsumed_checkpoints_on_retry() {
    let fixture = Fixture::new();
    let before = live_state(&fixture);
    let (mut owner, files) = files_complete(&fixture);
    let index = loop {
        owner = owner.advance().unwrap();
        if let Some(index) = (0..owner.file_count()).find(|index| {
            matches!(
                owner.result(*index).unwrap().1,
                ExtractionFileResult::Checkpoint(_)
            )
        }) {
            break index;
        }
    };
    let (_, ExtractionFileResult::Checkpoint(acceptance)) = owner.result(index).unwrap() else {
        unreachable!()
    };
    assert!(acceptance.accepted());
    let actual = acceptance.observation();
    let evidence = acceptance.evidence();
    let cursor = owner.checkpoint_position();
    let semantic = semantic_allocations(&owner);
    let remaining: Vec<_> = owner
        .live_outputs()
        .unwrap()
        .outputs()
        .filter_map(|output| {
            output
                .encoded_checkpoint()
                .map(|encoded| encoded.bytes().as_ptr())
        })
        .collect();
    assert!(!remaining.is_empty());
    let root = fixture.project.project_root();
    replace(&root, &files[index].1, &files[index].2);
    let external = current_files(&fixture, &files);
    assert_ne!(external[index].0, actual.0);
    let mut failure = owner.advance().unwrap_err();
    for attempt in 0..2 {
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionPublicationError::SourceChanged
        ));
        let refused = failure.owner();
        assert_eq!(refused.checkpoint_position(), cursor);
        let (_, ExtractionFileResult::Checkpoint(acceptance)) = refused.result(index).unwrap()
        else {
            panic!("an accepted checkpoint's actual leaf must stay owned on conflict")
        };
        assert!(acceptance.accepted());
        assert_eq!(acceptance.observation(), actual);
        assert_eq!(acceptance.evidence(), evidence);
        assert_eq!(semantic_allocations(refused), semantic);
        assert_eq!(
            refused
                .live_outputs()
                .unwrap()
                .outputs()
                .filter_map(|output| {
                    output
                        .encoded_checkpoint()
                        .map(|encoded| encoded.bytes().as_ptr())
                })
                .collect::<Vec<_>>(),
            remaining,
        );
        assert_eq!(current_files(&fixture, &files), external);
        assert_eq!(live_state(&fixture), before);
        if attempt == 0 {
            failure = failure.retry().unwrap_err();
        }
    }
    assert_pending(&fixture);
}

#[test]
fn recovered_file_completion_never_creates_an_old_workspace_checkpoint() {
    let mut fixture = Fixture::new();
    let mut owner = fresh_publication(&mut fixture);
    while owner.phase() != Phase::FilesComplete {
        owner = owner.advance().unwrap();
    }
    let generation = owner.recovered_generation().unwrap();
    let sources = generation.verified().unwrap().0.sources().as_ptr();
    let work = generation.source_usage();
    let host = owner.host_usage().unwrap();
    let physical: Vec<_> = (0..owner.file_count())
        .map(|index| owner.result(index).unwrap().1.observation())
        .collect();
    owner = owner.advance().unwrap();
    assert_eq!(owner.phase(), Phase::CheckpointsComplete);
    assert!(owner.live_outputs().is_none());
    assert_eq!(owner.recovered_generation().unwrap().source_usage(), work);
    assert_eq!(
        owner
            .recovered_generation()
            .unwrap()
            .verified()
            .unwrap()
            .0
            .sources()
            .as_ptr(),
        sources,
    );
    assert!(owner.host_usage().unwrap().decode_work() >= host.decode_work());
    for (index, before) in physical.into_iter().enumerate() {
        let (_, result) = owner.result(index).unwrap();
        assert!(!matches!(result, ExtractionFileResult::Checkpoint(_)));
        assert_eq!(result.observation(), before);
    }
    assert_pending(&fixture);
}
