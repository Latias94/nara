use nara_asset::AssetMetaCandidate;
use nara_scene::{
    __private::PrefabExtractionSourceOutput, SceneAuthoringContentState,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
};

use super::*;

use super::super::outputs::recovery::ProjectPrefabExtractionRecoveryError;
use super::super::recovery_data::{
    PrefabExtractionRecovery, PrefabExtractionRecoveryError, PrefabExtractionRecoveryLimits,
    RecoveryFileState, RecoverySourceDisposition,
};

#[derive(Debug, PartialEq, Eq)]
struct LiveState {
    document: EditorDocumentId,
    source: usize,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    dirty: Option<bool>,
}

fn live_state(fixture: &Fixture) -> Vec<LiveState> {
    fixture
        .open
        .iter()
        .map(|(document, _, _)| {
            let (source, revision, content, history, usage) =
                if let Some(scene) = fixture.workspace.scene(*document) {
                    let session = scene.session();
                    (
                        session.document() as *const SceneDocument as usize,
                        session.revision(),
                        session.content_state(),
                        session.history_status(),
                        session.history_usage(),
                    )
                } else {
                    let session = fixture.workspace.prefab(*document).unwrap().session();
                    (
                        session.document() as *const PrefabDocument as usize,
                        session.revision(),
                        session.content_state(),
                        session.history_status(),
                        session.history_usage(),
                    )
                };
            LiveState {
                document: *document,
                source,
                revision,
                content,
                history,
                usage,
                dirty: fixture.workspace.document_is_dirty(*document),
            }
        })
        .collect()
}

fn canonical(source: &CapturedExtractionSource, encoding: SceneFileEncoding) -> Vec<u8> {
    match (source, encoding) {
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Json) => {
            document.to_json_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Ron) => {
            document.to_ron_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Json) => {
            source.document().to_json_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Ron) => {
            source.document().to_ron_string().unwrap().into_bytes()
        }
    }
}

fn original_allocations(review: &PreparedProjectPrefabExtractionReview) -> [usize; 6] {
    let corpus = review.persistence.sources().prepared().corpus();
    [
        corpus.original_sources().as_ptr() as usize,
        corpus.sources().as_ptr() as usize,
        review.context.disk.as_ptr() as usize,
        review.saved.sources().as_ptr() as usize,
        review.saved.published().as_ptr() as usize,
        review.saved.extracted().document() as *const PrefabDocument as usize,
    ]
}

