#![allow(dead_code)]

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{self, Display, Formatter},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::{ContentDigest, RelativePath},
};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_PATH: &str =
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json";
pub const PROTOCOL_DIGEST_PATH: &str =
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.blake3";
pub const CALIBRATION_ENVELOPE_PATH: &str =
    "docs/benchmarks/data/envelope/v1/calibration-review.json";
pub const PRODUCT_BUDGETS_PATH: &str =
    "docs/benchmarks/data/sources/v1/first-playable-product-budgets.json";
pub const PRODUCT_BUDGET_REVIEW_PATH: &str =
    "docs/benchmarks/data/sources/v1/first-playable-product-budget-review.json";
pub const PERFORMANCE_REVIEW_EVIDENCE_PATH: &str =
    "docs/benchmarks/data/reviews/v1/performance-measurement-review.json";
pub const PROVENANCE_REVIEW_EVIDENCE_PATH: &str =
    "docs/benchmarks/data/reviews/v1/protocol-provenance-review.json";
pub const EVIDENCE_TRANSFER_PATH: &str = "evidence/calibration-review.json";

pub const REQUIRED_SUBJECT_IDS: &[&str] = &[
    "u14.desktop_product",
    "u14.headless_iteration",
    "u20.release_candidate",
    "u22.calibration_review",
    "u25.ownership_comparison",
    "u26.manual_counterfactual",
];

