#[path = "support/first_playable_evidence.rs"]
mod evidence;

use evidence::{
    Decision, EnvelopeLimits, EvidenceAggregationError, EvidenceEnvelope, EvidenceError,
    ExpectedEvidenceIdentity, ExpectedTransfer, FieldValue, TransferEntry, TransferEntryKind,
    TrustedEvidenceStore, ValidatedEvidence, aggregate_validated_observations,
    calibration_expected_identity, canonical_json_bytes, decide_validated_suite, decode_evidence,
    expected_transfer, ingest_evidence, load_calibration_fixture, load_protocol_fixture,
    preflight_transfer_table, refresh_payload_digest, reset_typed_decode_count, typed_decode_count,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

type LimitSetter = fn(&mut EnvelopeLimits, usize);
type EnvelopeMutation = fn(&mut EvidenceEnvelope);
type IdentityMutation = fn(&mut EvidenceEnvelope, &mut ExpectedEvidenceIdentity);

fn fixture() -> (
    evidence::FirstPlayableProtocol,
    Vec<u8>,
    EvidenceEnvelope,
    ExpectedTransfer,
    ExpectedEvidenceIdentity,
) {
    let protocol = load_protocol_fixture().expect("protocol fixture must be valid");
    let (bytes, envelope) = load_calibration_fixture().expect("calibration envelope must decode");
    let transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, &bytes);
    let identity = calibration_expected_identity(&protocol);
    (protocol, bytes, envelope, transfer, identity)
}

fn encode(envelope: &EvidenceEnvelope) -> (Vec<u8>, ExpectedTransfer) {
    let bytes = canonical_json_bytes(envelope);
    let transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, &bytes);
    (bytes, transfer)
}

fn transfer_table(transfer: &ExpectedTransfer) -> Vec<TransferEntry> {
    vec![TransferEntry {
        path: transfer.path.clone(),
        kind: TransferEntryKind::Regular,
        encoded_bytes: usize::try_from(transfer.digest.bytes).expect("fixture size must fit usize"),
    }]
}

fn identifier_field(key: &str, value: &str) -> evidence::EvidenceField {
    evidence::EvidenceField {
        key: key.to_owned(),
        value: FieldValue::Identifier {
            value: value.to_owned(),
        },
    }
}

fn u64_field(key: &str, value: u64) -> evidence::EvidenceField {
    evidence::EvidenceField {
        key: key.to_owned(),
        value: FieldValue::U64 { value },
    }
}

fn metric_sample_record(
    protocol: &evidence::FirstPlayableProtocol,
    metric_id: &str,
    population: &str,
    sample_index: u64,
    value: u64,
) -> evidence::EvidenceRecord {
    let metric = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == metric_id)
        .unwrap();
    evidence::EvidenceRecord {
        kind: "metric_sample".to_owned(),
        id: format!("{metric_id}.sample_{sample_index:06}"),
        fields: vec![
            identifier_field("end_boundary_id", &metric.end_boundary_id),
            identifier_field("method_id", &metric.method_id),
            identifier_field("metric_id", metric_id),
            identifier_field("population", population),
            u64_field("sample_index", sample_index),
            identifier_field("start_boundary_id", &metric.start_boundary_id),
            u64_field("value", value),
            identifier_field("workload_id", &metric.workload_id),
        ],
    }
}

fn ownership_inventory_record(
    protocol: &evidence::FirstPlayableProtocol,
    metric_id: &str,
    peer_subject: &str,
    value: u64,
) -> evidence::EvidenceRecord {
    let metric = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == metric_id)
        .unwrap();
    evidence::EvidenceRecord {
        kind: "ownership_inventory".to_owned(),
        id: metric_id.to_owned(),
        fields: vec![
            identifier_field("end_boundary_id", &metric.end_boundary_id),
            identifier_field("method_id", &metric.method_id),
            identifier_field("metric_id", metric_id),
            identifier_field("peer_subject", peer_subject),
            identifier_field("start_boundary_id", &metric.start_boundary_id),
            u64_field("value", value),
            identifier_field("workload_id", &metric.workload_id),
        ],
    }
}

fn bind_expected_context(
    protocol: &evidence::FirstPlayableProtocol,
    envelope: &mut EvidenceEnvelope,
    expected: &mut ExpectedEvidenceIdentity,
) {
    for record in &mut envelope.payload.records {
        if !matches!(
            record.kind.as_str(),
            "metric_sample" | "ownership_inventory"
        ) {
            continue;
        }
        let metric_id = record
            .fields
            .iter()
            .find_map(|field| match (&*field.key, &field.value) {
                ("metric_id", FieldValue::Identifier { value }) => Some(value.as_str()),
                _ => None,
            })
            .unwrap();
        let metric = protocol
            .metrics
            .iter()
            .find(|metric| metric.id == metric_id)
            .unwrap();
        for (key, value) in [
            ("end_boundary_id", metric.end_boundary_id.as_str()),
            ("method_id", metric.method_id.as_str()),
            ("start_boundary_id", metric.start_boundary_id.as_str()),
            ("workload_id", metric.workload_id.as_str()),
        ] {
            if record.fields.iter().all(|field| field.key != key) {
                record.fields.push(identifier_field(key, value));
            }
        }
        record
            .fields
            .sort_by(|left, right| left.key.cmp(&right.key));
    }

    let subject = protocol
        .subjects
        .iter()
        .find(|subject| subject.id == envelope.identity.subject)
        .unwrap();
    envelope.generator = subject.generator.clone();
    envelope.identity.run_provider = subject.run_providers[0].clone();
    expected.generator = envelope.generator.clone();
    expected.identity.run_provider = envelope.identity.run_provider.clone();
    envelope.identity.protocol_digest = evidence::protocol_digest(protocol);
    expected.identity.protocol_digest = envelope.identity.protocol_digest.clone();

    let mut context_ids = subject
        .required_context
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for record in &envelope.payload.records {
        let Some(metric_id) = record.fields.iter().find_map(|field| {
            if field.key != "metric_id" {
                return None;
            }
            match &field.value {
                FieldValue::Identifier { value } => Some(value.as_str()),
                _ => None,
            }
        }) else {
            continue;
        };
        let metric = protocol
            .metrics
            .iter()
            .find(|metric| metric.id == metric_id)
            .unwrap();
        if metric.subject == subject.id {
            context_ids.extend(metric.required_context.iter().cloned());
        }
    }
    let receipts = context_ids
        .into_iter()
        .map(|id| evidence::ContextReceipt {
            digest: evidence::digest_ref(format!("context:{id}").as_bytes()),
            id,
        })
        .collect::<Vec<_>>();
    envelope.payload.context_receipts = receipts.clone();
    expected.context_receipts = receipts;
}

