//! Complete-proposal tests reuse the actual v1/v2 temporary project from the source owner.

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath};
use nara_core::ByteLimit;
use nara_scene::__private::AuthoredSchemaUpgradeUsage;

use super::*;
use crate::project_host::AuthoredSchemaUpgradeHistoryPolicy;
use crate::project_host::file_publication::FilePublicationHost;

fn baseline(fixture: &Fixture, limits: AuthoredSchemaUpgradeLimits) -> SourceUpgradeBaseline {
    let mut budget = budget();
    // Refused attempts consume real work. Retry tests deliberately share one finite allowance
    // rather than replacing it after each refusal.
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let mut work = AuthoredSchemaUpgradeWork::new(limits);
    let input = fixture.capture_workspace(&mut work);
    let inventory = fixture.capture_inventory(&mut budget);
    let owned_metadata = fixture
        .open
        .iter()
        .try_fold(size_of::<SourceUpgradeBaseline>(), |bytes, (_, source)| {
            checked_sum([
                bytes,
                SourceUpgradeOpenInput::capture_retained_bytes(Some(source))?,
            ])
        })
        .unwrap();
    budget
        .charge_source_bytes(0, 0, owned_metadata, owned_metadata)
        .unwrap();
    let mut open = Vec::with_capacity(fixture.open.len());
    for (document, source) in &fixture.open {
        let file = fixture
            .schema
            .project_root
            .open_file_exact(&RelativePath::new(source.relative_path()).unwrap())
            .unwrap();
        let host = FilePublicationHost::bind_existing(
            &fixture.schema.project_root,
            source.relative_path(),
            &file,
            source.digest(),
            u64::try_from(SceneFileLimits::default().encoded_bytes().get()).unwrap(),
        )
        .unwrap();
        let cost = host.source_observation_cost().unwrap();
        budget
            .charge_source_bytes(0, 0, cost.retained_bytes, cost.retained_bytes)
            .unwrap();
        budget
            .admit_handle_peak(cost.retained_handle_capacity)
            .unwrap();
        budget
            .retain_open_handles(cost.retained_handle_capacity)
            .unwrap();
        let observation = host
            .capture_source_observation((
                source.identity(),
                source.parent_identity(),
                source.digest(),
            ))
            .unwrap();
        open.push(SourceUpgradeOpenInput {
            document: *document,
            source: Some(source.clone()),
            observation: Some(observation),
        });
    }
    open.sort_unstable_by(|left, right| {
        left.source
            .as_ref()
            .unwrap()
            .relative_path()
            .cmp(right.source.as_ref().unwrap().relative_path())
    });
    SourceUpgradeBaseline {
        schema: Arc::clone(&fixture.schema),
        basis: fixture.previous.clone(),
        scene_limits: SceneFileLimits::default(),
        budget,
        work,
        input,
        open,
        inventory,
    }
}

fn source_proposal(index: usize) -> AuthoredSchemaUpgradeSourceProposal {
    AuthoredSchemaUpgradeSourceProposal {
        key: SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(PATHS[index]).unwrap(),
            if index == 2 {
                SourceDocumentKind::Prefab
            } else {
                SourceDocumentKind::Scene
            },
        ),
        project_path: PATHS[index].into(),
        reviewed_working: encoded(index, true, false).into_boxed_slice(),
        successor: encoded(index, true, true).into_boxed_slice(),
    }
}

fn proposal(fixture: &Fixture) -> AuthoredSchemaUpgradeProposal {
    AuthoredSchemaUpgradeProposal {
        successor_bundle: fixture.successor_bundle.clone().into_boxed_slice(),
        component: ComponentTypeId::new(COMPONENT),
        field: ComponentFieldId::new("health"),
        successor_property: selector(),
        sources: (0..PATHS.len()).map(source_proposal).collect(),
    }
}

fn unrelated_armor_proposal(fixture: &Fixture) -> AuthoredSchemaUpgradeProposal {
    let mut proposed = proposal(fixture);
    let source = &mut proposed.sources[1];
    let mut document = SceneDocumentCandidate::decode_json_bytes(&source.successor)
        .unwrap()
        .into_composition_source()
        .unwrap();
    let record = document.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap();
    record.value = ComponentValue::map([
        (
            "wire_health",
            ComponentValue::map([("milli", ComponentValue::I64(110000))]),
        ),
        ("armor", ComponentValue::I64(8)),
    ]);
    source.successor = document
        .to_json_string()
        .unwrap()
        .into_bytes()
        .into_boxed_slice();
    proposed
}