#[test]
fn recovery_package_keeps_disk_working_and_fixed_output_roles_separate_without_io() {
    let fixture = Fixture::new();
    // This unopened desktop Prefab stores an actual target inside the cut. The shared Scene is
    // interpreted in both profiles, where the same authored alias selects different Prefabs.
    let closed_rewrite = "desktop/prefabs/closed.prefab.ron";
    let p = SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(P).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let mut overlay = nara_scene::PrefabOverlay::new(p);
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("desktop-leaf")));
    let mut instance = PrefabInstance::forest(AssetRef::path(ALIAS).unwrap());
    instance.overrides = Some(overlay);
    let mut actor = SceneEntityRecord::new(id("actor"));
    actor.prefab = Some(instance);
    fs::write(
        fixture.root.join(closed_rewrite),
        PrefabDocument::from_ordered_roots([actor])
            .to_ron_string()
            .unwrap(),
    )
    .unwrap();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap();
    let allocations = original_allocations(&review);
    let outputs = review.prepare_outputs().unwrap();
    let fixed = outputs
        .outputs()
        .map(|member| (member.path().to_owned(), member.bytes().unwrap().as_ptr()))
        .collect::<Vec<_>>();
    let retained = outputs.review().context.budget.retained_candidate_bytes;
    let prepared = outputs.prepare_recovery(Default::default()).unwrap();
    let data = prepared.recovery().unwrap();
    assert_eq!(
        original_allocations(prepared.outputs().review()),
        allocations
    );
    assert_eq!(data.source_universe().to_string(), UNIVERSE);
    assert_eq!(data.selected_profile(), None);
    assert_eq!(data.selection().root(), &id("branch"));
    assert_eq!(
        data.old_ready_manifest().bytes(),
        fixture.project.schema.manifest_bytes()
    );
    assert_eq!(
        data.final_ready_manifest().bytes(),
        fixture.project.schema.manifest_bytes()
    );
    assert_eq!(
        data.unchanged_bundle().blob().bytes(),
        fixture.project.schema.bundle_bytes()
    );
    let original = prepared
        .outputs()
        .review()
        .persistence
        .sources()
        .prepared()
        .corpus()
        .original_sources();
    assert_eq!(data.complete_original_sources().len(), original.len());
    for (source, (stored, actual)) in data
        .complete_original_sources()
        .iter()
        .zip(prepared.outputs().review().context.inventory.participants())
        .enumerate()
    {
        assert_eq!(stored.path(), actual.file().path());
        assert_eq!(stored.key(), actual.source_key());
        assert_eq!(stored.original_disk().bytes(), actual.bytes());
        assert_eq!(
            stored.reviewed_working().bytes(),
            canonical(&original[source], stored.encoding().into())
        );
        let output = prepared
            .outputs()
            .outputs()
            .find(|member| member.path() == stored.path());
        assert_eq!(
            stored.output().map(|blob| blob.bytes()),
            output.as_ref().map(|member| member.bytes().unwrap())
        );
        match stored.path() {
            P => assert_eq!(
                stored.disposition(),
                RecoverySourceDisposition::ReplaceSource
            ),
            INACTIVE => assert_eq!(stored.disposition(), RecoverySourceDisposition::SaveCurrent),
            path if path == closed_rewrite => assert_eq!(
                stored.disposition(),
                RecoverySourceDisposition::RewriteClosed
            ),
            _ => assert_eq!(stored.disposition(), RecoverySourceDisposition::Guard),
        }
        if stored.path() == GUARD {
            assert_ne!(
                stored.original_disk().bytes(),
                stored.reviewed_working().bytes()
            );
            assert!(stored.output().is_none());
        }
        if stored.path() == INACTIVE {
            assert_ne!(
                stored.original_disk().bytes(),
                stored.reviewed_working().bytes()
            );
            assert_eq!(
                stored.output().unwrap().bytes(),
                stored.reviewed_working().bytes()
            );
        }
        assert!(matches!(
            stored.classify(Some(actual.file().digest())),
            RecoveryFileState::Original | RecoveryFileState::Unchanged
        ));
        assert_eq!(stored.classify(None), RecoveryFileState::MissingOriginal);
    }
    assert_eq!(data.q().path(), Q);
    assert_eq!(data.q_sidecar().path(), Q_SIDECAR);
    assert_eq!(data.q().classify(None), RecoveryFileState::NotCreated);
    assert_eq!(
        data.q_sidecar().classify(None),
        RecoveryFileState::NotCreated
    );
    for (path, allocation) in fixed {
        assert_eq!(
            prepared
                .outputs()
                .outputs()
                .find(|member| member.path() == path)
                .unwrap()
                .bytes()
                .unwrap()
                .as_ptr(),
            allocation
        );
    }
    let mut decode_budget = AuthoringProjectAdmissionBudget::new(Default::default());
    let decoded = PrefabExtractionRecovery::decode(
        data.encoded_bytes(),
        Default::default(),
        &mut decode_budget,
    )
    .unwrap();
    assert_eq!(decoded.digest(), data.digest());
    assert_eq!(
        decoded.complete_original_sources(),
        data.complete_original_sources()
    );
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), live);
    let outputs = prepared.cancel().unwrap();
    assert_eq!(
        outputs.review().context.budget.retained_candidate_bytes,
        retained
    );
    outputs.cancel().unwrap();
    assert_eq!(live_state(&fixture), live);
}