fn validated_records(
    protocol: &evidence::FirstPlayableProtocol,
    base_envelope: &EvidenceEnvelope,
    base_identity: &ExpectedEvidenceIdentity,
    subject: &str,
    environment_class: &str,
    records: Vec<evidence::EvidenceRecord>,
) -> ValidatedEvidence {
    validated_records_with_context(
        protocol,
        base_envelope,
        base_identity,
        subject,
        environment_class,
        records,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn validated_records_at_revision(
    protocol: &evidence::FirstPlayableProtocol,
    base_envelope: &EvidenceEnvelope,
    base_identity: &ExpectedEvidenceIdentity,
    source_revision: &str,
    subject: &str,
    environment_class: &str,
    records: Vec<evidence::EvidenceRecord>,
) -> ValidatedEvidence {
    let mut envelope = base_envelope.clone();
    envelope.identity.source_revision = source_revision.to_owned();
    let mut identity = base_identity.clone();
    identity.identity.source_revision = source_revision.to_owned();
    validated_records(
        protocol,
        &envelope,
        &identity,
        subject,
        environment_class,
        records,
    )
}

struct GitRevisionFixture {
    root: PathBuf,
    prior_revision: String,
    current_revision: String,
}

impl GitRevisionFixture {
    fn new(changed_path: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        let root = std::env::temp_dir().join(format!(
            "nara-u22-revision-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "u22@example.invalid"]);
        git(&root, &["config", "user.name", "U22 Test"]);

        let changed_file = root.join(changed_path);
        std::fs::create_dir_all(changed_file.parent().unwrap()).unwrap();
        std::fs::write(&changed_file, "before\n").unwrap();
        git(&root, &["add", "--all"]);
        git(&root, &["commit", "--quiet", "-m", "baseline"]);
        let prior_revision = git_output(&root, &["rev-parse", "HEAD"]);

        std::fs::write(&changed_file, "after\n").unwrap();
        git(&root, &["add", "--all"]);
        git(&root, &["commit", "--quiet", "-m", "candidate"]);
        let current_revision = git_output(&root, &["rev-parse", "HEAD"]);

        Self {
            root,
            prior_revision,
            current_revision,
        }
    }
}

impl Drop for GitRevisionFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git command failed: {arguments:?}");
}

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git command failed: {arguments:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[allow(clippy::too_many_arguments)]
fn validated_records_with_context(
    protocol: &evidence::FirstPlayableProtocol,
    base_envelope: &EvidenceEnvelope,
    base_identity: &ExpectedEvidenceIdentity,
    subject: &str,
    environment_class: &str,
    records: Vec<evidence::EvidenceRecord>,
    context_overrides: &[(&str, evidence::DigestRef)],
) -> ValidatedEvidence {
    let mut envelope = base_envelope.clone();
    envelope.identity.subject = subject.to_owned();
    envelope.identity.environment_class = environment_class.to_owned();
    envelope.payload.records = records;
    let mut identity = base_identity.clone();
    identity.identity.subject = subject.to_owned();
    identity.identity.environment_class = environment_class.to_owned();
    bind_expected_context(protocol, &mut envelope, &mut identity);
    for (id, digest) in context_overrides {
        let receipt = envelope
            .payload
            .context_receipts
            .iter_mut()
            .find(|receipt| receipt.id == *id)
            .unwrap();
        receipt.digest = digest.clone();
        identity.context_receipts = envelope.payload.context_receipts.clone();
    }
    refresh_payload_digest(&mut envelope);
    let (bytes, transfer) = encode(&envelope);
    let entries = transfer_table(&transfer);
    let mut store = TrustedEvidenceStore::default();
    ingest_evidence(
        &bytes,
        &entries,
        EnvelopeLimits::from(&protocol.evidence),
        &transfer,
        &identity,
        protocol,
        &mut store,
    )
    .expect("test records must pass trusted publication");
    store.published()[0].clone()
}

fn minimum_passing_limit(
    maximum: usize,
    base: EnvelopeLimits,
    set_limit: impl Fn(&mut EnvelopeLimits, usize),
    bytes: &[u8],
    transfer: &ExpectedTransfer,
    identity: &ExpectedEvidenceIdentity,
    protocol: &evidence::FirstPlayableProtocol,
) -> usize {
    let passes = |value| {
        let mut limits = base;
        set_limit(&mut limits, value);
        decode_evidence(bytes, limits, transfer, identity, protocol).is_ok()
    };
    assert!(
        maximum > 0 && passes(maximum),
        "the committed policy limit must admit its fixture"
    );

    let mut rejected = 0;
    let mut admitted = maximum;
    while rejected + 1 < admitted {
        let candidate = rejected + (admitted - rejected) / 2;
        if passes(candidate) {
            admitted = candidate;
        } else {
            rejected = candidate;
        }
    }
    admitted
}

#[test]
fn valid_envelope_is_canonical_identity_bound_and_publishes_once() {
    let (protocol, bytes, envelope, transfer, identity) = fixture();
    assert_eq!(canonical_json_bytes(&envelope), bytes);
    assert_eq!(
        envelope.payload_digest,
        evidence::digest_ref(&serde_json::to_vec(&envelope.payload).unwrap()),
        "payload digest must bind compact canonical payload bytes"
    );

    let mut store = TrustedEvidenceStore::default();
    let entries = transfer_table(&transfer);
    ingest_evidence(
        &bytes,
        &entries,
        EnvelopeLimits::from(&protocol.evidence),
        &transfer,
        &identity,
        &protocol,
        &mut store,
    )
    .expect("valid evidence must publish");
    assert_eq!(store.published_count(), 1);
}

#[test]
fn encoded_shape_and_domain_limits_accept_exactly_and_reject_limit_plus_one() {
    let (protocol, bytes, envelope, transfer, identity) = fixture();
    let base = EnvelopeLimits::from(&protocol.evidence);

    let mut exact = base;
    exact.max_encoded_bytes = bytes.len();
    assert!(decode_evidence(&bytes, exact, &transfer, &identity, &protocol).is_ok());
    exact.max_encoded_bytes -= 1;
    reset_typed_decode_count();
    assert_eq!(
        decode_evidence(&bytes, exact, &transfer, &identity, &protocol),
        Err(EvidenceError::EncodedBytes)
    );
    assert_eq!(typed_decode_count(), 0);

    let shape_dimensions: &[(usize, LimitSetter)] = &[
        (base.max_depth, |limits, value| limits.max_depth = value),
        (base.max_nodes, |limits, value| limits.max_nodes = value),
        (base.max_container_items, |limits, value| {
            limits.max_container_items = value;
        }),
        (base.max_string_bytes, |limits, value| {
            limits.max_string_bytes = value;
        }),
        (base.max_total_string_bytes, |limits, value| {
            limits.max_total_string_bytes = value;
        }),
    ];
    for (maximum, set_limit) in shape_dimensions {
        let minimum = minimum_passing_limit(
            *maximum, base, *set_limit, &bytes, &transfer, &identity, &protocol,
        );
        assert!(minimum > 1);
        let mut below = base;
        set_limit(&mut below, minimum - 1);
        reset_typed_decode_count();
        assert_eq!(
            decode_evidence(&bytes, below, &transfer, &identity, &protocol),
            Err(EvidenceError::Shape)
        );
        assert_eq!(typed_decode_count(), 0);
    }

    let actual_records = envelope.payload.records.len();
    let actual_max_fields = envelope
        .payload
        .records
        .iter()
        .map(|record| record.fields.len())
        .max()
        .unwrap();
    let actual_total_fields = envelope.payload.environment.len()
        + envelope
            .payload
            .records
            .iter()
            .map(|record| record.fields.len())
            .sum::<usize>();
    for (exact_value, set_limit) in [
        (
            actual_records,
            (|limits: &mut EnvelopeLimits, value| limits.max_records = value)
                as fn(&mut EnvelopeLimits, usize),
        ),
        (
            actual_max_fields,
            (|limits: &mut EnvelopeLimits, value| limits.max_fields_per_record = value)
                as fn(&mut EnvelopeLimits, usize),
        ),
        (
            actual_total_fields,
            (|limits: &mut EnvelopeLimits, value| limits.max_total_fields = value)
                as fn(&mut EnvelopeLimits, usize),
        ),
    ] {
        let mut exact_limits = base;
        set_limit(&mut exact_limits, exact_value);
        assert!(decode_evidence(&bytes, exact_limits, &transfer, &identity, &protocol).is_ok());
        let mut below = base;
        set_limit(&mut below, exact_value - 1);
        reset_typed_decode_count();
        assert_eq!(
            decode_evidence(&bytes, below, &transfer, &identity, &protocol),
            Err(EvidenceError::DomainLimit)
        );
        assert_eq!(typed_decode_count(), 0);
    }

    let mut with_raw_log = envelope;
    with_raw_log
        .restricted_raw_logs
        .push(evidence::RestrictedRawLogRef {
            artifact_id: "u22_restricted_log_001".to_owned(),
            digest: evidence::digest_ref(b"restricted log fixture"),
            retention_until_unix_seconds: 1_800_000_000,
        });
    let mut raw_log_identity = identity;
    raw_log_identity.restricted_raw_logs = with_raw_log.restricted_raw_logs.clone();
    let (raw_log_bytes, raw_log_transfer) = encode(&with_raw_log);
    let mut exact_raw_logs = base;
    exact_raw_logs.max_raw_log_refs = 1;
    assert!(
        decode_evidence(
            &raw_log_bytes,
            exact_raw_logs,
            &raw_log_transfer,
            &raw_log_identity,
            &protocol,
        )
        .is_ok()
    );
    let mut below_raw_logs = base;
    below_raw_logs.max_raw_log_refs = 0;
    reset_typed_decode_count();
    assert_eq!(
        decode_evidence(
            &raw_log_bytes,
            below_raw_logs,
            &raw_log_transfer,
            &raw_log_identity,
            &protocol,
        ),
        Err(EvidenceError::DomainLimit)
    );
    assert_eq!(typed_decode_count(), 0);
}

#[test]
fn duplicate_and_unknown_fields_reject_without_publication() {
    let (protocol, bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);
    let text = String::from_utf8(bytes).unwrap();
    let duplicate = text.replacen(
        "  \"kind\": \"nara.evidence\",",
        "  \"kind\": \"nara.evidence\",\n  \"kind\": \"nara.evidence\",",
        1,
    );
    let duplicate = duplicate.into_bytes();
    let duplicate_transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, &duplicate);
    reset_typed_decode_count();
    assert_eq!(
        decode_evidence(
            &duplicate,
            limits,
            &duplicate_transfer,
            &identity,
            &protocol
        ),
        Err(EvidenceError::Shape)
    );
    assert_eq!(typed_decode_count(), 0);

    let mut unknown = serde_json::to_value(&envelope).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unknown".to_owned(), serde_json::Value::Bool(true));
    let mut unknown_bytes = serde_json::to_vec_pretty(&unknown).unwrap();
    unknown_bytes.push(b'\n');
    let unknown_transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, &unknown_bytes);
    let mut store = TrustedEvidenceStore::default();
    let entries = transfer_table(&unknown_transfer);
    assert_eq!(
        ingest_evidence(
            &unknown_bytes,
            &entries,
            limits,
            &unknown_transfer,
            &identity,
            &protocol,
            &mut store,
        ),
        Err(EvidenceError::Decode)
    );
    assert_eq!(store.published_count(), 0);

    let mut incomplete = envelope;
    incomplete.payload.records.pop();
    refresh_payload_digest(&mut incomplete);
    let (incomplete_bytes, incomplete_transfer) = encode(&incomplete);
    assert_eq!(
        decode_evidence(
            &incomplete_bytes,
            limits,
            &incomplete_transfer,
            &identity,
            &protocol,
        ),
        Err(EvidenceError::Catalogue)
    );
}

#[test]
fn every_self_reported_identity_environment_and_raw_log_field_is_trust_bound() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    let cases: &[(&str, EnvelopeMutation)] = &[
        ("generator", |value| {
            value.generator = "forged_generator".to_owned();
        }),
        ("run_provider", |value| {
            value.identity.run_provider = "forged".to_owned();
        }),
        ("run_id", |value| {
            value.identity.run_id = "forged".to_owned();
        }),
        ("run_attempt", |value| {
            value.identity.run_attempt += 1;
        }),
        ("repository", |value| {
            value.identity.repository = "forged/repository".to_owned();
        }),
        ("source_revision", |value| {
            value.identity.source_revision = "a".repeat(40);
        }),
        ("protocol_digest", |value| {
            value.identity.protocol_digest.blake3 = "a".repeat(64);
        }),
        ("subject", |value| {
            value.identity.subject = "u14.desktop_product".to_owned();
        }),
        ("environment_class", |value| {
            value.identity.environment_class = "desktop_timing_v1".to_owned();
        }),
        ("environment", |value| {
            value.payload.environment[0].value = FieldValue::Identifier {
                value: "forged_environment".to_owned(),
            };
        }),
        ("restricted_raw_logs", |value| {
            value
                .restricted_raw_logs
                .push(evidence::RestrictedRawLogRef {
                    artifact_id: "forged_log".to_owned(),
                    digest: evidence::digest_ref(b"forged log"),
                    retention_until_unix_seconds: 1_800_000_000,
                });
        }),
    ];
    for (name, mutate) in cases {
        let mut forged = envelope.clone();
        mutate(&mut forged);
        let (bytes, transfer) = encode(&forged);
        assert_eq!(
            decode_evidence(&bytes, limits, &transfer, &identity, &protocol),
            Err(EvidenceError::Identity),
            "identity case {name}"
        );
    }

    let mut weak_envelope = envelope;
    weak_envelope
        .payload
        .environment
        .iter_mut()
        .find(|field| field.key == "build_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "debug".to_owned(),
    };
    refresh_payload_digest(&mut weak_envelope);
    let mut weak_identity = identity;
    weak_identity
        .environment
        .iter_mut()
        .find(|field| field.key == "build_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "debug".to_owned(),
    };
    let (bytes, transfer) = encode(&weak_envelope);
    assert_eq!(
        decode_evidence(&bytes, limits, &transfer, &weak_identity, &protocol,),
        Err(EvidenceError::Identity)
    );
}

#[test]
fn calibration_records_cannot_drift_from_reviewed_source_bindings() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    for key in [
        "review_digest",
        "review_id",
        "source_digest",
        "source_revision",
    ] {
        let mut forged = envelope.clone();
        let field = forged.payload.records[0]
            .fields
            .iter_mut()
            .find(|field| field.key == key)
            .unwrap();
        field.value = match field.value {
            FieldValue::Digest { .. } => FieldValue::Digest {
                value: evidence::digest_ref(b"forged binding"),
            },
            FieldValue::Identifier { .. } => FieldValue::Identifier {
                value: "forged_binding".to_owned(),
            },
            _ => unreachable!(),
        };
        refresh_payload_digest(&mut forged);
        let (bytes, transfer) = encode(&forged);
        assert_eq!(
            decode_evidence(&bytes, limits, &transfer, &identity, &protocol),
            Err(EvidenceError::Catalogue),
            "binding {key}"
        );
    }
}