fn prepare(
    baseline: SourceUpgradeBaseline,
    proposal: AuthoredSchemaUpgradeProposal,
) -> Result<PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalFailure> {
    baseline.proposal_request(proposal)?.prepare()
}

#[test]
fn complete_proposal_uses_the_captured_dirty_scene_prefab_and_closed_profile_source() {
    let fixture = Fixture::new();
    let baseline = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    let original_file = baseline.inventory.shared_file(PATHS[3]).unwrap();
    let original_bytes = baseline
        .inventory
        .participants()
        .find(|member| member.file().path() == PATHS[3])
        .unwrap()
        .bytes()
        .as_ptr();
    let histories: Vec<_> = fixture
        .open
        .iter()
        .map(|(document, _)| history(&fixture.workspace, *document))
        .collect();
    let mut proposed = proposal(&fixture);
    proposed.sources.reverse();
    let request = baseline.proposal_request(proposed).unwrap();
    // The actual owned request, not a fresh baseline, crosses a real finite worker boundary.
    let mut prepared = std::thread::spawn(move || request.prepare())
        .join()
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(
        &prepared.sources.inventory().shared_file(PATHS[3]).unwrap(),
        &original_file,
    ));
    assert_eq!(
        prepared
            .sources
            .inventory()
            .participants()
            .find(|member| member.file().path() == PATHS[3])
            .unwrap()
            .bytes()
            .as_ptr(),
        original_bytes
    );
    prepared
        .sources
        .validate(
            &fixture.schema,
            &fixture.workspace,
            &fixture.open_sources(),
            &mut prepared.budget,
            &mut prepared.work,
        )
        .unwrap();
    let review = prepared.review();
    assert!(std::ptr::eq(
        review.schema_bundle(),
        fixture.schema.bundle()
    ));
    assert_eq!(
        review
            .successor_schema_bundle()
            .to_json_string()
            .unwrap()
            .as_bytes(),
        fixture.successor_bundle.as_slice()
    );
    assert_eq!(review.component(), &ComponentTypeId::new(COMPONENT));
    assert_eq!(review.field(), &ComponentFieldId::new("health"));
    assert_eq!(review.successor_property(), &selector());
    assert_eq!(review.source_count(), PATHS.len());
    assert_eq!(review.document_count(), fixture.workspace.len());
    assert_eq!(
        review.history_policy(),
        AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
    );
    let mut sources = review.sources();
    assert_eq!(sources.len(), PATHS.len());
    let mut ordered = PATHS;
    ordered.sort_unstable();
    for path in ordered {
        let source = sources.next().unwrap();
        let index = PATHS
            .iter()
            .position(|candidate| *candidate == path)
            .unwrap();
        assert_eq!(source.project_path, path);
        assert_eq!(source.key, &source_proposal(index).key);
        assert_eq!(source.original_disk, encoded(index, false, false));
        assert_eq!(source.reviewed_working, encoded(index, true, false));
        assert_eq!(source.successor, encoded(index, true, true));
        assert_eq!(
            source.document,
            fixture
                .open
                .iter()
                .find(|(_, fact)| fact.relative_path() == path)
                .map(|(document, _)| *document)
        );
        assert_eq!(
            fs::read(fixture.root.join(path)).unwrap(),
            encoded(index, false, false)
        );
    }
    assert_eq!(sources.len(), 0);
    assert!(sources.next().is_none());
    assert_eq!(
        review.sources().nth(2).unwrap().document,
        Some(fixture.open[0].0)
    );
    assert_eq!(
        review.sources().last().unwrap().document,
        Some(fixture.open[1].0)
    );
    for ((document, _), before) in fixture.open.iter().zip(histories) {
        assert_eq!(history(&fixture.workspace, *document), before);
    }
}

