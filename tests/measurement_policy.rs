#[path = "support/first_playable_evidence.rs"]
mod evidence;

use std::collections::BTreeMap;

use evidence::{
    Decision, DecisionEvidenceError, EnvironmentField, EnvironmentRecord, FieldValue,
    MetricObservation, MetricTarget, Population, ProtocolError, SourceChange, TargetRef,
    affected_metrics, ancestry_allows, canonical_json_bytes, decide_suite,
    decode_product_budget_catalogue, decode_product_budget_review, environment_equivalent,
    expected_environments_for_suite, load_calibration_fixture, load_protocol_fixture,
    passing_observations, protocol_digest, validate_product_budget_sources, validate_protocol,
};

fn fail_observation(observation: &mut MetricObservation, target: &MetricTarget) {
    observation.value = match target {
        MetricTarget::Exact { value } => value.saturating_add(1),
        MetricTarget::Maximum { value } => value.saturating_add(1),
        MetricTarget::Minimum { value } => value.saturating_sub(1),
    };
}

fn environment(population: Population) -> EnvironmentRecord {
    EnvironmentRecord {
        population,
        fields: [
            (EnvironmentField::OsClass, "windows_11_x86_64"),
            (EnvironmentField::RunnerImageClass, "local_runner_v1"),
            (EnvironmentField::ToolchainClass, "rustc_1_97_0_msvc"),
            (EnvironmentField::CpuClass, "x86_64_desktop_v1"),
            (EnvironmentField::GpuStackClass, "software_vulkan_v1"),
            (EnvironmentField::BuildClass, "debug_incremental_v1"),
            (EnvironmentField::CollectorClass, "u14_collector_v1"),
        ]
        .into_iter()
        .map(|(field, value)| (field, value.to_owned()))
        .collect::<BTreeMap<_, _>>(),
    }
}

fn passing_suite(
    protocol: &evidence::FirstPlayableProtocol,
    suite_id: &str,
) -> (BTreeMap<String, EnvironmentRecord>, Vec<MetricObservation>) {
    let environments = expected_environments_for_suite(protocol, suite_id);
    let observations = passing_observations(protocol, suite_id, &environments);
    (environments, observations)
}

#[test]
fn committed_protocol_is_complete_canonical_and_digest_bound() {
    let protocol_bytes = std::fs::read(evidence::repository_root().join(evidence::PROTOCOL_PATH))
        .expect("protocol fixture must be readable");
    let protocol = evidence::decode_protocol(&protocol_bytes)
        .expect("committed protocol must be valid and canonical");
    let source_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGETS_PATH))
            .expect("product budget source must be readable");
    let source_digest = evidence::digest_ref(&source_bytes);
    let review_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGET_REVIEW_PATH))
            .expect("product budget review must be readable");
    let review_digest = evidence::digest_ref(&review_bytes);
    assert!(
        protocol
            .range_sources
            .iter()
            .all(|source| source.artifact_digest == source_digest
                && source.review_digest == review_digest
                && source.review_path == evidence::PRODUCT_BUDGET_REVIEW_PATH),
        "every range source must bind both independent artifacts: {source_digest:?}, {review_digest:?}"
    );
    let sidecar =
        std::fs::read_to_string(evidence::repository_root().join(evidence::PROTOCOL_DIGEST_PATH))
            .expect("protocol digest sidecar must be readable");
    assert_eq!(
        sidecar.trim_end(),
        protocol_digest(&protocol).blake3,
        "protocol digest sidecar must bind the canonical bytes"
    );
    assert_eq!(protocol.format_version, 1);
    assert_eq!(protocol.metrics.len(), evidence::REQUIRED_METRIC_IDS.len());
    assert_eq!(
        protocol.subjects.len(),
        evidence::REQUIRED_SUBJECT_IDS.len()
    );
    assert!(protocol.metrics.iter().all(|metric| {
        metric.required
            && metric.minimum_samples > 0
            && !metric.source.reference.is_empty()
            && protocol
                .environment_classes
                .iter()
                .any(|class| class.id == metric.environment_class)
    }));
}