pub const REQUIRED_METRIC_IDS: &[&str] = &[
    "build.cold_ns",
    "build.incremental_ns",
    "candidate.size_bytes",
    "candidate.startup_p95_ns",
    "candidate.unpacked_size_bytes",
    "frame.p99_ns",
    "gameplay.desktop_playable_success",
    "gameplay.headless_wave_success",
    "iteration.body.p50_ns",
    "iteration.body.p95_ns",
    "iteration.data.p50_ns",
    "iteration.data.p95_ns",
    "iteration.structural.p50_ns",
    "iteration.structural.p95_ns",
    "journey.clean_to_desktop_playable_ns",
    "journey.clean_to_headless_wave_ns",
    "module.add.success",
    "module.add.time_ns",
    "ownership.caller_glue_regressions",
    "ownership.false_stopped",
    "ownership.invalid_transitions",
    "ownership.missed_fault_boundaries",
    "ownership.private_imports",
    "ownership.shutdown_leaks",
    "ownership.undocumented_interventions",
    "ownership.unjustified_extra_concepts",
    "ownership.unowned_states",
    "ownership.unreachable_states",
    "public.production.coverage_basis_points",
    "runtime.gpu_resource_bytes",
    "runtime.memory_bytes",
    "slot.configure.success",
    "slot.configure.time_ns",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstPlayableProtocol {
    pub kind: String,
    pub format_version: u32,
    pub protocol_id: String,
    pub range_sources: Vec<RangeSourceDefinition>,
    pub subjects: Vec<SubjectSpec>,
    pub suites: Vec<SuiteSpec>,
    pub metrics: Vec<MetricRule>,
    pub environment_classes: Vec<EnvironmentClass>,
    pub environment_value_policy: EnvironmentValuePolicy,
    pub identity_policy: IdentityPolicy,
    pub invalidation: InvalidationPolicy,
    pub decision: DecisionRules,
    pub aggregation: AggregationPolicy,
    pub evidence: EvidencePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectSpec {
    pub id: String,
    pub collector: Collector,
    pub generator: String,
    pub run_providers: Vec<String>,
    pub required_context: Vec<String>,
    pub environment_classes: Vec<String>,
    pub record_kinds: Vec<String>,
    pub metric_subjects: Vec<String>,
    pub peer_subjects: Vec<String>,
    pub populations: Vec<Population>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collector {
    U14,
    U20,
    U22,
    U25,
    U26,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteSpec {
    pub id: String,
    pub metrics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricRule {
    pub id: String,
    pub subject: String,
    pub suite: String,
    pub collector: Collector,
    pub value_kind: MetricValueKind,
    pub statistic: Statistic,
    pub population: Population,
    pub minimum_samples: usize,
    pub required: bool,
    pub hard_stop: bool,
    pub environment_class: String,
    pub workload_id: String,
    pub start_boundary_id: String,
    pub end_boundary_id: String,
    pub method_id: String,
    pub required_context: Vec<String>,
    pub reuse_policy: EvidenceReusePolicy,
    pub target: MetricTarget,
    pub source: RangeSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricValueKind {
    Boolean,
    Bytes,
    Count,
    BasisPoints,
    Nanoseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReusePolicy {
    CurrentRevisionOnly,
    InvalidationMapped,
}

impl MetricValueKind {
    fn accepts(self, value: u64) -> bool {
        match self {
            Self::Boolean => value <= 1,
            Self::BasisPoints => value <= 10_000,
            Self::Bytes | Self::Nanoseconds => value > 0,
            Self::Count => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Statistic {
    Exact,
    P50,
    P95,
    P99,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    Cold,
    Warm,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetricTarget {
    Exact { value: u64 },
    Maximum { value: u64 },
    Minimum { value: u64 },
}

impl MetricTarget {
    pub fn accepts(&self, value: u64) -> bool {
        match self {
            Self::Exact { value: expected } => value == *expected,
            Self::Maximum { value: maximum } => value <= *maximum,
            Self::Minimum { value: minimum } => value >= *minimum,
        }
    }

    pub fn passing_value(&self) -> u64 {
        match self {
            Self::Exact { value } | Self::Maximum { value } | Self::Minimum { value } => *value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeSource {
    pub kind: RangeSourceKind,
    pub reference: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeSourceDefinition {
    pub id: String,
    pub kind: RangeSourceKind,
    pub revision: String,
    pub artifact_path: String,
    pub artifact_digest: DigestRef,
    pub source_revision: String,
    pub review_id: String,
    pub review_path: String,
    pub review_digest: DigestRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeSourceKind {
    StrategyBudget,
    ZeroToleranceContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBudgetCatalogue {
    pub kind: String,
    pub format_version: u32,
    pub revision: String,
    pub target_results_seen: bool,
    pub adoption: ProductBudgetAdoption,
    pub correctness_contract: CorrectnessContract,
    pub sources: Vec<ProductBudgetSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBudgetAdoption {
    pub decision_id: String,
    pub decision_kind: ProductBudgetDecisionKind,
    pub adopted_at_utc: String,
    pub adopted_by: String,
    pub authority_path: String,
    pub authority_revision: String,
    pub baseline_policy: BaselinePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductBudgetDecisionKind {
    PreTargetNormativeConstraints,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselinePolicy {
    pub observed_baselines_describe_current_performance: bool,
    pub normative_constraints_precede_target_results: bool,
    pub result_informed_revision_requires_new_version: bool,
    pub revised_constraints_cannot_decide_source_results: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectnessContract {
    pub id: String,
    pub baseline_subject: String,
    pub candidate_subject: String,
    pub task_steps: Vec<String>,
    pub fault_cases: Vec<String>,
    pub initial_lifecycle_state: String,
    pub lifecycle_states: Vec<String>,
    pub lifecycle_transitions: Vec<LifecycleTransition>,
    pub coverage_denominator: String,
    pub comparison_baseline: String,
    pub independent_review_required: bool,
    pub rubrics: Vec<CorrectnessRubric>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleTransition {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectnessRubric {
    pub metric_id: String,
    pub rule_id: String,
    pub ambiguity_outcome: Decision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBudgetSource {
    pub id: String,
    pub kind: RangeSourceKind,
    pub basis_id: String,
    pub metrics: Vec<ProductBudgetMetric>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBudgetMetric {
    pub id: String,
    pub target: MetricTarget,
    pub rationale_id: String,
    pub measurement: ProductMeasurementDefinition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductMeasurementDefinition {
    pub subject: String,
    pub environment_class: String,
    pub value_kind: MetricValueKind,
    pub statistic: Statistic,
    pub population: Population,
    pub workload_id: String,
    pub start_boundary_id: String,
    pub end_boundary_id: String,
    pub method_id: String,
    pub required_context: Vec<String>,
    pub reuse_policy: EvidenceReusePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBudgetReview {
    pub kind: String,
    pub format_version: u32,
    pub review_id: String,
    pub reviewed_at_utc: String,
    pub source_revision: String,
    pub target_results_seen: bool,
    pub verdict: ReviewVerdict,
    pub reviewed_artifact: ReviewedArtifact,
    pub attestations: Vec<ReviewerAttestation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedArtifact {
    pub path: String,
    pub revision: String,
    pub digest: DigestRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerAttestation {
    pub reviewer_id: String,
    pub reviewer_class: String,
    pub scope_id: String,
    pub reviewed_at_utc: String,
    pub target_results_seen: bool,
    pub verdict: ReviewVerdict,
    pub evidence_path: String,
    pub evidence_digest: DigestRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependentReviewEvidence {
    pub kind: String,
    pub format_version: u32,
    pub reviewer_id: String,
    pub reviewer_class: String,
    pub scope_id: String,
    pub reviewed_at_utc: String,
    pub source_revision: String,
    pub target_results_seen: bool,
    pub verdict: ReviewVerdict,
    pub reviewed_artifact: ReviewedArtifact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approve,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentClass {
    pub id: String,
    pub required_fields: Vec<EnvironmentField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentValuePolicy {
    pub id: String,
    pub forbidden_values: Vec<String>,
    pub forbidden_fragments: Vec<String>,
    pub os_prefixes: Vec<String>,
    pub architectures: Vec<String>,
    pub toolchain_prefix: String,
    pub minimum_toolchain_numeric_segments: usize,
    pub versioned_fields: Vec<EnvironmentField>,
    pub optional_not_applicable_fields: Vec<EnvironmentField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityPolicy {
    pub id: String,
    pub identifier_grammar: IdentifierGrammar,
    pub repository_grammar: RepositoryGrammar,
    pub run_id_grammar: RunIdGrammar,
    pub forbidden_fragments: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierGrammar {
    SafeAsciiV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryGrammar {
    OwnerRepoSlugV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunIdGrammar {
    DecimalV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum EnvironmentField {
    OsClass,
    RunnerImageClass,
    ToolchainClass,
    CpuClass,
    GpuStackClass,
    BuildClass,
    CollectorClass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidationPolicy {
    pub rules: Vec<PathRule>,
    pub unknown: TargetRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathRule {
    pub id: String,
    pub selectors: Vec<PathSelector>,
    pub targets: Vec<TargetRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathSelector {
    Exact { path: String },
    Prefix { path: String },
}

impl PathSelector {
    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Exact { path: expected } => path == expected,
            Self::Prefix { path: prefix } => path.starts_with(prefix),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetRef {
    All,
    Suite { id: String },
    Metric { id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Continue,
    Redirect,
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRules {
    pub hard_stop_failure: Decision,
    pub required_failure: Decision,
    pub all_required_pass: Decision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregationPolicy {
    pub percentile_method: PercentileMethod,
    pub exact_method: ExactMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PercentileMethod {
    NearestRankV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactMethod {
    AllEqualV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicy {
    pub max_encoded_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_container_items: usize,
    pub max_records: usize,
    pub max_fields_per_record: usize,
    pub max_total_fields: usize,
    pub max_string_bytes: usize,
    pub max_total_string_bytes: usize,
    pub max_raw_log_refs: usize,
    pub max_context_receipts: usize,
    pub environment_fields: Vec<FieldSpec>,
    pub record_schemas: Vec<RecordSchema>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSchema {
    pub kind: String,
    pub fields: Vec<FieldSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    pub key: String,
    pub field_type: FieldType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Identifier,
    ProjectRelative,
    U64,
    I64,
    Bool,
    Digest,
    SensitiveRedacted,
    SecretRedacted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Decode,
    NonCanonical,
    Invalid,
    MissingRequiredSubject(&'static str),
    MissingRequiredMetric(&'static str),
    DigestSidecar,
}

impl Display for ProtocolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode => formatter.write_str("the evidence protocol could not be decoded"),
            Self::NonCanonical => {
                formatter.write_str("the evidence protocol is not canonically encoded")
            }
            Self::Invalid => formatter.write_str("the evidence protocol is invalid"),
            Self::MissingRequiredSubject(_) => {
                formatter.write_str("the evidence protocol is missing a required subject")
            }
            Self::MissingRequiredMetric(_) => {
                formatter.write_str("the evidence protocol is missing a required metric")
            }
            Self::DigestSidecar => {
                formatter.write_str("the evidence protocol digest sidecar does not match")
            }
        }
    }
}

pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("test protocol values must serialize");
    bytes.push(b'\n');
    bytes
}

pub fn digest_ref(bytes: &[u8]) -> DigestRef {
    let digest = ContentDigest::of_bytes(bytes);
    DigestRef {
        bytes: digest.length(),
        blake3: digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

pub fn protocol_digest(protocol: &FirstPlayableProtocol) -> DigestRef {
    digest_ref(&canonical_json_bytes(protocol))
}

pub fn decode_protocol(bytes: &[u8]) -> Result<FirstPlayableProtocol, ProtocolError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let shape_limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("non-zero"),
        ItemLimit::new(16_384).expect("non-zero"),
        ItemLimit::new(4_096).expect("non-zero"),
        ByteLimit::new(1_024).expect("non-zero"),
        ByteLimit::new(128 * 1_024).expect("non-zero"),
    );
    preflight_serde_shape(&mut deserializer, shape_limits).map_err(|_| ProtocolError::Decode)?;
    let protocol = serde_json::from_slice::<FirstPlayableProtocol>(bytes)
        .map_err(|_| ProtocolError::Decode)?;
    if canonical_json_bytes(&protocol) != bytes {
        return Err(ProtocolError::NonCanonical);
    }
    validate_protocol(&protocol)?;
    Ok(protocol)
}

pub fn load_protocol_fixture() -> Result<FirstPlayableProtocol, ProtocolError> {
    let bytes =
        fs::read(repository_root().join(PROTOCOL_PATH)).map_err(|_| ProtocolError::Decode)?;
    let protocol = decode_protocol(&bytes)?;
    let expected = fs::read_to_string(repository_root().join(PROTOCOL_DIGEST_PATH))
        .map_err(|_| ProtocolError::DigestSidecar)?;
    let expected = expected
        .strip_suffix('\n')
        .filter(|value| !value.contains(['\r', '\n']))
        .ok_or(ProtocolError::DigestSidecar)?;
    if expected != protocol_digest(&protocol).blake3 {
        return Err(ProtocolError::DigestSidecar);
    }
    let source_bytes = fs::read(repository_root().join(PRODUCT_BUDGETS_PATH))
        .map_err(|_| ProtocolError::Invalid)?;
    let review_bytes = fs::read(repository_root().join(PRODUCT_BUDGET_REVIEW_PATH))
        .map_err(|_| ProtocolError::Invalid)?;
    validate_product_budget_sources(&protocol, &source_bytes, &review_bytes)?;
    Ok(protocol)
}

pub fn decode_product_budget_catalogue(
    bytes: &[u8],
) -> Result<ProductBudgetCatalogue, ProtocolError> {
    let catalogue = serde_json::from_slice::<ProductBudgetCatalogue>(bytes)
        .map_err(|_| ProtocolError::Decode)?;
    if canonical_json_bytes(&catalogue) != bytes
        || catalogue.kind != "nara.first_playable_product_budgets"
        || catalogue.format_version != 1
        || catalogue.target_results_seen
        || !is_identifier(&catalogue.revision)
        || catalogue.adoption.decision_id != "u22_product_constraints_v1"
        || catalogue.adoption.decision_kind
            != ProductBudgetDecisionKind::PreTargetNormativeConstraints
        || !is_utc_timestamp(&catalogue.adoption.adopted_at_utc)
        || !is_identifier(&catalogue.adoption.adopted_by)
        || catalogue.adoption.authority_path
            != "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
        || !is_lower_hex(&catalogue.adoption.authority_revision, 40)
        || !catalogue
            .adoption
            .baseline_policy
            .observed_baselines_describe_current_performance
        || !catalogue
            .adoption
            .baseline_policy
            .normative_constraints_precede_target_results
        || !catalogue
            .adoption
            .baseline_policy
            .result_informed_revision_requires_new_version
        || !catalogue
            .adoption
            .baseline_policy
            .revised_constraints_cannot_decide_source_results
        || !valid_correctness_contract(&catalogue.correctness_contract)
        || !strictly_sorted_unique(&catalogue.sources, |source| source.id.as_str())
    {
        return Err(ProtocolError::Invalid);
    }
    for source in &catalogue.sources {
        if !is_identifier(&source.id)
            || !is_identifier(&source.basis_id)
            || !strictly_sorted_unique(&source.metrics, |metric| metric.id.as_str())
        {
            return Err(ProtocolError::Invalid);
        }
        for metric in &source.metrics {
            let measurement = &metric.measurement;
            if !is_identifier(&metric.id)
                || !is_identifier(&metric.rationale_id)
                || !is_identifier(&measurement.subject)
                || !is_identifier(&measurement.environment_class)
                || !is_identifier(&measurement.workload_id)
                || !is_identifier(&measurement.start_boundary_id)
                || !is_identifier(&measurement.end_boundary_id)
                || !is_identifier(&measurement.method_id)
                || measurement.required_context.is_empty()
                || !strictly_sorted_strings(&measurement.required_context)
                || measurement
                    .required_context
                    .iter()
                    .any(|id| !is_identifier(id))
            {
                return Err(ProtocolError::Invalid);
            }
        }
    }
    Ok(catalogue)
}

pub fn decode_product_budget_review(bytes: &[u8]) -> Result<ProductBudgetReview, ProtocolError> {
    let review =
        serde_json::from_slice::<ProductBudgetReview>(bytes).map_err(|_| ProtocolError::Decode)?;
    let scopes = review
        .attestations
        .iter()
        .map(|attestation| attestation.scope_id.as_str())
        .collect::<BTreeSet<_>>();
    if canonical_json_bytes(&review) != bytes
        || review.kind != "nara.first_playable_product_budget_review"
        || review.format_version != 1
        || review.review_id != "first_playable_product_budget_review_v1"
        || !is_utc_timestamp(&review.reviewed_at_utc)
        || !is_lower_hex(&review.source_revision, 40)
        || review.target_results_seen
        || review.verdict != ReviewVerdict::Approve
        || review.reviewed_artifact.path != PRODUCT_BUDGETS_PATH
        || !is_identifier(&review.reviewed_artifact.revision)
        || !valid_digest(&review.reviewed_artifact.digest)
        || review.attestations.len() < 2
        || !strictly_sorted_unique(&review.attestations, |value| value.reviewer_id.as_str())
        || !scopes.contains("performance_measurement_v1")
        || !scopes.contains("protocol_provenance_v1")
    {
        return Err(ProtocolError::Invalid);
    }
    for attestation in &review.attestations {
        if !is_identifier(&attestation.reviewer_id)
            || attestation.reviewer_class != "independent_pre_u4"
            || !is_identifier(&attestation.scope_id)
            || !is_utc_timestamp(&attestation.reviewed_at_utc)
            || attestation.reviewed_at_utc > review.reviewed_at_utc
            || attestation.target_results_seen
            || attestation.verdict != ReviewVerdict::Approve
            || expected_review_evidence_path(&attestation.scope_id)
                != Some(attestation.evidence_path.as_str())
            || !valid_digest(&attestation.evidence_digest)
        {
            return Err(ProtocolError::Invalid);
        }
    }
    Ok(review)
}

fn expected_review_evidence_path(scope_id: &str) -> Option<&'static str> {
    match scope_id {
        "performance_measurement_v1" => Some(PERFORMANCE_REVIEW_EVIDENCE_PATH),
        "protocol_provenance_v1" => Some(PROVENANCE_REVIEW_EVIDENCE_PATH),
        _ => None,
    }
}

pub fn decode_independent_review_evidence(
    bytes: &[u8],
) -> Result<IndependentReviewEvidence, ProtocolError> {
    let evidence = serde_json::from_slice::<IndependentReviewEvidence>(bytes)
        .map_err(|_| ProtocolError::Decode)?;
    if canonical_json_bytes(&evidence) != bytes
        || evidence.kind != "nara.first_playable_independent_review"
        || evidence.format_version != 1
        || !is_identifier(&evidence.reviewer_id)
        || evidence.reviewer_class != "independent_pre_u4"
        || expected_review_evidence_path(&evidence.scope_id).is_none()
        || !is_utc_timestamp(&evidence.reviewed_at_utc)
        || !is_lower_hex(&evidence.source_revision, 40)
        || evidence.target_results_seen
        || evidence.verdict != ReviewVerdict::Approve
        || evidence.reviewed_artifact.path != PRODUCT_BUDGETS_PATH
        || !is_identifier(&evidence.reviewed_artifact.revision)
        || !valid_digest(&evidence.reviewed_artifact.digest)
    {
        return Err(ProtocolError::Invalid);
    }
    Ok(evidence)
}

pub fn validate_product_budget_sources(
    protocol: &FirstPlayableProtocol,
    source_bytes: &[u8],
    review_bytes: &[u8],
) -> Result<(), ProtocolError> {
    let catalogue = decode_product_budget_catalogue(source_bytes)?;
    let review = decode_product_budget_review(review_bytes)?;
    validate_review_evidence(&review)?;
    validate_range_sources(protocol, &catalogue, source_bytes, &review, review_bytes)
}

fn validate_review_evidence(review: &ProductBudgetReview) -> Result<(), ProtocolError> {
    for attestation in &review.attestations {
        let bytes = fs::read(repository_root().join(&attestation.evidence_path))
            .map_err(|_| ProtocolError::Invalid)?;
        if digest_ref(&bytes) != attestation.evidence_digest {
            return Err(ProtocolError::Invalid);
        }
        let evidence = decode_independent_review_evidence(&bytes)?;
        if evidence.reviewer_id != attestation.reviewer_id
            || evidence.reviewer_class != attestation.reviewer_class
            || evidence.scope_id != attestation.scope_id
            || evidence.reviewed_at_utc != attestation.reviewed_at_utc
            || evidence.source_revision != review.source_revision
            || evidence.target_results_seen != attestation.target_results_seen
            || evidence.verdict != attestation.verdict
            || evidence.reviewed_artifact != review.reviewed_artifact
        {
            return Err(ProtocolError::Invalid);
        }
    }
    Ok(())
}

fn validate_range_sources(
    protocol: &FirstPlayableProtocol,
    catalogue: &ProductBudgetCatalogue,
    source_bytes: &[u8],
    review: &ProductBudgetReview,
    review_bytes: &[u8],
) -> Result<(), ProtocolError> {
    let artifact_digest = digest_ref(source_bytes);
    let review_digest = digest_ref(review_bytes);
    if review.source_revision != catalogue.adoption.authority_revision
        || review.reviewed_at_utc < catalogue.adoption.adopted_at_utc
        || review
            .attestations
            .iter()
            .any(|attestation| attestation.reviewed_at_utc < catalogue.adoption.adopted_at_utc)
        || review.reviewed_artifact.path != PRODUCT_BUDGETS_PATH
        || review.reviewed_artifact.revision != catalogue.revision
        || review.reviewed_artifact.digest != artifact_digest
        || review
            .attestations
            .iter()
            .any(|attestation| attestation.reviewer_id == catalogue.adoption.adopted_by)
    {
        return Err(ProtocolError::Invalid);
    }
    let definitions = protocol
        .range_sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let sources = catalogue
        .sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    if catalogue.correctness_contract.rubrics.iter().any(|rubric| {
        protocol
            .metrics
            .iter()
            .find(|metric| metric.id == rubric.metric_id)
            .is_none_or(|metric| {
                metric.source.reference != "zero_tolerance_contract_v1"
                    || metric.suite != "ownership_gate"
            })
    }) {
        return Err(ProtocolError::Invalid);
    }
    if definitions.len() != sources.len() {
        return Err(ProtocolError::Invalid);
    }
    for (id, definition) in definitions {
        let source = sources.get(id).ok_or(ProtocolError::Invalid)?;
        if definition.kind != source.kind
            || definition.revision != catalogue.revision
            || definition.artifact_path != PRODUCT_BUDGETS_PATH
            || definition.artifact_digest != artifact_digest
            || definition.source_revision != catalogue.adoption.authority_revision
            || definition.review_id != review.review_id
            || definition.review_path != PRODUCT_BUDGET_REVIEW_PATH
            || definition.review_digest != review_digest
        {
            return Err(ProtocolError::Invalid);
        }
        for budget in &source.metrics {
            let metric = protocol
                .metrics
                .iter()
                .find(|metric| metric.id == budget.id)
                .ok_or(ProtocolError::Invalid)?;
            if metric.source.reference != source.id
                || metric.source.kind != source.kind
                || metric.target != budget.target
                || metric.subject != budget.measurement.subject
                || metric.environment_class != budget.measurement.environment_class
                || metric.value_kind != budget.measurement.value_kind
                || metric.statistic != budget.measurement.statistic
                || metric.population != budget.measurement.population
                || metric.workload_id != budget.measurement.workload_id
                || metric.start_boundary_id != budget.measurement.start_boundary_id
                || metric.end_boundary_id != budget.measurement.end_boundary_id
                || metric.method_id != budget.measurement.method_id
                || metric.required_context != budget.measurement.required_context
                || metric.reuse_policy != budget.measurement.reuse_policy
            {
                return Err(ProtocolError::Invalid);
            }
        }
    }
    if catalogue
        .sources
        .iter()
        .flat_map(|source| &source.metrics)
        .count()
        != protocol.metrics.len()
    {
        return Err(ProtocolError::Invalid);
    }
    Ok(())
}

fn valid_correctness_contract(contract: &CorrectnessContract) -> bool {
    let lifecycle_states = contract
        .lifecycle_states
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    contract.id == "first_playable_correctness_v1"
        && contract.baseline_subject == "u26.manual_counterfactual"
        && contract.candidate_subject == "u25.ownership_comparison"
        && contract.independent_review_required
        && is_identifier(&contract.coverage_denominator)
        && contract.comparison_baseline == "relative_to_u26_v1"
        && !contract.task_steps.is_empty()
        && strictly_sorted_strings(&contract.task_steps)
        && !contract.fault_cases.is_empty()
        && strictly_sorted_strings(&contract.fault_cases)
        && contract.initial_lifecycle_state == "candidate"
        && lifecycle_states.contains(contract.initial_lifecycle_state.as_str())
        && lifecycle_states.contains("stopped")
        && contract.lifecycle_states.len() == lifecycle_states.len()
        && !contract.lifecycle_states.is_empty()
        && strictly_sorted_strings(&contract.lifecycle_states)
        && !contract.lifecycle_transitions.is_empty()
        && contract
            .lifecycle_transitions
            .is_sorted_by(|left, right| (&left.from, &left.to) < (&right.from, &right.to))
        && contract.lifecycle_transitions.iter().all(|transition| {
            is_identifier(&transition.from)
                && is_identifier(&transition.to)
                && transition.from != transition.to
                && lifecycle_states.contains(transition.from.as_str())
                && lifecycle_states.contains(transition.to.as_str())
        })
        && !contract
            .lifecycle_transitions
            .iter()
            .any(|transition| transition.from == "stopped")
        && lifecycle_states_reachable(contract)
        && lifecycle_states_can_reach_stopped(contract)
        && !contract.rubrics.is_empty()
        && strictly_sorted_unique(&contract.rubrics, |rubric| rubric.metric_id.as_str())
        && contract.rubrics.iter().all(|rubric| {
            is_identifier(&rubric.metric_id)
                && is_identifier(&rubric.rule_id)
                && rubric.ambiguity_outcome == Decision::Redirect
        })
}

fn lifecycle_states_reachable(contract: &CorrectnessContract) -> bool {
    let mut reachable = BTreeSet::from([contract.initial_lifecycle_state.as_str()]);
    let mut queue = VecDeque::from([contract.initial_lifecycle_state.as_str()]);
    while let Some(state) = queue.pop_front() {
        for transition in contract
            .lifecycle_transitions
            .iter()
            .filter(|transition| transition.from == state)
        {
            if reachable.insert(transition.to.as_str()) {
                queue.push_back(transition.to.as_str());
            }
        }
    }
    reachable.len() == contract.lifecycle_states.len()
}

fn lifecycle_states_can_reach_stopped(contract: &CorrectnessContract) -> bool {
    let mut terminating = BTreeSet::from(["stopped"]);
    let mut queue = VecDeque::from(["stopped"]);
    while let Some(state) = queue.pop_front() {
        for transition in contract
            .lifecycle_transitions
            .iter()
            .filter(|transition| transition.to == state)
        {
            if terminating.insert(transition.from.as_str()) {
                queue.push_back(transition.from.as_str());
            }
        }
    }
    terminating.len() == contract.lifecycle_states.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipContractDigests {
    pub correctness_contract: DigestRef,
    pub fault_matrix: DigestRef,
    pub lifecycle_graph: DigestRef,
}

#[derive(Serialize)]
struct LifecycleGraphDigestInput<'a> {
    initial_state: &'a str,
    states: &'a [String],
    transitions: &'a [LifecycleTransition],
}

pub fn ownership_contract_digests(catalogue: &ProductBudgetCatalogue) -> OwnershipContractDigests {
    let contract = &catalogue.correctness_contract;
    OwnershipContractDigests {
        correctness_contract: digest_ref(&canonical_json_bytes(contract)),
        fault_matrix: digest_ref(&canonical_json_bytes(&contract.fault_cases)),
        lifecycle_graph: digest_ref(&canonical_json_bytes(&LifecycleGraphDigestInput {
            initial_state: &contract.initial_lifecycle_state,
            states: &contract.lifecycle_states,
            transitions: &contract.lifecycle_transitions,
        })),
    }
}

pub fn validate_protocol(protocol: &FirstPlayableProtocol) -> Result<(), ProtocolError> {
    if protocol.kind != "nara.first_playable_evidence_protocol"
        || protocol.format_version != 1
        || protocol.protocol_id != "reference_game_first_playable_v1"
        || protocol.decision.hard_stop_failure != Decision::Stop
        || protocol.decision.required_failure != Decision::Redirect
        || protocol.decision.all_required_pass != Decision::Continue
        || protocol.aggregation.percentile_method != PercentileMethod::NearestRankV1
        || protocol.aggregation.exact_method != ExactMethod::AllEqualV1
        || !strictly_sorted_unique(&protocol.range_sources, |value| value.id.as_str())
        || !strictly_sorted_unique(&protocol.subjects, |value| value.id.as_str())
        || !strictly_sorted_unique(&protocol.suites, |value| value.id.as_str())
        || !strictly_sorted_unique(&protocol.metrics, |value| value.id.as_str())
        || !strictly_sorted_unique(&protocol.environment_classes, |value| value.id.as_str())
        || !strictly_sorted_unique(&protocol.invalidation.rules, |value| value.id.as_str())
        || !matches!(protocol.invalidation.unknown, TargetRef::All)
        || !valid_environment_value_policy(&protocol.environment_value_policy)
        || !valid_identity_policy(&protocol.identity_policy)
    {
        return Err(ProtocolError::Invalid);
    }

    let subject_ids = protocol
        .subjects
        .iter()
        .map(|value| value.id.as_str())
        .collect::<BTreeSet<_>>();
    let range_sources = protocol
        .range_sources
        .iter()
        .map(|value| (value.id.as_str(), value))
        .collect::<BTreeMap<_, _>>();
    for source in &protocol.range_sources {
        if !is_identifier(&source.id)
            || !is_identifier(&source.revision)
            || !is_repo_relative(&source.artifact_path)
            || !valid_digest(&source.artifact_digest)
            || !is_lower_hex(&source.source_revision, 40)
            || !is_identifier(&source.review_id)
            || !is_repo_relative(&source.review_path)
            || !valid_digest(&source.review_digest)
        {
            return Err(ProtocolError::Invalid);
        }
    }
    for required in REQUIRED_SUBJECT_IDS {
        if !subject_ids.contains(required) {
            return Err(ProtocolError::MissingRequiredSubject(required));
        }
    }

    let metric_ids = protocol
        .metrics
        .iter()
        .map(|value| value.id.as_str())
        .collect::<BTreeSet<_>>();
    for required in REQUIRED_METRIC_IDS {
        if !metric_ids.contains(required) {
            return Err(ProtocolError::MissingRequiredMetric(required));
        }
    }

    let suites = protocol
        .suites
        .iter()
        .map(|value| (value.id.as_str(), value))
        .collect::<BTreeMap<_, _>>();
    let environments = protocol
        .environment_classes
        .iter()
        .map(|value| (value.id.as_str(), value))
        .collect::<BTreeMap<_, _>>();
    let record_kinds = protocol
        .evidence
        .record_schemas
        .iter()
        .map(|schema| schema.kind.as_str())
        .collect::<BTreeSet<_>>();

    for subject in &protocol.subjects {
        if !is_identifier(&subject.id)
            || !identity_identifier_is_valid(protocol, &subject.generator)
            || subject.run_providers.is_empty()
            || !strictly_sorted_strings(&subject.run_providers)
            || subject
                .run_providers
                .iter()
                .any(|provider| !identity_identifier_is_valid(protocol, provider))
            || !strictly_sorted_strings(&subject.required_context)
            || subject
                .required_context
                .iter()
                .any(|id| !identity_identifier_is_valid(protocol, id))
            || subject.environment_classes.is_empty()
            || subject.record_kinds.is_empty()
            || subject.populations.is_empty()
            || !strictly_sorted_strings(&subject.environment_classes)
            || !strictly_sorted_strings(&subject.record_kinds)
            || !strictly_sorted_strings(&subject.metric_subjects)
            || !strictly_sorted_strings(&subject.peer_subjects)
            || !strictly_sorted_values(&subject.populations)
            || subject
                .environment_classes
                .iter()
                .any(|id| !environments.contains_key(id.as_str()))
            || subject
                .metric_subjects
                .iter()
                .chain(subject.peer_subjects.iter())
                .any(|id| !subject_ids.contains(id.as_str()))
            || subject
                .record_kinds
                .iter()
                .any(|kind| !record_kinds.contains(kind.as_str()))
        {
            return Err(ProtocolError::Invalid);
        }
    }
    for suite in &protocol.suites {
        let expected_metrics = protocol
            .metrics
            .iter()
            .filter(|metric| metric.suite == suite.id)
            .map(|metric| metric.id.clone())
            .collect::<Vec<_>>();
        if !is_identifier(&suite.id)
            || !strictly_sorted_strings(&suite.metrics)
            || suite.metrics != expected_metrics
        {
            return Err(ProtocolError::Invalid);
        }
    }
    for environment in &protocol.environment_classes {
        if !is_identifier(&environment.id)
            || environment.required_fields.is_empty()
            || !strictly_sorted_values(&environment.required_fields)
        {
            return Err(ProtocolError::Invalid);
        }
    }
    for metric in &protocol.metrics {
        let Some(subject) = protocol
            .subjects
            .iter()
            .find(|value| value.id == metric.subject)
        else {
            return Err(ProtocolError::Invalid);
        };
        let Some(suite) = suites.get(metric.suite.as_str()) else {
            return Err(ProtocolError::Invalid);
        };
        if !is_identifier(&metric.id)
            || metric.minimum_samples == 0
            || !metric.required
            || (metric.hard_stop && !metric.required)
            || metric.collector != subject.collector
            || !subject
                .environment_classes
                .contains(&metric.environment_class)
            || !subject.metric_subjects.contains(&metric.subject)
            || !subject.populations.contains(&metric.population)
            || !suite.metrics.contains(&metric.id)
            || !environments.contains_key(metric.environment_class.as_str())
            || !is_identifier(&metric.source.reference)
            || !identity_identifier_is_valid(protocol, &metric.workload_id)
            || !identity_identifier_is_valid(protocol, &metric.start_boundary_id)
            || !identity_identifier_is_valid(protocol, &metric.end_boundary_id)
            || !identity_identifier_is_valid(protocol, &metric.method_id)
            || metric.required_context.is_empty()
            || !strictly_sorted_strings(&metric.required_context)
            || metric
                .required_context
                .iter()
                .any(|id| !identity_identifier_is_valid(protocol, id))
            || metric.reuse_policy
                != if matches!(metric.suite.as_str(), "ownership_gate" | "candidate_gate") {
                    EvidenceReusePolicy::CurrentRevisionOnly
                } else {
                    EvidenceReusePolicy::InvalidationMapped
                }
            || !metric.value_kind.accepts(metric.target.passing_value())
        {
            return Err(ProtocolError::Invalid);
        }
        let Some(source) = range_sources.get(metric.source.reference.as_str()) else {
            return Err(ProtocolError::Invalid);
        };
        if source.kind != metric.source.kind {
            return Err(ProtocolError::Invalid);
        }
    }
    validate_invalidation(protocol, &suites, &metric_ids)?;
    validate_evidence_policy(&protocol.evidence)?;
    Ok(())
}

fn valid_environment_value_policy(policy: &EnvironmentValuePolicy) -> bool {
    policy.id == "environment_value_policy_v1"
        && !policy.forbidden_values.is_empty()
        && strictly_sorted_strings(&policy.forbidden_values)
        && policy
            .forbidden_values
            .iter()
            .all(|value| is_identifier(value) && *value == value.to_ascii_lowercase())
        && !policy.forbidden_fragments.is_empty()
        && strictly_sorted_strings(&policy.forbidden_fragments)
        && policy
            .forbidden_fragments
            .iter()
            .all(|fragment| valid_forbidden_fragment(fragment))
        && !policy.os_prefixes.is_empty()
        && strictly_sorted_strings(&policy.os_prefixes)
        && policy.os_prefixes.iter().all(|prefix| {
            prefix.ends_with('_') && prefix.strip_suffix('_').is_some_and(is_identifier)
        })
        && !policy.architectures.is_empty()
        && strictly_sorted_strings(&policy.architectures)
        && policy
            .architectures
            .iter()
            .all(|value| is_identifier(value))
        && policy.toolchain_prefix == "rustc_"
        && policy.minimum_toolchain_numeric_segments >= 2
        && policy.versioned_fields
            == [
                EnvironmentField::RunnerImageClass,
                EnvironmentField::CpuClass,
                EnvironmentField::GpuStackClass,
                EnvironmentField::BuildClass,
                EnvironmentField::CollectorClass,
            ]
        && policy.optional_not_applicable_fields == [EnvironmentField::GpuStackClass]
}

fn valid_identity_policy(policy: &IdentityPolicy) -> bool {
    policy.id == "identity_policy_v1"
        && policy.identifier_grammar == IdentifierGrammar::SafeAsciiV1
        && policy.repository_grammar == RepositoryGrammar::OwnerRepoSlugV1
        && policy.run_id_grammar == RunIdGrammar::DecimalV1
        && !policy.forbidden_fragments.is_empty()
        && strictly_sorted_strings(&policy.forbidden_fragments)
        && policy
            .forbidden_fragments
            .iter()
            .all(|fragment| valid_forbidden_fragment(fragment))
}

fn valid_forbidden_fragment(fragment: &str) -> bool {
    !fragment.is_empty()
        && fragment.len() <= 64
        && *fragment == fragment.to_ascii_lowercase()
        && !fragment.bytes().any(|byte| byte.is_ascii_whitespace())
}

fn validate_invalidation(
    protocol: &FirstPlayableProtocol,
    suites: &BTreeMap<&str, &SuiteSpec>,
    metrics: &BTreeSet<&str>,
) -> Result<(), ProtocolError> {
    for rule in &protocol.invalidation.rules {
        if !is_identifier(&rule.id)
            || rule.selectors.is_empty()
            || rule.targets.is_empty()
            || !strictly_sorted_targets(&rule.targets)
        {
            return Err(ProtocolError::Invalid);
        }
        for selector in &rule.selectors {
            let path = match selector {
                PathSelector::Exact { path } | PathSelector::Prefix { path } => path,
            };
            let valid = match selector {
                PathSelector::Exact { .. } => is_repo_relative(path),
                PathSelector::Prefix { .. } => is_repo_prefix(path),
            };
            if !valid {
                return Err(ProtocolError::Invalid);
            }
        }
        for target in &rule.targets {
            match target {
                TargetRef::All => {}
                TargetRef::Suite { id } if suites.contains_key(id.as_str()) => {}
                TargetRef::Metric { id } if metrics.contains(id.as_str()) => {}
                _ => return Err(ProtocolError::Invalid),
            }
        }
    }
    Ok(())
}

fn validate_evidence_policy(policy: &EvidencePolicy) -> Result<(), ProtocolError> {
    if [
        policy.max_encoded_bytes,
        policy.max_depth,
        policy.max_nodes,
        policy.max_container_items,
        policy.max_records,
        policy.max_fields_per_record,
        policy.max_total_fields,
        policy.max_string_bytes,
        policy.max_total_string_bytes,
        policy.max_raw_log_refs,
        policy.max_context_receipts,
    ]
    .contains(&0)
        || !strictly_sorted_unique(&policy.environment_fields, |value| value.key.as_str())
        || !strictly_sorted_unique(&policy.record_schemas, |value| value.kind.as_str())
    {
        return Err(ProtocolError::Invalid);
    }
    for schema in &policy.record_schemas {
        if !is_identifier(&schema.kind)
            || schema.fields.is_empty()
            || !strictly_sorted_unique(&schema.fields, |value| value.key.as_str())
        {
            return Err(ProtocolError::Invalid);
        }
    }
    if policy
        .environment_fields
        .iter()
        .any(|field| !is_identifier(&field.key))
    {
        return Err(ProtocolError::Invalid);
    }
    Ok(())
}

fn strictly_sorted_unique<T, F>(values: &[T], key: F) -> bool
where
    F: Fn(&T) -> &str,
{
    values.is_sorted_by(|left, right| key(left) < key(right))
}

fn strictly_sorted_strings(values: &[String]) -> bool {
    values.is_sorted_by(|left, right| left < right)
}

fn strictly_sorted_values<T: Ord>(values: &[T]) -> bool {
    values.is_sorted_by(|left, right| left < right)
}

fn target_sort_key(target: &TargetRef) -> (u8, &str) {
    match target {
        TargetRef::All => (0, ""),
        TargetRef::Metric { id } => (1, id),
        TargetRef::Suite { id } => (2, id),
    }
}

fn strictly_sorted_targets(values: &[TargetRef]) -> bool {
    values.is_sorted_by(|left, right| target_sort_key(left) < target_sort_key(right))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricObservation {
    pub metric_id: String,
    pub value: u64,
    pub samples: usize,
    pub raw_records: usize,
    pub population: Population,
    pub environment: EnvironmentRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionEvidenceError {
    InvalidProtocol,
    UnknownSuite,
    UnknownMetric,
    MetricOutsideSuite,
    DuplicateMetric,
    InvalidExpectedEnvironment,
    OwnershipAdmissionRequired,
}

pub fn expected_environments_for_suite(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
) -> BTreeMap<String, EnvironmentRecord> {
    protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == suite_id)
        .map(|metric| {
            (
                metric.id.clone(),
                EnvironmentRecord {
                    population: metric.population,
                    fields: [
                        (EnvironmentField::OsClass, "windows_11_x86_64"),
                        (EnvironmentField::RunnerImageClass, "trusted_runner_v1"),
                        (EnvironmentField::ToolchainClass, "rustc_1_97_0_msvc"),
                        (EnvironmentField::CpuClass, "x86_64_desktop_v1"),
                        (EnvironmentField::GpuStackClass, "software_vulkan_v1"),
                        (EnvironmentField::BuildClass, "debug_incremental_v1"),
                        (EnvironmentField::CollectorClass, "trusted_collector_v1"),
                    ]
                    .into_iter()
                    .map(|(field, value)| (field, value.to_owned()))
                    .collect(),
                },
            )
        })
        .collect()
}

pub fn passing_observations(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    environments: &BTreeMap<String, EnvironmentRecord>,
) -> Vec<MetricObservation> {
    protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == suite_id)
        .map(|metric| MetricObservation {
            metric_id: metric.id.clone(),
            value: metric.target.passing_value(),
            samples: metric.minimum_samples,
            raw_records: metric.minimum_samples,
            population: metric.population,
            environment: environments
                .get(&metric.id)
                .expect("every suite metric needs a trusted environment")
                .clone(),
        })
        .collect()
}

pub fn decide_suite(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    observations: &[MetricObservation],
    expected_environments: &BTreeMap<String, EnvironmentRecord>,
) -> Result<Decision, DecisionEvidenceError> {
    if suite_id == "ownership_gate" {
        return Err(DecisionEvidenceError::OwnershipAdmissionRequired);
    }
    decide_suite_inner(protocol, suite_id, observations, expected_environments)
}

fn decide_suite_inner(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    observations: &[MetricObservation],
    expected_environments: &BTreeMap<String, EnvironmentRecord>,
) -> Result<Decision, DecisionEvidenceError> {
    if validate_protocol(protocol).is_err() {
        return Err(DecisionEvidenceError::InvalidProtocol);
    }
    let suite = protocol
        .suites
        .iter()
        .find(|suite| suite.id == suite_id)
        .ok_or(DecisionEvidenceError::UnknownSuite)?;
    let suite_metrics = suite
        .metrics
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected_environments.len() != suite.metrics.len()
        || expected_environments
            .keys()
            .any(|metric| !suite_metrics.contains(metric.as_str()))
    {
        return Err(DecisionEvidenceError::InvalidExpectedEnvironment);
    }
    let mut indexed = BTreeMap::new();
    for observation in observations {
        let Some(metric) = protocol
            .metrics
            .iter()
            .find(|metric| metric.id == observation.metric_id)
        else {
            return Err(DecisionEvidenceError::UnknownMetric);
        };
        if metric.suite != suite_id {
            return Err(DecisionEvidenceError::MetricOutsideSuite);
        }
        if indexed
            .insert(observation.metric_id.as_str(), observation)
            .is_some()
        {
            return Err(DecisionEvidenceError::DuplicateMetric);
        }
    }

    let mut redirect = false;
    let mut stop = false;
    for metric in protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == suite_id && metric.required)
    {
        let Some(expected_environment) = expected_environments.get(&metric.id) else {
            return Err(DecisionEvidenceError::InvalidExpectedEnvironment);
        };
        let Some(observation) = indexed.get(metric.id.as_str()) else {
            redirect = true;
            continue;
        };
        let evidence_complete = observation.samples >= metric.minimum_samples
            && observation.raw_records >= observation.samples
            && observation.population == metric.population
            && metric.value_kind.accepts(observation.value)
            && environment_equivalent(
                protocol,
                &metric.id,
                &observation.environment,
                expected_environment,
            );
        if !evidence_complete {
            redirect = true;
        } else if !metric.target.accepts(observation.value) {
            if metric.hard_stop {
                stop = true;
            } else {
                redirect = true;
            }
        }
    }
    if stop {
        Ok(protocol.decision.hard_stop_failure)
    } else if redirect {
        Ok(protocol.decision.required_failure)
    } else {
        Ok(protocol.decision.all_required_pass)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SourceChange {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRevisionAdmission {
    current_source_revision: String,
    protocol_digest: DigestRef,
    prior_revisions: Vec<PriorRevisionAdmission>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PriorRevisionAdmission {
    source_revision: String,
    merge_base_revision: String,
    change_manifest_digest: DigestRef,
    changes: Vec<SourceChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionAdmissionError {
    InvalidRevision,
    RepositoryUnavailable,
    UncommittedSource,
    CurrentRevisionMismatch,
    RevisionNotAncestor,
    ChangeManifest,
}

impl Display for RevisionAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidRevision => "source revision identity rejected",
            Self::RepositoryUnavailable => "source repository proof unavailable",
            Self::UncommittedSource => "source repository contains uncommitted content",
            Self::CurrentRevisionMismatch => "source revision does not match repository head",
            Self::RevisionNotAncestor => "prior source revision is not an ancestor",
            Self::ChangeManifest => "source change manifest rejected",
        };
        formatter.write_str(message)
    }
}

#[derive(Serialize)]
struct SourceChangeManifestDigestInput<'a> {
    prior_revision: &'a str,
    current_revision: &'a str,
    merge_base_revision: &'a str,
    changes: &'a [SourceChange],
}

pub fn revision_admission_from_git(
    protocol: &FirstPlayableProtocol,
    repository_root: &Path,
    current_source_revision: &str,
    prior_source_revisions: &[String],
) -> Result<SourceRevisionAdmission, RevisionAdmissionError> {
    if !is_lower_hex(current_source_revision, 40)
        || prior_source_revisions
            .iter()
            .any(|revision| !is_lower_hex(revision, 40) || revision == current_source_revision)
    {
        return Err(RevisionAdmissionError::InvalidRevision);
    }
    let requested_root = fs::canonicalize(repository_root)
        .map_err(|_| RevisionAdmissionError::RepositoryUnavailable)?;
    let discovered_root = git_text(repository_root, &["rev-parse", "--show-toplevel"])?;
    let discovered_root = fs::canonicalize(discovered_root)
        .map_err(|_| RevisionAdmissionError::RepositoryUnavailable)?;
    if requested_root != discovered_root {
        return Err(RevisionAdmissionError::RepositoryUnavailable);
    }
    if !git_bytes(
        repository_root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err(RevisionAdmissionError::UncommittedSource);
    }
    let head = git_text(repository_root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if head != current_source_revision {
        return Err(RevisionAdmissionError::CurrentRevisionMismatch);
    }

    let unique_priors = prior_source_revisions
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if unique_priors.len() != prior_source_revisions.len() {
        return Err(RevisionAdmissionError::InvalidRevision);
    }

    let mut prior_revisions = Vec::with_capacity(unique_priors.len());
    for prior_revision in unique_priors {
        let ancestry = git_command(repository_root)
            .args([
                "merge-base",
                "--is-ancestor",
                prior_revision,
                current_source_revision,
            ])
            .status()
            .map_err(|_| RevisionAdmissionError::RepositoryUnavailable)?;
        if !ancestry.success() {
            return Err(RevisionAdmissionError::RevisionNotAncestor);
        }
        let merge_base = git_text(
            repository_root,
            &["merge-base", prior_revision, current_source_revision],
        )?;
        if merge_base != prior_revision {
            return Err(RevisionAdmissionError::RevisionNotAncestor);
        }
        let diff = git_bytes(
            repository_root,
            &[
                "diff",
                "--name-status",
                "--find-renames",
                "-z",
                prior_revision,
                current_source_revision,
                "--",
            ],
        )?;
        let changes = parse_git_name_status(&diff)?;
        if changes.is_empty() {
            return Err(RevisionAdmissionError::ChangeManifest);
        }
        let change_manifest_digest =
            digest_ref(&canonical_json_bytes(&SourceChangeManifestDigestInput {
                prior_revision,
                current_revision: current_source_revision,
                merge_base_revision: &merge_base,
                changes: &changes,
            }));
        prior_revisions.push(PriorRevisionAdmission {
            source_revision: prior_revision.to_owned(),
            merge_base_revision: merge_base,
            change_manifest_digest,
            changes,
        });
    }

    Ok(SourceRevisionAdmission {
        current_source_revision: current_source_revision.to_owned(),
        protocol_digest: protocol_digest(protocol),
        prior_revisions,
    })
}

fn git_text(repository_root: &Path, arguments: &[&str]) -> Result<String, RevisionAdmissionError> {
    let bytes = git_bytes(repository_root, arguments)?;
    let value =
        String::from_utf8(bytes).map_err(|_| RevisionAdmissionError::RepositoryUnavailable)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.contains(['\r', '\n']) {
        return Err(RevisionAdmissionError::RepositoryUnavailable);
    }
    Ok(value.to_owned())
}

fn git_bytes(
    repository_root: &Path,
    arguments: &[&str],
) -> Result<Vec<u8>, RevisionAdmissionError> {
    let output = git_command(repository_root)
        .args(arguments)
        .output()
        .map_err(|_| RevisionAdmissionError::RepositoryUnavailable)?;
    output
        .status
        .success()
        .then_some(output.stdout)
        .ok_or(RevisionAdmissionError::RepositoryUnavailable)
}

fn git_command(repository_root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(repository_root);
    for variable in [
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_WORK_TREE",
    ] {
        command.env_remove(variable);
    }
    command
}

fn parse_git_name_status(bytes: &[u8]) -> Result<Vec<SourceChange>, RevisionAdmissionError> {
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            if fields.peek().is_none() {
                break;
            }
            return Err(RevisionAdmissionError::ChangeManifest);
        }
        let status =
            std::str::from_utf8(status).map_err(|_| RevisionAdmissionError::ChangeManifest)?;
        let mut path = || {
            let value = fields
                .next()
                .filter(|value| !value.is_empty())
                .ok_or(RevisionAdmissionError::ChangeManifest)?;
            Ok(std::str::from_utf8(value)
                .ok()
                .filter(|value| is_git_repo_relative(value))
                .map(str::to_owned))
        };
        let unknown = || SourceChange {
            old_path: None,
            new_path: None,
        };
        let change = match status.as_bytes().first().copied() {
            Some(b'A') => path()?.map_or_else(unknown, |path| SourceChange {
                old_path: None,
                new_path: Some(path),
            }),
            Some(b'D') => path()?.map_or_else(unknown, |path| SourceChange {
                old_path: Some(path),
                new_path: None,
            }),
            Some(b'M' | b'T') if status.len() == 1 => {
                path()?.map_or_else(unknown, |path| SourceChange {
                    old_path: Some(path.clone()),
                    new_path: Some(path),
                })
            }
            Some(b'R' | b'C')
                if status.len() > 1 && status[1..].bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                match (path()?, path()?) {
                    (Some(old_path), Some(new_path)) => SourceChange {
                        old_path: Some(old_path),
                        new_path: Some(new_path),
                    },
                    _ => unknown(),
                }
            }
            _ => return Err(RevisionAdmissionError::ChangeManifest),
        };
        changes.push(change);
    }
    if !changes.is_sorted() {
        changes.sort();
    }
    if changes.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(RevisionAdmissionError::ChangeManifest);
    }
    Ok(changes)
}

pub fn affected_metrics(
    protocol: &FirstPlayableProtocol,
    observed_protocol_digest: &DigestRef,
    changes: &[SourceChange],
) -> BTreeSet<String> {
    let all = || {
        protocol
            .metrics
            .iter()
            .map(|metric| metric.id.clone())
            .collect::<BTreeSet<_>>()
    };
    if *observed_protocol_digest != protocol_digest(protocol) || changes.is_empty() {
        return all();
    }

    let suites = protocol
        .suites
        .iter()
        .map(|suite| (suite.id.as_str(), suite))
        .collect::<BTreeMap<_, _>>();
    let mut affected = BTreeSet::new();
    for change in changes {
        let paths = [change.old_path.as_deref(), change.new_path.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if paths.is_empty() || paths.iter().any(|path| !is_repo_relative(path)) {
            return all();
        }
        for path in paths {
            let mut matched = false;
            for rule in &protocol.invalidation.rules {
                if rule.selectors.iter().any(|selector| selector.matches(path)) {
                    matched = true;
                    for target in &rule.targets {
                        if matches!(target, TargetRef::All) {
                            return all();
                        }
                        expand_target(target, &suites, &mut affected);
                    }
                }
            }
            if !matched {
                return all();
            }
        }
    }
    affected
}

fn expand_target(
    target: &TargetRef,
    suites: &BTreeMap<&str, &SuiteSpec>,
    affected: &mut BTreeSet<String>,
) {
    match target {
        TargetRef::All => unreachable!("all targets return before expansion"),
        TargetRef::Suite { id } => {
            if let Some(suite) = suites.get(id.as_str()) {
                affected.extend(suite.metrics.iter().cloned());
            }
        }
        TargetRef::Metric { id } => {
            affected.insert(id.clone());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentRecord {
    pub population: Population,
    pub fields: BTreeMap<EnvironmentField, String>,
}

pub fn environment_equivalent(
    protocol: &FirstPlayableProtocol,
    metric_id: &str,
    left: &EnvironmentRecord,
    right: &EnvironmentRecord,
) -> bool {
    let Some(metric) = protocol
        .metrics
        .iter()
        .find(|metric| metric.id == metric_id)
    else {
        return false;
    };
    let Some(class) = protocol
        .environment_classes
        .iter()
        .find(|class| class.id == metric.environment_class)
    else {
        return false;
    };
    left.population == right.population
        && left.population == metric.population
        && class.required_fields.iter().all(|field| {
            left.fields
                .get(field)
                .zip(right.fields.get(field))
                .is_some_and(|(left, right)| {
                    valid_environment_value(protocol, *field, left, true)
                        && valid_environment_value(protocol, *field, right, true)
                        && left == right
                })
        })
}

fn valid_environment_value(
    protocol: &FirstPlayableProtocol,
    field: EnvironmentField,
    value: &str,
    required: bool,
) -> bool {
    let policy = &protocol.environment_value_policy;
    let lowercase = value.to_ascii_lowercase();
    if !is_identifier(value)
        || policy.forbidden_values.binary_search(&lowercase).is_ok()
        || contains_forbidden_fragment(&policy.forbidden_fragments, value)
    {
        return false;
    }

    match field {
        EnvironmentField::OsClass => {
            policy
                .os_prefixes
                .iter()
                .any(|prefix| value.starts_with(prefix))
                && policy
                    .architectures
                    .iter()
                    .any(|architecture| value.contains(architecture))
        }
        EnvironmentField::ToolchainClass => {
            value.starts_with(&policy.toolchain_prefix)
                && value
                    .split('_')
                    .filter(|segment| !segment.is_empty())
                    .filter(|segment| segment.bytes().all(|byte| byte.is_ascii_digit()))
                    .count()
                    >= policy.minimum_toolchain_numeric_segments
        }
        field
            if value == "not_applicable"
                && policy.optional_not_applicable_fields.contains(&field) =>
        {
            !required
        }
        field if policy.versioned_fields.contains(&field) => has_versioned_class_suffix(value),
        _ => false,
    }
}

fn contains_forbidden_fragment(fragments: &[String], value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    fragments
        .iter()
        .any(|fragment| lowercase.contains(fragment))
}

fn has_versioned_class_suffix(value: &str) -> bool {
    value
        .rsplit('_')
        .next()
        .and_then(|segment| segment.strip_prefix('v'))
        .is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
        })
}

pub fn is_ancestor(edges: &[(&str, &str)], ancestor: &str, descendant: &str) -> bool {
    let mut queue = VecDeque::from([ancestor]);
    let mut visited = BTreeSet::new();
    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if current == descendant {
            return true;
        }
        queue.extend(
            edges
                .iter()
                .filter_map(|(parent, child)| (*parent == current).then_some(*child)),
        );
    }
    false
}

pub fn ancestry_allows(edges: &[(&str, &str)]) -> bool {
    ["u4", "u12", "u5", "u26", "u24", "u25"]
        .iter()
        .all(|unit| is_ancestor(edges, "u22", unit))
        && is_ancestor(edges, "u26", "u24")
        && is_ancestor(edges, "u24", "u25")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestRef {
    pub bytes: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceEnvelope {
    pub kind: String,
    pub format_version: u32,
    pub generator: String,
    pub identity: EvidenceIdentity,
    pub payload_digest: DigestRef,
    pub payload: EvidencePayload,
    pub restricted_raw_logs: Vec<RestrictedRawLogRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceIdentity {
    pub run_provider: String,
    pub run_id: String,
    pub run_attempt: u64,
    pub repository: String,
    pub source_revision: String,
    pub protocol_digest: DigestRef,
    pub subject: String,
    pub environment_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedEvidenceIdentity {
    pub generator: String,
    pub identity: EvidenceIdentity,
    pub environment: Vec<EvidenceField>,
    pub context_receipts: Vec<ContextReceipt>,
    pub restricted_raw_logs: Vec<RestrictedRawLogRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedTransfer {
    pub path: String,
    pub digest: DigestRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePayload {
    pub context_receipts: Vec<ContextReceipt>,
    pub environment: Vec<EvidenceField>,
    pub records: Vec<EvidenceRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextReceipt {
    pub id: String,
    pub digest: DigestRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub kind: String,
    pub id: String,
    pub fields: Vec<EvidenceField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceField {
    pub key: String,
    pub value: FieldValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldValue {
    Identifier { value: String },
    ProjectRelative { value: String },
    U64 { value: u64 },
    I64 { value: i64 },
    Bool { value: bool },
    Digest { value: DigestRef },
    SensitiveRedacted,
    SecretRedacted,
}

impl FieldValue {
    fn field_type(&self) -> FieldType {
        match self {
            Self::Identifier { .. } => FieldType::Identifier,
            Self::ProjectRelative { .. } => FieldType::ProjectRelative,
            Self::U64 { .. } => FieldType::U64,
            Self::I64 { .. } => FieldType::I64,
            Self::Bool { .. } => FieldType::Bool,
            Self::Digest { .. } => FieldType::Digest,
            Self::SensitiveRedacted => FieldType::SensitiveRedacted,
            Self::SecretRedacted => FieldType::SecretRedacted,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestrictedRawLogRef {
    pub artifact_id: String,
    pub digest: DigestRef,
    pub retention_until_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeLimits {
    pub max_encoded_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_container_items: usize,
    pub max_records: usize,
    pub max_fields_per_record: usize,
    pub max_total_fields: usize,
    pub max_string_bytes: usize,
    pub max_total_string_bytes: usize,
    pub max_raw_log_refs: usize,
    pub max_context_receipts: usize,
}

impl From<&EvidencePolicy> for EnvelopeLimits {
    fn from(policy: &EvidencePolicy) -> Self {
        Self {
            max_encoded_bytes: policy.max_encoded_bytes,
            max_depth: policy.max_depth,
            max_nodes: policy.max_nodes,
            max_container_items: policy.max_container_items,
            max_records: policy.max_records,
            max_fields_per_record: policy.max_fields_per_record,
            max_total_fields: policy.max_total_fields,
            max_string_bytes: policy.max_string_bytes,
            max_total_string_bytes: policy.max_total_string_bytes,
            max_raw_log_refs: policy.max_raw_log_refs,
            max_context_receipts: policy.max_context_receipts,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceError {
    EncodedBytes,
    TransferDigest,
    Shape,
    Decode,
    NonCanonical,
    DomainLimit,
    Identity,
    Catalogue,
    InvalidValue,
    PayloadDigest,
}

impl Display for EvidenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EncodedBytes => "evidence encoded-byte limit exceeded",
            Self::TransferDigest => "evidence transfer identity rejected",
            Self::Shape => "evidence structural shape rejected",
            Self::Decode => "evidence typed decode rejected",
            Self::NonCanonical => "evidence canonical encoding rejected",
            Self::DomainLimit => "evidence domain limit rejected",
            Self::Identity => "evidence expected identity rejected",
            Self::Catalogue => "evidence field catalogue rejected",
            Self::InvalidValue => "evidence field value rejected",
            Self::PayloadDigest => "evidence payload digest rejected",
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEvidence(EvidenceEnvelope);

pub fn validated_evidence_digest(evidence: &ValidatedEvidence) -> DigestRef {
    digest_ref(&canonical_json_bytes(&evidence.0))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipDecisionAdmission {
    protocol_digest: DigestRef,
    source_revision: String,
    baseline_digest: DigestRef,
    candidate_digest: DigestRef,
    fault_matrix_digest: DigestRef,
    lifecycle_graph_digest: DigestRef,
    correctness_contract_digest: DigestRef,
    reviewer_attestation_digest: DigestRef,
    ownership_metric_ids: BTreeSet<String>,
}

pub fn ownership_decision_admission(
    protocol: &FirstPlayableProtocol,
    catalogue: &ProductBudgetCatalogue,
    baseline: &ValidatedEvidence,
    revision_admission: &SourceRevisionAdmission,
    candidate_digest: DigestRef,
    reviewer_attestation_digest: DigestRef,
) -> Result<OwnershipDecisionAdmission, EvidenceAggregationError> {
    validate_source_revision_admission(protocol, revision_admission)?;
    let protocol_digest = protocol_digest(protocol);
    let catalogue_digest = digest_ref(&canonical_json_bytes(catalogue));
    let ownership_metric_ids = protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == "ownership_gate")
        .map(|metric| metric.id.clone())
        .collect::<BTreeSet<_>>();
    let rubric_metric_ids = catalogue
        .correctness_contract
        .rubrics
        .iter()
        .map(|rubric| rubric.metric_id.clone())
        .collect::<BTreeSet<_>>();
    let baseline_metric_ids = ownership_record_metric_ids(&baseline.0)?;
    let contract_digests = ownership_contract_digests(catalogue);
    if !valid_correctness_contract(&catalogue.correctness_contract)
        || protocol
            .range_sources
            .iter()
            .any(|source| source.artifact_digest != catalogue_digest)
        || ownership_metric_ids.is_empty()
        || rubric_metric_ids != ownership_metric_ids
        || baseline_metric_ids != ownership_metric_ids
        || baseline.0.payload.records.len() != ownership_metric_ids.len()
        || baseline.0.identity.subject != catalogue.correctness_contract.baseline_subject
        || baseline.0.identity.source_revision != revision_admission.current_source_revision
        || baseline.0.identity.protocol_digest != protocol_digest
        || baseline.0.identity.environment_class != "paired_ownership_v1"
        || !valid_digest(&candidate_digest)
        || !valid_digest(&reviewer_attestation_digest)
        || context_digest(&baseline.0, "u22_correctness_contract_v1")
            != Some(&contract_digests.correctness_contract)
        || context_digest(&baseline.0, "u22_fault_matrix_v1")
            != Some(&contract_digests.fault_matrix)
        || context_digest(&baseline.0, "u22_lifecycle_graph_v1")
            != Some(&contract_digests.lifecycle_graph)
        || context_digest(&baseline.0, "independent_reviewer_attestation_v1")
            != Some(&reviewer_attestation_digest)
    {
        return Err(EvidenceAggregationError::OwnershipCohortMismatch);
    }
    Ok(OwnershipDecisionAdmission {
        protocol_digest,
        source_revision: revision_admission.current_source_revision.clone(),
        baseline_digest: validated_evidence_digest(baseline),
        candidate_digest,
        fault_matrix_digest: contract_digests.fault_matrix,
        lifecycle_graph_digest: contract_digests.lifecycle_graph,
        correctness_contract_digest: contract_digests.correctness_contract,
        reviewer_attestation_digest,
        ownership_metric_ids,
    })
}

fn ownership_record_metric_ids(
    envelope: &EvidenceEnvelope,
) -> Result<BTreeSet<String>, EvidenceAggregationError> {
    envelope
        .payload
        .records
        .iter()
        .map(|record| {
            if record.kind != "ownership_inventory" {
                return Err(EvidenceAggregationError::InvalidRecord);
            }
            identifier_field(record, "metric_id")
                .map(str::to_owned)
                .map_err(|_| EvidenceAggregationError::InvalidRecord)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceAggregationError {
    UnknownSuite,
    InvalidRevisionAdmission,
    StaleEvidence,
    InvalidRecord,
    MetricOutsideSuite,
    NonDecisionSubject,
    EnvironmentMismatch,
    DuplicateSample,
    NonContiguousSamples,
    InconsistentExactSamples,
    MissingOwnershipDecisionAdmission,
    MissingOwnershipCohort,
    OwnershipCohortMismatch,
    Arithmetic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceSuiteError {
    Aggregation(EvidenceAggregationError),
    Decision(DecisionEvidenceError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetricSampleGroup {
    environment: EnvironmentRecord,
    samples: BTreeMap<u64, u64>,
}

pub fn aggregate_validated_observations(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    evidence: &[ValidatedEvidence],
    revision_admission: &SourceRevisionAdmission,
) -> Result<Vec<MetricObservation>, EvidenceAggregationError> {
    aggregate_validated_observations_inner(protocol, suite_id, evidence, revision_admission, None)
}

pub fn aggregate_validated_ownership_observations(
    protocol: &FirstPlayableProtocol,
    evidence: &[ValidatedEvidence],
    revision_admission: &SourceRevisionAdmission,
    ownership_admission: &OwnershipDecisionAdmission,
) -> Result<Vec<MetricObservation>, EvidenceAggregationError> {
    aggregate_validated_observations_inner(
        protocol,
        "ownership_gate",
        evidence,
        revision_admission,
        Some(ownership_admission),
    )
}

fn aggregate_validated_observations_inner(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    evidence: &[ValidatedEvidence],
    revision_admission: &SourceRevisionAdmission,
    ownership_admission: Option<&OwnershipDecisionAdmission>,
) -> Result<Vec<MetricObservation>, EvidenceAggregationError> {
    if !protocol.suites.iter().any(|suite| suite.id == suite_id) {
        return Err(EvidenceAggregationError::UnknownSuite);
    }
    validate_source_revision_admission(protocol, revision_admission)?;
    if suite_id == "ownership_gate" {
        let ownership_admission = ownership_admission
            .ok_or(EvidenceAggregationError::MissingOwnershipDecisionAdmission)?;
        validate_ownership_cohort(protocol, evidence, revision_admission, ownership_admission)?;
    } else if ownership_admission.is_some() {
        return Err(EvidenceAggregationError::InvalidRecord);
    }

    let mut groups = BTreeMap::<String, MetricSampleGroup>::new();
    for validated in evidence {
        let envelope = &validated.0;
        if envelope.identity.protocol_digest != revision_admission.protocol_digest {
            return Err(EvidenceAggregationError::InvalidRevisionAdmission);
        }
        if suite_id == "ownership_gate" && envelope.identity.subject == "u26.manual_counterfactual"
        {
            continue;
        }
        let environment_fields = envelope_environment_fields(envelope)?;
        for record in &envelope.payload.records {
            let metric_id = identifier_field(record, "metric_id")
                .map_err(|_| EvidenceAggregationError::InvalidRecord)?;
            let metric = protocol
                .metrics
                .iter()
                .find(|metric| metric.id == metric_id)
                .ok_or(EvidenceAggregationError::InvalidRecord)?;
            if metric.suite != suite_id {
                return Err(EvidenceAggregationError::MetricOutsideSuite);
            }
            if envelope.identity.subject != metric.subject {
                return Err(EvidenceAggregationError::NonDecisionSubject);
            }
            if !metric_revision_is_admitted(
                protocol,
                metric,
                &envelope.identity.source_revision,
                revision_admission,
            ) {
                return Err(EvidenceAggregationError::StaleEvidence);
            }

            let (population, sample_index, value) = match record.kind.as_str() {
                "metric_sample" => {
                    let population = parse_population(
                        identifier_field(record, "population")
                            .map_err(|_| EvidenceAggregationError::InvalidRecord)?,
                    )
                    .ok_or(EvidenceAggregationError::InvalidRecord)?;
                    let sample_index = u64_field(record, "sample_index")
                        .map_err(|_| EvidenceAggregationError::InvalidRecord)?;
                    let value = u64_field(record, "value")
                        .map_err(|_| EvidenceAggregationError::InvalidRecord)?;
                    (population, sample_index, value)
                }
                "ownership_inventory" => {
                    let value = u64_field(record, "value")
                        .map_err(|_| EvidenceAggregationError::InvalidRecord)?;
                    (Population::NotApplicable, 1, value)
                }
                _ => return Err(EvidenceAggregationError::InvalidRecord),
            };
            let environment = EnvironmentRecord {
                population,
                fields: environment_fields.clone(),
            };
            let group = groups
                .entry(metric.id.clone())
                .or_insert_with(|| MetricSampleGroup {
                    environment: environment.clone(),
                    samples: BTreeMap::new(),
                });
            if !environment_equivalent(protocol, &metric.id, &group.environment, &environment) {
                return Err(EvidenceAggregationError::EnvironmentMismatch);
            }
            if group.samples.insert(sample_index, value).is_some() {
                return Err(EvidenceAggregationError::DuplicateSample);
            }
        }
    }

    let mut observations = Vec::new();
    for metric in protocol
        .metrics
        .iter()
        .filter(|metric| metric.suite == suite_id)
    {
        let Some(group) = groups.remove(&metric.id) else {
            continue;
        };
        for (position, sample_index) in group.samples.keys().enumerate() {
            let expected_index =
                u64::try_from(position + 1).map_err(|_| EvidenceAggregationError::Arithmetic)?;
            if *sample_index != expected_index {
                return Err(EvidenceAggregationError::NonContiguousSamples);
            }
        }
        let mut values = group.samples.into_values().collect::<Vec<_>>();
        let value = aggregate_metric_values(protocol, metric.statistic, &mut values)?;
        observations.push(MetricObservation {
            metric_id: metric.id.clone(),
            value,
            samples: values.len(),
            raw_records: values.len(),
            population: metric.population,
            environment: group.environment,
        });
    }
    Ok(observations)
}

pub fn decide_validated_suite(
    protocol: &FirstPlayableProtocol,
    suite_id: &str,
    evidence: &[ValidatedEvidence],
    expected_environments: &BTreeMap<String, EnvironmentRecord>,
    revision_admission: &SourceRevisionAdmission,
) -> Result<Decision, EvidenceSuiteError> {
    let observations =
        aggregate_validated_observations(protocol, suite_id, evidence, revision_admission)
            .map_err(EvidenceSuiteError::Aggregation)?;
    decide_suite(protocol, suite_id, &observations, expected_environments)
        .map_err(EvidenceSuiteError::Decision)
}

pub fn decide_validated_ownership_suite(
    protocol: &FirstPlayableProtocol,
    evidence: &[ValidatedEvidence],
    expected_environments: &BTreeMap<String, EnvironmentRecord>,
    revision_admission: &SourceRevisionAdmission,
    ownership_admission: &OwnershipDecisionAdmission,
) -> Result<Decision, EvidenceSuiteError> {
    let observations = aggregate_validated_ownership_observations(
        protocol,
        evidence,
        revision_admission,
        ownership_admission,
    )
    .map_err(EvidenceSuiteError::Aggregation)?;
    decide_suite_inner(
        protocol,
        "ownership_gate",
        &observations,
        expected_environments,
    )
    .map_err(EvidenceSuiteError::Decision)
}

fn validate_source_revision_admission(
    protocol: &FirstPlayableProtocol,
    admission: &SourceRevisionAdmission,
) -> Result<(), EvidenceAggregationError> {
    if !is_lower_hex(&admission.current_source_revision, 40)
        || admission.protocol_digest != protocol_digest(protocol)
        || !admission
            .prior_revisions
            .is_sorted_by(|left, right| left.source_revision < right.source_revision)
        || admission.prior_revisions.iter().any(|prior| {
            !is_lower_hex(&prior.source_revision, 40)
                || prior.source_revision == admission.current_source_revision
                || prior.merge_base_revision != prior.source_revision
                || prior.changes.is_empty()
                || !valid_digest(&prior.change_manifest_digest)
                || prior.change_manifest_digest
                    != digest_ref(&canonical_json_bytes(&SourceChangeManifestDigestInput {
                        prior_revision: &prior.source_revision,
                        current_revision: &admission.current_source_revision,
                        merge_base_revision: &prior.merge_base_revision,
                        changes: &prior.changes,
                    }))
        })
    {
        return Err(EvidenceAggregationError::InvalidRevisionAdmission);
    }
    Ok(())
}

fn metric_revision_is_admitted(
    protocol: &FirstPlayableProtocol,
    metric: &MetricRule,
    evidence_revision: &str,
    admission: &SourceRevisionAdmission,
) -> bool {
    if evidence_revision == admission.current_source_revision {
        return true;
    }
    if metric.reuse_policy == EvidenceReusePolicy::CurrentRevisionOnly {
        return false;
    }
    let Some(prior) = admission
        .prior_revisions
        .iter()
        .find(|prior| prior.source_revision == evidence_revision)
    else {
        return false;
    };
    !affected_metrics(protocol, &admission.protocol_digest, &prior.changes).contains(&metric.id)
}

fn validate_ownership_cohort(
    protocol: &FirstPlayableProtocol,
    evidence: &[ValidatedEvidence],
    revision_admission: &SourceRevisionAdmission,
    ownership_admission: &OwnershipDecisionAdmission,
) -> Result<(), EvidenceAggregationError> {
    let baselines = evidence
        .iter()
        .filter(|validated| validated.0.identity.subject == "u26.manual_counterfactual")
        .collect::<Vec<_>>();
    let candidates = evidence
        .iter()
        .filter(|validated| validated.0.identity.subject == "u25.ownership_comparison")
        .collect::<Vec<_>>();
    if baselines.len() != 1 || candidates.is_empty() {
        return Err(EvidenceAggregationError::MissingOwnershipCohort);
    }
    let baseline = &baselines[0].0;
    let baseline_digest = digest_ref(&canonical_json_bytes(baseline));
    let baseline_environment = envelope_environment_fields(baseline)?;
    let baseline_metric_ids = ownership_record_metric_ids(baseline)?;
    if ownership_admission.protocol_digest != protocol_digest(protocol)
        || ownership_admission.protocol_digest != revision_admission.protocol_digest
        || ownership_admission.source_revision != revision_admission.current_source_revision
        || baseline.identity.source_revision != revision_admission.current_source_revision
        || baseline.identity.protocol_digest != ownership_admission.protocol_digest
        || baseline_digest != ownership_admission.baseline_digest
        || baseline_metric_ids != ownership_admission.ownership_metric_ids
        || baseline.payload.records.len() != ownership_admission.ownership_metric_ids.len()
        || context_digest(baseline, "u22_correctness_contract_v1")
            != Some(&ownership_admission.correctness_contract_digest)
        || context_digest(baseline, "u22_fault_matrix_v1")
            != Some(&ownership_admission.fault_matrix_digest)
        || context_digest(baseline, "u22_lifecycle_graph_v1")
            != Some(&ownership_admission.lifecycle_graph_digest)
        || context_digest(baseline, "independent_reviewer_attestation_v1")
            != Some(&ownership_admission.reviewer_attestation_digest)
    {
        return Err(EvidenceAggregationError::OwnershipCohortMismatch);
    }

    let mut candidate_metric_ids = BTreeSet::new();
    let mut candidate_record_count = 0_usize;
    for candidate in candidates {
        let candidate = &candidate.0;
        let metric_ids = ownership_record_metric_ids(candidate)?;
        candidate_record_count = candidate_record_count
            .checked_add(candidate.payload.records.len())
            .ok_or(EvidenceAggregationError::Arithmetic)?;
        candidate_metric_ids.extend(metric_ids);
        if candidate.identity.source_revision != revision_admission.current_source_revision
            || candidate.identity.environment_class != baseline.identity.environment_class
            || envelope_environment_fields(candidate)? != baseline_environment
            || context_digest(candidate, "u26_baseline_digest_v1")
                != Some(&ownership_admission.baseline_digest)
            || context_digest(candidate, "u24_candidate_digest_v1")
                != Some(&ownership_admission.candidate_digest)
            || context_digest(candidate, "u22_correctness_contract_v1")
                != Some(&ownership_admission.correctness_contract_digest)
            || context_digest(candidate, "u22_fault_matrix_v1")
                != Some(&ownership_admission.fault_matrix_digest)
            || context_digest(candidate, "u22_lifecycle_graph_v1")
                != Some(&ownership_admission.lifecycle_graph_digest)
            || context_digest(candidate, "independent_reviewer_attestation_v1")
                != Some(&ownership_admission.reviewer_attestation_digest)
            || candidate.identity.protocol_digest != ownership_admission.protocol_digest
        {
            return Err(EvidenceAggregationError::OwnershipCohortMismatch);
        }
    }
    if candidate_metric_ids != ownership_admission.ownership_metric_ids
        || candidate_record_count != ownership_admission.ownership_metric_ids.len()
    {
        return Err(EvidenceAggregationError::OwnershipCohortMismatch);
    }
    Ok(())
}

fn context_digest<'a>(envelope: &'a EvidenceEnvelope, id: &str) -> Option<&'a DigestRef> {
    envelope
        .payload
        .context_receipts
        .iter()
        .find(|receipt| receipt.id == id)
        .map(|receipt| &receipt.digest)
}

fn envelope_environment_fields(
    envelope: &EvidenceEnvelope,
) -> Result<BTreeMap<EnvironmentField, String>, EvidenceAggregationError> {
    envelope
        .payload
        .environment
        .iter()
        .map(|field| {
            let environment_field = environment_field_from_key(&field.key)
                .ok_or(EvidenceAggregationError::InvalidRecord)?;
            let FieldValue::Identifier { value } = &field.value else {
                return Err(EvidenceAggregationError::InvalidRecord);
            };
            Ok((environment_field, value.clone()))
        })
        .collect()
}

fn aggregate_metric_values(
    protocol: &FirstPlayableProtocol,
    statistic: Statistic,
    values: &mut [u64],
) -> Result<u64, EvidenceAggregationError> {
    let Some(first) = values.first().copied() else {
        return Err(EvidenceAggregationError::InvalidRecord);
    };
    match statistic {
        Statistic::Exact => {
            if protocol.aggregation.exact_method != ExactMethod::AllEqualV1
                || values.iter().any(|value| *value != first)
            {
                return Err(EvidenceAggregationError::InconsistentExactSamples);
            }
            Ok(first)
        }
        Statistic::P50 | Statistic::P95 | Statistic::P99 => {
            if protocol.aggregation.percentile_method != PercentileMethod::NearestRankV1 {
                return Err(EvidenceAggregationError::InvalidRecord);
            }
            values.sort_unstable();
            let percentile = match statistic {
                Statistic::P50 => 50_usize,
                Statistic::P95 => 95,
                Statistic::P99 => 99,
                Statistic::Exact => unreachable!(),
            };
            let rank_numerator = percentile
                .checked_mul(values.len())
                .and_then(|value| value.checked_add(99))
                .ok_or(EvidenceAggregationError::Arithmetic)?;
            let rank = rank_numerator / 100;
            values
                .get(rank.saturating_sub(1))
                .copied()
                .ok_or(EvidenceAggregationError::Arithmetic)
        }
    }
}

#[derive(Default)]
pub struct TrustedEvidenceStore {
    published: Vec<ValidatedEvidence>,
}

impl TrustedEvidenceStore {
    pub fn published_count(&self) -> usize {
        self.published.len()
    }

    pub fn published(&self) -> &[ValidatedEvidence] {
        &self.published
    }
}

thread_local! {
    static TYPED_DECODE_COUNT: Cell<usize> = const { Cell::new(0) };
}

pub fn reset_typed_decode_count() {
    TYPED_DECODE_COUNT.set(0);
}

pub fn typed_decode_count() -> usize {
    TYPED_DECODE_COUNT.get()
}

pub fn ingest_evidence(
    bytes: &[u8],
    transfer_entries: &[TransferEntry],
    limits: EnvelopeLimits,
    expected_transfer: &ExpectedTransfer,
    expected_identity: &ExpectedEvidenceIdentity,
    protocol: &FirstPlayableProtocol,
    store: &mut TrustedEvidenceStore,
) -> Result<(), EvidenceError> {
    preflight_transfer_table(
        transfer_entries,
        expected_transfer,
        limits.max_encoded_bytes,
    )?;
    let validated = decode_evidence(
        bytes,
        limits,
        expected_transfer,
        expected_identity,
        protocol,
    )?;
    store.published.push(validated);
    Ok(())
}

pub fn decode_evidence(
    bytes: &[u8],
    limits: EnvelopeLimits,
    expected_transfer: &ExpectedTransfer,
    expected_identity: &ExpectedEvidenceIdentity,
    protocol: &FirstPlayableProtocol,
) -> Result<ValidatedEvidence, EvidenceError> {
    if bytes.len() > limits.max_encoded_bytes {
        return Err(EvidenceError::EncodedBytes);
    }
    if digest_ref(bytes) != expected_transfer.digest {
        return Err(EvidenceError::TransferDigest);
    }
    let shape_limits = SerdeShapeLimits::new(
        DepthLimit::new(limits.max_depth).ok_or(EvidenceError::Shape)?,
        ItemLimit::new(limits.max_nodes).ok_or(EvidenceError::Shape)?,
        ItemLimit::new(limits.max_container_items).ok_or(EvidenceError::Shape)?,
        ByteLimit::new(limits.max_string_bytes).ok_or(EvidenceError::Shape)?,
        ByteLimit::new(limits.max_total_string_bytes).ok_or(EvidenceError::Shape)?,
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, shape_limits).map_err(|_| EvidenceError::Shape)?;
    preflight_domain_limits(bytes, limits)?;

    TYPED_DECODE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    let envelope =
        serde_json::from_slice::<EvidenceEnvelope>(bytes).map_err(|_| EvidenceError::Decode)?;
    if canonical_json_bytes(&envelope) != bytes {
        return Err(EvidenceError::NonCanonical);
    }
    validate_domain_limits(&envelope, limits)?;
    validate_identity(&envelope, expected_identity, protocol)?;
    validate_payload(&envelope, protocol)?;
    let payload_bytes = serde_json::to_vec(&envelope.payload).map_err(|_| EvidenceError::Decode)?;
    if digest_ref(&payload_bytes) != envelope.payload_digest {
        return Err(EvidenceError::PayloadDigest);
    }
    Ok(ValidatedEvidence(envelope))
}

fn preflight_domain_limits(bytes: &[u8], limits: EnvelopeLimits) -> Result<(), EvidenceError> {
    let value =
        serde_json::from_slice::<serde_json::Value>(bytes).map_err(|_| EvidenceError::Decode)?;
    let payload = value
        .get("payload")
        .and_then(serde_json::Value::as_object)
        .ok_or(EvidenceError::Decode)?;
    let environment = payload
        .get("environment")
        .and_then(serde_json::Value::as_array)
        .ok_or(EvidenceError::Decode)?;
    let context_receipts = payload
        .get("context_receipts")
        .and_then(serde_json::Value::as_array)
        .ok_or(EvidenceError::Decode)?;
    let records = payload
        .get("records")
        .and_then(serde_json::Value::as_array)
        .ok_or(EvidenceError::Decode)?;
    let raw_logs = value
        .get("restricted_raw_logs")
        .and_then(serde_json::Value::as_array)
        .ok_or(EvidenceError::Decode)?;

    if records.len() > limits.max_records
        || raw_logs.len() > limits.max_raw_log_refs
        || context_receipts.len() > limits.max_context_receipts
    {
        return Err(EvidenceError::DomainLimit);
    }

    let mut total_fields = environment.len();
    for record in records {
        let fields = record
            .get("fields")
            .and_then(serde_json::Value::as_array)
            .ok_or(EvidenceError::Decode)?;
        if fields.len() > limits.max_fields_per_record {
            return Err(EvidenceError::DomainLimit);
        }
        total_fields = total_fields
            .checked_add(fields.len())
            .ok_or(EvidenceError::DomainLimit)?;
    }
    if total_fields > limits.max_total_fields {
        return Err(EvidenceError::DomainLimit);
    }
    Ok(())
}

fn validate_domain_limits(
    envelope: &EvidenceEnvelope,
    limits: EnvelopeLimits,
) -> Result<(), EvidenceError> {
    if envelope.payload.records.len() > limits.max_records
        || envelope.restricted_raw_logs.len() > limits.max_raw_log_refs
        || envelope.payload.context_receipts.len() > limits.max_context_receipts
        || envelope
            .payload
            .records
            .iter()
            .any(|record| record.fields.len() > limits.max_fields_per_record)
    {
        return Err(EvidenceError::DomainLimit);
    }
    let total_fields = envelope.payload.environment.len()
        + envelope
            .payload
            .records
            .iter()
            .map(|record| record.fields.len())
            .sum::<usize>();
    if total_fields > limits.max_total_fields {
        return Err(EvidenceError::DomainLimit);
    }
    Ok(())
}

fn validate_identity(
    envelope: &EvidenceEnvelope,
    expected: &ExpectedEvidenceIdentity,
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    let subject = protocol
        .subjects
        .iter()
        .find(|subject| subject.id == envelope.identity.subject)
        .ok_or(EvidenceError::Identity)?;
    if envelope.kind != "nara.evidence"
        || envelope.format_version != 1
        || envelope.generator != expected.generator
        || envelope.identity != expected.identity
        || envelope.payload.environment != expected.environment
        || envelope.payload.context_receipts != expected.context_receipts
        || envelope.restricted_raw_logs != expected.restricted_raw_logs
        || !trusted_environment_is_valid(expected, protocol)
        || envelope.identity.protocol_digest != protocol_digest(protocol)
        || subject.generator != envelope.generator
        || !subject
            .run_providers
            .contains(&envelope.identity.run_provider)
        || !subject
            .environment_classes
            .contains(&envelope.identity.environment_class)
        || !identity_identifier_is_valid(protocol, &envelope.generator)
        || !identity_identifier_is_valid(protocol, &envelope.identity.run_provider)
        || !identity_run_id_is_valid(protocol, &envelope.identity.run_id)
        || envelope.identity.run_attempt == 0
        || !identity_repository_is_valid(protocol, &envelope.identity.repository)
        || !is_lower_hex(&envelope.identity.source_revision, 40)
    {
        return Err(EvidenceError::Identity);
    }
    Ok(())
}

fn trusted_environment_is_valid(
    expected: &ExpectedEvidenceIdentity,
    protocol: &FirstPlayableProtocol,
) -> bool {
    let Some(class) = protocol
        .environment_classes
        .iter()
        .find(|class| class.id == expected.identity.environment_class)
    else {
        return false;
    };
    if validate_field_list(
        &expected.environment,
        &protocol.evidence.environment_fields,
        protocol,
    )
    .is_err()
    {
        return false;
    }
    expected.environment.iter().all(|field| {
        let Some(environment_field) = environment_field_from_key(&field.key) else {
            return false;
        };
        let FieldValue::Identifier { value } = &field.value else {
            return false;
        };
        valid_environment_value(
            protocol,
            environment_field,
            value,
            class.required_fields.contains(&environment_field),
        )
    })
}

fn environment_field_from_key(key: &str) -> Option<EnvironmentField> {
    match key {
        "os_class" => Some(EnvironmentField::OsClass),
        "runner_image_class" => Some(EnvironmentField::RunnerImageClass),
        "toolchain_class" => Some(EnvironmentField::ToolchainClass),
        "cpu_class" => Some(EnvironmentField::CpuClass),
        "gpu_stack_class" => Some(EnvironmentField::GpuStackClass),
        "build_class" => Some(EnvironmentField::BuildClass),
        "collector_class" => Some(EnvironmentField::CollectorClass),
        _ => None,
    }
}

fn validate_payload(
    envelope: &EvidenceEnvelope,
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    let expected_environment = &protocol.evidence.environment_fields;
    validate_field_list(
        &envelope.payload.environment,
        expected_environment,
        protocol,
    )?;
    let subject = protocol
        .subjects
        .iter()
        .find(|subject| subject.id == envelope.identity.subject)
        .ok_or(EvidenceError::Identity)?;
    validate_context_receipts(envelope, subject, protocol)?;
    if !strictly_sorted_records(&envelope.payload.records) {
        return Err(EvidenceError::Catalogue);
    }
    for record in &envelope.payload.records {
        if !identity_identifier_is_valid(protocol, &record.kind)
            || !identity_identifier_is_valid(protocol, &record.id)
        {
            return Err(EvidenceError::InvalidValue);
        }
        if !subject.record_kinds.contains(&record.kind) {
            return Err(EvidenceError::Catalogue);
        }
        let schema = protocol
            .evidence
            .record_schemas
            .iter()
            .find(|schema| schema.kind == record.kind)
            .ok_or(EvidenceError::Catalogue)?;
        validate_field_list(&record.fields, &schema.fields, protocol)?;
        validate_record_semantics(
            record,
            subject,
            &envelope.identity.environment_class,
            protocol,
        )?;
    }
    if subject.id == "u22.calibration_review" {
        let expected_sources = protocol
            .range_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<BTreeSet<_>>();
        let observed_sources = envelope
            .payload
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<BTreeSet<_>>();
        if observed_sources != expected_sources {
            return Err(EvidenceError::Catalogue);
        }
    }
    if !strictly_sorted_unique(&envelope.restricted_raw_logs, |value| {
        value.artifact_id.as_str()
    }) {
        return Err(EvidenceError::Catalogue);
    }
    for raw_log in &envelope.restricted_raw_logs {
        if !identity_identifier_is_valid(protocol, &raw_log.artifact_id)
            || !valid_digest(&raw_log.digest)
            || raw_log.retention_until_unix_seconds == 0
        {
            return Err(EvidenceError::InvalidValue);
        }
    }
    Ok(())
}

fn validate_context_receipts(
    envelope: &EvidenceEnvelope,
    subject: &SubjectSpec,
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    if !strictly_sorted_unique(&envelope.payload.context_receipts, |receipt| {
        receipt.id.as_str()
    }) || envelope.payload.context_receipts.iter().any(|receipt| {
        !identity_identifier_is_valid(protocol, &receipt.id) || !valid_digest(&receipt.digest)
    }) {
        return Err(EvidenceError::Catalogue);
    }

    let mut required = subject
        .required_context
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for record in &envelope.payload.records {
        if !matches!(
            record.kind.as_str(),
            "metric_sample" | "ownership_inventory"
        ) {
            continue;
        }
        let metric_id = identifier_field(record, "metric_id")?;
        let metric = protocol
            .metrics
            .iter()
            .find(|metric| metric.id == metric_id)
            .ok_or(EvidenceError::Catalogue)?;
        if metric.subject == subject.id {
            required.extend(metric.required_context.iter().cloned());
        }
    }
    let observed = envelope
        .payload
        .context_receipts
        .iter()
        .map(|receipt| receipt.id.clone())
        .collect::<BTreeSet<_>>();
    (observed == required)
        .then_some(())
        .ok_or(EvidenceError::Catalogue)
}

fn validate_record_semantics(
    record: &EvidenceRecord,
    subject: &SubjectSpec,
    envelope_environment_class: &str,
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    match record.kind.as_str() {
        "calibration_review" => {
            let source_id = identifier_field(record, "source_id")?;
            let source = protocol
                .range_sources
                .iter()
                .find(|source| source.id == source_id)
                .ok_or(EvidenceError::Catalogue)?;
            if record.id != source.id
                || project_relative_field(record, "source_path")? != source.artifact_path
                || digest_field(record, "source_digest")? != &source.artifact_digest
                || identifier_field(record, "source_revision")? != source.source_revision
                || identifier_field(record, "review_id")? != source.review_id
                || digest_field(record, "review_digest")? != &source.review_digest
                || identifier_field(record, "reviewer_class")? != "independent_pre_u4"
                || !bool_field(record, "approved")?
                || bool_field(record, "target_results_seen")?
            {
                return Err(EvidenceError::Catalogue);
            }
        }
        "metric_sample" => {
            let metric_id = identifier_field(record, "metric_id")?;
            let metric = protocol
                .metrics
                .iter()
                .find(|metric| metric.id == metric_id)
                .ok_or(EvidenceError::Catalogue)?;
            let population = parse_population(identifier_field(record, "population")?)
                .ok_or(EvidenceError::Catalogue)?;
            let sample_index = u64_field(record, "sample_index")?;
            let value = u64_field(record, "value")?;
            if !subject.metric_subjects.contains(&metric.subject)
                || !subject.populations.contains(&population)
                || metric.population != population
                || metric.environment_class != envelope_environment_class
                || !record_matches_measurement(record, metric)?
                || !metric.value_kind.accepts(value)
                || sample_index == 0
                || record.id != format!("{}.sample_{sample_index:06}", metric.id)
            {
                return Err(EvidenceError::Catalogue);
            }
        }
        "ownership_inventory" => {
            let metric_id = identifier_field(record, "metric_id")?;
            let metric = protocol
                .metrics
                .iter()
                .find(|metric| metric.id == metric_id)
                .ok_or(EvidenceError::Catalogue)?;
            let peer_subject = identifier_field(record, "peer_subject")?;
            let value = u64_field(record, "value")?;
            if record.id != metric.id
                || !subject.metric_subjects.contains(&metric.subject)
                || !subject
                    .peer_subjects
                    .iter()
                    .any(|peer| peer == peer_subject)
                || metric.population != Population::NotApplicable
                || metric.environment_class != envelope_environment_class
                || !record_matches_measurement(record, metric)?
                || !metric.value_kind.accepts(value)
                || !subject.populations.contains(&metric.population)
            {
                return Err(EvidenceError::Catalogue);
            }
        }
        _ => return Err(EvidenceError::Catalogue),
    }
    Ok(())
}

fn record_matches_measurement(
    record: &EvidenceRecord,
    metric: &MetricRule,
) -> Result<bool, EvidenceError> {
    Ok(
        identifier_field(record, "workload_id")? == metric.workload_id
            && identifier_field(record, "start_boundary_id")? == metric.start_boundary_id
            && identifier_field(record, "end_boundary_id")? == metric.end_boundary_id
            && identifier_field(record, "method_id")? == metric.method_id,
    )
}

fn record_field<'a>(
    record: &'a EvidenceRecord,
    key: &str,
) -> Result<&'a FieldValue, EvidenceError> {
    record
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| &field.value)
        .ok_or(EvidenceError::Catalogue)
}

fn identifier_field<'a>(record: &'a EvidenceRecord, key: &str) -> Result<&'a str, EvidenceError> {
    match record_field(record, key)? {
        FieldValue::Identifier { value } => Ok(value),
        _ => Err(EvidenceError::Catalogue),
    }
}

fn project_relative_field<'a>(
    record: &'a EvidenceRecord,
    key: &str,
) -> Result<&'a str, EvidenceError> {
    match record_field(record, key)? {
        FieldValue::ProjectRelative { value } => Ok(value),
        _ => Err(EvidenceError::Catalogue),
    }
}

fn bool_field(record: &EvidenceRecord, key: &str) -> Result<bool, EvidenceError> {
    match record_field(record, key)? {
        FieldValue::Bool { value } => Ok(*value),
        _ => Err(EvidenceError::Catalogue),
    }
}

fn digest_field<'a>(record: &'a EvidenceRecord, key: &str) -> Result<&'a DigestRef, EvidenceError> {
    match record_field(record, key)? {
        FieldValue::Digest { value } => Ok(value),
        _ => Err(EvidenceError::Catalogue),
    }
}

fn u64_field(record: &EvidenceRecord, key: &str) -> Result<u64, EvidenceError> {
    match record_field(record, key)? {
        FieldValue::U64 { value } => Ok(*value),
        _ => Err(EvidenceError::Catalogue),
    }
}

fn parse_population(value: &str) -> Option<Population> {
    match value {
        "cold" => Some(Population::Cold),
        "warm" => Some(Population::Warm),
        "not_applicable" => Some(Population::NotApplicable),
        _ => None,
    }
}

fn validate_field_list(
    fields: &[EvidenceField],
    expected: &[FieldSpec],
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    if fields.len() != expected.len() || !strictly_sorted_unique(fields, |field| field.key.as_str())
    {
        return Err(EvidenceError::Catalogue);
    }
    for (field, expected) in fields.iter().zip(expected) {
        if field.key != expected.key || field.value.field_type() != expected.field_type {
            return Err(EvidenceError::Catalogue);
        }
        validate_field_value(&field.value, protocol)?;
    }
    Ok(())
}

fn validate_field_value(
    value: &FieldValue,
    protocol: &FirstPlayableProtocol,
) -> Result<(), EvidenceError> {
    let valid = match value {
        FieldValue::Identifier { value } => identity_identifier_is_valid(protocol, value),
        FieldValue::ProjectRelative { value } => is_repo_relative(value),
        FieldValue::U64 { .. } | FieldValue::I64 { .. } | FieldValue::Bool { .. } => true,
        FieldValue::Digest { value } => valid_digest(value),
        FieldValue::SensitiveRedacted | FieldValue::SecretRedacted => true,
    };
    valid.then_some(()).ok_or(EvidenceError::InvalidValue)
}

fn strictly_sorted_records(records: &[EvidenceRecord]) -> bool {
    records
        .windows(2)
        .all(|window| (&window[0].kind, &window[0].id) < (&window[1].kind, &window[1].id))
}

pub fn calibration_expected_identity(protocol: &FirstPlayableProtocol) -> ExpectedEvidenceIdentity {
    ExpectedEvidenceIdentity {
        generator: "nara_evidence_v1".to_owned(),
        identity: EvidenceIdentity {
            run_provider: "local".to_owned(),
            run_id: "2026071501".to_owned(),
            run_attempt: 1,
            repository: "latias94/nara".to_owned(),
            source_revision: "09695166d16d4c9411c53fe68a86ebed177bcda7".to_owned(),
            protocol_digest: protocol_digest(protocol),
            subject: "u22.calibration_review".to_owned(),
            environment_class: "portable_correctness_v1".to_owned(),
        },
        environment: [
            ("build_class", "debug_incremental_v1"),
            ("collector_class", "u22_calibration_review_v1"),
            ("cpu_class", "x86_64_desktop_v1"),
            ("gpu_stack_class", "not_applicable"),
            ("os_class", "windows_11_x86_64"),
            ("runner_image_class", "local_workstation_v1"),
            ("toolchain_class", "rustc_1_97_0_x86_64_pc_windows_msvc"),
        ]
        .into_iter()
        .map(|(key, value)| EvidenceField {
            key: key.to_owned(),
            value: FieldValue::Identifier {
                value: value.to_owned(),
            },
        })
        .collect(),
        context_receipts: Vec::new(),
        restricted_raw_logs: Vec::new(),
    }
}

pub fn expected_transfer(path: &str, bytes: &[u8]) -> ExpectedTransfer {
    ExpectedTransfer {
        path: path.to_owned(),
        digest: digest_ref(bytes),
    }
}

pub fn load_calibration_fixture() -> Result<(Vec<u8>, EvidenceEnvelope), EvidenceError> {
    let bytes = fs::read(repository_root().join(CALIBRATION_ENVELOPE_PATH))
        .map_err(|_| EvidenceError::Decode)?;
    let envelope = serde_json::from_slice(&bytes).map_err(|_| EvidenceError::Decode)?;
    Ok((bytes, envelope))
}

pub fn refresh_payload_digest(envelope: &mut EvidenceEnvelope) {
    let bytes = serde_json::to_vec(&envelope.payload).expect("test payload must serialize");
    envelope.payload_digest = digest_ref(&bytes);
}

fn valid_digest(digest: &DigestRef) -> bool {
    digest.bytes > 0 && is_lower_hex(&digest.blake3, 64)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_utc_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}

pub fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("//")
        && !value
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
}

fn identity_identifier_is_valid(protocol: &FirstPlayableProtocol, value: &str) -> bool {
    protocol.identity_policy.identifier_grammar == IdentifierGrammar::SafeAsciiV1
        && is_identifier(value)
        && !contains_forbidden_fragment(&protocol.identity_policy.forbidden_fragments, value)
}

fn identity_run_id_is_valid(protocol: &FirstPlayableProtocol, value: &str) -> bool {
    protocol.identity_policy.run_id_grammar == RunIdGrammar::DecimalV1
        && !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}

fn identity_repository_is_valid(protocol: &FirstPlayableProtocol, value: &str) -> bool {
    if protocol.identity_policy.repository_grammar != RepositoryGrammar::OwnerRepoSlugV1
        || contains_forbidden_fragment(&protocol.identity_policy.forbidden_fragments, value)
    {
        return false;
    }
    let mut segments = value.split('/');
    let Some(owner) = segments.next() else {
        return false;
    };
    let Some(repository) = segments.next() else {
        return false;
    };
    segments.next().is_none()
        && valid_repository_segment(owner)
        && valid_repository_segment(repository)
}

fn valid_repository_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

pub fn is_repo_relative(value: &str) -> bool {
    is_identifier(value) && RelativePath::new(value).is_ok()
}

fn is_git_repo_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
}

fn is_repo_prefix(value: &str) -> bool {
    value.strip_suffix('/').is_some_and(is_repo_relative)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferEntryKind {
    Regular,
    Symlink,
    Special,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferEntry {
    pub path: String,
    pub kind: TransferEntryKind,
    pub encoded_bytes: usize,
}

pub fn preflight_transfer_table(
    entries: &[TransferEntry],
    expected: &ExpectedTransfer,
    max_encoded_bytes: usize,
) -> Result<(), EvidenceError> {
    let expected_bytes =
        usize::try_from(expected.digest.bytes).map_err(|_| EvidenceError::Catalogue)?;
    if entries.len() != 1
        || !is_repo_relative(&expected.path)
        || !valid_digest(&expected.digest)
        || expected_bytes > max_encoded_bytes
    {
        return Err(EvidenceError::Catalogue);
    }
    let entry = &entries[0];
    if entry.kind != TransferEntryKind::Regular
        || entry.path != expected.path
        || !is_repo_relative(&entry.path)
        || entry.encoded_bytes != expected_bytes
    {
        return Err(EvidenceError::Catalogue);
    }
    Ok(())
}