#[test]
fn recovery_data_refusal_returns_the_same_outputs_and_spent_allowances_for_retry_or_cancel() {
    for retry in [false, true] {
        let fixture = Fixture::new();
        let disk = fixture.disk_state();
        let live = live_state(&fixture);
        let outputs = fixture
            .request()
            .capture()
            .unwrap()
            .prepare()
            .unwrap()
            .prepare_review(vec![fixture.document(INACTIVE)])
            .unwrap()
            .prepare_outputs()
            .unwrap();
        let allocations = original_allocations(outputs.review());
        let fixed = outputs
            .outputs()
            .map(|member| member.bytes().unwrap().as_ptr())
            .collect::<Vec<_>>();
        let retained = outputs.review().context.budget.retained_candidate_bytes;
        let before = outputs.review().persistence.usage();
        let limits = PrefabExtractionRecoveryLimits {
            aggregate_bytes: ByteLimit::new(1).unwrap(),
            ..Default::default()
        };
        let failure = outputs.prepare_recovery(limits).unwrap_err();
        assert!(matches!(
            failure.error(),
            ProjectPrefabExtractionRecoveryError::Data(
                PrefabExtractionRecoveryError::AggregateBytesExceeded
            )
        ));
        assert_eq!(
            original_allocations(failure.prepared().outputs().review()),
            allocations
        );
        assert!(
            failure
                .prepared()
                .outputs()
                .review()
                .persistence
                .usage()
                .work_bytes
                > before.work_bytes
        );
        assert_eq!(
            failure
                .prepared()
                .outputs()
                .review()
                .context
                .budget
                .retained_candidate_bytes,
            retained
        );
        let outputs = if retry {
            let spent = failure.prepared().outputs().review().persistence.usage();
            let prepared = failure.retry(Default::default()).unwrap();
            assert!(prepared.outputs().review().persistence.usage().work_bytes > spent.work_bytes);
            assert!(
                prepared
                    .recovery()
                    .unwrap()
                    .complete_original_sources()
                    .len()
                    > 1
            );
            prepared.cancel().unwrap()
        } else {
            failure.cancel().unwrap()
        };
        assert_eq!(
            outputs.review().context.budget.retained_candidate_bytes,
            retained
        );
        assert_eq!(
            outputs
                .outputs()
                .map(|member| member.bytes().unwrap().as_ptr())
                .collect::<Vec<_>>(),
            fixed
        );
        outputs.cancel().unwrap().revise(vec![]).unwrap();
        assert_eq!(fixture.disk_state(), disk);
        assert_eq!(live_state(&fixture), live);
    }
}