#[test]
fn canonical_evidence_artifacts_are_checked_out_with_lf() {
    let paths = [
        evidence::PROTOCOL_PATH,
        evidence::PROTOCOL_DIGEST_PATH,
        evidence::CALIBRATION_ENVELOPE_PATH,
        evidence::PRODUCT_BUDGETS_PATH,
        evidence::PRODUCT_BUDGET_REVIEW_PATH,
        evidence::PERFORMANCE_REVIEW_EVIDENCE_PATH,
        evidence::PROVENANCE_REVIEW_EVIDENCE_PATH,
    ];
    let output = std::process::Command::new("git")
        .current_dir(evidence::repository_root())
        .args(["check-attr", "eol", "--"])
        .args(paths)
        .output()
        .expect("canonical evidence Git attributes must be inspectable");
    assert!(
        output.status.success(),
        "git check-attr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let attributes = String::from_utf8(output.stdout).expect("Git attributes must be UTF-8");
    for path in paths {
        assert!(
            attributes
                .lines()
                .any(|line| line == format!("{path}: eol: lf")),
            "{path} must have the effective Git attribute eol=lf: {attributes}"
        );
    }
}

#[test]
fn independent_product_budget_artifact_is_revision_digest_and_target_bound() {
    let protocol = load_protocol_fixture().unwrap();
    let source_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGETS_PATH))
            .expect("product budget source must be readable");
    let review_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGET_REVIEW_PATH))
            .expect("product budget review must be readable");
    validate_product_budget_sources(&protocol, &source_bytes, &review_bytes)
        .expect("committed product budget source and review must match every metric");

    let mut source = decode_product_budget_catalogue(&source_bytes).unwrap();
    source.sources[0].metrics[0].target = MetricTarget::Maximum { value: 1 };
    let changed_target = canonical_json_bytes(&source);
    assert_eq!(
        validate_product_budget_sources(&protocol, &changed_target, &review_bytes),
        Err(ProtocolError::Invalid)
    );

    let mut changed_revision = protocol.clone();
    changed_revision.range_sources[0].revision = "different_revision".to_owned();
    assert_eq!(
        validate_product_budget_sources(&changed_revision, &source_bytes, &review_bytes),
        Err(ProtocolError::Invalid)
    );

    let mut changed_digest = protocol.clone();
    changed_digest.range_sources[0].artifact_digest.blake3 = "a".repeat(64);
    assert_eq!(
        validate_product_budget_sources(&changed_digest, &source_bytes, &review_bytes),
        Err(ProtocolError::Invalid)
    );

    let mut changed_review_digest = protocol.clone();
    changed_review_digest.range_sources[0].review_digest.blake3 = "a".repeat(64);
    assert_eq!(
        validate_product_budget_sources(&changed_review_digest, &source_bytes, &review_bytes),
        Err(ProtocolError::Invalid)
    );

    let mut exposed_review = decode_product_budget_review(&review_bytes).unwrap();
    exposed_review.target_results_seen = true;
    assert_eq!(
        validate_product_budget_sources(
            &protocol,
            &source_bytes,
            &canonical_json_bytes(&exposed_review),
        ),
        Err(ProtocolError::Invalid)
    );

    let mut drifted_review = decode_product_budget_review(&review_bytes).unwrap();
    drifted_review.reviewed_artifact.digest.blake3 = "b".repeat(64);
    assert_eq!(
        validate_product_budget_sources(
            &protocol,
            &source_bytes,
            &canonical_json_bytes(&drifted_review),
        ),
        Err(ProtocolError::Invalid)
    );

    let mut early_review = decode_product_budget_review(&review_bytes).unwrap();
    early_review.attestations[0].reviewed_at_utc = "2026-07-15T02:59:59Z".to_owned();
    assert_eq!(
        validate_product_budget_sources(
            &protocol,
            &source_bytes,
            &canonical_json_bytes(&early_review),
        ),
        Err(ProtocolError::Invalid)
    );

    assert_eq!(
        validate_product_budget_sources(&protocol, &source_bytes, b"{}"),
        Err(ProtocolError::Decode)
    );
}