#[test]
fn subject_catalogue_closes_record_metric_population_and_peer_domains() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    let mut metric_envelope = envelope.clone();
    metric_envelope.identity.subject = "u14.headless_iteration".to_owned();
    metric_envelope.identity.environment_class = "cpu_timing_v1".to_owned();
    metric_envelope.payload.records = vec![evidence::EvidenceRecord {
        kind: "metric_sample".to_owned(),
        id: "iteration.data.p50_ns.sample_000001".to_owned(),
        fields: vec![
            identifier_field("metric_id", "iteration.data.p50_ns"),
            identifier_field("population", "warm"),
            u64_field("sample_index", 1),
            u64_field("value", 1_000_000_000),
        ],
    }];
    let mut metric_identity = identity.clone();
    metric_identity.identity.subject = metric_envelope.identity.subject.clone();
    metric_identity.identity.environment_class = metric_envelope.identity.environment_class.clone();
    bind_expected_context(&protocol, &mut metric_envelope, &mut metric_identity);
    refresh_payload_digest(&mut metric_envelope);
    let (metric_bytes, metric_transfer) = encode(&metric_envelope);
    assert!(
        decode_evidence(
            &metric_bytes,
            limits,
            &metric_transfer,
            &metric_identity,
            &protocol,
        )
        .is_ok()
    );

    let mut wrong_environment = metric_envelope.clone();
    wrong_environment.identity.environment_class = "portable_correctness_v1".to_owned();
    let mut wrong_environment_identity = metric_identity.clone();
    wrong_environment_identity.identity.environment_class =
        wrong_environment.identity.environment_class.clone();
    let (bytes, transfer) = encode(&wrong_environment);
    assert_eq!(
        decode_evidence(
            &bytes,
            limits,
            &transfer,
            &wrong_environment_identity,
            &protocol,
        ),
        Err(EvidenceError::Catalogue)
    );

    let mut cross_subject = metric_envelope.clone();
    cross_subject.identity.subject = "u20.release_candidate".to_owned();
    cross_subject.identity.environment_class = "portable_correctness_v1".to_owned();
    let mut cross_subject_identity = metric_identity.clone();
    cross_subject_identity.identity.subject = cross_subject.identity.subject.clone();
    cross_subject_identity.identity.environment_class =
        cross_subject.identity.environment_class.clone();
    let (bytes, transfer) = encode(&cross_subject);
    assert_eq!(
        decode_evidence(
            &bytes,
            limits,
            &transfer,
            &cross_subject_identity,
            &protocol,
        ),
        Err(EvidenceError::Catalogue)
    );

    for (key, value) in [
        ("metric_id", "frankorz_private_value"),
        ("population", "cold"),
    ] {
        let mut invalid = metric_envelope.clone();
        invalid.payload.records[0]
            .fields
            .iter_mut()
            .find(|field| field.key == key)
            .unwrap()
            .value = FieldValue::Identifier {
            value: value.to_owned(),
        };
        refresh_payload_digest(&mut invalid);
        let (bytes, transfer) = encode(&invalid);
        let error = decode_evidence(&bytes, limits, &transfer, &metric_identity, &protocol)
            .expect_err("unknown metric or mismatched population must reject");
        assert_eq!(error, EvidenceError::Catalogue);
        assert!(!format!("{error:?} {error}").contains(value));
    }

    for (metric_id, population, value) in [
        ("gameplay.headless_wave_success", "not_applicable", 2),
        (
            "public.production.coverage_basis_points",
            "not_applicable",
            10_001,
        ),
    ] {
        let mut invalid = metric_envelope.clone();
        invalid.identity.environment_class = "portable_correctness_v1".to_owned();
        invalid.payload.records = vec![evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: format!("{metric_id}.sample_000001"),
            fields: vec![
                identifier_field("metric_id", metric_id),
                identifier_field("population", population),
                u64_field("sample_index", 1),
                u64_field("value", value),
            ],
        }];
        let mut expected = metric_identity.clone();
        expected.identity.environment_class = invalid.identity.environment_class.clone();
        bind_expected_context(&protocol, &mut invalid, &mut expected);
        refresh_payload_digest(&mut invalid);
        let (bytes, transfer) = encode(&invalid);
        assert_eq!(
            decode_evidence(&bytes, limits, &transfer, &expected, &protocol),
            Err(EvidenceError::Catalogue),
            "metric {metric_id} value domain"
        );
    }

    let mut ownership_envelope = envelope;
    ownership_envelope.identity.subject = "u26.manual_counterfactual".to_owned();
    ownership_envelope.identity.environment_class = "paired_ownership_v1".to_owned();
    ownership_envelope.payload.records = vec![evidence::EvidenceRecord {
        kind: "ownership_inventory".to_owned(),
        id: "ownership.false_stopped".to_owned(),
        fields: vec![
            identifier_field("metric_id", "ownership.false_stopped"),
            identifier_field("peer_subject", "u25.ownership_comparison"),
            u64_field("value", 0),
        ],
    }];
    let mut ownership_identity = identity;
    ownership_identity.identity.subject = ownership_envelope.identity.subject.clone();
    ownership_identity.identity.environment_class =
        ownership_envelope.identity.environment_class.clone();
    bind_expected_context(&protocol, &mut ownership_envelope, &mut ownership_identity);
    refresh_payload_digest(&mut ownership_envelope);
    let (ownership_bytes, ownership_transfer) = encode(&ownership_envelope);
    assert!(
        decode_evidence(
            &ownership_bytes,
            limits,
            &ownership_transfer,
            &ownership_identity,
            &protocol,
        )
        .is_ok()
    );

    let mut wrong_peer = ownership_envelope;
    wrong_peer.payload.records[0]
        .fields
        .iter_mut()
        .find(|field| field.key == "peer_subject")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "u14.headless_iteration".to_owned(),
    };
    refresh_payload_digest(&mut wrong_peer);
    let (bytes, transfer) = encode(&wrong_peer);
    assert_eq!(
        decode_evidence(&bytes, limits, &transfer, &ownership_identity, &protocol,),
        Err(EvidenceError::Catalogue)
    );
}