#[test]
fn fixed_members_use_exact_reviewed_bytes_keys_encodings_and_physical_order() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let tabs = fixture.workspace.authoring_model().documents;
    let inactive = fixture.document(INACTIVE);
    let guard = fixture.document(GUARD);
    let review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![inactive, guard])
        .unwrap();
    let allocations = original_allocations(&review);
    let mut paths = review
        .context
        .inventory
        .participants()
        .enumerate()
        .filter(|(source, _)| review.saved.published()[*source])
        .map(|(_, member)| member.file().path().to_owned())
        .collect::<Vec<_>>();
    paths.extend([Q.to_owned(), Q_SIDECAR.to_owned()]);
    let mut outputs = review.prepare_outputs().unwrap();
    assert!(outputs.is_complete());
    assert_eq!(outputs.position(), paths.len());
    assert_eq!(
        outputs
            .outputs()
            .map(|member| member.path().to_owned())
            .collect::<Vec<_>>(),
        paths
    );
    assert_eq!(original_allocations(outputs.review()), allocations);

    for member in outputs.outputs() {
        let bytes = member.bytes().unwrap();
        assert_eq!(member.digest(), Some(ContentDigest::of_bytes(bytes)));
        match member.role() {
            ProjectPrefabExtractionOutputRole::Existing { source, document } => {
                let review = outputs.review();
                let original = review.context.inventory.participants().nth(source).unwrap();
                assert!(review.saved.published()[source]);
                assert_eq!(
                    member.source_key(),
                    Some(review.saved.sources()[source].key())
                );
                assert_eq!(
                    member.observation(),
                    ProjectPrefabExtractionOutputObservation::Existing {
                        identity: original.file().identity(),
                        parent_identity: original.file().parent_identity(),
                        digest: original.file().digest(),
                    }
                );
                let ProjectPrefabExtractionOutputEncoding::Source(encoding) = member.encoding()
                else {
                    panic!("an original source must retain its actual source encoding")
                };
                assert_eq!(bytes, canonical(&review.saved.sources()[source], encoding));
                if let Some(document) = document {
                    let checkpoint = member.encoded_checkpoint().unwrap().checkpoint();
                    assert_eq!(checkpoint.document(), document);
                    assert_eq!(
                        checkpoint.source_binding().unwrap().locator(),
                        member.path()
                    );
                    match &review.saved.sources()[source] {
                        CapturedExtractionSource::Scene { document, .. } => {
                            assert!(std::ptr::eq(checkpoint.scene().unwrap(), document.as_ref()));
                        }
                        CapturedExtractionSource::Prefab { source } => {
                            assert!(std::ptr::eq(
                                checkpoint.prefab().unwrap(),
                                source.document()
                            ));
                        }
                    }
                    assert!(member.encoded_source().is_none());
                } else {
                    assert_eq!(
                        member.encoded_source().unwrap().source(),
                        PrefabExtractionSourceOutput::Existing(source)
                    );
                }
            }
            ProjectPrefabExtractionOutputRole::ExtractedPrefab => {
                assert_eq!(member.path(), Q);
                assert_eq!(
                    member.source_key(),
                    Some(outputs.review().saved.extracted().key())
                );
                assert_eq!(
                    member.encoding(),
                    ProjectPrefabExtractionOutputEncoding::Source(SceneFileEncoding::Json)
                );
                assert_eq!(
                    member.observation(),
                    ProjectPrefabExtractionOutputObservation::ObservedAbsent
                );
                assert_eq!(
                    member.encoded_source().unwrap().source(),
                    PrefabExtractionSourceOutput::Extracted
                );
                assert!(member.encoded_checkpoint().is_none());
                assert_eq!(
                    bytes,
                    outputs
                        .review()
                        .saved
                        .extracted()
                        .document()
                        .to_json_string()
                        .unwrap()
                        .as_bytes()
                );
            }
            ProjectPrefabExtractionOutputRole::ExtractedMetadata => {
                assert_eq!(member.path(), Q_SIDECAR);
                assert_eq!(member.source_key(), None);
                assert_eq!(
                    member.encoding(),
                    ProjectPrefabExtractionOutputEncoding::MetadataJson
                );
                assert_eq!(
                    member.observation(),
                    ProjectPrefabExtractionOutputObservation::ObservedAbsent
                );
                assert_eq!(
                    bytes,
                    AssetMeta::new(fixture.target.clone(), ())
                        .to_json_string()
                        .unwrap()
                        .as_bytes()
                );
                let metadata = AssetMetaCandidate::<()>::decode_json_bytes(bytes).unwrap();
                assert_eq!(metadata.identity(), &fixture.target);
                assert_eq!(metadata.identity().stable_id(), Q_ID.parse().unwrap());
                assert_eq!(metadata.identity().path().as_str(), "new.prefab.json");
                assert_eq!(metadata.settings(), &());
                assert!(member.encoded_source().is_none());
                assert!(member.encoded_checkpoint().is_none());
            }
        }
    }
    let ron = outputs
        .outputs()
        .find(|member| member.path() == GUARD)
        .unwrap();
    assert_eq!(
        ron.encoding(),
        ProjectPrefabExtractionOutputEncoding::Source(SceneFileEncoding::Ron)
    );
    outputs.require_saved_generation().unwrap();
    outputs.revalidate_project(&fixture.project).unwrap();
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), live);
    assert_eq!(fixture.workspace.authoring_model().documents, tabs);
    assert!(!fixture.root.join(Q).exists());
    assert!(!fixture.root.join(Q_SIDECAR).exists());
}