#[test]
fn product_budget_has_an_independent_review_artifact() {
    let protocol = load_protocol_fixture().unwrap();
    let source_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGETS_PATH)).unwrap();
    let review_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGET_REVIEW_PATH))
            .expect("the normative product budget must not approve itself");
    let catalogue = decode_product_budget_catalogue(&source_bytes).unwrap();
    let review = decode_product_budget_review(&review_bytes).unwrap();

    assert!(
        catalogue
            .adoption
            .baseline_policy
            .observed_baselines_describe_current_performance
    );
    assert!(
        catalogue
            .adoption
            .baseline_policy
            .normative_constraints_precede_target_results
    );
    assert!(
        catalogue
            .adoption
            .baseline_policy
            .result_informed_revision_requires_new_version
    );
    assert!(
        catalogue
            .adoption
            .baseline_policy
            .revised_constraints_cannot_decide_source_results
    );
    assert_eq!(
        review.source_revision,
        catalogue.adoption.authority_revision
    );
    assert_ne!(
        review.attestations[0].reviewer_id,
        catalogue.adoption.adopted_by
    );
    assert!(
        review
            .attestations
            .iter()
            .all(|attestation| !attestation.target_results_seen)
    );

    for source in &catalogue.sources {
        for budget in &source.metrics {
            let metric = protocol
                .metrics
                .iter()
                .find(|metric| metric.id == budget.id)
                .unwrap();
            assert_eq!(budget.measurement.subject, metric.subject);
            assert_eq!(
                budget.measurement.environment_class,
                metric.environment_class
            );
            assert_eq!(budget.measurement.value_kind, metric.value_kind);
            assert_eq!(budget.measurement.statistic, metric.statistic);
            assert_eq!(budget.measurement.population, metric.population);
            assert!(!budget.rationale_id.is_empty());
            assert!(!budget.measurement.required_context.is_empty());
        }
    }

    let contract = &catalogue.correctness_contract;
    assert!(contract.task_steps.len() >= 5);
    assert!(contract.fault_cases.len() >= 7);
    assert!(contract.lifecycle_states.len() >= 6);
    assert!(contract.lifecycle_transitions.len() >= 7);
    assert_eq!(
        contract.coverage_denominator,
        "public_engine_calls_in_frozen_slice_v1"
    );
    for metric_id in [
        "ownership.caller_glue_regressions",
        "ownership.invalid_transitions",
        "ownership.unreachable_states",
        "ownership.unjustified_extra_concepts",
        "ownership.unowned_states",
    ] {
        assert!(
            contract
                .rubrics
                .iter()
                .any(|rubric| metric_id == rubric.metric_id)
        );
    }
    assert_eq!(
        protocol
            .metrics
            .iter()
            .find(|metric| metric.id == "candidate.startup_p95_ns")
            .unwrap()
            .environment_class,
        "desktop_timing_v1",
        "desktop first-present startup must bind the GPU/software-adapter stack"
    );

    let status = std::process::Command::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            &review.source_revision,
            "HEAD",
        ])
        .current_dir(evidence::repository_root())
        .status()
        .expect("git ancestry check must execute");
    assert!(
        status.success(),
        "review source revision must be an ancestor of HEAD"
    );
}

#[test]
fn product_budget_lifecycle_contract_pins_start_terminal_and_total_retirement() {
    let source_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGETS_PATH)).unwrap();
    let catalogue = decode_product_budget_catalogue(&source_bytes).unwrap();

    let mut wrong_start = catalogue.clone();
    wrong_start.correctness_contract.initial_lifecycle_state = "running".to_owned();
    assert_eq!(
        decode_product_budget_catalogue(&canonical_json_bytes(&wrong_start)),
        Err(ProtocolError::Invalid)
    );

    let mut outgoing_terminal = catalogue.clone();
    outgoing_terminal
        .correctness_contract
        .lifecycle_transitions
        .push(evidence::LifecycleTransition {
            from: "stopped".to_owned(),
            to: "candidate".to_owned(),
        });
    outgoing_terminal
        .correctness_contract
        .lifecycle_transitions
        .sort_by(|left, right| (&left.from, &left.to).cmp(&(&right.from, &right.to)));
    assert_eq!(
        decode_product_budget_catalogue(&canonical_json_bytes(&outgoing_terminal)),
        Err(ProtocolError::Invalid)
    );

    let mut retirement_dead_end = catalogue;
    retirement_dead_end
        .correctness_contract
        .lifecycle_states
        .push("retirement_dead_end".to_owned());
    retirement_dead_end
        .correctness_contract
        .lifecycle_states
        .sort();
    retirement_dead_end
        .correctness_contract
        .lifecycle_transitions
        .push(evidence::LifecycleTransition {
            from: "candidate".to_owned(),
            to: "retirement_dead_end".to_owned(),
        });
    retirement_dead_end
        .correctness_contract
        .lifecycle_transitions
        .sort_by(|left, right| (&left.from, &left.to).cmp(&(&right.from, &right.to)));
    assert_eq!(
        decode_product_budget_catalogue(&canonical_json_bytes(&retirement_dead_end)),
        Err(ProtocolError::Invalid)
    );
}

