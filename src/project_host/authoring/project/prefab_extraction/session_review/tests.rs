use super::*;
use nara_tooling::EditorPrefabExtractionDisposition;

fn prepare(
    fixture: &Fixture,
    save_current: Vec<EditorDocumentId>,
) -> Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure> {
    PrefabExtractionReviewRequest::new(fixture.request(), save_current)
        .unwrap()
        .prepare()
}

fn corpus_allocations(prepared: &PreparedEditorWorkspacePrefabExtraction) -> [usize; 3] {
    let corpus = prepared.corpus();
    [
        corpus.original_sources().as_ptr() as usize,
        corpus.sources().as_ptr() as usize,
        corpus.extracted().document() as *const PrefabDocument as usize,
    ]
}

#[test]
fn saved_generation_refusal_revises_the_same_session_corpus_after_actual_task_return() {
    let fixture = Fixture::with_guard_route("mobile/assets", "", true);
    let disk = fixture.disk_state();
    let guard = fixture.document(GUARD);
    let mut request = PrefabExtractionReviewRequest::new(fixture.request(), vec![]).unwrap();
    request.admit_task_storage(256).unwrap();
    let mut failure = request.prepare().unwrap_err();
    assert!(failure.has_retained_preparation());
    assert!(failure.can_revise());
    assert!(failure.matches(&fixture.project));
    assert_eq!(failure.open_count(), fixture.open.len());
    assert!(failure.source_count() > failure.open_count());
    assert_eq!(
        failure.schema_basis(),
        Some(&fixture.project.startup_scene.schema_basis())
    );
    assert!(failure.diagnostics().unwrap().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-extraction-saved-guard-relocation"
    }));
    let prepared = failure.test_prepared().unwrap();
    let allocations = corpus_allocations(prepared);
    let spent = prepared.usage();
    let retained = failure.host_usage().unwrap().retained_candidate_bytes();
    // Only now has the synchronous worker result physically returned with its owner.
    failure.release_task_storage(256);
    assert_eq!(
        failure.host_usage().unwrap().retained_candidate_bytes(),
        retained - 256
    );
    let failure = failure.into_error().unwrap_err();
    let mut revision = failure.into_revision(vec![guard]).unwrap();
    assert!(revision.matches(&fixture.project));
    revision.admit_task_storage(256).unwrap();
    let mut review = revision.prepare().unwrap();
    review.release_task_storage(256);
    assert_eq!(corpus_allocations(review.test_prepared()), allocations);
    assert!(review.test_prepared().usage().work_bytes > spent.work_bytes);
    assert!(
        review
            .documents()
            .any(|entry| { entry == (guard, EditorPrefabExtractionDisposition::SaveCurrent) })
    );
    review
        .validate_live(&fixture.project, &fixture.workspace)
        .unwrap();
    drop(review);
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(fixture.workspace.document_is_dirty(guard), Some(true));
}

#[test]
fn complete_session_review_can_refuse_then_correct_selection_without_new_sources_or_publication() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let inactive = fixture.document(INACTIVE);
    let guard = fixture.document(GUARD);
    let history = fixture
        .workspace
        .scene(inactive)
        .unwrap()
        .session()
        .history_status();
    let review = prepare(&fixture, vec![inactive]).unwrap();
    let allocations = corpus_allocations(review.test_prepared());
    let source_work = review.test_prepared().usage();
    let host_work = review.host_usage().decode_work();
    let digest = review.digest();
    let q_digest = review
        .outputs()
        .find(|output| output.path() == Q)
        .unwrap()
        .digest();
    assert!(review.outputs().any(|output| output.path() == INACTIVE));
    let failure = review
        .into_revision(vec![fixture.document(P)])
        .unwrap()
        .prepare()
        .unwrap_err();
    assert!(failure.has_retained_preparation());
    assert!(failure.diagnostics().unwrap().has_errors());
    assert_eq!(failure.open_count(), fixture.open.len());
    let failure = failure.into_error().unwrap_err();
    let mut review = failure
        .into_revision(vec![guard])
        .unwrap()
        .prepare()
        .unwrap();
    assert_eq!(corpus_allocations(review.test_prepared()), allocations);
    assert!(review.test_prepared().usage().work_bytes > source_work.work_bytes);
    assert!(review.host_usage().decode_work() > host_work);
    assert_ne!(review.digest(), digest);
    assert_eq!(
        review
            .outputs()
            .find(|output| output.path() == Q)
            .unwrap()
            .digest(),
        q_digest
    );
    assert!(!review.outputs().any(|output| output.path() == INACTIVE));
    assert!(review.outputs().any(|output| output.path() == GUARD));
    assert!(
        review
            .documents()
            .any(|entry| { entry == (inactive, EditorPrefabExtractionDisposition::Guard) })
    );
    assert!(
        review
            .documents()
            .any(|entry| { entry == (guard, EditorPrefabExtractionDisposition::SaveCurrent) })
    );
    review
        .validate_live(&fixture.project, &fixture.workspace)
        .unwrap();
    // Explicit cancellation at this unpublished seam is disposal of the physically returned
    // complete owner. Its original live sources/history and disk were never installed.
    drop(review);
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(
        fixture
            .workspace
            .scene(inactive)
            .unwrap()
            .session()
            .history_status(),
        history
    );
    assert_eq!(fixture.workspace.document_is_dirty(inactive), Some(true));
}