#[test]
fn zero_byte_and_timing_samples_are_rejected_before_publication() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    for (metric_id, environment_class, population) in [
        (
            "candidate.size_bytes",
            "portable_correctness_v1",
            "not_applicable",
        ),
        ("candidate.startup_p95_ns", "desktop_timing_v1", "warm"),
    ] {
        let mut invalid = envelope.clone();
        invalid.identity.subject = "u20.release_candidate".to_owned();
        invalid.identity.environment_class = environment_class.to_owned();
        invalid.payload.records = vec![evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: format!("{metric_id}.sample_000001"),
            fields: vec![
                identifier_field("metric_id", metric_id),
                identifier_field("population", population),
                u64_field("sample_index", 1),
                u64_field("value", 0),
            ],
        }];
        let mut expected = identity.clone();
        expected.identity.subject = invalid.identity.subject.clone();
        expected.identity.environment_class = invalid.identity.environment_class.clone();
        if environment_class == "desktop_timing_v1" {
            for fields in [&mut invalid.payload.environment, &mut expected.environment] {
                fields
                    .iter_mut()
                    .find(|field| field.key == "gpu_stack_class")
                    .unwrap()
                    .value = FieldValue::Identifier {
                    value: "software_vulkan_v1".to_owned(),
                };
            }
        }
        bind_expected_context(&protocol, &mut invalid, &mut expected);
        refresh_payload_digest(&mut invalid);
        let (bytes, transfer) = encode(&invalid);

        assert_eq!(
            decode_evidence(&bytes, limits, &transfer, &expected, &protocol),
            Err(EvidenceError::Catalogue),
            "metric {metric_id} must reject zero"
        );
    }
}