#[test]
fn queue_admission_refuses_missing_duplicate_wrong_key_or_wrong_path_without_losing_baseline() {
    let fixture = Fixture::new();
    let mut captured = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    let original = captured.inventory.shared_file(PATHS[0]).unwrap();
    let retained = captured.usage().retained_candidate_bytes();
    let handles = captured.usage().retained_open_handles();
    let frozen = captured.input.scene_document(fixture.open[0].0).unwrap() as *const _;
    for case in 0..4 {
        let mut proposed = proposal(&fixture);
        match case {
            0 => {
                let mut sources = proposed.sources.into_vec();
                sources.pop();
                proposed.sources = sources.into();
            }
            1 => proposed.sources[1] = source_proposal(0),
            2 => proposed.sources[1].key = source_proposal(0).key,
            3 => proposed.sources[1].project_path = "scenes/not-captured.scene.json".into(),
            _ => unreachable!(),
        }
        let failure = captured.proposal_request(proposed).err().unwrap();
        assert!(failure.error.diagnostics().is_none());
        captured = failure.baseline;
        assert_eq!(captured.usage().retained_candidate_bytes(), retained);
        assert_eq!(captured.usage().retained_open_handles(), handles);
        assert!(Arc::ptr_eq(
            &captured.inventory.shared_file(PATHS[0]).unwrap(),
            &original
        ));
        assert_eq!(
            captured.input.scene_document(fixture.open[0].0).unwrap() as *const _,
            frozen
        );
    }
    prepare(captured, proposal(&fixture)).unwrap();
}

#[test]
fn queued_proposal_storage_is_admitted_before_a_worker_can_retain_it() {
    let fixture = Fixture::new();
    let mut captured = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    let retained = captured.usage().retained_candidate_bytes();
    let handles = captured.usage().retained_open_handles();
    let original = captured.inventory.shared_file(PATHS[0]).unwrap();
    captured.budget.limits = captured
        .budget
        .limits
        .with_retained_candidate_bytes(ByteLimit::new(retained + 1).unwrap());
    let failure = captured
        .proposal_request(proposal(&fixture))
        .err()
        .expect("queue custody must never precede raw proposal admission");
    assert_eq!(
        failure.baseline.usage().retained_candidate_bytes(),
        retained
    );
    assert_eq!(failure.baseline.usage().retained_open_handles(), handles);
    assert!(Arc::ptr_eq(
        &failure.baseline.inventory.shared_file(PATHS[0]).unwrap(),
        &original
    ));
}

#[test]
fn wrong_reviewed_working_copy_returns_the_same_inputs_and_refunds_attempt_storage() {
    let fixture = Fixture::new();
    let captured = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    let retained = captured.usage().retained_candidate_bytes();
    let handles = captured.usage().retained_open_handles();
    let before_work = captured.work_usage();
    let original = captured.inventory.shared_file(PATHS[1]).unwrap();
    let mut proposed = proposal(&fixture);
    proposed.sources[1].reviewed_working = encoded(1, false, false).into_boxed_slice();
    let failure = prepare(captured, proposed).err().unwrap();
    assert!(failure.error.diagnostics().is_none());
    assert_eq!(
        failure.baseline.usage().retained_candidate_bytes(),
        retained
    );
    assert_eq!(failure.baseline.usage().retained_open_handles(), handles);
    assert!(failure.baseline.work_usage().work_bytes >= before_work.work_bytes);
    assert!(Arc::ptr_eq(
        &failure.baseline.inventory.shared_file(PATHS[1]).unwrap(),
        &original
    ));
    prepare(failure.baseline, proposal(&fixture)).unwrap();
    assert_eq!(
        fs::read(fixture.root.join(PATHS[1])).unwrap(),
        encoded(1, false, false)
    );
}

#[test]
fn unrelated_field_refusal_exposes_the_actual_workspace_diagnostics_and_keeps_history() {
    let fixture = Fixture::new();
    let captured = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    let retained = captured.usage().retained_candidate_bytes();
    let handles = captured.usage().retained_open_handles();
    let before = history(&fixture.workspace, fixture.open[1].0);
    let failure = prepare(captured, unrelated_armor_proposal(&fixture))
        .err()
        .unwrap();
    let diagnostics = failure
        .error
        .diagnostics()
        .expect("the real Workspace refusal owns a report");
    assert!(diagnostics.has_errors());
    assert_eq!(
        failure.baseline.usage().retained_candidate_bytes(),
        retained
    );
    assert_eq!(failure.baseline.usage().retained_open_handles(), handles);
    assert_eq!(history(&fixture.workspace, fixture.open[1].0), before);
    prepare(failure.baseline, proposal(&fixture)).unwrap();
}

