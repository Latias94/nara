use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use nara::fs::ContentDigest;
use serde::Deserialize;

const PROTOCOL_PATH: &str = "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json";
const PROTOCOL_DIGEST_PATH: &str =
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.blake3";
const RUN_ROOT: &str = "docs/benchmarks/data/runs/v1/rgd-u9-f098876";
const SOURCE_REVISION: &str = "f09887600d2161144b920b6e2618fc8151dad4fa";
const METRIC_CATALOG_SHA256: &str =
    "e55fe4205584b801e3e3301e391b23e86b2f9ded1e04e4936c92436da1c988c2";
const RAW_SAMPLES_SHA256: &str = "040a9779de7c4f4d915f101626a832618e9e9d98b679e57bd7ba651782de3f80";
const PROTOCOL_BLAKE3: &str = "82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03";
const RUN_MANIFEST_BLAKE3: &str =
    "a0ef28d347412437d7c9ff4d1aef7185bf4028843686eba3d079ae9926576cd8";
const RAW_SAMPLES_BLAKE3: &str = "e59b5529f863c9df15fad3339790c0597f352aa262a8ecd7120bbae5fd5da39c";

#[derive(Debug, Deserialize)]
struct Protocol {
    kind: String,
    format_version: u64,
    protocol_id: String,
    suites: Vec<Suite>,
    metrics: Vec<Metric>,
    decision: DecisionRules,
    aggregation: Aggregation,
}