#[test]
fn guards_have_no_output_and_cancel_returns_exact_checkpoints_for_review_revision() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let inactive = fixture.document(INACTIVE);
    let review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![inactive])
        .unwrap();
    let allocations = original_allocations(&review);
    let retained_before = review.context.budget.retained_candidate_bytes;
    let outputs = review.prepare_outputs().unwrap();
    assert!(outputs.outputs().any(|member| member.path() == INACTIVE));
    assert!(!outputs.outputs().any(|member| member.path() == GUARD));
    let saved = outputs
        .review()
        .persistence
        .saved_scene(inactive)
        .unwrap()
        .as_ref() as *const SceneDocument;
    let spent = outputs.review().persistence.usage();
    let host_work = outputs.review().context.budget.usage().decode_work();
    assert!(outputs.review().context.budget.retained_candidate_bytes > retained_before);
    let mut review = outputs.cancel().unwrap();
    assert_eq!(original_allocations(&review), allocations);
    assert_eq!(
        review.context.budget.retained_candidate_bytes,
        retained_before
    );
    assert_eq!(review.context.budget.usage().decode_work(), host_work);
    assert!(review.persistence.usage().work_bytes > spent.work_bytes);
    let (checkpoint, _) = review.persistence.take_checkpoint(inactive).unwrap();
    assert_eq!(checkpoint.scene().unwrap() as *const SceneDocument, saved);
    review.persistence.return_checkpoint(checkpoint).unwrap();
    let mut revised = review.revise(vec![]).unwrap();
    revised.require_saved_generation().unwrap();
    assert_eq!(original_allocations(&revised)[..3], allocations[..3]);
    assert_eq!(original_allocations(&revised)[5], allocations[5]);
    assert_eq!(
        revised.persistence.disposition(inactive),
        Some(nara_tooling::EditorPrefabExtractionDisposition::Guard)
    );
    assert!(
        !revised
            .prepare_outputs()
            .unwrap()
            .outputs()
            .any(|member| member.path() == INACTIVE)
    );
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), live);
}

#[test]
fn retry_after_a_real_open_encoding_retains_completed_bytes_and_original_allocations() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap();
    let allocations = original_allocations(&review);
    // Exhausted real allowances cannot recover without a new operation. A single finite
    // test-only refusal isolates resumable progress, immediately after a real open encoding.
    let failure = review.test_prepare_outputs_refusing_before(1).unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionOutputError::Injected
    ));
    assert_eq!(failure.outputs().position(), 1);
    let first = failure.outputs().outputs().next().unwrap();
    assert_eq!(first.path(), P);
    assert!(first.encoded_checkpoint().is_some());
    let allocation = first.bytes().unwrap().as_ptr();
    let bytes = first.bytes().unwrap().to_vec();
    let digest = first.digest();
    let spent = failure.outputs().review().persistence.usage();
    let outputs = failure.retry().unwrap();
    assert!(outputs.is_complete());
    let first = outputs.outputs().next().unwrap();
    assert_eq!(first.bytes().unwrap().as_ptr(), allocation);
    assert_eq!(first.bytes().unwrap(), bytes);
    assert_eq!(first.digest(), digest);
    assert_eq!(original_allocations(outputs.review()), allocations);
    assert!(outputs.review().persistence.usage().work_nodes > spent.work_nodes);
    assert!(outputs.review().persistence.usage().work_bytes > spent.work_bytes);
    let mut review = outputs.cancel().unwrap();
    review.require_saved_generation().unwrap();
    assert!(review.revise(vec![]).is_ok());
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), live);
}