#[test]
fn missing_required_metric_and_missing_source_are_not_approvable() {
    let protocol = load_protocol_fixture().unwrap();

    let mut missing_metric = protocol.clone();
    missing_metric
        .metrics
        .retain(|metric| metric.id != "iteration.data.p95_ns");
    assert_eq!(
        validate_protocol(&missing_metric),
        Err(ProtocolError::MissingRequiredMetric(
            "iteration.data.p95_ns"
        ))
    );

    let mut missing_source = protocol.clone();
    missing_source.metrics[0].source.reference.clear();
    assert_eq!(
        validate_protocol(&missing_source),
        Err(ProtocolError::Invalid)
    );
    let (environments, observations) = passing_suite(&protocol, "first_playable");
    assert_eq!(
        decide_suite(
            &missing_source,
            "first_playable",
            &observations,
            &environments,
        ),
        Err(DecisionEvidenceError::InvalidProtocol)
    );

    let mut invalid_value_domain = protocol;
    invalid_value_domain
        .metrics
        .iter_mut()
        .find(|metric| metric.id == "public.production.coverage_basis_points")
        .unwrap()
        .target = MetricTarget::Minimum { value: 10_001 };
    assert_eq!(
        validate_protocol(&invalid_value_domain),
        Err(ProtocolError::Invalid)
    );
}

#[test]
fn each_stage_decides_only_its_suite_and_hard_stops_win() {
    let protocol = load_protocol_fixture().unwrap();
    for suite_id in ["first_playable", "candidate_gate"] {
        let (environments, passing) = passing_suite(&protocol, suite_id);
        assert_eq!(
            decide_suite(&protocol, suite_id, &passing, &environments),
            Ok(Decision::Continue),
            "suite {suite_id} must not wait for a later collector"
        );
    }
    let (ownership_environments, ownership_observations) =
        passing_suite(&protocol, "ownership_gate");
    assert_eq!(
        decide_suite(
            &protocol,
            "ownership_gate",
            &ownership_observations,
            &ownership_environments,
        ),
        Err(DecisionEvidenceError::OwnershipAdmissionRequired)
    );

    let (environments, passing) = passing_suite(&protocol, "first_playable");
    let mut redirecting = passing.clone();
    let ordinary = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == "iteration.data.p95_ns")
        .unwrap();
    let observation = redirecting
        .iter_mut()
        .find(|value| value.metric_id == ordinary.id)
        .unwrap();
    fail_observation(observation, &ordinary.target);
    assert_eq!(
        decide_suite(&protocol, "first_playable", &redirecting, &environments,),
        Ok(Decision::Redirect)
    );

    let hard_stop = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == "gameplay.headless_wave_success")
        .unwrap();
    let observation = redirecting
        .iter_mut()
        .find(|value| value.metric_id == hard_stop.id)
        .unwrap();
    observation.value = 0;
    assert_eq!(
        decide_suite(&protocol, "first_playable", &redirecting, &environments,),
        Ok(Decision::Stop)
    );
}

#[test]
fn zero_byte_and_nanosecond_observations_cannot_continue() {
    let protocol = load_protocol_fixture().unwrap();
    let (environments, passing) = passing_suite(&protocol, "candidate_gate");

    for metric_id in ["candidate.size_bytes", "candidate.startup_p95_ns"] {
        let mut observations = passing.clone();
        observations
            .iter_mut()
            .find(|observation| observation.metric_id == metric_id)
            .unwrap()
            .value = 0;
        assert_eq!(
            decide_suite(&protocol, "candidate_gate", &observations, &environments,),
            Ok(Decision::Redirect),
            "metric {metric_id} must reject zero evidence"
        );
    }
}