#[test]
fn measurement_binding_and_context_receipts_are_exact() {
    let (protocol, _bytes, mut envelope, _transfer, mut expected) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);
    envelope.identity.subject = "u14.headless_iteration".to_owned();
    envelope.identity.environment_class = "cpu_timing_v1".to_owned();
    expected.identity.subject = envelope.identity.subject.clone();
    expected.identity.environment_class = envelope.identity.environment_class.clone();
    envelope.payload.records = vec![metric_sample_record(
        &protocol,
        "iteration.data.p50_ns",
        "warm",
        1,
        1,
    )];
    bind_expected_context(&protocol, &mut envelope, &mut expected);
    assert!(!envelope.payload.context_receipts.is_empty());
    refresh_payload_digest(&mut envelope);
    let (bytes, transfer) = encode(&envelope);
    assert!(decode_evidence(&bytes, limits, &transfer, &expected, &protocol).is_ok());

    let mut missing_context = envelope.clone();
    let mut missing_context_expected = expected.clone();
    missing_context.payload.context_receipts.pop();
    missing_context_expected.context_receipts = missing_context.payload.context_receipts.clone();
    refresh_payload_digest(&mut missing_context);
    let (bytes, transfer) = encode(&missing_context);
    assert_eq!(
        decode_evidence(
            &bytes,
            limits,
            &transfer,
            &missing_context_expected,
            &protocol,
        ),
        Err(EvidenceError::Catalogue)
    );

    let mut extra_context = envelope.clone();
    let mut extra_context_expected = expected.clone();
    extra_context
        .payload
        .context_receipts
        .push(evidence::ContextReceipt {
            id: "unrequested_context_v1".to_owned(),
            digest: evidence::digest_ref(b"unrequested context"),
        });
    extra_context
        .payload
        .context_receipts
        .sort_by(|left, right| left.id.cmp(&right.id));
    extra_context_expected.context_receipts = extra_context.payload.context_receipts.clone();
    refresh_payload_digest(&mut extra_context);
    let (bytes, transfer) = encode(&extra_context);
    assert_eq!(
        decode_evidence(
            &bytes,
            limits,
            &transfer,
            &extra_context_expected,
            &protocol,
        ),
        Err(EvidenceError::Catalogue)
    );

    for key in [
        "workload_id",
        "start_boundary_id",
        "end_boundary_id",
        "method_id",
    ] {
        let mut drifted = envelope.clone();
        let field = drifted.payload.records[0]
            .fields
            .iter_mut()
            .find(|field| field.key == key)
            .unwrap();
        field.value = FieldValue::Identifier {
            value: "drifted_measurement_v1".to_owned(),
        };
        refresh_payload_digest(&mut drifted);
        let (bytes, transfer) = encode(&drifted);
        assert_eq!(
            decode_evidence(&bytes, limits, &transfer, &expected, &protocol),
            Err(EvidenceError::Catalogue),
            "measurement binding {key} must be protocol-owned"
        );
    }
}

#[test]
fn matching_collector_and_expected_identity_canaries_are_still_rejected() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    let cases: [(&str, IdentityMutation); 4] = [
        ("generator", |envelope, expected| {
            envelope.generator = "credential_token_generator_v1".to_owned();
            expected.generator = envelope.generator.clone();
        }),
        ("run_provider", |envelope, expected| {
            envelope.identity.run_provider = "credential_token_provider_v1".to_owned();
            expected.identity.run_provider = envelope.identity.run_provider.clone();
        }),
        ("run_id", |envelope, expected| {
            envelope.identity.run_id = "credential_token_run_v1".to_owned();
            expected.identity.run_id = envelope.identity.run_id.clone();
        }),
        ("repository", |envelope, expected| {
            envelope.identity.repository = "credential_token/repository".to_owned();
            expected.identity.repository = envelope.identity.repository.clone();
        }),
    ];

    for (name, mutate) in cases {
        let mut invalid = envelope.clone();
        let mut expected = identity.clone();
        mutate(&mut invalid, &mut expected);
        let (bytes, transfer) = encode(&invalid);
        let mut store = TrustedEvidenceStore::default();

        assert_eq!(
            ingest_evidence(
                &bytes,
                &transfer_table(&transfer),
                limits,
                &transfer,
                &expected,
                &protocol,
                &mut store,
            ),
            Err(EvidenceError::Identity),
            "matching identity canary {name} must reject"
        );
        assert_eq!(store.published_count(), 0);
    }
}

#[test]
fn validated_raw_samples_use_frozen_aggregation_before_suite_decision() {
    let (protocol, _bytes, mut envelope, _transfer, mut identity) = fixture();
    let revisions = GitRevisionFixture::new("docs/current-candidate.md");
    envelope.identity.source_revision = revisions.current_revision.clone();
    identity.identity.source_revision = revisions.current_revision.clone();
    for fields in [&mut envelope.payload.environment, &mut identity.environment] {
        fields
            .iter_mut()
            .find(|field| field.key == "gpu_stack_class")
            .unwrap()
            .value = FieldValue::Identifier {
            value: "software_vulkan_v1".to_owned(),
        };
    }
    let revision_admission = evidence::revision_admission_from_git(
        &protocol,
        &revisions.root,
        &revisions.current_revision,
        &[],
    )
    .expect("current evidence requires a clean Git-backed admission");
    let startup_records = (1..=5)
        .map(|sample_index| evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: format!("candidate.startup_p95_ns.sample_{sample_index:06}"),
            fields: vec![
                identifier_field("metric_id", "candidate.startup_p95_ns"),
                identifier_field("population", "warm"),
                u64_field("sample_index", sample_index),
                u64_field("value", sample_index),
            ],
        })
        .collect();
    let startup_first = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "desktop_timing_v1",
        startup_records,
    );
    let startup_second = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "desktop_timing_v1",
        (6..=10)
            .map(|sample_index| evidence::EvidenceRecord {
                kind: "metric_sample".to_owned(),
                id: format!("candidate.startup_p95_ns.sample_{sample_index:06}"),
                fields: vec![
                    identifier_field("metric_id", "candidate.startup_p95_ns"),
                    identifier_field("population", "warm"),
                    u64_field("sample_index", sample_index),
                    u64_field("value", sample_index),
                ],
            })
            .collect(),
    );
    let size = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "portable_correctness_v1",
        vec![evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: "candidate.size_bytes.sample_000001".to_owned(),
            fields: vec![
                identifier_field("metric_id", "candidate.size_bytes"),
                identifier_field("population", "not_applicable"),
                u64_field("sample_index", 1),
                u64_field("value", 100),
            ],
        }],
    );
    let unpacked_size = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "portable_correctness_v1",
        vec![evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: "candidate.unpacked_size_bytes.sample_000001".to_owned(),
            fields: vec![
                identifier_field("metric_id", "candidate.unpacked_size_bytes"),
                identifier_field("population", "not_applicable"),
                u64_field("sample_index", 1),
                u64_field("value", 200),
            ],
        }],
    );
    let validated = vec![startup_first.clone(), startup_second, size, unpacked_size];
    let observations = aggregate_validated_observations(
        &protocol,
        "candidate_gate",
        &validated,
        &revision_admission,
    )
    .unwrap();
    let startup_observation = observations
        .iter()
        .find(|observation| observation.metric_id == "candidate.startup_p95_ns")
        .unwrap();
    assert_eq!(startup_observation.value, 10);
    assert_eq!(startup_observation.samples, 10);
    assert_eq!(startup_observation.raw_records, 10);
    let expected_environments = observations
        .iter()
        .map(|observation| {
            (
                observation.metric_id.clone(),
                observation.environment.clone(),
            )
        })
        .collect();
    assert_eq!(
        decide_validated_suite(
            &protocol,
            "candidate_gate",
            &validated,
            &expected_environments,
            &revision_admission,
        ),
        Ok(Decision::Continue)
    );

    let mut gpu_mismatch_envelope = envelope.clone();
    gpu_mismatch_envelope
        .payload
        .environment
        .iter_mut()
        .find(|field| field.key == "gpu_stack_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "software_vulkan_v2".to_owned(),
    };
    let mut gpu_mismatch_identity = identity.clone();
    gpu_mismatch_identity
        .environment
        .iter_mut()
        .find(|field| field.key == "gpu_stack_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "software_vulkan_v2".to_owned(),
    };
    let gpu_mismatch = validated_records(
        &protocol,
        &gpu_mismatch_envelope,
        &gpu_mismatch_identity,
        "u20.release_candidate",
        "desktop_timing_v1",
        (6..=10)
            .map(|sample_index| evidence::EvidenceRecord {
                kind: "metric_sample".to_owned(),
                id: format!("candidate.startup_p95_ns.sample_{sample_index:06}"),
                fields: vec![
                    identifier_field("metric_id", "candidate.startup_p95_ns"),
                    identifier_field("population", "warm"),
                    u64_field("sample_index", sample_index),
                    u64_field("value", sample_index),
                ],
            })
            .collect(),
    );
    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "candidate_gate",
            &[startup_first.clone(), gpu_mismatch],
            &revision_admission,
        ),
        Err(EvidenceAggregationError::EnvironmentMismatch)
    );

    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "candidate_gate",
            &[startup_first.clone(), startup_first.clone()],
            &revision_admission,
        ),
        Err(EvidenceAggregationError::DuplicateSample)
    );

    let mut required_mismatch_envelope = envelope.clone();
    required_mismatch_envelope
        .payload
        .environment
        .iter_mut()
        .find(|field| field.key == "toolchain_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "rustc_1_98_0_msvc".to_owned(),
    };
    let mut required_mismatch_identity = identity.clone();
    required_mismatch_identity
        .environment
        .iter_mut()
        .find(|field| field.key == "toolchain_class")
        .unwrap()
        .value = FieldValue::Identifier {
        value: "rustc_1_98_0_msvc".to_owned(),
    };
    let required_mismatch = validated_records(
        &protocol,
        &required_mismatch_envelope,
        &required_mismatch_identity,
        "u20.release_candidate",
        "desktop_timing_v1",
        (6..=10)
            .map(|sample_index| evidence::EvidenceRecord {
                kind: "metric_sample".to_owned(),
                id: format!("candidate.startup_p95_ns.sample_{sample_index:06}"),
                fields: vec![
                    identifier_field("metric_id", "candidate.startup_p95_ns"),
                    identifier_field("population", "warm"),
                    u64_field("sample_index", sample_index),
                    u64_field("value", sample_index),
                ],
            })
            .collect(),
    );
    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "candidate_gate",
            &[startup_first, required_mismatch],
            &revision_admission,
        ),
        Err(EvidenceAggregationError::EnvironmentMismatch)
    );

    let gapped_records = (1..=10)
        .map(|position| if position == 10 { 11 } else { position })
        .map(|sample_index| evidence::EvidenceRecord {
            kind: "metric_sample".to_owned(),
            id: format!("candidate.startup_p95_ns.sample_{sample_index:06}"),
            fields: vec![
                identifier_field("metric_id", "candidate.startup_p95_ns"),
                identifier_field("population", "warm"),
                u64_field("sample_index", sample_index),
                u64_field("value", sample_index),
            ],
        })
        .collect();
    let gapped = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "desktop_timing_v1",
        gapped_records,
    );
    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "candidate_gate",
            &[gapped],
            &revision_admission,
        ),
        Err(EvidenceAggregationError::NonContiguousSamples)
    );

    let inconsistent_exact = validated_records(
        &protocol,
        &envelope,
        &identity,
        "u20.release_candidate",
        "portable_correctness_v1",
        vec![
            evidence::EvidenceRecord {
                kind: "metric_sample".to_owned(),
                id: "candidate.size_bytes.sample_000001".to_owned(),
                fields: vec![
                    identifier_field("metric_id", "candidate.size_bytes"),
                    identifier_field("population", "not_applicable"),
                    u64_field("sample_index", 1),
                    u64_field("value", 100),
                ],
            },
            evidence::EvidenceRecord {
                kind: "metric_sample".to_owned(),
                id: "candidate.size_bytes.sample_000002".to_owned(),
                fields: vec![
                    identifier_field("metric_id", "candidate.size_bytes"),
                    identifier_field("population", "not_applicable"),
                    u64_field("sample_index", 2),
                    u64_field("value", 101),
                ],
            },
        ],
    );
    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "candidate_gate",
            &[inconsistent_exact],
            &revision_admission,
        ),
        Err(EvidenceAggregationError::InconsistentExactSamples)
    );
}