#[test]
fn late_metadata_refusals_keep_q_and_completed_buffers_and_continue_original_counters() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let mut review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap();
    let allocations = original_allocations(&review);
    let before = review.context.budget.usage();
    review.context.budget.limits.decode_work = ItemLimit::new(before.decode_work()).unwrap();
    let failure = review.prepare_outputs().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionOutputError::Project(_)
    ));
    let outputs = failure.outputs();
    assert_eq!(outputs.position() + 1, outputs.outputs().len());
    let completed = outputs
        .outputs()
        .take(outputs.position())
        .map(|member| {
            (
                member.path().to_owned(),
                member.bytes().unwrap().as_ptr(),
                member.bytes().unwrap().to_vec(),
                member.digest(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        outputs
            .outputs()
            .any(|member| member.encoded_checkpoint().is_some())
    );
    assert!(
        outputs
            .outputs()
            .any(|member| member.path() == Q && member.encoded_source().is_some())
    );
    assert_eq!(
        outputs.review().context.budget.retained_candidate_bytes,
        before.retained_candidate_bytes()
    );
    let host_work = outputs.review().context.budget.usage().decode_work();
    let source_work = outputs.review().persistence.usage();
    let failure = failure.retry().unwrap_err();
    assert_eq!(
        original_allocations(failure.outputs().review()),
        allocations
    );
    assert!(
        failure
            .outputs()
            .review()
            .context
            .budget
            .usage()
            .decode_work()
            > host_work
    );
    assert!(failure.outputs().review().persistence.usage().work_bytes > source_work.work_bytes);
    assert_eq!(
        failure
            .outputs()
            .review()
            .context
            .budget
            .retained_candidate_bytes,
        before.retained_candidate_bytes()
    );
    for (member, (path, allocation, bytes, digest)) in
        failure.outputs().outputs().zip(completed.iter())
    {
        assert_eq!(member.path(), path);
        assert_eq!(member.bytes().unwrap().as_ptr(), *allocation);
        assert_eq!(member.bytes().unwrap(), bytes);
        assert_eq!(member.digest(), *digest);
    }
    let review = failure.cancel().unwrap();
    assert_eq!(original_allocations(&review), allocations);
    assert!(review.revise(vec![]).is_ok());
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn constructor_refusal_and_retries_preserve_the_same_review_and_spent_allowance() {
    let fixture = Fixture::new();
    let baseline = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![])
        .unwrap();
    let limit = baseline.persistence.usage().work_bytes;
    drop(baseline);
    let limits =
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(limit).unwrap());
    let review = fixture
        .request_with_limits(limits)
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![])
        .unwrap();
    let allocations = original_allocations(&review);
    let spent = review.persistence.usage();
    let failure = review.prepare_outputs().unwrap_err();
    assert_eq!(failure.outputs().outputs().len(), 0);
    assert_eq!(failure.outputs().position(), 0);
    assert_eq!(failure.outputs().review().persistence.usage(), spent);
    let failure = failure.retry().unwrap_err();
    assert_eq!(failure.outputs().review().persistence.usage(), spent);
    assert_eq!(
        original_allocations(failure.outputs().review()),
        allocations
    );
    assert_eq!(
        original_allocations(&failure.cancel().unwrap()),
        allocations
    );
}

#[test]
fn changed_closed_prefab_uses_the_sealed_source_output_without_a_workspace_session() {
    let fixture = Fixture::new();
    let path = "desktop/prefabs/closed.prefab.ron";
    let key = SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(P).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let mut overlay = nara_scene::PrefabOverlay::new(key);
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("desktop-leaf")));
    let mut instance = PrefabInstance::forest(AssetRef::path(ALIAS).unwrap());
    instance.overrides = Some(overlay);
    let mut entity = SceneEntityRecord::new(id("closed"));
    entity.prefab = Some(instance);
    fs::write(
        fixture.root.join(path),
        PrefabDocument::from_ordered_roots([entity])
            .to_ron_string()
            .unwrap(),
    )
    .unwrap();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let tabs = fixture.workspace.authoring_model().documents;
    let review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![])
        .unwrap();
    let source = source_index(review.saved.sources(), path);
    assert!(review.saved.published()[source]);
    let outputs = review.prepare_outputs().unwrap();
    let member = outputs
        .outputs()
        .find(|member| member.path() == path)
        .unwrap();
    assert_eq!(
        member.role(),
        ProjectPrefabExtractionOutputRole::Existing {
            source,
            document: None
        }
    );
    assert_eq!(
        member.encoding(),
        ProjectPrefabExtractionOutputEncoding::Source(SceneFileEncoding::Ron)
    );
    assert_eq!(
        member.encoded_source().unwrap().source(),
        PrefabExtractionSourceOutput::Existing(source)
    );
    assert!(member.encoded_checkpoint().is_none());
    assert_eq!(
        member.bytes().unwrap(),
        canonical(
            &outputs.review().saved.sources()[source],
            SceneFileEncoding::Ron
        )
    );
    assert_eq!(fixture.workspace.authoring_model().documents, tabs);
    assert_eq!(live_state(&fixture), live);
    assert_eq!(fixture.disk_state(), disk);
    assert!(outputs.cancel().unwrap().revise(vec![]).is_ok());
}