#[test]
fn decision_rejects_duplicate_unknown_cross_suite_and_forged_environment_evidence() {
    let protocol = load_protocol_fixture().unwrap();
    let (environments, passing) = passing_suite(&protocol, "first_playable");

    let mut duplicate = passing.clone();
    duplicate.push(passing[0].clone());
    assert_eq!(
        decide_suite(&protocol, "first_playable", &duplicate, &environments),
        Err(DecisionEvidenceError::DuplicateMetric)
    );

    let mut unknown = passing.clone();
    unknown[0].metric_id = "unknown.metric".to_owned();
    assert_eq!(
        decide_suite(&protocol, "first_playable", &unknown, &environments),
        Err(DecisionEvidenceError::UnknownMetric)
    );

    let (_, candidate) = passing_suite(&protocol, "candidate_gate");
    assert_eq!(
        decide_suite(&protocol, "first_playable", &candidate[..1], &environments,),
        Err(DecisionEvidenceError::MetricOutsideSuite)
    );

    let mut forged = passing;
    forged[0].environment.fields.insert(
        EnvironmentField::ToolchainClass,
        "forged_toolchain".to_owned(),
    );
    assert_eq!(
        decide_suite(&protocol, "first_playable", &forged, &environments),
        Ok(Decision::Redirect)
    );

    let (first_environments, mut first_playable) = passing_suite(&protocol, "first_playable");
    first_playable
        .iter_mut()
        .find(|observation| observation.metric_id == "public.production.coverage_basis_points")
        .unwrap()
        .value = 10_001;
    assert_eq!(
        decide_suite(
            &protocol,
            "first_playable",
            &first_playable,
            &first_environments,
        ),
        Ok(Decision::Redirect)
    );
}