#[test]
fn revision_admission_rejects_affected_and_current_only_reuse() {
    let (base_protocol, _bytes, envelope, _transfer, identity) = fixture();
    let revisions = GitRevisionFixture::new("docs/selective.md");

    let protocol_with_target = |metric_id: &str| {
        let mut protocol = base_protocol.clone();
        protocol.invalidation.rules.push(evidence::PathRule {
            id: "path.test_selective".to_owned(),
            selectors: vec![evidence::PathSelector::Exact {
                path: "docs/selective.md".to_owned(),
            }],
            targets: vec![evidence::TargetRef::Metric {
                id: metric_id.to_owned(),
            }],
        });
        protocol
            .invalidation
            .rules
            .sort_by(|left, right| left.id.cmp(&right.id));
        protocol
    };
    let admission = |protocol: &evidence::FirstPlayableProtocol| {
        evidence::revision_admission_from_git(
            protocol,
            &revisions.root,
            &revisions.current_revision,
            std::slice::from_ref(&revisions.prior_revision),
        )
        .expect("trusted Git comparison must produce a complete admission")
    };

    let unaffected_protocol = protocol_with_target("iteration.body.p50_ns");
    let unaffected = validated_records_at_revision(
        &unaffected_protocol,
        &envelope,
        &identity,
        &revisions.prior_revision,
        "u14.headless_iteration",
        "cpu_timing_v1",
        vec![metric_sample_record(
            &unaffected_protocol,
            "iteration.data.p50_ns",
            "warm",
            1,
            1,
        )],
    );
    assert!(
        aggregate_validated_observations(
            &unaffected_protocol,
            "first_playable",
            &[unaffected],
            &admission(&unaffected_protocol),
        )
        .is_ok(),
        "mapped evidence may be reused only when its metric is unaffected"
    );

    let affected_protocol = protocol_with_target("iteration.data.p50_ns");
    let affected = validated_records_at_revision(
        &affected_protocol,
        &envelope,
        &identity,
        &revisions.prior_revision,
        "u14.headless_iteration",
        "cpu_timing_v1",
        vec![metric_sample_record(
            &affected_protocol,
            "iteration.data.p50_ns",
            "warm",
            1,
            1,
        )],
    );
    assert_eq!(
        aggregate_validated_observations(
            &affected_protocol,
            "first_playable",
            &[affected],
            &admission(&affected_protocol),
        ),
        Err(EvidenceAggregationError::StaleEvidence)
    );

    let current_only_protocol = protocol_with_target("iteration.data.p50_ns");
    let current_only = validated_records_at_revision(
        &current_only_protocol,
        &envelope,
        &identity,
        &revisions.prior_revision,
        "u20.release_candidate",
        "portable_correctness_v1",
        vec![metric_sample_record(
            &current_only_protocol,
            "candidate.size_bytes",
            "not_applicable",
            1,
            1,
        )],
    );
    assert_eq!(
        aggregate_validated_observations(
            &current_only_protocol,
            "candidate_gate",
            &[current_only],
            &admission(&current_only_protocol),
        ),
        Err(EvidenceAggregationError::StaleEvidence)
    );

    assert_eq!(
        evidence::revision_admission_from_git(
            &base_protocol,
            &revisions.root,
            &revisions.current_revision,
            &["a".repeat(40)],
        ),
        Err(evidence::RevisionAdmissionError::RevisionNotAncestor)
    );

    let nested = revisions.root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        evidence::revision_admission_from_git(
            &base_protocol,
            &nested,
            &revisions.current_revision,
            std::slice::from_ref(&revisions.prior_revision),
        ),
        Err(evidence::RevisionAdmissionError::RepositoryUnavailable),
        "admission must bind the explicitly supplied repository root"
    );

    std::fs::write(revisions.root.join("uncommitted.txt"), "not committed\n").unwrap();
    assert_eq!(
        evidence::revision_admission_from_git(
            &base_protocol,
            &revisions.root,
            &revisions.current_revision,
            std::slice::from_ref(&revisions.prior_revision),
        ),
        Err(evidence::RevisionAdmissionError::UncommittedSource)
    );
}