#[test]
fn cancellation_refusal_retains_the_actual_checkpoint_metadata_and_original_counters() {
    let fixture = Fixture::new();
    let baseline = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![])
        .unwrap()
        .prepare_outputs()
        .unwrap();
    let limit = baseline.review().persistence.usage().work_bytes;
    drop(baseline);
    let limits =
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(limit).unwrap());
    let review = fixture
        .request_with_limits(limits)
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![])
        .unwrap();
    let allocations = original_allocations(&review);
    let outputs = review.prepare_outputs().unwrap();
    let metadata = outputs
        .outputs()
        .find(|member| member.path() == Q_SIDECAR)
        .unwrap();
    let bytes = metadata.bytes().unwrap().to_vec();
    let allocation = metadata.bytes().unwrap().as_ptr();
    let retained = outputs.review().context.budget.retained_candidate_bytes;
    let source_work = outputs.review().persistence.usage();
    let host_work = outputs.review().context.budget.usage().decode_work();
    let expected = outputs
        .outputs()
        .next()
        .unwrap()
        .checkpoint()
        .unwrap()
        .prefab()
        .unwrap() as *const PrefabDocument;
    let failure = outputs.cancel().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionOutputError::Workspace(_)
    ));
    let first = failure.outputs().outputs().next().unwrap();
    assert!(first.encoded_checkpoint().is_none());
    assert_eq!(
        first.checkpoint().unwrap().prefab().unwrap() as *const _,
        expected
    );
    assert_eq!(
        original_allocations(failure.outputs().review()),
        allocations
    );
    assert!(!failure.outputs().is_complete());
    let failure = failure.retry().unwrap_err();
    let metadata = failure
        .outputs()
        .outputs()
        .find(|member| member.path() == Q_SIDECAR)
        .unwrap();
    assert_eq!(metadata.bytes().unwrap().as_ptr(), allocation);
    assert_eq!(metadata.bytes().unwrap(), bytes);
    assert_eq!(
        failure
            .outputs()
            .review()
            .context
            .budget
            .retained_candidate_bytes,
        retained
    );
    assert_eq!(
        failure
            .outputs()
            .review()
            .context
            .budget
            .usage()
            .decode_work(),
        host_work
    );
    assert_eq!(failure.outputs().review().persistence.usage(), source_work);
    assert_eq!(
        failure
            .outputs()
            .outputs()
            .next()
            .unwrap()
            .checkpoint()
            .unwrap()
            .prefab()
            .unwrap() as *const _,
        expected
    );
}

#[test]
fn cancellation_preserves_accepted_evidence_and_the_original_review_revision_gate() {
    use crate::project_host::file_publication::{FilePublicationBinding, FilePublicationOutcome};
    use nara_tooling::EditorPersistenceCommit;

    let mut fixture = Fixture::new();
    let inactive = fixture.document(INACTIVE);
    let mut review = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![inactive])
        .unwrap();
    let allocations = original_allocations(&review);
    let encoded = review
        .persistence
        .encode_checkpoint(inactive, SceneFileEncoding::Json)
        .unwrap();
    let (_, _, publisher) = fixture
        .open
        .iter_mut()
        .find(|(document, _, _)| *document == inactive)
        .unwrap();
    let FilePublicationBinding::Existing { identity, .. } = publisher.binding() else {
        panic!("the actual open Scene must retain its original binding")
    };
    let FilePublicationOutcome::Published(accepted) = publisher.publish(encoded.bytes()) else {
        panic!("the fixed checkpoint must produce actual accepted file evidence")
    };
    let (publication, _) = accepted.into_parts();
    let (receipt, _) = publication.into_parts();
    let commit = EditorPersistenceCommit::from_publication(encoded, identity, receipt).unwrap();
    review.persistence.accept_commit(commit).unwrap();
    let disk = fixture.disk_state();
    let live = live_state(&fixture);
    let failure = review.prepare_outputs().unwrap_err();
    assert!(matches!(
        failure.error(),
        ProjectPrefabExtractionOutputError::Open(_)
    ));
    assert!(failure.outputs().position() > 0);
    let mut review = failure.cancel().unwrap();
    assert_eq!(original_allocations(&review), allocations);
    review.require_saved_generation().unwrap();
    let failure = review.revise(vec![]).unwrap_err();
    let mut review = failure.into_review().unwrap();
    assert_eq!(original_allocations(&review), allocations);
    assert!(review.persistence.take_checkpoint(inactive).is_err());
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), live);
}