#[derive(Debug, Deserialize)]
struct Suite {
    id: String,
    metrics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Metric {
    id: String,
    suite: String,
    statistic: Statistic,
    minimum_samples: usize,
    required: bool,
    hard_stop: bool,
    target: Target,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Statistic {
    Exact,
    P50,
    P95,
    P99,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Target {
    Exact { value: u64 },
    Maximum { value: u64 },
    Minimum { value: u64 },
}

#[derive(Debug, Deserialize)]
struct DecisionRules {
    hard_stop_failure: Decision,
    required_failure: Decision,
    all_required_pass: Decision,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Decision {
    Continue,
    Redirect,
    Stop,
}

#[derive(Debug, Deserialize)]
struct Aggregation {
    percentile_method: String,
    exact_method: String,
}

#[derive(Debug, Deserialize)]
struct RunManifest {
    failure: Option<serde_json::Value>,
    metric_catalog_sha256: String,
    raw_samples: RawSampleManifest,
    schema: String,
    source_revision: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct RawSampleManifest {
    count: usize,
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawSample {
    command: CommandResult,
    metric_id: String,
    sample_index: usize,
    sample_value: u64,
    source_revision: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CommandResult {
    exit_code: i64,
    output_overflowed: bool,
    timed_out: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetricOutcome {
    Pass,
    Fail,
    Missing,
}

#[derive(Debug)]
struct Evaluation {
    decision: Decision,
    outcomes: BTreeMap<String, MetricOutcome>,
}

#[test]
fn frozen_catalog_has_one_complete_first_playable_decision_contract() {
    let protocol_bytes = read_bytes(PROTOCOL_PATH);
    let protocol = decode_protocol(&protocol_bytes);
    assert_eq!(protocol.kind, "nara.first_playable_evidence_protocol");
    assert_eq!(protocol.format_version, 1);
    assert_eq!(protocol.protocol_id, "reference_game_first_playable_v1");
    assert_eq!(protocol.aggregation.percentile_method, "nearest_rank_v1");
    assert_eq!(protocol.aggregation.exact_method, "all_equal_v1");
    assert_eq!(protocol.decision.hard_stop_failure, Decision::Stop);
    assert_eq!(protocol.decision.required_failure, Decision::Redirect);
    assert_eq!(protocol.decision.all_required_pass, Decision::Continue);

    let first_playable = suite(&protocol, "first_playable");
    assert_eq!(first_playable.metrics.len(), 20);
    assert_eq!(
        first_playable.metrics.iter().collect::<BTreeSet<_>>().len(),
        first_playable.metrics.len(),
        "suite metric IDs must be unique"
    );
    let catalogue_ids = protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == first_playable.id)
        .map(|metric| &metric.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        catalogue_ids,
        first_playable.metrics.iter().collect(),
        "suite and metric catalogue must agree"
    );
    assert!(
        protocol
            .metrics
            .iter()
            .filter(|metric| metric.suite == first_playable.id)
            .all(|metric| metric.required && metric.minimum_samples > 0)
    );

    assert_eq!(content_digest(&protocol_bytes), PROTOCOL_BLAKE3);
    assert_eq!(read(PROTOCOL_DIGEST_PATH).trim(), PROTOCOL_BLAKE3);
}

#[test]
fn committed_u9_population_reproduces_the_redirect() {
    let protocol = decode_protocol(&read_bytes(PROTOCOL_PATH));
    let manifest_bytes = read_bytes(&format!("{RUN_ROOT}/run-manifest.json"));
    assert_eq!(content_digest(&manifest_bytes), RUN_MANIFEST_BLAKE3);
    let manifest: RunManifest =
        serde_json::from_slice(&manifest_bytes).expect("run manifest must remain valid");
    assert_eq!(
        manifest.schema,
        "nara.reference-game.first-playable-collection-transport-v1"
    );
    assert_eq!(manifest.status, "collected");
    assert_eq!(manifest.source_revision, SOURCE_REVISION);
    assert_eq!(manifest.metric_catalog_sha256, METRIC_CATALOG_SHA256);
    assert_eq!(manifest.raw_samples.path, "raw-samples.jsonl");
    assert_eq!(manifest.raw_samples.sha256, RAW_SAMPLES_SHA256);
    assert!(manifest.failure.is_none());

    let raw_bytes = read_bytes(&format!("{RUN_ROOT}/{}", manifest.raw_samples.path));
    assert_eq!(content_digest(&raw_bytes), RAW_SAMPLES_BLAKE3);
    let samples = decode_samples(&raw_bytes);
    assert_eq!(samples.len(), manifest.raw_samples.count);
    let evaluation = evaluate(&protocol, "first_playable", &samples, SOURCE_REVISION);
    assert_eq!(evaluation.decision, Decision::Redirect);
    assert_eq!(count(&evaluation, MetricOutcome::Pass), 9);
    assert_eq!(count(&evaluation, MetricOutcome::Fail), 1);
    assert_eq!(count(&evaluation, MetricOutcome::Missing), 10);
    assert_eq!(
        evaluation.outcomes.get("iteration.data.p95_ns"),
        Some(&MetricOutcome::Fail)
    );
    assert_eq!(
        evaluation.outcomes.get("gameplay.headless_wave_success"),
        Some(&MetricOutcome::Pass)
    );
}

#[test]
fn decision_precedence_is_small_and_explicit() {
    let protocol = decode_protocol(&read_bytes(PROTOCOL_PATH));
    let metrics = protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == "first_playable")
        .cloned()
        .collect::<Vec<_>>();
    let mut samples = passing_samples(&metrics);
    assert_eq!(
        evaluate(&protocol, "first_playable", &samples, SOURCE_REVISION).decision,
        Decision::Continue
    );

    samples.retain(|sample| sample.metric_id != "runtime.memory_bytes");
    assert_eq!(
        evaluate(&protocol, "first_playable", &samples, SOURCE_REVISION).decision,
        Decision::Redirect,
        "missing evidence redirects rather than inventing a result"
    );

    samples = passing_samples(&metrics);
    samples
        .iter_mut()
        .find(|sample| sample.metric_id == "gameplay.headless_wave_success")
        .expect("hard-stop sample must exist")
        .sample_value = 0;
    assert_eq!(
        evaluate(&protocol, "first_playable", &samples, SOURCE_REVISION).decision,
        Decision::Stop,
        "a complete failed hard stop takes precedence"
    );
}

fn evaluate(
    protocol: &Protocol,
    suite_id: &str,
    samples: &[RawSample],
    revision: &str,
) -> Evaluation {
    let suite = suite(protocol, suite_id);
    let mut outcomes = BTreeMap::new();
    let mut hard_stop_failed = false;
    let mut required_incomplete = false;

    for metric_id in &suite.metrics {
        let metric = protocol
            .metrics
            .iter()
            .find(|metric| metric.id == *metric_id && metric.suite == suite_id)
            .expect("suite metric must exist exactly once");
        let metric_samples = samples
            .iter()
            .filter(|sample| sample.metric_id == metric.id)
            .collect::<Vec<_>>();
        let outcome = if metric_samples.len() < metric.minimum_samples {
            MetricOutcome::Missing
        } else {
            validate_samples(metric, &metric_samples, revision);
            let aggregate = aggregate(metric.statistic, &metric_samples);
            if target_passes(metric.target, aggregate) {
                MetricOutcome::Pass
            } else {
                MetricOutcome::Fail
            }
        };
        if outcome == MetricOutcome::Fail && metric.hard_stop {
            hard_stop_failed = true;
        }
        if metric.required && outcome != MetricOutcome::Pass {
            required_incomplete = true;
        }
        outcomes.insert(metric.id.clone(), outcome);
    }

    let decision = if hard_stop_failed {
        protocol.decision.hard_stop_failure
    } else if required_incomplete {
        protocol.decision.required_failure
    } else {
        protocol.decision.all_required_pass
    };
    Evaluation { decision, outcomes }
}

fn validate_samples(metric: &Metric, samples: &[&RawSample], revision: &str) {
    let indices = samples
        .iter()
        .map(|sample| sample.sample_index)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        indices,
        (1..=samples.len()).collect(),
        "{} sample indices must be contiguous",
        metric.id
    );
    for sample in samples {
        assert_eq!(sample.source_revision, revision, "mixed source revisions");
        assert_eq!(sample.command.exit_code, 0, "{} command failed", metric.id);
        assert!(!sample.command.timed_out, "{} command timed out", metric.id);
        assert!(
            !sample.command.output_overflowed,
            "{} command overflowed output",
            metric.id
        );
    }
}

fn aggregate(statistic: Statistic, samples: &[&RawSample]) -> u64 {
    let mut values = samples
        .iter()
        .map(|sample| sample.sample_value)
        .collect::<Vec<_>>();
    values.sort_unstable();
    match statistic {
        Statistic::Exact => {
            assert!(
                values.windows(2).all(|pair| pair[0] == pair[1]),
                "Exact samples disagree"
            );
            values[0]
        }
        Statistic::P50 => nearest_rank(&values, 50),
        Statistic::P95 => nearest_rank(&values, 95),
        Statistic::P99 => nearest_rank(&values, 99),
    }
}

fn nearest_rank(values: &[u64], percentile: usize) -> u64 {
    assert!(!values.is_empty());
    let rank = (percentile * values.len()).div_ceil(100);
    values[rank - 1]
}

fn target_passes(target: Target, value: u64) -> bool {
    match target {
        Target::Exact { value: expected } => value == expected,
        Target::Maximum { value: maximum } => value <= maximum,
        Target::Minimum { value: minimum } => value >= minimum,
    }
}

fn passing_samples(metrics: &[Metric]) -> Vec<RawSample> {
    metrics
        .iter()
        .flat_map(|metric| {
            let value = match metric.target {
                Target::Exact { value } | Target::Maximum { value } | Target::Minimum { value } => {
                    value
                }
            };
            (1..=metric.minimum_samples).map(move |sample_index| RawSample {
                command: CommandResult {
                    exit_code: 0,
                    output_overflowed: false,
                    timed_out: false,
                },
                metric_id: metric.id.clone(),
                sample_index,
                sample_value: value,
                source_revision: SOURCE_REVISION.to_owned(),
            })
        })
        .collect()
}

fn count(evaluation: &Evaluation, expected: MetricOutcome) -> usize {
    evaluation
        .outcomes
        .values()
        .filter(|outcome| **outcome == expected)
        .count()
}

fn decode_protocol(bytes: &[u8]) -> Protocol {
    serde_json::from_slice(bytes).expect("frozen metric catalog must remain valid")
}

fn decode_samples(bytes: &[u8]) -> Vec<RawSample> {
    std::str::from_utf8(bytes)
        .expect("raw samples must remain UTF-8")
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("invalid raw sample line {}: {error}", index + 1))
        })
        .collect()
}

fn content_digest(bytes: &[u8]) -> String {
    ContentDigest::of_bytes(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn suite<'a>(protocol: &'a Protocol, id: &str) -> &'a Suite {
    protocol
        .suites
        .iter()
        .find(|suite| suite.id == id)
        .unwrap_or_else(|| panic!("missing suite {id}"))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repository_root().join(path))
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"))
}

fn read_bytes(path: &str) -> Vec<u8> {
    std::fs::read(repository_root().join(path))
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"))
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