#[test]
fn exhausted_checkpoint_return_keeps_the_same_editable_session_owner_and_source_diagnostics() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let probe = prepare(&fixture, vec![]).unwrap();
    let required = probe.test_prepared().usage().work_bytes;
    drop(probe);
    let limits =
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(required).unwrap());
    let review = PrefabExtractionReviewRequest::new(fixture.request_with_limits(limits), vec![])
        .unwrap()
        .prepare()
        .unwrap();
    let allocations = corpus_allocations(review.test_prepared());
    let source_work = review.test_prepared().usage();
    let mut revision = review.into_revision(vec![]).unwrap();
    revision.admit_task_storage(128).unwrap();
    let mut failure = revision.prepare().unwrap_err();
    assert!(failure.has_retained_preparation());
    assert!(failure.error().source_limit_exceeded());
    assert!(!failure.error().host_work_limit_exceeded());
    assert!(
        failure
            .diagnostics()
            .unwrap()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-extraction-capacity" })
    );
    assert_eq!(
        corpus_allocations(failure.test_prepared().unwrap()),
        allocations
    );
    failure.release_task_storage(128);
    let failure = failure
        .into_revision(vec![])
        .unwrap()
        .prepare()
        .unwrap_err();
    assert_eq!(
        corpus_allocations(failure.test_prepared().unwrap()),
        allocations
    );
    assert_eq!(failure.test_prepared().unwrap().usage(), source_work);
    assert!(failure.matches(&fixture.project));
    assert!(failure.error().source_limit_exceeded());
    drop(failure);
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn unstarted_revision_and_task_admission_refusal_preserve_original_owner_and_spent_host_work() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let review = prepare(&fixture, vec![]).unwrap();
    let allocations = corpus_allocations(review.test_prepared());
    let baseline = review.host_usage();
    let mut request = review
        .into_revision(vec![fixture.document(INACTIVE)])
        .unwrap();
    let spent = request.schema_basis().clone();
    request.admit_task_storage(256).unwrap();
    // The unchanged request has physically returned from an unstarted callback. Turning it
    // into a refusal transfers ownership and does not consume another admission allowance.
    let mut failure = request.into_failure(None);
    let before = failure.host_usage().unwrap();
    assert_eq!(
        before.retained_candidate_bytes(),
        baseline.retained_candidate_bytes() + 256
    );
    failure.release_task_storage(256);
    assert_eq!(
        failure.host_usage().unwrap().decode_work(),
        before.decode_work()
    );
    assert_eq!(
        failure.host_usage().unwrap().retained_candidate_bytes(),
        before.retained_candidate_bytes() - 256
    );
    // Repeated unstarted corrections drop each new selection before refunding it. Only the
    // one original bridge envelope remains live; cumulative Host work continues increasing.
    for _ in 0..4 {
        let previous = failure.host_usage().unwrap();
        let mut selection = Vec::with_capacity(32);
        selection.push(fixture.document(GUARD));
        failure = failure.into_revision(selection).unwrap().into_failure(None);
        let current = failure.host_usage().unwrap();
        assert_eq!(
            current.retained_candidate_bytes(),
            baseline.retained_candidate_bytes()
        );
        assert!(current.decode_work() > previous.decode_work());
    }
    let mut request = failure
        .into_revision(vec![fixture.document(GUARD)])
        .unwrap();
    assert_eq!(request.schema_basis(), &spent);
    let error = request.admit_task_storage(usize::MAX).unwrap_err();
    assert!(error.host_work_limit_exceeded());
    let failure = request.into_failure(Some(error));
    let after = failure.host_usage().unwrap();
    assert!(after.decode_work() > before.decode_work());
    assert!(failure.error().host_work_limit_exceeded());
    assert!(failure.matches(&fixture.project));
    assert_eq!(
        corpus_allocations(failure.test_prepared().unwrap()),
        allocations
    );
    let failure = failure.into_revision(vec![]).unwrap_err();
    assert!(failure.error().host_work_limit_exceeded());
    assert!(failure.host_usage().unwrap().decode_work() >= after.decode_work());
    assert_eq!(
        failure.host_usage().unwrap().retained_candidate_bytes(),
        after.retained_candidate_bytes()
    );
    assert_eq!(
        corpus_allocations(failure.test_prepared().unwrap()),
        allocations
    );
    drop(failure);
    assert_eq!(fixture.disk_state(), disk);
}