#[test]
fn revision_admission_preserves_legal_git_paths_outside_identifier_grammar() {
    let (protocol, _bytes, _envelope, _transfer, _identity) = fixture();
    let changed_path = format!("docs/source evidence 测试 {}.md", "x".repeat(140));
    assert!(changed_path.len() > 160);
    let revisions = GitRevisionFixture::new(&changed_path);

    evidence::revision_admission_from_git(
        &protocol,
        &revisions.root,
        &revisions.current_revision,
        std::slice::from_ref(&revisions.prior_revision),
    )
    .expect("Git paths are not evidence identifiers and must retain their legal spelling");
}

#[test]
fn ownership_decision_requires_a_digest_matched_u26_u25_cohort() {
    let (protocol, _bytes, mut envelope, _transfer, mut identity) = fixture();
    let revisions = GitRevisionFixture::new("docs/current-ownership.md");
    envelope.identity.source_revision = revisions.current_revision.clone();
    identity.identity.source_revision = revisions.current_revision.clone();
    let catalogue_bytes =
        std::fs::read(evidence::repository_root().join(evidence::PRODUCT_BUDGETS_PATH)).unwrap();
    let catalogue = evidence::decode_product_budget_catalogue(&catalogue_bytes).unwrap();
    let contract_digests = evidence::ownership_contract_digests(&catalogue);
    let reviewer_digest = evidence::digest_ref(b"independent U26 baseline review");
    let candidate_digest = evidence::digest_ref(b"trusted U24 candidate");
    let ownership_metrics = protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == "ownership_gate")
        .map(|metric| metric.id.clone())
        .collect::<Vec<_>>();
    let baseline_context = [
        (
            "independent_reviewer_attestation_v1",
            reviewer_digest.clone(),
        ),
        (
            "u22_correctness_contract_v1",
            contract_digests.correctness_contract.clone(),
        ),
        ("u22_fault_matrix_v1", contract_digests.fault_matrix.clone()),
        (
            "u22_lifecycle_graph_v1",
            contract_digests.lifecycle_graph.clone(),
        ),
    ];
    let partial_baseline = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u26.manual_counterfactual",
        "paired_ownership_v1",
        vec![ownership_inventory_record(
            &protocol,
            &ownership_metrics[0],
            "u25.ownership_comparison",
            0,
        )],
        &baseline_context,
    );
    let revision_admission = evidence::revision_admission_from_git(
        &protocol,
        &revisions.root,
        &revisions.current_revision,
        &[],
    )
    .expect("ownership decisions require a clean Git-backed admission");
    assert_eq!(
        evidence::ownership_decision_admission(
            &protocol,
            &catalogue,
            &partial_baseline,
            &revision_admission,
            candidate_digest.clone(),
            reviewer_digest.clone(),
        ),
        Err(EvidenceAggregationError::OwnershipCohortMismatch)
    );

    let baseline_records = ownership_metrics
        .iter()
        .map(|metric_id| {
            ownership_inventory_record(&protocol, metric_id, "u25.ownership_comparison", 0)
        })
        .collect::<Vec<_>>();
    let baseline = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u26.manual_counterfactual",
        "paired_ownership_v1",
        baseline_records,
        &baseline_context,
    );
    let baseline_digest = evidence::validated_evidence_digest(&baseline);
    let ownership_admission = evidence::ownership_decision_admission(
        &protocol,
        &catalogue,
        &baseline,
        &revision_admission,
        candidate_digest.clone(),
        reviewer_digest.clone(),
    )
    .unwrap();
    let candidate_records = ownership_metrics
        .iter()
        .map(|metric_id| {
            ownership_inventory_record(&protocol, metric_id, "u26.manual_counterfactual", 0)
        })
        .collect::<Vec<_>>();
    let candidate_context = [
        (
            "independent_reviewer_attestation_v1",
            reviewer_digest.clone(),
        ),
        (
            "u22_correctness_contract_v1",
            contract_digests.correctness_contract.clone(),
        ),
        ("u22_fault_matrix_v1", contract_digests.fault_matrix.clone()),
        (
            "u22_lifecycle_graph_v1",
            contract_digests.lifecycle_graph.clone(),
        ),
        ("u24_candidate_digest_v1", candidate_digest.clone()),
        ("u26_baseline_digest_v1", baseline_digest.clone()),
    ];
    let mut mismatched_candidate_context = candidate_context.clone();
    mismatched_candidate_context
        .iter_mut()
        .find(|(id, _)| *id == "u24_candidate_digest_v1")
        .unwrap()
        .1 = evidence::digest_ref(b"different U24 candidate");
    let unmatched_candidate = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u25.ownership_comparison",
        "paired_ownership_v1",
        candidate_records.clone(),
        &mismatched_candidate_context,
    );
    assert_eq!(
        evidence::aggregate_validated_ownership_observations(
            &protocol,
            std::slice::from_ref(&unmatched_candidate),
            &revision_admission,
            &ownership_admission,
        ),
        Err(EvidenceAggregationError::MissingOwnershipCohort)
    );
    assert_eq!(
        evidence::aggregate_validated_ownership_observations(
            &protocol,
            &[baseline.clone(), unmatched_candidate],
            &revision_admission,
            &ownership_admission,
        ),
        Err(EvidenceAggregationError::OwnershipCohortMismatch)
    );

    let split_at = candidate_records.len() / 2;
    let first_candidate = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u25.ownership_comparison",
        "paired_ownership_v1",
        candidate_records[..split_at].to_vec(),
        &candidate_context,
    );
    let second_candidate = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u25.ownership_comparison",
        "paired_ownership_v1",
        candidate_records[split_at..].to_vec(),
        &mismatched_candidate_context,
    );
    assert_eq!(
        evidence::aggregate_validated_ownership_observations(
            &protocol,
            &[baseline.clone(), first_candidate, second_candidate],
            &revision_admission,
            &ownership_admission,
        ),
        Err(EvidenceAggregationError::OwnershipCohortMismatch)
    );

    let candidate = validated_records_with_context(
        &protocol,
        &envelope,
        &identity,
        "u25.ownership_comparison",
        "paired_ownership_v1",
        candidate_records,
        &candidate_context,
    );
    let evidence = [baseline, candidate];
    assert_eq!(
        aggregate_validated_observations(
            &protocol,
            "ownership_gate",
            &evidence,
            &revision_admission,
        ),
        Err(EvidenceAggregationError::MissingOwnershipDecisionAdmission)
    );
    let observations = evidence::aggregate_validated_ownership_observations(
        &protocol,
        &evidence,
        &revision_admission,
        &ownership_admission,
    )
    .unwrap();
    let expected_environments = observations
        .iter()
        .map(|observation| {
            (
                observation.metric_id.clone(),
                observation.environment.clone(),
            )
        })
        .collect();
    assert_eq!(
        evidence::decide_validated_ownership_suite(
            &protocol,
            &evidence,
            &expected_environments,
            &revision_admission,
            &ownership_admission,
        ),
        Ok(Decision::Continue)
    );
}