#[test]
fn corrected_complete_proposal_retries_with_the_same_single_success_source_peak() {
    let fixture = Fixture::new();
    let sample = prepare(
        baseline(&fixture, AuthoredSchemaUpgradeLimits::default()),
        proposal(&fixture),
    )
    .unwrap();
    let AuthoredSchemaUpgradeUsage {
        work_bytes,
        peak_retained_bytes,
    } = sample.work.usage();
    drop(sample);
    for exact in [true, false] {
        let captured = baseline(
            &fixture,
            AuthoredSchemaUpgradeLimits::new(
                ByteLimit::new(512 * 1024 * 1024).unwrap(),
                ByteLimit::new(peak_retained_bytes - usize::from(!exact)).unwrap(),
            ),
        );
        let retained = captured.usage().retained_candidate_bytes();
        let original = captured.inventory.shared_file(PATHS[0]).unwrap();
        let failure = prepare(captured, unrelated_armor_proposal(&fixture))
            .err()
            .unwrap();
        assert!(failure.error.diagnostics().is_some());
        assert_eq!(
            failure.baseline.usage().retained_candidate_bytes(),
            retained
        );
        let prior_work = failure.baseline.work_usage().work_bytes;
        let result = prepare(failure.baseline, proposal(&fixture));
        if exact {
            let prepared = result.unwrap();
            assert_eq!(
                prepared.work.usage().peak_retained_bytes,
                peak_retained_bytes
            );
            assert!(prepared.work.usage().work_bytes > work_bytes);
            assert!(prepared.work.usage().work_bytes > prior_work);
            assert!(Arc::ptr_eq(
                &prepared.sources.inventory().shared_file(PATHS[0]).unwrap(),
                &original
            ));
        } else {
            let failure = result
                .err()
                .expect("one byte below the actual peak must refuse");
            assert!(
                failure
                    .error
                    .diagnostics()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
                    })
            );
            assert_eq!(
                failure.baseline.usage().retained_candidate_bytes(),
                retained
            );
            assert!(Arc::ptr_eq(
                &failure.baseline.inventory.shared_file(PATHS[0]).unwrap(),
                &original
            ));
        }
    }
}

#[test]
fn recovery_uses_captured_original_metadata_bytes_and_confirmation_detects_later_changes() {
    let fixture = Fixture::new();
    let path = "assets/image.png.meta";
    fs::write(
        fixture.root.join("assets/image.png"),
        b"unrelated image source",
    )
    .unwrap();
    let mut metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            "ad322a15-01a2-4c04-847f-83e4df33f139".parse().unwrap(),
            AssetPath::new("image.png").unwrap(),
            AssetSourceKind::Image,
        ),
        serde_json::json!({"opaque_settings": [1, 2, 3]}),
    )
    .to_json_string()
    .unwrap()
    .into_bytes();
    metadata.extend_from_slice(b" \r\n");
    fs::write(fixture.root.join(path), &metadata).unwrap();
    let captured = baseline(&fixture, AuthoredSchemaUpgradeLimits::default());
    assert_eq!(
        captured
            .inventory
            .metadata()
            .find(|member| member.file().path() == path)
            .unwrap()
            .bytes(),
        metadata
    );
    // Preparation must use the immutable reviewed inventory; confirmation must reject this
    // later external edit rather than quietly incorporating it into a different operation.
    fs::write(fixture.root.join(path), b"external invalid metadata edit").unwrap();
    let mut prepared = prepare(captured, proposal(&fixture)).unwrap();
    let stored = prepared
        .sources
        .recovery()
        .metadata()
        .iter()
        .find(|member| member.path == path)
        .unwrap();
    assert_eq!(stored.observed.bytes(), metadata);
    let retained = prepared.budget.usage().retained_candidate_bytes();
    assert!(
        prepared
            .sources
            .validate(
                &fixture.schema,
                &fixture.workspace,
                &fixture.open_sources(),
                &mut prepared.budget,
                &mut prepared.work,
            )
            .is_err()
    );
    assert_eq!(prepared.budget.usage().retained_candidate_bytes(), retained);
    assert_eq!(
        fs::read(fixture.root.join(path)).unwrap(),
        b"external invalid metadata edit"
    );
}