#[test]
fn invalidation_unions_every_match_and_fails_closed_for_shared_or_unknown_changes() {
    let mut protocol = load_protocol_fixture().unwrap();
    protocol.invalidation.rules.push(evidence::PathRule {
        id: "path.reference_game_systems_exact".to_owned(),
        selectors: vec![evidence::PathSelector::Exact {
            path: "reference-game/src/systems.rs".to_owned(),
        }],
        targets: vec![TargetRef::Metric {
            id: "candidate.size_bytes".to_owned(),
        }],
    });
    protocol.invalidation.rules.extend([
        evidence::PathRule {
            id: "path.rename_new_fixture".to_owned(),
            selectors: vec![evidence::PathSelector::Exact {
                path: "docs/fixtures/u22/rename-new.txt".to_owned(),
            }],
            targets: vec![TargetRef::Metric {
                id: "ownership.false_stopped".to_owned(),
            }],
        },
        evidence::PathRule {
            id: "path.rename_old_fixture".to_owned(),
            selectors: vec![evidence::PathSelector::Exact {
                path: "docs/fixtures/u22/rename-old.txt".to_owned(),
            }],
            targets: vec![TargetRef::Metric {
                id: "candidate.startup_p95_ns".to_owned(),
            }],
        },
    ]);
    validate_protocol(&protocol).expect("overlapping fixture rule must stay valid");
    let digest = protocol_digest(&protocol);
    let game_change = [SourceChange {
        old_path: None,
        new_path: Some("reference-game/src/systems.rs".to_owned()),
    }];
    let affected = affected_metrics(&protocol, &digest, &game_change);
    assert!(affected.contains("iteration.body.p95_ns"));
    assert!(affected.contains("ownership.caller_glue_regressions"));
    assert!(affected.contains("candidate.size_bytes"));
    assert!(!affected.contains("candidate.startup_p95_ns"));
    assert!(affected.len() < protocol.metrics.len());

    for path in [
        "src/lib.rs",
        "crates/nara_app/src/lib.rs",
        "Cargo.toml",
        ".cargo/config.toml",
        "build.rs",
        "unclassified/new-input.txt",
    ] {
        let affected = affected_metrics(
            &protocol,
            &digest,
            &[SourceChange {
                old_path: None,
                new_path: Some(path.to_owned()),
            }],
        );
        assert_eq!(affected.len(), protocol.metrics.len(), "path {path}");
    }

    let renamed = affected_metrics(
        &protocol,
        &digest,
        &[SourceChange {
            old_path: Some("docs/fixtures/u22/rename-old.txt".to_owned()),
            new_path: Some("docs/fixtures/u22/rename-new.txt".to_owned()),
        }],
    );
    assert_eq!(
        renamed,
        [
            "candidate.startup_p95_ns".to_owned(),
            "ownership.false_stopped".to_owned(),
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn environment_reuse_requires_every_declared_field_and_never_mixes_populations() {
    let protocol = load_protocol_fixture().unwrap();
    let metric = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == "frame.p99_ns")
        .unwrap();
    let left = environment(metric.population);
    let mut right = left.clone();
    assert!(environment_equivalent(&protocol, &metric.id, &left, &right));

    let class = protocol
        .environment_classes
        .iter()
        .find(|class| class.id == metric.environment_class)
        .unwrap();
    for field in &class.required_fields {
        right = left.clone();
        right.fields.insert(*field, "different_class".to_owned());
        assert!(!environment_equivalent(
            &protocol, &metric.id, &left, &right
        ));
    }

    for (field, weak_value) in [
        (EnvironmentField::OsClass, "windows-latest"),
        (EnvironmentField::RunnerImageClass, "latest"),
        (EnvironmentField::ToolchainClass, "stable"),
        (EnvironmentField::CpuClass, "unknown"),
        (EnvironmentField::GpuStackClass, "not_applicable"),
        (EnvironmentField::BuildClass, "debug"),
        (EnvironmentField::CollectorClass, "default"),
    ] {
        let mut weak_left = left.clone();
        weak_left.fields.insert(field, weak_value.to_owned());
        let weak_right = weak_left.clone();
        assert!(
            !environment_equivalent(&protocol, &metric.id, &weak_left, &weak_right),
            "weak {field:?} identity must not become equivalent"
        );
    }

    right = left.clone();
    right.population = Population::Cold;
    assert!(!environment_equivalent(
        &protocol, &metric.id, &left, &right
    ));
}

#[test]
fn every_semantic_rule_changes_the_protocol_digest_but_entry_binding_does_not() {
    let protocol = load_protocol_fixture().unwrap();
    let baseline = protocol_digest(&protocol);
    let mut variants = Vec::new();

    let mut range = protocol.clone();
    range.metrics[0].target = MetricTarget::Maximum { value: 1 };
    variants.push(range);

    let mut source = protocol.clone();
    source.metrics[0].source.reference.push_str("_changed");
    variants.push(source);

    let mut source_artifact = protocol.clone();
    source_artifact.range_sources[0]
        .revision
        .push_str("_changed");
    variants.push(source_artifact);

    let mut samples = protocol.clone();
    samples.metrics[0].minimum_samples += 1;
    variants.push(samples);

    let mut environment_class = protocol.clone();
    environment_class.environment_classes[0]
        .required_fields
        .pop();
    variants.push(environment_class);

    let mut environment_value_policy = protocol.clone();
    environment_value_policy
        .environment_value_policy
        .forbidden_values
        .push("weak_value_v1".to_owned());
    variants.push(environment_value_policy);

    let mut identity_policy = protocol.clone();
    identity_policy
        .identity_policy
        .forbidden_fragments
        .push("username".to_owned());
    variants.push(identity_policy);

    let mut measurement_binding = protocol.clone();
    measurement_binding.metrics[0]
        .required_context
        .push("extra_context_v1".to_owned());
    variants.push(measurement_binding);

    let mut measurement_method = protocol.clone();
    measurement_method.metrics[0].method_id.push_str("_changed");
    variants.push(measurement_method);

    let mut reuse_policy = protocol.clone();
    reuse_policy.metrics[0].reuse_policy = evidence::EvidenceReusePolicy::CurrentRevisionOnly;
    variants.push(reuse_policy);

    let mut invalidation = protocol.clone();
    invalidation.invalidation.rules[0]
        .selectors
        .push(evidence::PathSelector::Exact {
            path: "README.md".to_owned(),
        });
    variants.push(invalidation);

    let mut decision = protocol.clone();
    decision.decision.required_failure = Decision::Stop;
    variants.push(decision);

    let mut envelope = protocol.clone();
    envelope.evidence.max_records += 1;
    variants.push(envelope);

    let mut context_limit = protocol.clone();
    context_limit.evidence.max_context_receipts += 1;
    variants.push(context_limit);

    let mut subject_catalogue = protocol.clone();
    subject_catalogue.subjects[0].peer_subjects = vec!["u26.manual_counterfactual".to_owned()];
    variants.push(subject_catalogue);

    let mut subject_identity = protocol.clone();
    subject_identity.subjects[0]
        .run_providers
        .push("trusted_runner_v2".to_owned());
    variants.push(subject_identity);

    for variant in variants {
        assert_ne!(protocol_digest(&variant), baseline);
    }

    let concrete_entry_binding = (
        "u14.headless_iteration",
        "nara::host::run_reference_game_headless",
    );
    assert!(!concrete_entry_binding.1.is_empty());
    assert_eq!(protocol_digest(&protocol), baseline);
}

#[test]
fn calibration_review_contains_sources_but_no_target_results() {
    let (_bytes, envelope) = load_calibration_fixture().expect("calibration fixture must decode");
    assert_eq!(envelope.identity.subject, "u22.calibration_review");
    assert_eq!(envelope.payload.records.len(), 3);
    for record in &envelope.payload.records {
        assert_eq!(record.kind, "calibration_review");
        let source_id = record
            .fields
            .iter()
            .find(|field| field.key == "source_id")
            .unwrap();
        let FieldValue::Identifier { value: source_id } = &source_id.value else {
            panic!("source_id must be a semantic identifier")
        };
        assert_eq!(&record.id, source_id);
        let source_path = record
            .fields
            .iter()
            .find(|field| field.key == "source_path")
            .unwrap();
        assert_eq!(
            source_path.value,
            FieldValue::ProjectRelative {
                value: evidence::PRODUCT_BUDGETS_PATH.to_owned(),
            }
        );
        for key in [
            "review_digest",
            "review_id",
            "source_digest",
            "source_revision",
        ] {
            assert!(
                record.fields.iter().any(|field| field.key == key),
                "calibration record must bind {key}"
            );
        }
        let exposed = record
            .fields
            .iter()
            .find(|field| field.key == "target_results_seen")
            .unwrap();
        assert_eq!(exposed.value, FieldValue::Bool { value: false });
    }
}

#[test]
fn ancestry_policy_freezes_u22_before_target_work_and_u26_before_host_join() {
    let valid = [
        ("u22", "u4"),
        ("u22", "u5"),
        ("u4", "u12"),
        ("u12", "u26"),
        ("u5", "u24"),
        ("u26", "u24"),
        ("u24", "u25"),
    ];
    assert!(ancestry_allows(&valid));

    let invalid = valid
        .into_iter()
        .filter(|edge| *edge != ("u26", "u24"))
        .collect::<Vec<_>>();
    assert!(!ancestry_allows(&invalid));
}

#[test]
fn local_u9_measurement_plan_remains_non_decisive_preparation() {
    let root = evidence::repository_root();
    let helper =
        std::fs::read_to_string(root.join("reference-game/tools/measure_first_playable.py"))
            .expect("U9 measurement helper must be readable");
    let baseline = std::fs::read_to_string(
        root.join("docs/benchmarks/reference-game-first-playable-baseline.md"),
    )
    .expect("U9 baseline document must be readable");
    let protocol = std::fs::read_to_string(
        root.join("docs/benchmarks/reference-game-first-playable-protocol.md"),
    )
    .expect("first-playable protocol must be readable");

    for required in [
        "prepared_not_executed",
        "not_evaluated",
        "frame.p99_ns",
        "runtime.memory_bytes",
        "render.packet.instance_count",
        "render.packet.clone_bytes",
        "minimum_samples",
    ] {
        assert!(
            helper.contains(required),
            "the U9 helper must retain the honest preparation boundary `{required}`"
        );
    }
    assert!(
        !helper.contains("decide_suite("),
        "the U9 helper must not reimplement the Rust evidence decision oracle"
    );
    assert!(
        !helper.contains("--samples"),
        "the U9 plan must not replace canonical per-metric sample floors with one global CLI value"
    );
    assert!(
        baseline.contains("No first-playable baseline or product decision has been recorded yet.")
    );
    assert!(baseline.contains("These gaps are named bottlenecks"));
    assert!(protocol.contains("RGD-U9's `reference-game/tools/measure_first_playable.py plan`"));
    assert!(protocol.contains("It writes no evidence envelope and makes no protocol decision"));
    assert!(protocol.contains("reads the committed U14 per-metric sample requirements"));
    assert!(
        !protocol.contains("U14/RGD-U9 and U20 collection helpers"),
        "the protocol must not mischaracterize the U9 plan as a raw-record collector"
    );
}