#[test]
fn payload_and_outer_transfer_digests_detect_distinct_tampering() {
    let (protocol, _bytes, mut envelope, _transfer, mut identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);

    envelope.identity.subject = "u14.headless_iteration".to_owned();
    envelope.identity.environment_class = "cpu_timing_v1".to_owned();
    envelope.payload.records = vec![evidence::EvidenceRecord {
        kind: "metric_sample".to_owned(),
        id: "iteration.data.p50_ns.sample_000001".to_owned(),
        fields: vec![
            identifier_field("metric_id", "iteration.data.p50_ns"),
            identifier_field("population", "warm"),
            u64_field("sample_index", 1),
            u64_field("value", 1_000_000_000),
        ],
    }];
    identity.identity.subject = envelope.identity.subject.clone();
    identity.identity.environment_class = envelope.identity.environment_class.clone();
    bind_expected_context(&protocol, &mut envelope, &mut identity);
    refresh_payload_digest(&mut envelope);
    let (bytes, transfer) = encode(&envelope);

    let mut payload_tamper = envelope;
    payload_tamper.payload.records[0]
        .fields
        .iter_mut()
        .find(|field| field.key == "value")
        .unwrap()
        .value = FieldValue::U64 {
        value: 1_000_000_001,
    };
    let (tampered_bytes, tampered_transfer) = encode(&payload_tamper);
    assert_eq!(
        decode_evidence(
            &tampered_bytes,
            limits,
            &tampered_transfer,
            &identity,
            &protocol
        ),
        Err(EvidenceError::PayloadDigest)
    );

    let mut fully_rehashed = payload_tamper;
    refresh_payload_digest(&mut fully_rehashed);
    let (fully_rehashed_bytes, _attacker_transfer) = encode(&fully_rehashed);
    assert_eq!(
        decode_evidence(
            &fully_rehashed_bytes,
            limits,
            &transfer,
            &identity,
            &protocol
        ),
        Err(EvidenceError::TransferDigest)
    );
    assert_ne!(fully_rehashed_bytes, bytes);
}

#[test]
fn paths_and_privacy_canaries_are_rejected_with_static_errors() {
    let (protocol, _bytes, envelope, _transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);
    let bad_paths = [
        "/absolute/path",
        "../parent",
        "a/./b",
        "a//b",
        "C:/host/path",
        r"C:\host\path",
        r"\\server\share",
        "https://example.invalid/file",
        "aux/file",
        "trailing./file",
    ];
    for bad_path in bad_paths {
        let mut candidate = envelope.clone();
        let field = candidate.payload.records[0]
            .fields
            .iter_mut()
            .find(|field| field.key == "source_path")
            .unwrap();
        field.value = FieldValue::ProjectRelative {
            value: bad_path.to_owned(),
        };
        refresh_payload_digest(&mut candidate);
        let (bytes, transfer) = encode(&candidate);
        let error = decode_evidence(&bytes, limits, &transfer, &identity, &protocol)
            .expect_err("unsafe path must reject");
        assert_eq!(error, EvidenceError::InvalidValue);
        assert!(!format!("{error:?} {error}").contains(bad_path));
    }

    let canaries = [
        "user@example.invalid",
        "http://127.0.0.1:10809",
        "Bearer_secret_value",
        "AKIAIOSFODNN7EXAMPLE",
        "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        "frankorz",
        "name;Remove-Item",
        "name$(command)",
    ];
    for canary in canaries {
        let mut candidate = envelope.clone();
        let field = candidate.payload.records[0]
            .fields
            .iter_mut()
            .find(|field| field.key == "reviewer_class")
            .unwrap();
        field.value = FieldValue::Identifier {
            value: canary.to_owned(),
        };
        refresh_payload_digest(&mut candidate);
        let (bytes, transfer) = encode(&candidate);
        let error = decode_evidence(&bytes, limits, &transfer, &identity, &protocol)
            .expect_err("privacy canary must reject");
        assert!(matches!(
            error,
            EvidenceError::InvalidValue | EvidenceError::Catalogue
        ));
        assert!(!format!("{error:?} {error}").contains(canary));
    }
}

#[test]
fn raw_log_references_carry_only_identity_digest_and_retention() {
    let (protocol, _bytes, mut envelope, _transfer, mut identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);
    envelope
        .restricted_raw_logs
        .push(evidence::RestrictedRawLogRef {
            artifact_id: "u22_restricted_log_001".to_owned(),
            digest: evidence::digest_ref(b"restricted log fixture"),
            retention_until_unix_seconds: 1_800_000_000,
        });
    let raw_log = &envelope.restricted_raw_logs[0];
    assert!(!raw_log.artifact_id.contains(['/', '\\', ':']));
    assert_eq!(raw_log.digest.blake3.len(), 64);
    assert!(raw_log.retention_until_unix_seconds > 0);
    identity.restricted_raw_logs = envelope.restricted_raw_logs.clone();
    let (valid_bytes, valid_transfer) = encode(&envelope);
    assert!(decode_evidence(&valid_bytes, limits, &valid_transfer, &identity, &protocol,).is_ok());

    let mut value = serde_json::to_value(envelope).unwrap();
    value["restricted_raw_logs"][0]
        .as_object_mut()
        .unwrap()
        .insert(
            "path".to_owned(),
            serde_json::Value::String("C:/secret/log.txt".to_owned()),
        );
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
    bytes.push(b'\n');
    let transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, &bytes);
    assert_eq!(
        decode_evidence(&bytes, limits, &transfer, &identity, &protocol),
        Err(EvidenceError::Decode)
    );
}

#[test]
fn transfer_table_rejects_traversal_alias_links_special_and_unexpected_entries() {
    let expected_path = "evidence/calibration-review.json";
    let expected = ExpectedTransfer {
        path: expected_path.to_owned(),
        digest: evidence::DigestRef {
            bytes: 512,
            blake3: "a".repeat(64),
        },
    };
    let valid = [TransferEntry {
        path: expected_path.to_owned(),
        kind: TransferEntryKind::Regular,
        encoded_bytes: 512,
    }];
    assert!(preflight_transfer_table(&valid, &expected, 512).is_ok());

    let cases = [
        vec![TransferEntry {
            path: "../calibration-review.json".to_owned(),
            kind: TransferEntryKind::Regular,
            encoded_bytes: 512,
        }],
        vec![TransferEntry {
            path: "Evidence/calibration-review.json".to_owned(),
            kind: TransferEntryKind::Regular,
            encoded_bytes: 512,
        }],
        vec![TransferEntry {
            path: expected_path.to_owned(),
            kind: TransferEntryKind::Symlink,
            encoded_bytes: 512,
        }],
        vec![TransferEntry {
            path: expected_path.to_owned(),
            kind: TransferEntryKind::Special,
            encoded_bytes: 512,
        }],
        vec![
            valid[0].clone(),
            TransferEntry {
                path: "evidence/unexpected.json".to_owned(),
                kind: TransferEntryKind::Regular,
                encoded_bytes: 1,
            },
        ],
        vec![TransferEntry {
            path: expected_path.to_owned(),
            kind: TransferEntryKind::Regular,
            encoded_bytes: 513,
        }],
    ];
    for entries in cases {
        assert_eq!(
            preflight_transfer_table(&entries, &expected, 512),
            Err(EvidenceError::Catalogue)
        );
    }
}

#[test]
fn trusted_publication_cannot_bypass_transfer_table_preflight() {
    let (protocol, bytes, _envelope, transfer, identity) = fixture();
    let limits = EnvelopeLimits::from(&protocol.evidence);
    let invalid_tables = [
        vec![TransferEntry {
            path: transfer.path.clone(),
            kind: TransferEntryKind::Symlink,
            encoded_bytes: bytes.len(),
        }],
        vec![
            TransferEntry {
                path: transfer.path.clone(),
                kind: TransferEntryKind::Regular,
                encoded_bytes: bytes.len(),
            },
            TransferEntry {
                path: "evidence/unexpected.json".to_owned(),
                kind: TransferEntryKind::Regular,
                encoded_bytes: 1,
            },
        ],
    ];

    for entries in invalid_tables {
        reset_typed_decode_count();
        let mut store = TrustedEvidenceStore::default();
        assert_eq!(
            ingest_evidence(
                &bytes, &entries, limits, &transfer, &identity, &protocol, &mut store,
            ),
            Err(EvidenceError::Catalogue)
        );
        assert_eq!(typed_decode_count(), 0);
        assert_eq!(store.published_count(), 0);
    }
}
